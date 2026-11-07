//! A instalação do pacote conferido: o gerenciador de pacotes de cada formato, com a senha
//! pedida pelo polkit (`pkexec`), ou o `install.sh` do `.tar.gz`, sem root.
//!
//! O pacote baixado fica numa pasta do usuário, que outro programa dele poderia trocar entre a
//! conferência e a instalação. Por isso o passo como root (`ROOT_SCRIPT`) copia o pacote para
//! uma pasta só dele, confere de novo a soma da cópia e instala a cópia.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use super::kind::TAR_SUFFIX;
use super::{InstallKind, UpgradeError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstallOutcome {
    Done,
    /// A pessoa fechou a janela de senha: não é erro.
    Canceled,
}

#[async_trait::async_trait]
pub trait Installer: Send + Sync {
    async fn install(
        &self,
        kind: &InstallKind,
        package: &Path,
        sha256: &str,
    ) -> Result<InstallOutcome, UpgradeError>;
}

/// O passo como root: `sh -c ROOT_SCRIPT sh <pacote> <sha256> <gerenciador e argumentos>`.
/// Copia o pacote para uma pasta temporária do root, confere a soma da cópia e roda o
/// gerenciador com ela no fim; a pasta some na saída. Não sai com 126 nem 127, os códigos do
/// `pkexec` sem autorização (ver `outcome`).
pub const ROOT_SCRIPT: &str = r#"set -eu
src=$1 sum=$2
shift 2
dir=$(mktemp -d)
trap 'rm -rf "$dir"' EXIT
file="$dir/${src##*/}"
cp -- "$src" "$file"
echo "$sum  $file" | sha256sum -c --status -
"$@" "$file" || { c=$?; case $c in 126|127) exit 125 ;; esac; exit "$c"; }
"#;

/// A linha de comando que instala `package` (caminho absoluto, já conferido, com a soma
/// `sha256`) numa instalação `kind`; `has(programa)` diz se o programa está no `PATH`. No
/// `.tar.gz` (sem root), `package` já foi extraído ao lado dele. `None`: não há como instalar
/// sozinho.
pub fn command_for(
    kind: &InstallKind,
    package: &Path,
    sha256: &str,
    has: impl Fn(&str) -> bool,
) -> Option<Vec<OsString>> {
    let pkexec = |manager: &[&str]| {
        let head = ["pkexec", "sh", "-c", ROOT_SCRIPT, "sh"].map(OsString::from);
        let args = [package.as_os_str().to_owned(), sha256.into()];
        let manager = manager.iter().map(OsString::from);
        head.into_iter().chain(args).chain(manager).collect()
    };
    Some(match kind {
        InstallKind::Deb => pkexec(&["apt-get", "install", "-y"]),
        InstallKind::Rpm if has("dnf") => pkexec(&["dnf", "install", "-y"]),
        InstallKind::Rpm if has("zypper") => pkexec(&[
            "zypper",
            "--non-interactive",
            "install",
            "--allow-unsigned-rpm",
        ]),
        InstallKind::Arch => pkexec(&["pacman", "-U", "--noconfirm"]),
        InstallKind::Tar { prefix } => {
            let script = unpacked_dir(package)?.join("install.sh");
            vec!["sh".into(), script.into(), "--prefix".into(), prefix.into()]
        }
        InstallKind::Rpm | InstallKind::Source => return None,
    })
}

/// A pasta `<nome>-<versão>` que o `.tar.gz` `<nome>-<versão><TAR_SUFFIX>` cria ao lado dele.
fn unpacked_dir(tarball: &Path) -> Option<PathBuf> {
    let name = tarball.file_name()?.to_str()?.strip_suffix(TAR_SUFFIX)?;
    Some(tarball.with_file_name(name))
}

