//! Histórico das notificações (o sino da janela principal): as 100 mais novas, guardadas em
//! `~/.local/state/gdrive-desktop-linux/notifications.json`.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard};

use gdl_common::fs::write_atomic;
use gdl_common::time::now_ms;
use gdl_ipc::NotificationItem;
use serde::{Deserialize, Serialize};

use crate::lock;
use crate::notify::Notification;

/// Quantos avisos o histórico guarda (os mais antigos saem).
pub const MAX_ENTRIES: usize = 100;

/// O que vai para o arquivo.
#[derive(Default, Serialize, Deserialize)]
struct Saved {
    /// Último ID dado: um aviso dispensado nunca empresta o ID a outro.
    last_id: u64,
    /// O mais novo primeiro.
    entries: VecDeque<Entry>,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    #[serde(flatten)]
    item: NotificationItem,
    /// `Notification::replaces_key`: o aviso novo da mesma chave toma o lugar deste.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    replaces_key: Option<String>,
    /// ID que o servidor de notificações deu, enquanto o serviço roda.
    #[serde(skip)]
    desktop_id: Option<u32>,
}

pub struct History {
    path: PathBuf,
    saved: Mutex<Saved>,
}

impl History {
    /// Lê o histórico de `path`; ausente ou ilegível, começa vazio (só registra no log).
    pub fn load(path: PathBuf) -> History {
        let saved = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|e| {
                tracing::warn!(erro = %e, arquivo = %path.display(), "histórico de notificações ilegível; começa vazio");
                Saved::default()
            }),
            Err(_) => Saved::default(),
        };
        History {
            path,
            saved: Mutex::new(saved),
        }
    }

    fn saved(&self) -> MutexGuard<'_, Saved> {
        lock(&self.saved)
    }

    /// Acrescenta o aviso no topo, no lugar do anterior da mesma chave, e devolve o ID dele.
    pub fn push(&self, note: &Notification) -> u64 {
        let mut saved = self.saved();
        if let Some(key) = &note.replaces_key {
            saved
                .entries
                .retain(|e| e.replaces_key.as_ref() != Some(key));
        }
        saved.last_id += 1;
        let id = saved.last_id;
        let (action_key, action_label) = note
            .action
            .as_ref()
            .map_or_else(Default::default, |(key, label)| {
                (key.to_string(), label.clone())
            });
        saved.entries.push_front(Entry {
            item: NotificationItem {
                id,
                kind: note.kind_id.into(),
                title: note.summary.clone(),
                body: note.body.clone(),
                time_ms: now_ms(),
                read: false,
                action_key,
                action_label,
            },
            replaces_key: note.replaces_key.clone(),
            desktop_id: None,
        });
        saved.entries.truncate(MAX_ENTRIES);
        self.save(&saved);
        id
    }

    /// Liga o aviso `id` ao que o servidor de notificações mostrou.
    pub fn set_desktop_id(&self, id: u64, desktop_id: u32) {
        if let Some(entry) = self.saved().entries.iter_mut().find(|e| e.item.id == id) {
            entry.desktop_id = Some(desktop_id);
        }
    }

    /// O aviso que o servidor de notificações mostrou como `desktop_id`.
    pub fn by_desktop_id(&self, desktop_id: u32) -> Option<u64> {
        let saved = self.saved();
        let entry = saved
            .entries
            .iter()
            .find(|e| e.desktop_id == Some(desktop_id))?;
        Some(entry.item.id)
    }

    pub fn items(&self) -> Vec<NotificationItem> {
        self.saved()
            .entries
            .iter()
            .map(|e| e.item.clone())
            .collect()
    }

    pub fn get(&self, id: u64) -> Option<NotificationItem> {
        let saved = self.saved();
        let entry = saved.entries.iter().find(|e| e.item.id == id)?;
        Some(entry.item.clone())
    }

    /// Marca o aviso como lido; `false` se ele não existe ou já estava lido.
    pub fn mark_read(&self, id: u64) -> bool {
        self.mark_read_where(|item| item.id == id)
    }

    /// Marca todos como lidos; `false` se já estavam.
    pub fn mark_all_read(&self) -> bool {
        self.mark_read_where(|_| true)
    }

    fn mark_read_where(&self, wanted: impl Fn(&NotificationItem) -> bool) -> bool {
        let mut saved = self.saved();
        let mut changed = false;
        for entry in saved
            .entries
            .iter_mut()
            .filter(|e| !e.item.read && wanted(&e.item))
        {
            entry.item.read = true;
            changed = true;
        }
        if changed {
            self.save(&saved);
        }
        changed
    }

    /// Tira o aviso do histórico; `false` se ele não estava lá.
    pub fn dismiss(&self, id: u64) -> bool {
        let mut saved = self.saved();
        let before = saved.entries.len();
        saved.entries.retain(|e| e.item.id != id);
        let removed = saved.entries.len() != before;
        if removed {
            self.save(&saved);
        }
        removed
    }

    /// Grava com a trava pega, para duas gravações nunca chegarem fora de ordem. Falhar só vai
    /// para o log (o histórico em memória continua valendo).
    fn save(&self, saved: &Saved) {
        let written = serde_json::to_vec(saved)
            .map_err(std::io::Error::from)
            .and_then(|bytes| write_atomic(&self.path, &bytes));
        if let Err(e) = written {
            tracing::warn!(erro = %e, arquivo = %self.path.display(), "não deu para gravar o histórico de notificações");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notify::Notification;

    fn note(n: usize) -> Notification {
        Notification {
            kind_id: "offline",
            summary: format!("aviso {n}"),
            body: "corpo".into(),
            action: None,
            replaces_key: None,
        }
    }

    #[test]
    fn history_keeps_newest_100_and_survives_restart() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state/notifications.json");
        let history = History::load(path.clone());
        for n in 0..105 {
            history.push(&note(n));
        }
        let with_action = history.push(&Notification {
            action: Some(("resume", "Retomar".into())),
            ..note(105)
        });

        let items = history.items();
        assert_eq!(items.len(), MAX_ENTRIES);
        assert_eq!(items[0].title, "aviso 105");
        assert_eq!(items[0].id, with_action);
        assert_eq!(
            (items[0].action_key.as_str(), items[0].action_label.as_str()),
            ("resume", "Retomar")
        );
        assert_eq!(items[0].kind, "offline");
        assert_eq!(items[99].title, "aviso 6");
        assert!(items[0].id > items[1].id);
        assert!(items.iter().all(|i| !i.read));
        assert!(history.mark_read(items[1].id));
        assert!(!history.mark_read(items[1].id), "já estava lido");
        assert!(history.dismiss(with_action));
        drop(history);

        let again = History::load(path.clone());
        let reloaded = again.items();
        assert_eq!(reloaded.len(), MAX_ENTRIES - 1);
        assert!(reloaded[0].read && !reloaded[1].read);
        assert!(
            again.push(&note(200)) > with_action,
            "os IDs não se repetem, nem o de um aviso dispensado"
        );

        std::fs::write(&path, "{ quebrado").unwrap();
        assert!(History::load(path).items().is_empty());
    }

    /// Bug: cada falha de envio do mesmo arquivo (e cada queda da rede) abria uma linha nova no
    /// sino, enquanto a área de trabalho só atualizava o aviso.
    #[test]
    fn note_with_the_same_key_replaces_the_previous_one() {
        let tmp = tempfile::tempdir().unwrap();
        let history = History::load(tmp.path().join("notifications.json"));
        let keyed = |key: &str, n| Notification {
            replaces_key: Some(key.into()),
            ..note(n)
        };
        history.push(&keyed("upload-failed:a.txt", 1));
        let first = history.push(&note(2));
        history.set_desktop_id(first, 7);
        history.push(&keyed("upload-failed:b.txt", 3));
        let latest = history.push(&keyed("upload-failed:a.txt", 4));

        let titles: Vec<_> = history.items().into_iter().map(|i| i.title).collect();
        assert_eq!(titles, ["aviso 4", "aviso 3", "aviso 2"]);
        assert_eq!(history.items()[0].id, latest);
        assert_eq!(history.by_desktop_id(7), Some(first));
        assert_eq!(history.by_desktop_id(8), None);
    }
}
