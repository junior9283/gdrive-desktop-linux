//! O pacote de informações de diagnóstico: textos (logs, sistema, preferências, contas) num
//! `.tar.gz`, cada um passado pelo `scrub` antes de entrar. Nunca entram tokens, segredos,
//! e-mails inteiros nem conteúdo de arquivos.

use std::borrow::Cow;
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

/// Cada padrão e o que fica no lugar.
static SECRETS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    [
        (
            r#"("(?:access_token|refresh_token|id_token|client_secret|code)"\s*:\s*")[^"]*""#,
            r#"$1***""#,
        ),
        (
            r"\b(access_token|refresh_token|id_token|client_secret|code)=[^&\s\x22]+",
            "$1=***",
        ),
        (r"(?i)\bBearer\s+[A-Za-z0-9._~+/=-]+", "Bearer ***"),
        (r"\bya29\.[A-Za-z0-9._-]+", "***"),
        (r"\b1//[A-Za-z0-9._-]+", "***"),
        // Usuário e senha dentro de um endereço (proxy).
        (r"(://)[^/@\s:]+:[^/@\s]+@", "$1***@"),
        (
            r"\b([A-Za-z0-9._%+-])[A-Za-z0-9._%+-]*@([A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,})",
            "$1***@$2",
        ),
    ]
    .into_iter()
    .map(|(pattern, with)| (Regex::new(pattern).expect("padrão fixo"), with))
    .collect()
});

/// O texto sem tokens, segredos e e-mails inteiros (`j***@gmail.com`).
pub fn scrub(text: &str) -> String {
    SECRETS
        .iter()
        .fold(Cow::Borrowed(text), |text, (pattern, with)| {
            match pattern.replace_all(&text, *with) {
                Cow::Borrowed(_) => text,
                Cow::Owned(changed) => Cow::Owned(changed),
            }
        })
        .into_owned()
}

/// O conteúdo de um arquivo do pacote: um texto pronto ou um arquivo lido só na gravação.
enum Source {
    Text(String),
    File(PathBuf),
}

/// Os arquivos do pacote; cada um passa pelo `scrub` ao ser gravado.
#[derive(Default)]
pub struct Package {
    files: Vec<(String, Source)>,
}

impl Package {
    pub fn add(&mut self, name: &str, text: String) {
        self.files.push((name.to_owned(), Source::Text(text)));
    }

    /// Cada arquivo de `dir` (sem subpastas) em `<prefix>/<nome>`, lido como texto.
    pub fn add_dir(&mut self, prefix: &str, dir: &Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|e| e.path()).collect();
        paths.sort();
        for path in paths.into_iter().filter(|p| p.is_file()) {
            let name = format!("{prefix}/{}", crate::file_name(&path));
            self.files.push((name, Source::File(path)));
        }
    }

    /// Grava o `.tar.gz` em `file`, que passa a ser só do usuário.
    pub fn write(&self, file: File) -> io::Result<()> {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        let gz = flate2::write::GzEncoder::new(file, flate2::Compression::default());
        let mut tar = tar::Builder::new(gz);
        let mtime = gdl_common::time::now_ms() as u64 / 1000;
        for (name, source) in &self.files {
            // Um arquivo que sumiu (o log girou) fica de fora.
            let text = match source {
                Source::Text(text) => scrub(text),
                Source::File(path) => match std::fs::read(path) {
                    Ok(bytes) => scrub(&String::from_utf8_lossy(&bytes)),
                    Err(_) => continue,
                },
            };
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o600);
            header.set_mtime(mtime);
            tar.append_data(&mut header, name, text.as_bytes())?;
        }
        tar.into_inner()?.finish()?.sync_all()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostics_scrub_tokens_and_emails() {
        let text = concat!(
            "conta juniorgeraldo@gmail.com conectada\n",
            "Authorization: Bearer ya29.a0AfB_byC-123.xyz\n",
            "{\"access_token\": \"ya29.oculto\", \"refresh_token\":\"1//0gAbC-def\", \"expires_in\": 3599}\n",
            "POST /token code=4/0Ab_xyz&client_secret=GOCSPX-abc&grant_type=authorization_code\n",
            "token solto 1//0eXyZ-123 e ya29.outro\n",
            "proxy socks5://usuario:senha@10.0.0.1:1080\n",
            "pasta /home/geraldo/Documentos sem segredo\n",
        );

        let clean = scrub(text);

        for secret in [
            "juniorgeraldo",
            "ya29.",
            "1//",
            "oculto",
            "4/0Ab_xyz",
            "GOCSPX",
            "usuario:senha",
        ] {
            assert!(!clean.contains(secret), "{secret} ficou:\n{clean}");
        }
        assert!(clean.contains("j***@gmail.com"), "{clean}");
        assert!(
            clean.contains("\"expires_in\": 3599"),
            "o resto fica: {clean}"
        );
        assert!(clean.contains("grant_type=authorization_code"), "{clean}");
        assert!(clean.contains("socks5://***@10.0.0.1:1080"), "{clean}");
        assert!(
            clean.contains("/home/geraldo/Documentos sem segredo"),
            "{clean}"
        );
    }

    #[test]
    fn package_is_a_private_tar_gz_of_scrubbed_texts() {
        let tmp = tempfile::tempdir().unwrap();
        let logs = tmp.path().join("logs");
        std::fs::create_dir(&logs).unwrap();
        std::fs::write(logs.join("a.log"), "Bearer ya29.x de ana@exemplo.com").unwrap();
        let mut package = Package::default();
        package.add("sistema.txt", "Linux".into());
        package.add_dir("logs", &logs);
        let out = tmp.path().join("pacote.tar.gz");

        package.write(File::create(&out).unwrap()).unwrap();

        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&out).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
        let files = crate::testutil::read_package(&out);
        assert_eq!(
            files.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(),
            ["sistema.txt", "logs/a.log"]
        );
        assert_eq!(files[1].1, "Bearer *** de a***@exemplo.com");
    }
}
