//! A instalação do pacote conferido: o gerenciador de pacotes de cada formato, com a senha
//! pedida pelo polkit (`pkexec`), ou o `install.sh` do `.tar.gz`, sem root.

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
    ) -> Result<InstallOutcome, UpgradeError>;
}

/// A linha de comando que instala `package` (caminho absoluto, já conferido) numa instalação
/// `kind`; `has(programa)` diz se o programa está no `PATH`. No `.tar.gz`, `package` já foi
/// extraído ao lado dele. `None`: não há como instalar sozinho.
pub fn command_for(
    kind: &InstallKind,
    package: &Path,
    has: impl Fn(&str) -> bool,
) -> Option<Vec<OsString>> {
    let path = package.as_os_str().to_owned();
    let pkexec = |words: &[&str]| {
        let mut command: Vec<OsString> =
            ["pkexec"].iter().chain(words).map(OsString::from).collect();
        command.push(path.clone());
        command
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

/// O resultado de um instalador que saiu com `code` e escreveu `stderr`.
pub fn outcome(code: Option<i32>, stderr: &str) -> Result<InstallOutcome, UpgradeError> {
    let tail = || {
        let lines: Vec<&str> = stderr.lines().collect();
        lines[lines.len().saturating_sub(TAIL_LINES)..].join("\n")
    };
    match code {
        Some(0) => Ok(InstallOutcome::Done),
        // O pkexec: a janela de senha foi fechada.
        Some(126) => Ok(InstallOutcome::Canceled),
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
    ) -> Result<InstallOutcome, UpgradeError> {
        let failed = |e: std::io::Error| UpgradeError::Install(e.to_string());
        if matches!(kind, InstallKind::Tar { .. }) {
            let tarball = package.to_path_buf();
            tokio::task::spawn_blocking(move || unpack(&tarball))
                .await
                .map_err(|e| UpgradeError::Install(e.to_string()))?
                .map_err(failed)?;
        }
        let command = command_for(kind, package, in_path)
            .ok_or_else(|| UpgradeError::Install("sem instalador para este formato".into()))?;
        let output = tokio::process::Command::new(&command[0])
            .args(&command[1..])
            .stdout(std::process::Stdio::null())
            .output()
            .await
            .map_err(failed)?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::debug!(saida = %stderr, "instalador da atualização");
        outcome(output.status.code(), &stderr)
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

    fn cmd(kind: &InstallKind, package: &str, has: &[&str]) -> Option<Vec<String>> {
        words(command_for(kind, Path::new(package), |p| has.contains(&p)))
    }

    #[test]
    fn package_formats_use_their_manager_through_pkexec() {
        assert_eq!(
            cmd(&InstallKind::Deb, "/c/a.deb", &[]).unwrap(),
            ["pkexec", "apt-get", "install", "-y", "/c/a.deb"]
        );
        assert_eq!(
            cmd(&InstallKind::Rpm, "/c/a.rpm", &["dnf", "zypper"]).unwrap(),
            ["pkexec", "dnf", "install", "-y", "/c/a.rpm"]
        );
        assert_eq!(
            cmd(&InstallKind::Rpm, "/c/a.rpm", &["zypper"]).unwrap(),
            [
                "pkexec",
                "zypper",
                "--non-interactive",
                "install",
                "--allow-unsigned-rpm",
                "/c/a.rpm"
            ]
        );
        assert_eq!(cmd(&InstallKind::Rpm, "/c/a.rpm", &[]), None);
        assert_eq!(
            cmd(&InstallKind::Arch, "/c/a.pkg.tar.zst", &[]).unwrap(),
            ["pkexec", "pacman", "-U", "--noconfirm", "/c/a.pkg.tar.zst"]
        );
        assert_eq!(cmd(&InstallKind::Source, "/c/a.deb", &[]), None);
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
        assert_eq!(outcome(Some(0), ""), Ok(InstallOutcome::Done));
        assert_eq!(outcome(Some(126), ""), Ok(InstallOutcome::Canceled));
        // 127: sem autorização ou sem agente do polkit (decisão da Tarefa 0): falha.
        assert!(matches!(
            outcome(Some(127), "Error executing command as another user"),
            Err(UpgradeError::Install(m)) if m.contains("another user")
        ));
        let long: String = (1..=30).map(|i| format!("linha {i}\n")).collect();
        let Err(UpgradeError::Install(message)) = outcome(Some(100), &long) else {
            panic!("falha esperada");
        };
        assert!(
            message.contains("linha 30") && !message.contains("linha 1\n"),
            "{message}"
        );
        assert!(matches!(outcome(None, ""), Err(UpgradeError::Install(_))));
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
            .install(&tar, &tarball(dir.path(), code))
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
