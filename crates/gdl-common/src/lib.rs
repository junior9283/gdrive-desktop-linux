//! Utilidades comuns: nomes, caminhos, preferências e idiomas.

pub mod browser;
pub mod config;
pub mod crash;
pub mod fs;
pub mod gdocs;
pub mod i18n;
pub mod links;
pub mod names;
pub mod paths;
pub mod temp_files;
pub mod time;

/// O nome do sistema (`PRETTY_NAME` do `os-release`), vazio se não houver; lido uma vez.
pub fn os_name() -> &'static str {
    static NAME: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    NAME.get_or_init(|| {
        std::fs::read_to_string("/etc/os-release")
            .unwrap_or_default()
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME="))
            .map(|v| v.trim_matches('"').to_owned())
            .unwrap_or_default()
    })
}

/// O id do app: o nome do `.desktop`, do ícone e da classe das janelas.
pub const APP_ID: &str = "io.github.junior9283.GDriveDesktopLinux";
