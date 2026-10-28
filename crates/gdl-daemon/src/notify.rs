//! Notificações da área de trabalho (`org.freedesktop.Notifications`) para o que o usuário precisa
//! saber: edição guardada em Lost and Found, sem internet, disco quase cheio, armazenamento cheio,
//! falha de envio, cópia de Documento Google que virou atalho, gravação com a sincronização
//! pausada, envio cancelado, arquivo movido para um Drive compartilhado e pasta que não pode
//! mudar de drive.

use std::collections::HashMap;
use std::sync::Mutex;

use gdl_common::i18n::I18n;
use gdl_engine::events::{DeniedAction, EngineEvent, LostReason};
use gdl_store::folders::{PairState, StopReason};
use zbus::Connection;
use zbus::zvariant::Value;

pub const NOTIFICATIONS_PATH: &str = "/org/freedesktop/Notifications";

/// Ação da notificação do Lost and Found que abre a pasta.
pub const OPEN_LOST_FOUND: &str = "open-lost-found";
/// Ação do aviso de envio cancelado que abre a pasta de cancelados.
pub const OPEN_CANCELED: &str = "open-canceled";
/// Ação do aviso de gravação com a sincronização pausada que a retoma.
pub const RESUME: &str = "resume";
/// Ação dos avisos de pasta do computador e de dispositivo que abre Preferências > Meu computador.
pub const OPEN_FOLDERS: &str = "open-folders";
/// Ação do aviso de conta sem login que abre a janela no login dela.
pub const SIGN_IN: &str = "sign-in";
/// Ação do aviso de falha que abre a *issue* com o relatório; fechar é "Não enviar".
pub const SEND_CRASH_REPORT: &str = "send-crash-report";

/// Os avisos que o usuário pode desligar (Preferências > Notificações); os outros avisam de
/// algo que ele precisa saber e sempre aparecem.
pub const OPTIONAL_KINDS: [&str; 9] = [
    "copied-as-shortcut",
    "moved-to-shared-drive",
    "mount-moved",
    "resume-syncing",
    "offline",
    "quota-almost-full",
    "uploads-stuck",
    "items-trashed",
    "no-permission",
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

    fn get_capabilities(&self) -> zbus::Result<Vec<String>>;

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
        EngineEvent::MovedToSharedDrive { name } => note(
            "moved-to-shared-drive",
            "notify-moved-shared-title",
            i18n.tr("notify-moved-shared-body", &[("name", name)]),
        ),
        EngineEvent::FolderMoveUnsupported { name } => note(
            "folder-move-unsupported",
            "notify-folder-move-title",
            i18n.tr("notify-folder-move-body", &[("name", name)]),
        ),
        EngineEvent::NoPermission { action, name } => {
            let action = match action {
                DeniedAction::Create => "no-permission-create",
                DeniedAction::Move => "no-permission-move",
                DeniedAction::Rename => "no-permission-rename",
                DeniedAction::Delete => "no-permission-delete",
            };
            Notification {
                // Uma recusa atrás da outra (uma pasta inteira) vira um aviso só, o mais recente.
                replaces_key: Some("no-permission".into()),
                ..note(
                    "no-permission",
                    "notify-no-permission-title",
                    i18n.tr(
                        "notify-no-permission-body",
                        &[("action", &i18n.tr(action, &[])), ("name", name)],
                    ),
                )
            }
        }
        EngineEvent::QuotaAlmostFull { percent } => state(
            "quota-almost-full",
            "notify-quota-almost-title",
            i18n.tr(
                "notify-quota-almost-body",
                &[("percent", &percent.to_string())],
            ),
        ),
        EngineEvent::UploadsStuck { count } => note(
            "uploads-stuck",
            "notify-uploads-stuck-title",
            i18n.tr_count("notify-uploads-stuck-body", *count as u64),
        ),
        EngineEvent::LocalDiskFull => state(
            "local-disk-full",
            "notify-local-disk-full-title",
            i18n.tr("notify-local-disk-full-body", &[]),
        ),
        EngineEvent::AccountDisabled => state(
            "account-disabled",
            "notify-account-disabled-title",
            i18n.tr("notify-account-disabled-body", &[]),
        ),
        EngineEvent::Offline(false)
        | EngineEvent::NeedsSignIn(false)
        | EngineEvent::SyncStateChanged(_)
        | EngineEvent::ItemsChanged { .. } => return None,
    })
}

