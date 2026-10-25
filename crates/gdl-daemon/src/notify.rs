//! Notificações da área de trabalho (`org.freedesktop.Notifications`) para o que o usuário precisa
//! saber: edição guardada em Lost and Found, sem internet, disco quase cheio, armazenamento cheio,
//! falha de envio e cópia de Documento Google que virou atalho.

use std::collections::HashMap;
use std::sync::Mutex;

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
    /// Aviso que substitui o anterior da mesma chave em vez de abrir outro: um por estado (sem
    /// internet, armazenamento cheio) e um por arquivo com falha de envio; `None` abre sempre.
    pub kind: Option<String>,
}

/// O aviso de um evento do motor; `None` para o que não merece interromper o usuário.
pub fn for_event(event: &EngineEvent, i18n: &I18n) -> Option<Notification> {
    let note = |summary: &'static str, body: String| Notification {
        summary: i18n.tr(summary, &[]),
        body,
        action: None,
        kind: None,
    };
    // Avisos de estado: o novo substitui o anterior do mesmo tipo.
    let state = |summary: &'static str, body: String| Notification {
        kind: Some(summary.to_string()),
        ..note(summary, body)
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
            state("notify-offline-title", i18n.tr("notify-offline-body", &[]))
        }
        EngineEvent::DiskLow { free_kb } => state(
            "notify-disk-low-title",
            i18n.tr(
                "notify-disk-low-body",
                &[("free", &(free_kb / 1024).to_string())],
            ),
        ),
        EngineEvent::QuotaFull => state("notify-quota-title", i18n.tr("notify-quota-body", &[])),
        EngineEvent::UploadFailed { name, error } => Notification {
            // Cada nova tentativa do mesmo arquivo atualiza o aviso dele.
            kind: Some(format!("upload-failed:{name}")),
            ..note(
                "notify-upload-failed-title",
                i18n.tr(
                    "notify-upload-failed-body",
                    &[("name", name), ("error", error)],
                ),
            )
        },
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
    /// Último aviso mostrado de cada chave, para o próximo substituí-lo.
    shown: Mutex<HashMap<String, u32>>,
}

impl Notifier {
    pub async fn new(conn: &Connection, i18n: &I18n) -> zbus::Result<Notifier> {
        Ok(Notifier {
            proxy: NotificationsProxy::new(conn).await?,
            app_name: i18n.tr("notify-title", &[]),
            shown: Mutex::default(),
        })
    }

    fn shown(&self) -> std::sync::MutexGuard<'_, HashMap<String, u32>> {
        self.shown.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn proxy(&self) -> &NotificationsProxy<'static> {
        &self.proxy
    }

    /// Mostra o aviso (no lugar do anterior do mesmo tipo). Sem servidor de notificações, só
    /// registra no log.
    pub async fn send(&self, note: &Notification) {
        let replaces = note
            .kind
            .as_ref()
            .and_then(|kind| self.shown().get(kind).copied())
            .unwrap_or(0);
        let actions: Vec<&str> = note
            .action
            .iter()
            .flat_map(|(key, label)| [*key, label.as_str()])
            .collect();
        let sent = self
            .proxy
            .notify(
                &self.app_name,
                replaces,
                "google-drive",
                &note.summary,
                &note.body,
                &actions,
                HashMap::new(),
                -1,
            )
            .await;
        match (sent, &note.kind) {
            (Ok(id), Some(kind)) => {
                self.shown().insert(kind.clone(), id);
            }
            (Ok(_), None) => {}
            (Err(e), _) => {
                tracing::warn!(erro = %e, aviso = %note.summary, "notificação não enviada");
            }
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

    /// Bug: a falha de envio de um arquivo substituía o aviso da falha de outro.
    #[test]
    fn upload_failures_replace_only_the_same_file() {
        let i18n = I18n::new("en");
        let kind = |event: EngineEvent| for_event(&event, &i18n).unwrap().kind;
        let failed = |name: &str| EngineEvent::UploadFailed {
            name: name.into(),
            error: "x".into(),
        };
        assert!(kind(failed("a.txt")).is_some());
        assert_eq!(kind(failed("a.txt")), kind(failed("a.txt")));
        assert_ne!(kind(failed("a.txt")), kind(failed("b.txt")));
        assert!(kind(EngineEvent::QuotaFull).is_some());
        let shortcut = EngineEvent::CopiedAsShortcut {
            name: "Plano".into(),
        };
        assert_eq!(kind(shortcut), None);
    }
}