/// Extrai o `.tar.gz` na pasta dele (onde `unpacked_dir` procura).
fn unpack(tarball: &Path) -> std::io::Result<()> {
    let file = std::fs::File::open(tarball)?;
    let dir = tarball.parent().unwrap_or(Path::new("."));
    tar::Archive::new(flate2::read::GzDecoder::new(file)).unpack(dir)
}

/// O resultado de um instalador que saiu com `code` e escreveu `stderr`; `pkexec`: ele rodou
/// pelo `pkexec`.
pub fn outcome(
    code: Option<i32>,
    stderr: &str,
    pkexec: bool,
) -> Result<InstallOutcome, UpgradeError> {
    let tail = || {
        let lines: Vec<&str> = stderr.lines().collect();
        lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
    };
    match code {
        Some(0) => Ok(InstallOutcome::Done),
        // A janela de senha fechada: 126, ou 127 "Not authorized" (o agente do Plasma). O 127
        // sem agente do polkit continua falha.
        Some(126) if pkexec => Ok(InstallOutcome::Canceled),
        Some(127) if pkexec && stderr.contains("Not authorized") => Ok(InstallOutcome::Canceled),
        Some(code) => Err(UpgradeError::Install(format!(
            "saiu com {code}: {}",
            tail()
        ))),
        None => Err(UpgradeError::Install(format!("interrompido: {}", tail()))),
    }
}

/// Quantas linhas do fim da saída de erro vão para o motivo da falha.
const TAIL_LINES: usize = 10;

/// O instalador de verdade: extrai o `.tar.gz` (se for o caso) e roda `command_for`.
pub struct SystemInstaller;

#[async_trait::async_trait]
impl Installer for SystemInstaller {
    async fn install(
        &self,
        kind: &InstallKind,
        package: &Path,
        sha256: &str,
    ) -> Result<InstallOutcome, UpgradeError> {
        let failed = |e: std::io::Error| UpgradeError::Install(e.to_string());
        if matches!(kind, InstallKind::Tar { .. }) {
            let tarball = package.to_path_buf();
            tokio::task::spawn_blocking(move || unpack(&tarball))
                .await
                .map_err(|e| UpgradeError::Install(e.to_string()))?
                .map_err(failed)?;
        }
        let command = command_for(kind, package, sha256, in_path)
            .ok_or_else(|| UpgradeError::Install("sem instalador para este formato".into()))?;
        let mut installer = tokio::process::Command::new(&command[0]);
        installer
            .args(&command[1..])
            .stdout(std::process::Stdio::null());
        if kind.needs_root() {
            // As mensagens do pkexec em inglês: `outcome` procura "Not authorized".
            installer.env("LC_ALL", "C");
        }
        let output = installer.output().await.map_err(failed)?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::debug!(saida = %stderr, "instalador da atualização");
        outcome(output.status.code(), &stderr, kind.needs_root())
    }
}

