//! Notificações da área de trabalho (`org.freedesktop.Notifications`) para o que o usuário precisa
//! saber: edição guardada em Lost and Found, sem internet, disco quase cheio, armazenamento cheio,
//! falha de envio e cópia de Documento Google que virou atalho.

use std::collections::HashMap;

use gdl_common::i18n::I18n;
use gdl_engine::events::{EngineEvent, LostReason};
use zbus::Connection;
use zbus::zvariant::Value;

pub const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

/// Ação da notificação do Lost and Found que abre a pasta.
pub const OPEN_LOST_FOUND: &str = "open-lost-found";

#[zbus::proxy(
    interface = "org.freedesktop.Notifications",
    default_service = "org.freedesktop.Notifications",
    default_path = "/org/freedesktop/Notifications"
)]
pub trait Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: &str,
        replaces_id: u32,
        app_icon: &str,
        summary: &str,
        body: &str,
        actions: &[&str],
        hints: HashMap<&str, Value<'_>>,
        expire_timeout: i32,
    ) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn action_invoked(&self, id: u32, action_key: String) -> zbus::Result<()>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub summary: String,
    pub body: String,
    /// Chave e rótulo do botão.
    pub action: Option<(&'static str, String)>,
}

/// O aviso de um evento do motor; `None` para o que não merece interromper o usuário.
pub fn for_event(event: &EngineEvent, i18n: &I18n) -> Option<Notification> {
    let note = |summary: &str, body: String| Notification {
        summary: i18n.tr(summary, &[]),
        body,
        action: None,
    };
    Some(match event {
        EngineEvent::LostAndFound { path, reason } => {
            let reason = match reason {
                LostReason::RemoteChanged => "reason-remote-changed",
                LostReason::RemoteDeleted => "reason-remote-deleted",
                LostReason::NoPermission => "reason-no-permission",
            };
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            Notification {
                action: Some((OPEN_LOST_FOUND, i18n.tr("notify-lost-found-action", &[]))),
                ..note(
                    "notify-lost-found-title",
                    i18n.tr(
                        "notify-lost-found-body",
                        &[("name", &name), ("reason", &i18n.tr(reason, &[]))],
                    ),
                )
            }
        }
        EngineEvent::Offline(true) => {
            note("notify-offline-title", i18n.tr("notify-offline-body", &[]))
        }
        EngineEvent::DiskLow { free_kb } => note(
            "notify-disk-low-title",
            i18n.tr(
                "notify-disk-low-body",
                &[("free", &(free_kb / 1024).to_string())],
            ),
        ),
        EngineEvent::QuotaFull => note("notify-quota-title", i18n.tr("notify-quota-body", &[])),
        EngineEvent::UploadFailed { name, error } => note(
            "notify-upload-failed-title",
            i18n.tr(
                "notify-upload-failed-body",
                &[("name", name), ("error", error)],
            ),
        ),
        EngineEvent::CopiedAsShortcut { name } => note(
            "notify-shortcut-title",
            i18n.tr("notify-shortcut-body", &[("name", name)]),
        ),
        EngineEvent::Offline(false) | EngineEvent::SyncStateChanged(_) => return None,
    })
}

/// Envia notificações pela conexão do serviço (no barramento de sessão, ao servidor de
/// notificações da área de trabalho).
pub struct Notifier {
    proxy: NotificationsProxy<'static>,
    app_name: String,
}

impl Notifier {
    pub async fn new(conn: &Connection, i18n: &I18n) -> zbus::Result<Notifier> {
        Ok(Notifier {
            proxy: NotificationsProxy::new(conn).await?,
            app_name: i18n.tr("notify-title", &[]),
        })
    }

    pub fn proxy(&self) -> &NotificationsProxy<'static> {
        &self.proxy
    }

    /// Mostra o aviso. Sem servidor de notificações, só registra no log.
    pub async fn send(&self, note: &Notification) {
        let actions: Vec<&str> = note
            .action
            .iter()
            .flat_map(|(key, label)| [*key, label.as_str()])
            .collect();
        let sent = self
            .proxy
            .notify(
                &self.app_name,
                0,
                "google-drive",
                &note.summary,
                &note.body,
                &actions,
                HashMap::new(),
                -1,
            )
            .await;
        if let Err(e) = sent {
            tracing::warn!(erro = %e, aviso = %note.summary, "notificação não enviada");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn lost_and_found_note_names_the_file_and_offers_to_open_the_folder() {
        let i18n = I18n::new("pt_BR");
        let event = EngineEvent::LostAndFound {
            path: PathBuf::from("/x/contrato (conflito 2026-10-25 10-00-00).txt"),
            reason: LostReason::RemoteChanged,
        };

        let note = for_event(&event, &i18n).unwrap();

        assert!(note.body.contains("contrato"), "{note:?}");
        assert!(note.body.contains("alterado na nuvem"), "{note:?}");
        assert_eq!(note.action, Some((OPEN_LOST_FOUND, "Ver arquivos".into())));
    }

    #[test]
    fn every_problem_has_a_note_and_routine_changes_have_none() {
        let i18n = I18n::new("en");
        for event in [
            EngineEvent::Offline(true),
            EngineEvent::DiskLow { free_kb: 2048 },
            EngineEvent::QuotaFull,
            EngineEvent::UploadFailed {
                name: "a.txt".into(),
                error: "x".into(),
            },
            EngineEvent::CopiedAsShortcut {
                name: "Plano".into(),
            },
        ] {
            let note = for_event(&event, &i18n).unwrap();
            assert!(!note.summary.starts_with("notify-"), "{event:?} sem texto");
        }
        assert_eq!(for_event(&EngineEvent::Offline(false), &i18n), None);
        assert_eq!(
            for_event(&EngineEvent::SyncStateChanged(Default::default()), &i18n),
            None
        );
    }
}
