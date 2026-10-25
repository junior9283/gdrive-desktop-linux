//! Atalho global da pesquisa (`SearchHotKey`), pelo portal `org.freedesktop.portal.GlobalShortcuts`
//! — no Wayland, só o portal pode ouvir teclas fora da janela.

use std::time::Duration;

use ashpd::desktop::global_shortcuts::{BindShortcutsOptions, GlobalShortcuts, NewShortcut};
use ashpd::desktop::{CreateSessionOptions, Session};
use futures_util::StreamExt;
use tokio::task::JoinHandle;
use zbus::Connection;

/// O atalho quando `SearchHotKey` não foi definido.
pub const DEFAULT_TRIGGER: &str = "Ctrl+Alt+G";
/// Identificador do atalho no portal.
const SHORTCUT_ID: &str = "search";
/// Identificador do app no registro do portal (apps fora de sandbox).
const APP_ID: &str = "io.github.junior9283.GDriveDesktopLinux";
/// Quanto esperar o portal responder que existe (ele pode estar subindo pela ativação do D-Bus).
const PORTAL_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    #[error("atalho inválido: {0}")]
    InvalidTrigger(String),
    #[error("portal de atalhos globais indisponível: {0}")]
    Unavailable(String),
}

/// Converte `Ctrl+Alt+G` no formato da especificação de atalhos do XDG (`CTRL+ALT+g`): os
/// modificadores em maiúsculas e a tecla como nome de keysym (letra minúscula).
pub fn parse_trigger(text: &str) -> Result<String, HotkeyError> {
    let invalid = || HotkeyError::InvalidTrigger(text.to_string());
    let mut modifiers: Vec<String> = Vec::new();
    let mut key = None;
    for part in text.split('+').map(str::trim) {
        if let Some(modifier) = modifier(part) {
            if !modifiers.iter().any(|m| m == modifier) {
                modifiers.push(modifier.into());
            }
        } else if key.is_none() && !part.is_empty() {
            key = Some(if part.chars().count() == 1 {
                part.to_lowercase()
            } else {
                part.to_string()
            });
        } else {
            return Err(invalid());
        }
    }
    modifiers.push(key.ok_or_else(invalid)?);
    Ok(modifiers.join("+"))
}

/// O nome do modificador no formato do portal.
fn modifier(part: &str) -> Option<&'static str> {
    Some(match part.to_ascii_lowercase().as_str() {
        "ctrl" | "control" => "CTRL",
        "alt" => "ALT",
        "shift" => "SHIFT",
        "super" | "meta" | "logo" | "win" => "LOGO",
        _ => return None,
    })
}

/// O atalho registrado; ao soltar (`unregister`), o portal esquece a sessão.
pub struct Hotkey {
    session: Session<GlobalShortcuts>,
    listener: JoinHandle<()>,
}

fn unavailable(e: impl std::fmt::Display) -> HotkeyError {
    HotkeyError::Unavailable(e.to_string())
}

impl Hotkey {
    /// Registra `trigger` (o portal pode pedir confirmação ao usuário, ou escolher outro) e chama
    /// `on_press` a cada toque.
    pub async fn register(
        conn: &Connection,
        trigger: &str,
        description: &str,
        on_press: impl Fn() + Send + 'static,
    ) -> Result<Hotkey, HotkeyError> {
        let trigger = parse_trigger(trigger)?;
        // O portal só existe num barramento (numa conexão ponto a ponto, o `ashpd` nem tem como
        // montar os caminhos das respostas).
        if conn.unique_name().is_none() {
            return Err(unavailable("conexão sem barramento"));
        }
        let open = async {
            if let Ok(app_id) = APP_ID.try_into()
                && let Err(e) = ashpd::register_host_app_with_connection(conn.clone(), app_id).await
            {
                tracing::debug!(erro = %e, "registro do app no portal");
            }
            let portal = GlobalShortcuts::with_connection(conn.clone()).await?;
            let session = portal
                .create_session(CreateSessionOptions::default())
                .await?;
            ashpd::Result::Ok((portal, session))
        };
        let (portal, session) = tokio::time::timeout(PORTAL_TIMEOUT, open)
            .await
            .map_err(|_| unavailable("sem resposta"))?
            .map_err(unavailable)?;
        let mut activated = portal.receive_activated().await.map_err(unavailable)?;
        let shortcut =
            NewShortcut::new(SHORTCUT_ID, description).preferred_trigger(trigger.as_str());
        portal
            .bind_shortcuts(&session, &[shortcut], None, BindShortcutsOptions::default())
            .await
            .and_then(|request| request.response())
            .map_err(unavailable)?;
        let listener = tokio::spawn(async move {
            // O portal fica vivo enquanto a tarefa ouve.
            let _portal = portal;
            while let Some(event) = activated.next().await {
                if event.shortcut_id() == SHORTCUT_ID {
                    on_press();
                }
            }
        });
        Ok(Hotkey { session, listener })
    }

    pub async fn unregister(self) {
        self.listener.abort();
        if let Err(e) = self.session.close().await {
            tracing::debug!(erro = %e, "sessão do atalho já fechada");
        }
    }
}

#[cfg(test)]
mod tests {
    use gdl_ipc::testing::PrivateBus;

    use super::*;

    #[test]
    fn parse_trigger_accepts_and_normalizes() {
        for (given, expected) in [
            ("Ctrl+Alt+G", "CTRL+ALT+g"),
            ("CTRL+ALT+g", "CTRL+ALT+g"),
            ("control + shift + F", "CTRL+SHIFT+f"),
            ("Super+F12", "LOGO+F12"),
            ("Meta+space", "LOGO+space"),
            ("Ctrl+Ctrl+K", "CTRL+k"),
        ] {
            assert_eq!(parse_trigger(given).as_deref(), Ok(expected), "{given}");
        }
    }

    #[test]
    fn parse_trigger_rejects_modifier_only() {
        for bad in ["Ctrl+Alt", "Ctrl+", "", "Ctrl+A+B", "+G"] {
            assert!(
                matches!(parse_trigger(bad), Err(HotkeyError::InvalidTrigger(_))),
                "{bad:?}"
            );
        }
    }

    /// Sem portal, a resposta vem logo ("nome sem dono"), sem esperar `PORTAL_TIMEOUT`.
    const QUICK: Duration = Duration::from_secs(2);

    #[tokio::test]
    async fn missing_portal_is_not_fatal() {
        let bus = PrivateBus::start();
        let conn = bus.connect().await;
        let start = std::time::Instant::now();

        let result = Hotkey::register(&conn, DEFAULT_TRIGGER, "Pesquisar", || {}).await;

        assert!(
            matches!(result, Err(HotkeyError::Unavailable(_))),
            "{:?}",
            result.err()
        );
        assert!(start.elapsed() < QUICK, "{:?}", start.elapsed());
    }

    #[tokio::test]
    async fn point_to_point_connection_is_not_a_bus() {
        let (_server, client) = gdl_ipc::testing::p2p_pair().await;

        let result = Hotkey::register(&client, DEFAULT_TRIGGER, "Pesquisar", || {}).await;

        assert!(matches!(result, Err(HotkeyError::Unavailable(_))));
    }
}
