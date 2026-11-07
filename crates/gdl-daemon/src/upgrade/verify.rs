//! O download conferido: o `SHA256SUMS` só vale com a assinatura minisign da chave do projeto, e
//! o pacote só vale com a soma que ele lista. Primeiro as somas e a assinatura (pequenas); o
//! pacote só desce depois que elas conferem.

use std::path::{Path, PathBuf};

use gdl_common::paths::create_private_dir;
use minisign_verify::{PublicKey, Signature};
use sha2::{Digest, Sha256};

use super::{ReleaseFiles, UpgradeError};

/// O SHA-256 de `data` em hexadecimal minúsculo (como o `sha256sum` escreve).
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyError {
    /// Assinatura inválida, de outra chave, ou chave pública ausente.
    BadSignature,
    /// O `SHA256SUMS` não lista o pacote.
    NotListed,
    /// O pacote baixado não tem a soma listada.
    Mismatch,
}

/// Confere `signature` sobre `sums` com `public_key` (base64 do minisign) e devolve o SHA-256
/// (hexadecimal, minúsculo) que `sums` lista para `file_name`.
pub fn expected_sha256(
    sums: &[u8],
    signature: &[u8],
    public_key: &str,
    file_name: &str,
) -> Result<String, VerifyError> {
    let key = PublicKey::from_base64(public_key).map_err(|_| VerifyError::BadSignature)?;
    let signature = std::str::from_utf8(signature)
        .ok()
        .and_then(|text| Signature::decode(text).ok())
        .ok_or(VerifyError::BadSignature)?;
    key.verify(sums, &signature, false)
        .map_err(|_| VerifyError::BadSignature)?;
    // Linhas `<hex>  <nome>` (o `*` do modo binário do sha256sum também vale).
    String::from_utf8_lossy(sums)
        .lines()
        .find_map(|line| {
            let (hash, name) = line.split_once(char::is_whitespace)?;
            let name = name.trim_start().trim_start_matches('*');
            (name == file_name).then(|| hash.to_ascii_lowercase())
        })
        .ok_or(VerifyError::NotListed)
}

/// O pacote baixado e conferido, com a soma que o instalador confere de novo como root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
}

/// Baixa os arquivos de `files` para `dir` (criado, privado) e confere; em qualquer erro, `dir`
/// é apagada.
pub async fn download(
    files: &ReleaseFiles,
    dir: &Path,
    public_key: &str,
) -> Result<Downloaded, UpgradeError> {
    let got = fetch_checked(files, dir, public_key).await;
    if got.is_err() {
        let _ = std::fs::remove_dir_all(dir);
    }
    got
}

