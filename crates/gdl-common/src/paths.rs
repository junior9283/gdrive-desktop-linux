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

/// Os relatórios de falha dos programas (`<state_dir>/crashes`).
pub fn crashes_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("crashes")
}

/// Uma pasta do usuário do `user-dirs.dirs` (`kind` como `DOWNLOAD` ou `DESKTOP`), já com o
/// `$HOME` trocado; sem a linha, `<home>/<default>`.
pub fn user_dir(config_home: &Path, home: &Path, kind: &str, default: &str) -> PathBuf {
    let key = format!("XDG_{kind}_DIR=");
    std::fs::read_to_string(config_home.join("user-dirs.dirs"))
        .ok()
        .and_then(|text| {
            let line = text
                .lines()
                .find_map(|l| l.trim().strip_prefix(key.as_str()))?;
            let value = line.trim().trim_matches('"');
            match value.strip_prefix("$HOME") {
                Some(rest) => Some(home.join(rest.trim_start_matches('/'))),
                None => Some(PathBuf::from(value)).filter(|p| p.is_absolute()),
            }
        })
        .unwrap_or_else(|| home.join(default))
}

/// As pastas que o primeiro uso sugere sincronizar: Área de trabalho, Documentos, Imagens e Vídeos
/// do usuário, as que existem (sem a pasta pessoal inteira, quando alguma aponta para ela).
pub fn suggested_folders(config_home: &Path, home: &Path) -> Vec<PathBuf> {
    [
        ("DESKTOP", "Desktop"),
        ("DOCUMENTS", "Documents"),
        ("PICTURES", "Pictures"),
        ("VIDEOS", "Videos"),
    ]
    .into_iter()
    .map(|(kind, default)| user_dir(config_home, home, kind, default))
    .filter(|dir| dir != home && dir.is_dir())
    .collect()
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
    fn user_dir_reads_user_dirs_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let (config, home) = (tmp.path().join("config"), Path::new("/home/ana"));
        assert_eq!(
            user_dir(&config, home, "DOWNLOAD", "Downloads"),
            Path::new("/home/ana/Downloads"),
            "sem o arquivo, o padrão"
        );
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("user-dirs.dirs"),
            "# comentário\nXDG_DESKTOP_DIR=\"$HOME/Área de trabalho\"\nXDG_DOWNLOAD_DIR=\"/dados/baixados\"\n",
        )
        .unwrap();
        assert_eq!(
            user_dir(&config, home, "DESKTOP", "Desktop"),
            Path::new("/home/ana/Área de trabalho")
        );
        assert_eq!(
            user_dir(&config, home, "DOWNLOAD", "Downloads"),
            Path::new("/dados/baixados")
        );
    }

    #[test]
    fn suggested_folders_are_the_existing_user_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let (config, home) = (tmp.path().join("config"), tmp.path().join("ana"));
        std::fs::create_dir_all(&config).unwrap();
        std::fs::write(
            config.join("user-dirs.dirs"),
            "XDG_DESKTOP_DIR=\"$HOME/Área de trabalho\"\nXDG_PICTURES_DIR=\"$HOME\"\n",
        )
        .unwrap();
        for dir in ["Área de trabalho", "Documents", "Videos"] {
            std::fs::create_dir_all(home.join(dir)).unwrap();
        }

        assert_eq!(
            suggested_folders(&config, &home),
            [
                home.join("Área de trabalho"),
                home.join("Documents"),
                home.join("Videos")
            ],
            "a pasta pessoal inteira (Imagens apontando para ela) e as que não existem ficam de fora"
        );
    }

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
