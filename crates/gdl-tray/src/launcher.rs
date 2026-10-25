//! Abre uma tela da janela: pede à que já está aberta (`Ui.Open`) ou inicia o programa.

use std::path::PathBuf;
use std::time::Duration;

use gdl_ipc::{UiProxy, nobody_there};
use zbus::Connection;

/// O programa da janela.
pub const UI_PROGRAM: &str = "gdrive-desktop-linux-ui";
/// Quanto esperar a janela aberta responder.
const UI_TIMEOUT: Duration = Duration::from_secs(3);

pub struct Launcher {
    conn: Connection,
    program: PathBuf,
}

impl Launcher {
    pub fn new(conn: Connection, program: PathBuf) -> Launcher {
        Launcher { conn, program }
    }

    /// O programa da janela ao lado deste executável; senão, o do `PATH`.
    pub fn default_program() -> PathBuf {
        exe_dir()
            .map(|dir| dir.join(UI_PROGRAM))
            .filter(|p| p.is_file())
            .unwrap_or_else(|| UI_PROGRAM.into())
    }

    /// Mostra `screen` na janela aberta; sem janela (ninguém com o nome), inicia uma com
    /// `--open <screen>`. Se a janela existe mas não respondeu, não abre outra.
    pub async fn open(&self, screen: &str) {
        let shown = async {
            UiProxy::new(&self.conn).await?.open(screen).await?;
            zbus::Result::Ok(())
        };
        // Uma janela travada (ou subindo pela ativação do D-Bus) não prende a bandeja.
        let shown = match tokio::time::timeout(UI_TIMEOUT, shown).await {
            Ok(shown) => shown,
            Err(_) => {
                tracing::warn!(tela = screen, "a janela não respondeu");
                return;
            }
        };
        match shown {
            Ok(()) => {}
            Err(e) if nobody_there(&e) => {
                if let Err(e) = self.spawn(screen) {
                    tracing::warn!(erro = %e, tela = screen, "não deu para abrir a janela");
                }
            }
            Err(e) => tracing::warn!(erro = %e, tela = screen, "a janela recusou o pedido"),
        }
    }

    fn spawn(&self, screen: &str) -> std::io::Result<()> {
        let mut child = tokio::process::Command::new(&self.program)
            .args(["--open", screen])
            .stdin(std::process::Stdio::null())
            .spawn()?;
        // Recolhe o processo quando ele terminar (sem deixar zumbi).
        tokio::spawn(async move { child.wait().await });
        Ok(())
    }
}

/// A pasta deste executável.
pub fn exe_dir() -> Option<PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;

    #[tokio::test]
    async fn launcher_spawns_ui_when_name_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("args.txt");
        let program = dir.path().join("ui-falsa");
        std::fs::write(
            &program,
            format!("#!/bin/sh\necho \"$@\" > '{}'\n", log.display()),
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        // Um barramento ponto a ponto sem ninguém do outro lado servindo a janela.
        let (server, client) = gdl_ipc::testing::p2p_pair().await;
        // `object_server()` liga o servidor de objetos do outro lado, que passa a responder
        // "objeto desconhecido" (como o barramento real responde "nome sem dono"); sem ele, o
        // pedido ficaria sem resposta.
        server.object_server();
        let launcher = Launcher::new(client, program);
        let start = std::time::Instant::now();

        launcher.open("errors").await;

        for _ in 0..200 {
            if let Ok(args) = std::fs::read_to_string(&log) {
                assert_eq!(args.trim(), "--open errors");
                assert!(start.elapsed() < UI_TIMEOUT, "não esperou o prazo");
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("a janela não foi iniciada");
    }
}