/// O aviso de uma pasta do computador (o par `pair`, chamado `name`) que parou ou espera
/// confirmação; `None` quando ela voltou a sincronizar. Um aviso por pasta: o novo substitui o
/// anterior.
pub fn for_folder(pair: i64, name: &str, state: &PairState, i18n: &I18n) -> Option<Notification> {
    let (kind_id, count) = match state {
        PairState::Active => return None,
        PairState::Stopped(StopReason::Missing) => ("folder-missing", 0),
        PairState::Stopped(StopReason::NoPermission) => ("folder-no-permission", 0),
        PairState::Stopped(StopReason::Moved) => ("folder-moved", 0),
        PairState::ConfirmDeletes(n) => ("folder-mass-delete", *n),
    };
    // Os textos são `notify-<tipo>-title` e `notify-<tipo>-body`.
    let text =
        |part: &str, args: &[(&str, &str)]| i18n.tr(&format!("notify-{kind_id}-{part}"), args);
    Some(Notification {
        kind_id,
        summary: text("title", &[]),
        body: text("body", &[("name", name), ("count", &count.to_string())]),
        action: Some((OPEN_FOLDERS, i18n.tr("notify-folder-action", &[]))),
        replaces_key: Some(format!("folder:{pair}")),
        account: String::new(),
    })
}

/// Uma comparação de pasta do computador mandou `count` itens que saíram do Drive para a
/// lixeira do sistema (um aviso por comparação).
pub fn items_trashed(count: usize, i18n: &I18n) -> Notification {
    Notification {
        kind_id: "items-trashed",
        summary: i18n.tr("notify-items-trashed-title", &[]),
        body: i18n.tr_count("notify-items-trashed-body", count as u64),
        action: None,
        replaces_key: None,
        account: String::new(),
    }
}

/// Um programa do app fechou de forma inesperada: oferece enviar o relatório.
pub fn crash(i18n: &I18n) -> Notification {
    Notification {
        kind_id: "crash",
        summary: i18n.tr("notify-crash-title", &[]),
        body: i18n.tr("notify-crash-body", &[]),
        action: Some((SEND_CRASH_REPORT, i18n.tr("notify-crash-action", &[]))),
        replaces_key: Some("crash".into()),
        account: String::new(),
    }
}

/// A pergunta de um dispositivo que entrou: "Fazer backup" abre Preferências > Meu computador,
/// onde ele aparece com a opção de lembrar; fechar é "Agora não".
pub fn device_ask(id: &str, label: &str, i18n: &I18n) -> Notification {
    let action = (OPEN_FOLDERS, i18n.tr("notify-device-ask-action", &[]));
    let body = i18n.tr("notify-device-ask-body", &[("name", label)]);
    device_note(
        id,
        "device-ask",
        "notify-device-ask-title",
        body,
        Some(action),
        i18n,
    )
}

/// O fim do backup de um dispositivo (no lugar da pergunta): pode remover, ou o erro.
pub fn device_done(id: &str, label: &str, error: Option<&str>, i18n: &I18n) -> Notification {
    match error {
        None => {
            let body = i18n.tr("notify-device-done-body", &[("name", label)]);
            device_note(
                id,
                "device-done",
                "notify-device-done-title",
                body,
                None,
                i18n,
            )
        }
        Some(error) => {
            let args = [("name", label), ("error", error)];
            let body = i18n.tr("notify-device-error-body", &args);
            let action = (OPEN_FOLDERS, i18n.tr("notify-folder-action", &[]));
            let title = "notify-device-error-title";
            device_note(id, "device-error", title, body, Some(action), i18n)
        }
    }
}