/// Se `program` está numa pasta do `PATH`.
fn in_path(program: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(command: Option<Vec<OsString>>) -> Option<Vec<String>> {
        command.map(|c| c.into_iter().map(|w| w.into_string().unwrap()).collect())
    }

    const SUM: &str = "abc123";

    fn cmd(kind: &InstallKind, package: &str, has: &[&str]) -> Option<Vec<String>> {
        words(command_for(kind, Path::new(package), SUM, |p| {
            has.contains(&p)
        }))
    }

    /// O comando como root: o `ROOT_SCRIPT` com o pacote, a soma e o gerenciador.
    fn as_root(package: &str, manager: &[&str]) -> Vec<String> {
        ["pkexec", "sh", "-c", ROOT_SCRIPT, "sh", package, SUM]
            .iter()
            .chain(manager)
            .map(|w| w.to_string())
            .collect()
    }

    #[test]
    fn package_formats_use_their_manager_through_pkexec() {
        assert_eq!(
            cmd(&InstallKind::Deb, "/c/a.deb", &[]).unwrap(),
            as_root("/c/a.deb", &["apt-get", "install", "-y"])
        );
        assert_eq!(
            cmd(&InstallKind::Rpm, "/c/a.rpm", &["dnf", "zypper"]).unwrap(),
            as_root("/c/a.rpm", &["dnf", "install", "-y"])
        );
        assert_eq!(
            cmd(&InstallKind::Rpm, "/c/a.rpm", &["zypper"]).unwrap(),
            as_root(
                "/c/a.rpm",
                &[
                    "zypper",
                    "--non-interactive",
                    "install",
                    "--allow-unsigned-rpm"
                ]
            )
        );
        assert_eq!(cmd(&InstallKind::Rpm, "/c/a.rpm", &[]), None);
        assert_eq!(
            cmd(&InstallKind::Arch, "/c/a.pkg.tar.zst", &[]).unwrap(),
            as_root("/c/a.pkg.tar.zst", &["pacman", "-U", "--noconfirm"])
        );
        assert_eq!(cmd(&InstallKind::Source, "/c/a.deb", &[]), None);
    }

    /// O `ROOT_SCRIPT` rodado sem root, com um "gerenciador" que grava o que recebeu: a soma
    /// `sum` do pacote com conteúdo `body`. Devolve a saída e o que o gerenciador recebeu.
    fn run_root_script(body: &[u8], sum: &str) -> (std::process::ExitStatus, Option<String>) {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("app_1.4.0_amd64.deb");
        std::fs::write(&package, body).unwrap();
        let log = dir.path().join("recebido");
        let manager = format!("{{ cat \"$1\"; echo; echo \"$1\"; }} > '{}'", log.display());
        let status = std::process::Command::new("sh")
            .args(["-c", ROOT_SCRIPT, "sh"])
            .arg(&package)
            .args([sum, "sh", "-c", &manager, "gerenciador"])
            .status()
            .unwrap();
        (status, std::fs::read_to_string(log).ok())
    }

    /// O pacote que o root instala é uma cópia dele, conferida de novo: trocar o arquivo da
    /// pasta do usuário depois do download não passa.
    #[test]
    fn root_step_installs_a_rechecked_copy() {
        let sum = crate::upgrade::verify::sha256_hex(b"pacote");

        let (status, got) = run_root_script(b"pacote", &sum);

        assert!(status.success(), "{status}");
        let got = got.unwrap();
        let (body, path) = got.trim_end().split_once('\n').unwrap();
        assert_eq!(body, "pacote");
        assert!(path.ends_with("/app_1.4.0_amd64.deb"), "{path}");
        assert!(!Path::new(path).exists(), "a cópia some no fim");

        let (status, got) = run_root_script(b"trocado", &sum);
        assert!(!status.success());
        assert_eq!(got, None, "o gerenciador não roda");
    }

    /// 126 e 127 só podem vir do próprio `pkexec` (a senha cancelada): os do gerenciador viram
    /// 125, e os outros passam como estão.
    #[test]
    fn root_script_never_exits_like_pkexec() {
        let dir = tempfile::tempdir().unwrap();
        let package = dir.path().join("app.deb");
        std::fs::write(&package, b"pacote").unwrap();
        let sum = crate::upgrade::verify::sha256_hex(b"pacote");
        for (manager, want) in [(126, 125), (127, 125), (100, 100)] {
            let status = std::process::Command::new("sh")
                .args(["-c", ROOT_SCRIPT, "sh"])
                .arg(&package)
                .args([&sum, "sh", "-c", &format!("exit {manager}")])
                .status()
                .unwrap();
            assert_eq!(status.code(), Some(want), "gerenciador saiu com {manager}");
        }
    }

    #[test]
    fn tarball_runs_its_install_sh_in_the_same_prefix() {
        let tar = InstallKind::Tar {
            prefix: "/home/x/.local".into(),
        };
        assert_eq!(
            cmd(&tar, "/c/gerasync-1.4.0-x86_64.tar.gz", &[]).unwrap(),
            [
                "sh",
                "/c/gerasync-1.4.0/install.sh",
                "--prefix",
                "/home/x/.local"
            ]
        );
        assert_eq!(cmd(&tar, "/c/outro.zip", &[]), None);
    }

    #[test]
    fn pkexec_cancel_is_not_an_error() {
        let pkexec = |code, stderr| outcome(Some(code), stderr, true);
        assert_eq!(pkexec(0, ""), Ok(InstallOutcome::Done));
        assert_eq!(pkexec(126, ""), Ok(InstallOutcome::Canceled));
        // A janela de senha cancelada no Plasma (polkit-kde-agent), visto no teste manual.
        let cancel = "Error executing command as another user: Not authorized\n\n\
                      This incident has been reported.\n";
        assert_eq!(pkexec(127, cancel), Ok(InstallOutcome::Canceled));
        // 127 sem agente do polkit (decisão da Tarefa 0): falha.
        let no_agent = "Error executing command as another user: No authentication agent found.";
        assert!(matches!(
            pkexec(127, no_agent),
            Err(UpgradeError::Install(m)) if m.contains("agent")
        ));
        // Sem o pkexec (o install.sh do .tar.gz), 126 e 127 são falhas do instalador.
        assert!(outcome(Some(126), "", false).is_err());
        assert!(outcome(Some(127), cancel, false).is_err());
        let long: String = (1..=30).map(|i| format!("linha {i}\n")).collect();
        let Err(UpgradeError::Install(message)) = pkexec(100, &long) else {
            panic!("falha esperada");
        };
        assert!(
            message.contains("linha 30") && !message.contains("linha 1\n"),
            "{message}"
        );
        assert!(matches!(
            outcome(None, "", true),
            Err(UpgradeError::Install(_))
        ));
    }

    /// Um `.tar.gz` como o do release, com um `install.sh` que grava os argumentos em `args` e
    /// sai com `code`.
    fn tarball(dir: &Path, code: i32) -> PathBuf {
        let root = dir.join("app-1.4.0");
        std::fs::create_dir_all(&root).unwrap();
        let log = dir.join("args");
        let script = format!(
            "echo \"$@\" > '{}'\necho falhou >&2\nexit {code}\n",
            log.display()
        );
        std::fs::write(root.join("install.sh"), script).unwrap();
        let tarball = dir.join("app-1.4.0-x86_64.tar.gz");
        let packed = std::process::Command::new("tar")
            .arg("-czf")
            .arg(&tarball)
            .arg("-C")
            .arg(dir)
            .arg("app-1.4.0")
            .status()
            .unwrap();
        assert!(packed.success());
        std::fs::remove_dir_all(root).unwrap();
        tarball
    }

    /// O `SystemInstaller` no `.tar.gz` de `tarball(code)`, com prefixo `/pfx`: a pasta (com o
    /// `args` que o `install.sh` gravou) e o resultado.
    async fn run_tarball(code: i32) -> (tempfile::TempDir, Result<InstallOutcome, UpgradeError>) {
        let dir = tempfile::tempdir().unwrap();
        let tar = InstallKind::Tar {
            prefix: "/pfx".into(),
        };
        let got = SystemInstaller
            .install(&tar, &tarball(dir.path(), code), "")
            .await;
        (dir, got)
    }

    #[tokio::test]
    async fn system_installer_unpacks_and_runs_install_sh() {
        let (dir, done) = run_tarball(0).await;

        assert_eq!(done, Ok(InstallOutcome::Done));
        let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert_eq!(args.trim(), "--prefix /pfx");
    }

    #[tokio::test]
    async fn system_installer_reports_the_failure() {
        let (_dir, failed) = run_tarball(3).await;

        assert!(
            matches!(&failed, Err(UpgradeError::Install(m)) if m.contains("falhou")),
            "{failed:?}"
        );
    }
}
