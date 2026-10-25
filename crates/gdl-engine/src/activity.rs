//! Atividade de sincronização: o que subiu, desceu ou foi para a lixeira por último, um registro
//! por item, para a janela principal.

use std::collections::VecDeque;
use std::sync::Mutex;

use gdl_common::time::now_ms;
use gdl_store::{Inode, Item, Store};
use tokio::sync::watch;

/// Quantos itens a atividade guarda (os mais antigos saem).
pub const MAX_ROWS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivityState {
    UploadQueued,
    Uploading,
    Uploaded,
    UploadCanceling,
    UploadCanceled,
    UploadFailed,
    Downloading,
    Downloaded,
    Trashing,
    Trashed,
    Restoring,
    Restored,
}

impl ActivityState {
    pub fn as_str(self) -> &'static str {
        match self {
            ActivityState::UploadQueued => "upload-queued",
            ActivityState::Uploading => "uploading",
            ActivityState::Uploaded => "uploaded",
            ActivityState::UploadCanceling => "upload-canceling",
            ActivityState::UploadCanceled => "upload-canceled",
            ActivityState::UploadFailed => "upload-failed",
            ActivityState::Downloading => "downloading",
            ActivityState::Downloaded => "downloaded",
            ActivityState::Trashing => "trashing",
            ActivityState::Trashed => "trashed",
            ActivityState::Restoring => "restoring",
            ActivityState::Restored => "restored",
        }
    }
}

/// O que aconteceu com um item. A chave é o inode, que não muda quando o ID local vira o do
/// Drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    pub inode: Inode,
    pub item_id: String,
    pub name: String,
    /// Caminho a partir do Meu Drive (`Pasta/arquivo.txt`).
    pub path: String,
    pub size: u64,
    pub state: ActivityState,
    /// Andamento do envio, quando conhecido.
    pub percent: Option<u8>,
}

impl Activity {
    /// A atividade de `item` no estado `state`, com o caminho tirado do banco.
    pub fn of(store: &Store, item: &Item, state: ActivityState, percent: Option<u8>) -> Activity {
        let path = store
            .path_of(&item.id)
            .ok()
            .flatten()
            .unwrap_or_else(|| item.local_name.clone());
        Activity {
            inode: item.inode,
            item_id: item.id.0.clone(),
            name: item.local_name.clone(),
            path,
            size: item.size,
            state,
            percent,
        }
    }
}

/// Uma linha da atividade, com a ordem e a hora do último registro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityRow {
    pub seq: u64,
    pub time_ms: i64,
    pub activity: Activity,
}

impl std::ops::Deref for ActivityRow {
    type Target = Activity;

    fn deref(&self) -> &Activity {
        &self.activity
    }
}

/// A atividade em memória (some ao reiniciar, como no original), com a versão que muda a cada
/// registro, para quem mostra a lista saber quando atualizar.
pub struct ActivityLog {
    rows: Mutex<VecDeque<ActivityRow>>,
    version: watch::Sender<u64>,
}

impl Default for ActivityLog {
    fn default() -> ActivityLog {
        ActivityLog {
            rows: Mutex::default(),
            version: watch::Sender::new(0),
        }
    }
}

impl ActivityLog {
    /// `record` da atividade de `item` (ver `Activity::of`).
    pub fn note(&self, store: &Store, item: &Item, state: ActivityState, percent: Option<u8>) {
        self.record(Activity::of(store, item, state, percent));
    }

    /// Registra o estado novo do item, que vai para o topo da lista.
    pub fn record(&self, activity: Activity) {
        let mut rows = self.rows.lock().unwrap_or_else(|e| e.into_inner());
        rows.retain(|r| r.inode != activity.inode);
        let seq = rows.front().map_or(1, |r| r.seq + 1);
        rows.push_front(ActivityRow {
            seq,
            time_ms: now_ms(),
            activity,
        });
        rows.truncate(MAX_ROWS);
        drop(rows);
        self.bump();
    }

    fn bump(&self) {
        self.version.send_modify(|v| *v += 1);
    }

    /// Só o andamento do envio de um item que já está na lista (não muda a ordem).
    pub fn progress(&self, inode: Inode, percent: u8) {
        let mut rows = self.rows.lock().unwrap_or_else(|e| e.into_inner());
        let Some(row) = rows.iter_mut().find(|r| r.inode == inode) else {
            return;
        };
        if row.activity.percent == Some(percent) {
            return;
        }
        row.activity.percent = Some(percent);
        drop(rows);
        self.bump();
    }

    /// O mais recente primeiro.
    pub fn rows(&self) -> Vec<ActivityRow> {
        let rows = self.rows.lock().unwrap_or_else(|e| e.into_inner());
        rows.iter().cloned().collect()
    }

    pub fn subscribe(&self) -> watch::Receiver<u64> {
        self.version.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use gdl_store::Inode;

    use super::*;

    fn entry(inode: u64, state: ActivityState) -> Activity {
        Activity {
            inode: Inode(inode),
            item_id: format!("id{inode}"),
            name: format!("f{inode}"),
            path: format!("f{inode}"),
            size: 1,
            state,
            percent: None,
        }
    }

    #[test]
    fn activity_keeps_one_row_per_item_newest_first() {
        let log = ActivityLog::default();
        log.record(entry(1, ActivityState::Uploading));
        log.record(entry(2, ActivityState::Uploading));
        log.record(entry(1, ActivityState::Uploaded));

        let rows = log.rows();
        let states: Vec<_> = rows.iter().map(|r| (r.name.as_str(), r.state)).collect();
        assert_eq!(
            states,
            [
                ("f1", ActivityState::Uploaded),
                ("f2", ActivityState::Uploading)
            ]
        );
        assert!(rows[0].seq > rows[1].seq);

        for n in 0..501 {
            log.record(entry(100 + n, ActivityState::Downloaded));
        }
        assert_eq!(log.rows().len(), 500);
    }

    #[test]
    fn every_change_bumps_the_version() {
        let log = ActivityLog::default();
        let changes = log.subscribe();
        log.record(entry(1, ActivityState::Uploading));
        assert!(changes.has_changed().unwrap());
    }
}