/// Um aviso por dispositivo: o novo substitui o anterior.
fn device_note(
    id: &str,
    kind_id: &'static str,
    title: &str,
    body: String,
    action: Option<(&'static str, String)>,
    i18n: &I18n,
) -> Notification {
    Notification {
        kind_id,
        summary: i18n.tr(title, &[]),
        body,
        action,
        replaces_key: Some(format!("device:{id}")),
        account: String::new(),
    }
}

/// Envia notificações pela conexão do serviço (no barramento de sessão, ao servidor de
/// notificações da área de trabalho).
pub struct Notifier {
    proxy: NotificationsProxy<'static>,
    app_name: String,
    /// Último aviso mostrado de cada chave, para o próximo substituí-lo.
    shown: Mutex<HashMap<String, u32>>,
    /// O servidor interpreta marcação no corpo (`body-markup`): nomes de arquivo e rótulos vão
    /// escapados, para não virar negrito, link ou imagem. Perguntado no primeiro aviso (não na
    /// partida, quando o servidor pode ainda não ter subido) e guardado só quando ele responde.
    markup: tokio::sync::OnceCell<bool>,
}

impl Notifier {
    pub async fn new(conn: &Connection, i18n: &I18n) -> zbus::Result<Notifier> {
        Ok(Notifier {
            proxy: NotificationsProxy::new(conn).await?,
            app_name: i18n.tr("notify-title", &[]),
            shown: Mutex::default(),
            markup: tokio::sync::OnceCell::new(),
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
        let markup = self
            .markup
            .get_or_try_init(|| async {
                let caps = self.proxy.get_capabilities().await?;
                zbus::Result::Ok(caps.iter().any(|c| c == "body-markup"))
            })
            .await;
        // Só sai cru se o servidor disse que não interpreta marcação; na dúvida, escapado.
        let body = match markup {
            Ok(false) => std::borrow::Cow::Borrowed(&note.body),
            _ => std::borrow::Cow::Owned(crate::desktop::xml_text(&note.body)),
        };
        let sent = self
            .proxy
            .notify(
                &self.app_name,
                replaces,
                "google-drive",
                &note.summary,
                &body,
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
            EngineEvent::MovedToSharedDrive {
                name: "a.txt".into(),
            },
            EngineEvent::FolderMoveUnsupported {
                name: "Pasta".into(),
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
    fn folder_states_have_notes_that_open_the_folders() {
        use gdl_store::folders::{PairState, StopReason};
        let i18n = I18n::new("pt_BR");
        let note = |state| for_folder(7, "Documentos", &state, &i18n);
        assert_eq!(note(PairState::Active), None);
        for (state, kind) in [
            (PairState::Stopped(StopReason::Missing), "folder-missing"),
            (
                PairState::Stopped(StopReason::NoPermission),
                "folder-no-permission",
            ),
            (PairState::Stopped(StopReason::Moved), "folder-moved"),
            (PairState::ConfirmDeletes(40), "folder-mass-delete"),
        ] {
            let note = note(state).unwrap();
            assert_eq!(note.kind_id, kind);
            assert!(note.body.contains("Documentos"), "{kind}: {}", note.body);
            assert!(!note.summary.starts_with("notify-"), "{kind} sem texto");
            assert_eq!(note.action, Some((OPEN_FOLDERS, "Ver pastas".into())));
            assert_eq!(
                note.replaces_key.as_deref(),
                Some("folder:7"),
                "um aviso por pasta"
            );
        }
        let mass = note(PairState::ConfirmDeletes(40)).unwrap();
        assert!(mass.body.contains("40"), "{}", mass.body);
    }

    #[test]
    fn remaining_notes_name_what_happened() {
        use gdl_engine::events::DeniedAction;
        for lang in ["pt_BR", "en"] {
            let i18n = I18n::new(lang);
            let note = |event| for_event(&event, &i18n).unwrap();
            for action in [
                DeniedAction::Create,
                DeniedAction::Move,
                DeniedAction::Rename,
                DeniedAction::Delete,
            ] {
                let denied = note(EngineEvent::NoPermission {
                    action,
                    name: "a.txt".into(),
                });
                assert_eq!(denied.kind_id, "no-permission");
                assert!(denied.body.contains("a.txt"), "{lang}: {denied:?}");
                assert!(
                    !denied.body.contains("no-permission-"),
                    "{lang}: {denied:?}"
                );
                // Uma recusa atrás da outra (uma pasta inteira) vira um aviso só.
                assert_eq!(denied.replaces_key.as_deref(), Some("no-permission"));
            }
            let quota = note(EngineEvent::QuotaAlmostFull { percent: 95 });
            assert_eq!(quota.kind_id, "quota-almost-full");
            assert!(quota.body.contains("95"), "{lang}: {quota:?}");
            let stuck = note(EngineEvent::UploadsStuck { count: 3 });
            assert_eq!(stuck.kind_id, "uploads-stuck");
            assert!(stuck.body.contains('3'), "{lang}: {stuck:?}");
            let trashed = items_trashed(4, &i18n);
            assert_eq!(trashed.kind_id, "items-trashed");
            assert!(trashed.body.contains('4'), "{lang}: {trashed:?}");
            for note in [
                quota,
                stuck,
                trashed,
                note(EngineEvent::LocalDiskFull),
                note(EngineEvent::AccountDisabled),
            ] {
                assert!(!note.summary.starts_with("notify-"), "{lang}: {note:?}");
                assert!(!note.body.starts_with("notify-"), "{lang}: {note:?}");
            }
        }
        for kind in [
            "quota-almost-full",
            "uploads-stuck",
            "items-trashed",
            "no-permission",
        ] {
            assert!(OPTIONAL_KINDS.contains(&kind), "{kind} pode ser desligado");
        }
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

    #[test]
    fn device_notes_replace_each_other_and_have_texts() {
        for lang in ["pt_BR", "en"] {
            let i18n = I18n::new(lang);
            let ask = device_ask("AAAA", "FOTOS", &i18n);
            let done = device_done("AAAA", "FOTOS", None, &i18n);
            let error = device_done("AAAA", "FOTOS", Some("sem espaço"), &i18n);
            for note in [&ask, &done, &error] {
                assert!(!note.summary.starts_with("notify-"), "{lang}: {note:?}");
                assert!(note.body.contains("FOTOS"), "{lang}: {note:?}");
                assert_eq!(note.slot(), ask.slot(), "um aviso por dispositivo");
            }
            assert!(error.body.contains("sem espaço"), "{error:?}");
            assert_eq!(ask.action.map(|a| a.0), Some(OPEN_FOLDERS));
            assert_eq!(done.action, None);
            assert_eq!(error.action.map(|a| a.0), Some(OPEN_FOLDERS));
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
