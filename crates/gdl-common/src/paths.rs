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

/// A pasta deste executável (os outros programas do app ficam ao lado).
pub fn exe_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

/// Base XDG: o valor da variável só vale se for caminho absoluto; senão, `<home>/<padrão>`.
fn xdg_base(value: Option<std::ffi::OsString>, home: &Path, default: &str) -> PathBuf {
    value
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(default))
}

/// Onde o app guarda configuração, dados, estado e onde lê as políticas do sistema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Paths {
    /// As bases XDG (`~/.config`, `~/.local/share`), para o que fica fora da pasta do app:
    /// `autostart` e os Locais do KDE.
    pub config_home: PathBuf,
    pub data_home: PathBuf,
    /// Cache de miniaturas do freedesktop (`$XDG_CACHE_HOME/thumbnails`).
    pub thumbnails_dir: PathBuf,
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
        let xdg = |var: &str, default: &str| xdg_base(std::env::var_os(var), &home, default);
        Paths::under(
            &xdg("XDG_CONFIG_HOME", ".config"),
            &xdg("XDG_DATA_HOME", ".local/share"),
            &xdg("XDG_STATE_HOME", ".local/state"),
            &xdg("XDG_CACHE_HOME", ".cache"),
            Path::new("/etc"),
        )
    }

    /// Todos os diretórios dentro de `root`, para testes.
    pub fn for_test(root: &Path) -> Paths {
        Paths::under(
            &root.join("config"),
            &root.join("data"),
            &root.join("state"),
            &root.join("cache"),
            &root.join("etc"),
        )
    }

    /// A subpasta do app dentro de cada diretório-base.
    fn under(config: &Path, data: &Path, state: &Path, cache: &Path, etc: &Path) -> Paths {
        Paths {
            config_home: config.into(),
            data_home: data.into(),
            thumbnails_dir: cache.join("thumbnails"),
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
        assert_eq!(p.thumbnails_dir, PathBuf::from("/tmp/x/cache/thumbnails"));
    }

    #[test]
    fn relative_xdg_cache_home_is_ignored() {
        let home = Path::new("/home/fulano");
        let base = |v: &str| xdg_base(Some(v.into()), home, ".cache");
        assert_eq!(base("cache-relativo"), PathBuf::from("/home/fulano/.cache"));
        assert_eq!(base("/c"), PathBuf::from("/c"));
        assert_eq!(
            xdg_base(None, home, ".cache"),
            PathBuf::from("/home/fulano/.cache")
        );
    }
}
