//! A identidade do app: nome, IDs, pastas e endereços. A variante GeraSync (Client ID oficial,
//! verificada pelo Google) sai do mesmo código compilado com `--cfg gerasync`
//! (`scripts/cargo-gerasync.sh`); sem ele, a variante de sempre.

/// O mesmo repositório nas duas variantes (issues, releases e a busca por versão nova).
pub const PROJECT_URL: &str = "https://github.com/junior9283/gdrive-desktop-linux";

/// Define as constantes de uma variante. As do D-Bus e dos programas derivam dos literais
/// (`concat!` só aceita literais, e os atributos do zbus precisam deles).
macro_rules! brand {
    (
        official: $official:literal,
        app_id: $app_id:literal,
        package: $package:literal,
        name: $name:literal,
        path: $path:literal,
        site: $site:literal $(,)?
    ) => {
        /// Se o build traz o Client ID oficial (GeraSync) em vez de pedir o do usuário.
        pub const OFFICIAL: bool = $official;
        /// O id do app: o nome do `.desktop`, do ícone e da classe das janelas.
        pub const APP_ID: &str = $app_id;
        /// O `.desktop` principal (menu e início automático).
        pub const DESKTOP_FILE: &str = concat!($app_id, ".desktop");
        /// O nome do pacote, das pastas XDG, do chaveiro e do programa da linha de comando.
        pub const PACKAGE: &str = $package;
        pub const DAEMON: &str = concat!($package, "d");
        pub const TRAY_PROGRAM: &str = concat!($package, "-tray");
        pub const UI_PROGRAM: &str = concat!($package, "-ui");
        pub const MCP_PROGRAM: &str = concat!($package, "-mcp");
        /// O log do serviço, em `<state_dir>/logs`.
        pub const LOG_FILE: &str = concat!($package, "d.log");
        /// O nome que o usuário vê (menus, notificações, barra lateral).
        pub const DISPLAY_NAME: &str = $name;
        /// A pasta montada em `~`.
        pub const MOUNT_DIR: &str = $name;
        /// O nome do serviço no D-Bus: o id do app com a versão da interface.
        pub const BUS_NAME: &str = concat!($app_id, "1");
        pub const OBJECT_PATH: &str = $path;
        pub const UI_BUS_NAME: &str = concat!($app_id, "1.Ui");
        pub const UI_OBJECT_PATH: &str = concat!($path, "/Ui");
        pub const TRAY_BUS_NAME: &str = concat!($app_id, "1.Tray");
        /// A página do app (ajuda, privacidade e termos).
        pub const SITE_URL: &str = $site;
    };
}

#[cfg(not(gerasync))]
brand! {
    official: false,
    app_id: "io.github.junior9283.GDriveDesktopLinux",
    package: "gdrive-desktop-linux",
    name: "Google Drive",
    path: "/io/github/junior9283/GDriveDesktopLinux1",
    site: "https://junior9283.github.io/gdrive-desktop-linux/",
}

#[cfg(gerasync)]
brand! {
    official: true,
    app_id: "io.github.junior9283.GeraSync",
    package: "gerasync",
    name: "GeraSync",
    path: "/io/github/junior9283/GeraSync1",
    site: "https://gerasync.solusapp.net/",
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(gerasync))]
    #[test]
    fn default_values_match_v1_2_3() {
        const { assert!(!OFFICIAL) };
        assert_eq!(APP_ID, "io.github.junior9283.GDriveDesktopLinux");
        assert_eq!(PACKAGE, "gdrive-desktop-linux");
        assert_eq!(DAEMON, "gdrive-desktop-linuxd");
        assert_eq!(TRAY_PROGRAM, "gdrive-desktop-linux-tray");
        assert_eq!(UI_PROGRAM, "gdrive-desktop-linux-ui");
        assert_eq!(MCP_PROGRAM, "gdrive-desktop-linux-mcp");
        assert_eq!(DISPLAY_NAME, "Google Drive");
        assert_eq!(MOUNT_DIR, "Google Drive");
        assert_eq!(BUS_NAME, "io.github.junior9283.GDriveDesktopLinux1");
        assert_eq!(OBJECT_PATH, "/io/github/junior9283/GDriveDesktopLinux1");
        assert_eq!(UI_BUS_NAME, "io.github.junior9283.GDriveDesktopLinux1.Ui");
        assert_eq!(
            UI_OBJECT_PATH,
            "/io/github/junior9283/GDriveDesktopLinux1/Ui"
        );
        assert_eq!(
            TRAY_BUS_NAME,
            "io.github.junior9283.GDriveDesktopLinux1.Tray"
        );
        assert_eq!(
            SITE_URL,
            "https://junior9283.github.io/gdrive-desktop-linux/"
        );
    }

    #[cfg(gerasync)]
    #[test]
    fn gerasync_values() {
        const { assert!(OFFICIAL) };
        assert_eq!(APP_ID, "io.github.junior9283.GeraSync");
        assert_eq!(PACKAGE, "gerasync");
        assert_eq!(DAEMON, "gerasyncd");
        assert_eq!(TRAY_PROGRAM, "gerasync-tray");
        assert_eq!(UI_PROGRAM, "gerasync-ui");
        assert_eq!(MCP_PROGRAM, "gerasync-mcp");
        assert_eq!(DISPLAY_NAME, "GeraSync");
        assert_eq!(MOUNT_DIR, "GeraSync");
        assert_eq!(BUS_NAME, "io.github.junior9283.GeraSync1");
        assert_eq!(OBJECT_PATH, "/io/github/junior9283/GeraSync1");
        assert_eq!(UI_BUS_NAME, "io.github.junior9283.GeraSync1.Ui");
        assert_eq!(UI_OBJECT_PATH, "/io/github/junior9283/GeraSync1/Ui");
        assert_eq!(TRAY_BUS_NAME, "io.github.junior9283.GeraSync1.Tray");
        assert_eq!(SITE_URL, "https://gerasync.solusapp.net/");
        for value in [
            APP_ID,
            PACKAGE,
            DAEMON,
            TRAY_PROGRAM,
            UI_PROGRAM,
            MCP_PROGRAM,
            DISPLAY_NAME,
            MOUNT_DIR,
            BUS_NAME,
            OBJECT_PATH,
            UI_BUS_NAME,
            UI_OBJECT_PATH,
            TRAY_BUS_NAME,
            SITE_URL,
        ] {
            assert!(!value.to_lowercase().contains("gdrive"), "{value}");
        }
    }

    /// A janela do Tauri recebe a identidade pelo `TAURI_CONFIG` do script, que não lê esta
    /// constante: os dois precisam bater.
    #[cfg(gerasync)]
    #[test]
    fn cargo_script_gives_tauri_the_same_identity() {
        let script = include_str!("../../../scripts/cargo-gerasync.sh");
        let config = format!(r#"{{"identifier":"{APP_ID}","productName":"{PACKAGE}","#);
        assert!(script.contains(&config), "{config}");
        // O ícone da janela do GeraSync é o dele.
        assert!(script.contains(r#""bundle":{"icon":["icons/gerasync.png"]}"#));
    }
}
