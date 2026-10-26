//! `menu-action`: as ações do menu do botão direito do COSMIC Files, que roda um comando sem
//! terminal. O resultado aparece como na área de trabalho: o link vai para a área de
//! transferência e os erros viram notificação.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::Write;
use std::process::{Command, Stdio};

/// Os programas que copiam a entrada para a área de transferência, do Wayland ao X11.
const CLIPBOARD: [&[&str]; 3] = [
    &["wl-copy"],
    &["xclip", "-selection", "clipboard"],
    &["xsel", "--clipboard", "--input"],
];

/// Copia `text` com o primeiro programa de `CLIPBOARD` encontrado em `path`; `false` sem nenhum.
pub fn copy(text: &str, path: &OsStr) -> bool {
    CLIPBOARD.iter().any(|argv| {
        let Ok(mut child) = Command::new(argv[0])
            .args(&argv[1..])
            .env("PATH", path)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        else {
            return false;
        };
        // O wl-copy fica rodando para servir a área de transferência: não se espera por ele.
        child
            .stdin
            .take()
            .is_some_and(|mut stdin| stdin.write_all(text.as_bytes()).is_ok())
    })
}

/// Uma notificação da área de trabalho (melhor esforço: sem servidor, nada aparece).
pub async fn notify(conn: &zbus::Connection, summary: &str, body: &str) {
    let hints: HashMap<&str, zbus::zvariant::Value> = HashMap::new();
    let _ = conn
        .call_method(
            Some("org.freedesktop.Notifications"),
            "/org/freedesktop/Notifications",
            Some("org.freedesktop.Notifications"),
            "Notify",
            &(
                "Google Drive",
                0u32,
                gdl_common::APP_ID,
                summary,
                body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_with_the_first_tool_found() {
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("copiado");
        let tool = dir.path().join("xclip");
        std::fs::write(
            &tool,
            format!("#!/bin/sh\n/bin/cat > '{}'\n", out.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(copy("https://exemplo", dir.path().as_os_str()));
        for _ in 0..100 {
            if std::fs::read_to_string(&out).is_ok_and(|s| s == "https://exemplo") {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("o xclip falso não recebeu o link");
    }

    #[test]
    fn without_a_tool_nothing_is_copied() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!copy("x", dir.path().as_os_str()));
    }
}
