//! Diretórios do app (padrão XDG).

use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

const APP: &str = "gdrive-desktop-linux";

/// Cria a pasta (e as que faltam no caminho) só para o dono e restringe a que já existia: o
/// conteúdo do Drive e o banco não ficam legíveis por outros usuários.
pub fn create_private_dir(path: &Path) -> std::io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

/// Onde o app guarda configuração, dados, estado e onde lê as políticas do sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// As bases XDG (`~/.config`, `~/.local/share`), para o que fica fora da pasta do app:
    /// `autostart` e os Locais do KDE.
    pub config_home: PathBuf,
    pub data_home: PathBuf,
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub state_dir: PathBuf,
    pub etc_dir: PathBuf,
}

impl Paths {
    /// Usa `XDG_CONFIG_HOME`, `XDG_DATA_HOME` e `XDG_STATE_HOME`, com os padrões do XDG
    /// relativos ao `HOME`, e `/etc/gdrive-desktop-linux` para as políticas.
    pub fn from_env() -> Paths {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| "/".into());
        let xdg = |var: &str, default: &str| {
            std::env::var_os(var)
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .unwrap_or_else(|| home.join(default))
        };
        Paths::under(
            &xdg("XDG_CONFIG_HOME", ".config"),
            &xdg("XDG_DATA_HOME", ".local/share"),
            &xdg("XDG_STATE_HOME", ".local/state"),
            Path::new("/etc"),
        )
    }

    /// Todos os diretórios dentro de `root`, para testes.
    pub fn for_test(root: &Path) -> Paths {
        Paths::under(
            &root.join("config"),
            &root.join("data"),
            &root.join("state"),
            &root.join("etc"),
        )
    }

    /// A subpasta do app dentro de cada diretório-base.
    fn under(config: &Path, data: &Path, state: &Path, etc: &Path) -> Paths {
        Paths {
            config_home: config.into(),
            data_home: data.into(),
            config_dir: config.join(APP),
            data_dir: data.join(APP),
            state_dir: state.join(APP),
            etc_dir: etc.join(APP),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn for_test_puts_everything_under_root() {
        let p = Paths::for_test(Path::new("/tmp/x"));
        assert_eq!(
            p.config_dir,
            PathBuf::from("/tmp/x/config/gdrive-desktop-linux")
        );
        assert_eq!(
            p.data_dir,
            PathBuf::from("/tmp/x/data/gdrive-desktop-linux")
        );
        assert_eq!(
            p.state_dir,
            PathBuf::from("/tmp/x/state/gdrive-desktop-linux")
        );
        assert_eq!(p.etc_dir, PathBuf::from("/tmp/x/etc/gdrive-desktop-linux"));
    }
}
