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

/// O id do app: o nome do `.desktop`, do ícone e da classe das janelas.
pub const APP_ID: &str = "io.github.junior9283.GDriveDesktopLinux";
