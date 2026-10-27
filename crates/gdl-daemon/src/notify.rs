//! Notificações da área de trabalho (`org.freedesktop.Notifications`) para o que o usuário precisa
//! saber: edição guardada em Lost and Found, sem internet, disco quase cheio, armazenamento cheio,
//! falha de envio, cópia de Documento Google que virou atalho, gravação com a sincronização
//! pausada e envio cancelado.

use std::collections::HashMap;
use std::sync::Mutex;

use gdl_common::i18n::I18n;
use gdl_engine::events::{EngineEvent, LostReason};
use zbus::Connection;
use zbus::zvariant::Value;

pub const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

/// Ação da notificação do Lost and Found que abre a pasta.
pub const OPEN_LOST_FOUND: &str = "open-lost-found";
/// Ação do aviso de envio cancelado que abre a pasta de cancelados.
pub const OPEN_CANCELED: &str = "open-canceled";
/// Ação do aviso de gravação com a sincronização pausada que a retoma.
pub const RESUME: &str = "resume";
/// Ação do aviso de conta sem login que abre a janela no login dela.
pub const SIGN_IN: &str = "sign-in";

/// Os avisos que o usuário pode desligar (Preferências > Notificações); os outros avisam de
/// algo que ele precisa saber e sempre aparecem.
pub const OPTIONAL_KINDS: [&str; 4] = [
    "copied-as-shortcut",
    "mount-moved",
    "resume-syncing",
    "offline",
];

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
    /// Família do aviso (`NotificationItem::kind`), para o histórico e o "não mostrar de novo".
    pub kind_id: &'static str,
    pub summary: String,
    pub body: String,
    /// Chave e rótulo do botão.
    pub action: Option<(&'static str, String)>,
    /// Aviso que substitui o anterior da mesma chave (na área de trabalho e no histórico) em vez
    /// de abrir outro: um por estado (sem internet, armazenamento cheio) e um por arquivo com
    /// falha de envio; `None` abre sempre.
    pub replaces_key: Option<String>,
    /// A conta (e-mail) que originou o aviso; vazio nos que não são de uma conta. A ação do botão
    /// age nela.
    pub account: String,
}

impl Notification {
    /// O lugar que o aviso ocupa: o de mesma chave e mesma conta é substituído por ele.
    pub fn slot(&self) -> Option<String> {
        let key = self.replaces_key.as_ref()?;
        Some(format!("{}:{key}", self.account))
    }
}