async fn fetch_checked(
    files: &ReleaseFiles,
    dir: &Path,
    public_key: &str,
) -> Result<Downloaded, UpgradeError> {
    let fetch = |url| async move {
        gdl_drive::fetch_bytes(url).await.map_err(|e| {
            tracing::debug!(erro = %e, "não deu para baixar um arquivo do release");
            UpgradeError::Network
        })
    };
    let (sums, signature) = tokio::try_join!(fetch(&files.sums.url), fetch(&files.signature.url))?;
    let expected = expected_sha256(&sums, &signature, public_key, &files.package.name)
        .map_err(UpgradeError::Verify)?;
    let package = fetch(&files.package.url).await?;
    // Dezenas de MB: a soma e a gravação ficam fora das threads do runtime.
    let path = dir.join(&files.package.name);
    let (dir, target, sum) = (dir.to_path_buf(), path.clone(), expected.clone());
    tokio::task::spawn_blocking(move || {
        if sha256_hex(&package) != sum {
            return Err(UpgradeError::Verify(VerifyError::Mismatch));
        }
        create_private_dir(&dir)
            .and_then(|()| std::fs::write(&target, package))
            .map_err(|e| {
                tracing::warn!(erro = %e, "não deu para gravar o pacote baixado");
                UpgradeError::Install(e.to_string())
            })
    })
    .await
    .map_err(|e| UpgradeError::Install(e.to_string()))??;
    Ok(Downloaded {
        path,
        sha256: expected,
    })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::upgrade::offer::{SIGNATURE, SUMS};
    use minisign::KeyPair;

    /// O par de chaves que o `Harness` confia (`UpdateCheck::public_key`).
    fn test_pair() -> &'static KeyPair {
        static PAIR: std::sync::OnceLock<KeyPair> = std::sync::OnceLock::new();
        PAIR.get_or_init(|| KeyPair::generate_unencrypted_keypair().unwrap())
    }

    pub(crate) fn test_public_key() -> String {
        test_pair().pk.to_base64()
    }

    /// A assinatura de `data` com a chave que o `Harness` confia.
    pub(crate) fn sign_for_harness(data: &[u8]) -> Vec<u8> {
        let pair = test_pair();
        let signature = minisign::sign(Some(&pair.pk), &pair.sk, data, None, None).unwrap();
        signature.into_string().into_bytes()
    }

    /// Um par de chaves só de teste: a pública (base64) e a assinatura de `data`.
    pub(crate) fn signed(data: &[u8]) -> (String, Vec<u8>) {
        let pair = KeyPair::generate_unencrypted_keypair().unwrap();
        let signature = minisign::sign(Some(&pair.pk), &pair.sk, data, None, None).unwrap();
        (pair.pk.to_base64(), signature.into_string().into_bytes())
    }

    /// O `SHA256SUMS` de `files` (nome, conteúdo), no formato do `sha256sum`.
    pub(crate) fn sums_of(files: &[(&str, &[u8])]) -> Vec<u8> {
        files
            .iter()
            .map(|(name, body)| format!("{}  {name}\n", sha256_hex(body)))
            .collect::<String>()
            .into_bytes()
    }

    const PKG: &str = "app_1.4.0_amd64.deb";

    #[test]
    fn good_signature_returns_hash() {
        let sums = sums_of(&[("outro.rpm", b"r"), (PKG, b"conteudo")]);
        let (key, sig) = signed(&sums);
        let hash = expected_sha256(&sums, &sig, &key, PKG).unwrap();
        assert_eq!(hash, sha256_hex(b"conteudo"));
    }

    #[test]
    fn release_signature_from_the_cli_checks_with_the_project_key() {
        // Assinado pelo `scripts/sign-sums.sh` (a CLI do minisign) com a chave do projeto.
        let sums = include_bytes!("../../tests/fixtures/release/SHA256SUMS");
        let signature = include_bytes!("../../tests/fixtures/release/SHA256SUMS.minisig");
        let key = gdl_common::links::RELEASE_PUBLIC_KEY;
        assert_eq!(
            expected_sha256(sums, signature, key, "pkg.rpm").unwrap(),
            sha256_hex(b"pacote\n")
        );
    }

    #[test]
    fn sha256_hex_matches_sha256sum() {
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn tampered_sums_is_bad_signature() {
        let sums = sums_of(&[(PKG, b"conteudo")]);
        let (key, sig) = signed(&sums);
        let mut tampered = sums.clone();
        tampered[0] ^= 1;
        assert_eq!(
            expected_sha256(&tampered, &sig, &key, PKG),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn other_key_or_no_key_is_bad_signature() {
        let sums = sums_of(&[(PKG, b"conteudo")]);
        let (_, sig) = signed(&sums);
        let (other, _) = signed(b"x");
        for key in [other.as_str(), "", "lixo"] {
            assert_eq!(
                expected_sha256(&sums, &sig, key, PKG),
                Err(VerifyError::BadSignature),
                "{key:?}"
            );
        }
        assert_eq!(
            expected_sha256(&sums, b"assinatura qualquer", &other, PKG),
            Err(VerifyError::BadSignature)
        );
    }

    #[test]
    fn missing_line_is_not_listed() {
        let sums = sums_of(&[("outro_1.4.0_amd64.deb", b"x")]);
        let (key, sig) = signed(&sums);
        assert_eq!(
            expected_sha256(&sums, &sig, &key, PKG),
            Err(VerifyError::NotListed)
        );
        // O nome é comparado inteiro: um que só termina igual não serve.
        let sums = sums_of(&[(&format!("x{PKG}"), b"x")]);
        let (key, sig) = signed(&sums);
        assert_eq!(
            expected_sha256(&sums, &sig, &key, PKG),
            Err(VerifyError::NotListed)
        );
    }

    /// O pacote `.deb` desta variante no release 1.4.0.
    fn deb() -> String {
        crate::upgrade::InstallKind::Deb
            .package_name((1, 4, 0))
            .unwrap()
    }

    /// O release 1.4.0 servido pelo Drive falso, os arquivos da oferta, a chave que assinou as
    /// somas e a pasta (ainda inexistente) para onde baixar.
    struct Served {
        fake: gdl_fake_drive::FakeDrive,
        files: ReleaseFiles,
        key: String,
        target: PathBuf,
        _dir: tempfile::TempDir,
    }

    /// `Served` com as somas de `listed` e o pacote `served`.
    async fn served(listed: &[u8], served: &[u8]) -> Served {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let sums = sums_of(&[(&deb(), listed)]);
        let (key, sig) = signed(&sums);
        fake.set_release(
            Some("v1.4.0"),
            vec![
                (deb(), served.to_vec()),
                (SUMS.into(), sums),
                (SIGNATURE.into(), sig),
            ],
        );
        let release = crate::updates::latest(&fake.releases_url())
            .await
            .unwrap()
            .unwrap();
        let files = crate::upgrade::offer(release, &crate::upgrade::InstallKind::Deb)
            .files
            .unwrap();
        let dir = tempfile::tempdir().unwrap();
        Served {
            fake,
            files,
            key,
            target: dir.path().join("atualizacao"),
            _dir: dir,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn download_returns_the_checked_package() {
        let Served {
            files, key, target, ..
        } = &served(b"pacote", b"pacote").await;

        let got = download(files, target, key).await.unwrap();

        assert_eq!(got.path, target.join(deb()));
        assert_eq!(got.sha256, sha256_hex(b"pacote"));
        assert_eq!(std::fs::read(&got.path).unwrap(), b"pacote");
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn download_checks_hash_and_cleans_up() {
        let s = served(b"pacote", b"<html>erro</html>").await;

        let got = download(&s.files, &s.target, &s.key).await;

        assert_eq!(got, Err(UpgradeError::Verify(VerifyError::Mismatch)));
        assert!(!s.target.exists());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn download_failure_is_network_and_cleans_up() {
        let s = served(b"pacote", b"pacote").await;
        s.fake.set_release(None, vec![]);

        let got = download(&s.files, &s.target, &s.key).await;

        assert_eq!(got, Err(UpgradeError::Network));
        assert!(!s.target.exists());
    }
}
