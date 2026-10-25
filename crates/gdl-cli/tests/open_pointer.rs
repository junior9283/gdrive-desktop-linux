//! `open-pointer`: o manipulador dos ponteiros `.gdoc`, `.gsheet`… abre o documento no navegador.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};

/// Roda `open-pointer` com um navegador falso no PATH, que só anota o endereço (nada abre na
/// tela). Devolve o resultado e o endereço aberto, se houve.
fn open_pointer(dir: &Path, pointer: &Path) -> (ExitStatus, Option<String>) {
    let opened: PathBuf = dir.join("aberto.txt");
    let browser = dir.join("xdg-open");
    std::fs::write(
        &browser,
        format!("#!/bin/sh\nprintf '%s' \"$1\" > '{}'\n", opened.display()),
    )
    .unwrap();
    std::fs::set_permissions(&browser, std::fs::Permissions::from_mode(0o755)).unwrap();
    let path = format!("{}:{}", dir.display(), std::env::var("PATH").unwrap());
    let status = Command::new(env!("CARGO_BIN_EXE_gdrive-desktop-linux"))
        .arg("open-pointer")
        .arg(pointer)
        .env("PATH", path)
        .env("XDG_CONFIG_HOME", dir.join("config"))
        .status()
        .unwrap();
    (status, std::fs::read_to_string(opened).ok())
}

#[test]
fn cli_open_pointer_calls_browser_with_url() {
    let dir = tempfile::tempdir().unwrap();
    let pointer = dir.path().join("Plano.gdoc");
    std::fs::write(
        &pointer,
        gdl_engine::gdocs::pointer_json("abc123", "application/vnd.google-apps.document", "a@b.c"),
    )
    .unwrap();

    let (status, opened) = open_pointer(dir.path(), &pointer);

    assert!(status.success());
    assert_eq!(
        opened.as_deref(),
        Some("https://docs.google.com/open?id=abc123")
    );
}

#[test]
fn cli_open_pointer_only_opens_google_documents() {
    let dir = tempfile::tempdir().unwrap();
    let pointer = dir.path().join("nota.gdoc");
    std::fs::write(&pointer, br#"{"url": "file:///etc/passwd"}"#).unwrap();

    let (status, opened) = open_pointer(dir.path(), &pointer);

    assert!(!status.success());
    assert_eq!(opened, None);
}