/// O aviso de um evento do motor; `None` para o que não merece interromper o usuário.
pub fn for_event(event: &EngineEvent, i18n: &I18n) -> Option<Notification> {
    let note = |kind_id: &'static str, summary: &'static str, body: String| Notification {
        kind_id,
        summary: i18n.tr(summary, &[]),
        body,
        action: None,
        replaces_key: None,
        account: String::new(),
    };
    // Avisos de estado: o novo substitui o anterior do mesmo tipo.
    let state = |kind_id: &'static str, summary: &'static str, body: String| Notification {
        replaces_key: Some(kind_id.to_string()),
        ..note(kind_id, summary, body)
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
                    "lost-and-found",
                    "notify-lost-found-title",
                    i18n.tr(
                        "notify-lost-found-body",
                        &[("name", &name), ("reason", &i18n.tr(reason, &[]))],
                    ),
                )
            }
        }
        EngineEvent::Offline(true) => state(
            "offline",
            "notify-offline-title",
            i18n.tr("notify-offline-body", &[]),
        ),
        EngineEvent::NeedsSignIn(true) => Notification {
            action: Some((SIGN_IN, i18n.tr("notify-signed-out-action", &[]))),
            ..state(
                "account-signed-out",
                "notify-signed-out-title",
                i18n.tr("notify-signed-out-body", &[]),
            )
        },
        EngineEvent::DiskLow { free_kb } => state(
            "disk-low",
            "notify-disk-low-title",
            i18n.tr(
                "notify-disk-low-body",
                &[("free", &(free_kb / 1024).to_string())],
            ),
        ),
        EngineEvent::QuotaFull => state(
            "quota-full",
            "notify-quota-title",
            i18n.tr("notify-quota-body", &[]),
        ),
        EngineEvent::UploadFailed { name, error } => Notification {
            // Cada nova tentativa do mesmo arquivo atualiza o aviso dele.
            replaces_key: Some(format!("upload-failed:{name}")),
            ..note(
                "upload-failed",
                "notify-upload-failed-title",
                i18n.tr(
                    "notify-upload-failed-body",
                    &[("name", name), ("error", error)],
                ),
            )
        },
        EngineEvent::CopiedAsShortcut { name } => note(
            "copied-as-shortcut",
            "notify-shortcut-title",
            i18n.tr("notify-shortcut-body", &[("name", name)]),
        ),
        EngineEvent::SavedWhilePaused { name } => Notification {
            action: Some((RESUME, i18n.tr("notify-paused-action", &[]))),
            ..state(
                "resume-syncing",
                "notify-paused-title",
                i18n.tr("notify-paused-body", &[("name", name)]),
            )
        },
        EngineEvent::UploadCanceled { name } => Notification {
            action: Some((OPEN_CANCELED, i18n.tr("notify-canceled-action", &[]))),
            ..note(
                "upload-canceled",
                "notify-canceled-title",
                i18n.tr("notify-canceled-body", &[("name", name)]),
            )
        },
        EngineEvent::Offline(false)
        | EngineEvent::NeedsSignIn(false)
        | EngineEvent::SyncStateChanged(_) => return None,
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
        crate::lock(&self.shown)
    }

    pub fn proxy(&self) -> &NotificationsProxy<'static> {
        &self.proxy
    }

    /// Mostra o aviso (no lugar do anterior do mesmo tipo) e devolve o ID que o servidor deu.
    /// Sem servidor de notificações, só registra no log.
    pub async fn send(&self, note: &Notification) -> Option<u32> {
        let slot = note.slot();
        let replaces = slot
            .as_ref()
            .and_then(|slot| self.shown().get(slot).copied())
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
        match (sent, slot) {
            (Ok(id), Some(slot)) => {
                self.shown().insert(slot, id);
                Some(id)
            }
            (Ok(id), None) => Some(id),
            (Err(e), _) => {
                tracing::warn!(erro = %e, aviso = %note.summary, "notificação não enviada");
                None
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
            EngineEvent::NeedsSignIn(true),
            EngineEvent::DiskLow { free_kb: 2048 },
            EngineEvent::QuotaFull,
            EngineEvent::UploadFailed {
                name: "a.txt".into(),
                error: "x".into(),
            },
            EngineEvent::CopiedAsShortcut {
                name: "Plano".into(),
            },
            EngineEvent::SavedWhilePaused {
                name: "a.txt".into(),
            },
            EngineEvent::UploadCanceled {
                name: "a.txt".into(),
            },
        ] {
            let note = for_event(&event, &i18n).unwrap();
            assert!(!note.summary.starts_with("notify-"), "{event:?} sem texto");
            assert!(!note.body.starts_with("notify-"), "{event:?} sem texto");
        }
        assert_eq!(for_event(&EngineEvent::Offline(false), &i18n), None);
        assert_eq!(for_event(&EngineEvent::NeedsSignIn(false), &i18n), None);
        assert_eq!(
            for_event(&EngineEvent::SyncStateChanged(Default::default()), &i18n),
            None
        );
    }

    #[test]
    fn every_optional_kind_has_a_label() {
        for lang in ["pt_BR", "en"] {
            let i18n = I18n::new(lang);
            for kind in OPTIONAL_KINDS {
                let key = format!("notify-kind-{kind}");
                assert_ne!(i18n.tr(&key, &[]), key, "{lang}: {kind} sem rótulo");
            }
        }
    }

    /// Bug: a falha de envio de um arquivo substituía o aviso da falha de outro.
    #[test]
    fn upload_failures_replace_only_the_same_file() {
        let i18n = I18n::new("en");
        let kind = |event: EngineEvent| for_event(&event, &i18n).unwrap().replaces_key;
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
