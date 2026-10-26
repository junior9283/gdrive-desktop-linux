//! A linha de comando do serviço: `--version` e `--help` só respondem, sem subir o serviço.

use std::path::Path;
use std::process::{Command, Output};

/// Roda o serviço com `arg` numa casa vazia e sem barramento: se ele tentasse subir, falharia
/// aqui em vez de usar o D-Bus da sessão de quem roda os testes.
fn run(arg: &str, home: &Path) -> Output {
    let no_bus = "unix:path=/nonexistent/gdrive-desktop-linux-test";
    Command::new(env!("CARGO_BIN_EXE_gdrive-desktop-linuxd"))
        .arg(arg)
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env("XDG_STATE_HOME", home.join(".local/state"))
        .env("XDG_CACHE_HOME", home.join(".cache"))
        .env("DBUS_SESSION_BUS_ADDRESS", no_bus)
        .env("DBUS_SYSTEM_BUS_ADDRESS", no_bus)
        .output()
        .unwrap()
}

fn untouched(home: &Path) -> bool {
    std::fs::read_dir(home).unwrap().next().is_none()
}

/// Bug: `--version` subia uma segunda instância, que montava "~/Google Drive (1)".
#[test]
fn version_prints_the_version_and_exits() {
    let home = tempfile::tempdir().unwrap();
    let out = run("--version", home.path());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        stdout.trim(),
        format!("gdrive-desktop-linuxd {}", env!("CARGO_PKG_VERSION"))
    );
    assert!(untouched(home.path()), "não cria nada");
}

#[test]
fn help_lists_the_options_and_exits() {
    let home = tempfile::tempdir().unwrap();
    for arg in ["--help", "-h"] {
        let out = run(arg, home.path());
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(out.status.success(), "{arg}");
        assert!(
            stdout.contains("--version") && stdout.contains("--help"),
            "{stdout}"
        );
    }
    assert!(untouched(home.path()));
}

#[test]
fn unknown_option_fails_without_starting() {
    let home = tempfile::tempdir().unwrap();
    let out = run("--montar", home.path());
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("--montar"));
    assert!(untouched(home.path()));
}
