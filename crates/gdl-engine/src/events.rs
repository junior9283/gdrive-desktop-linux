//! Eventos que o motor publica para a bandeja, as notificações e a CLI.

use std::path::PathBuf;

/// Resumo do estado da sincronização, mostrado na bandeja e na CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SyncSummary {
    pub pending_uploads: u32,
    pub paused: bool,
    pub offline: bool,
    pub last_sync_ms: Option<i64>,
    pub errors: u32,
    /// O último envio esbarrou na cota cheia (até um envio dar certo).
    pub quota_full: bool,
    /// Itens subindo ou descendo: os envios na fila e os fixados que faltam baixar.
    pub syncing: u32,
    /// Estimativa pelo ritmo dos envios recentes; `None` sem amostras.
    pub remaining_secs: Option<u64>,
}

/// O estado que a bandeja e a janela principal mostram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncState {
    /// Sem conta conectada (o serviço responde isto; o motor só existe com conta).
    SignedOut,
    Synced,
    Syncing,
    Paused,
    Offline,
    QuotaFull,
    Error,
}

impl SyncState {
    pub fn as_str(self) -> &'static str {
        match self {
            SyncState::SignedOut => "signed-out",
            SyncState::Synced => "synced",
            SyncState::Syncing => "syncing",
            SyncState::Paused => "paused",
            SyncState::Offline => "offline",
            SyncState::QuotaFull => "quota-full",
            SyncState::Error => "error",
        }
    }
}

impl SyncSummary {
    /// Vale o primeiro que se aplicar: pausado, sem internet, cota cheia, sincronizando, erro.
    pub fn state(&self) -> SyncState {
        if self.paused {
            SyncState::Paused
        } else if self.offline {
            SyncState::Offline
        } else if self.quota_full {
            SyncState::QuotaFull
        } else if self.syncing > 0 {
            SyncState::Syncing
        } else if self.errors > 0 {
            SyncState::Error
        } else {
            SyncState::Synced
        }
    }
}

/// Por que uma edição local foi guardada em `Lost and Found` em vez de subir.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LostReason {
    RemoteChanged,
    RemoteDeleted,
    NoPermission,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EngineEvent {
    SyncStateChanged(SyncSummary),
    LostAndFound {
        path: PathBuf,
        reason: LostReason,
    },
    Offline(bool),
    DiskLow {
        free_kb: u64,
    },
    /// O armazenamento da conta está cheio: nada sobe até liberar espaço.
    QuotaFull,
    UploadFailed {
        name: String,
        error: String,
    },
    /// Um ponteiro de Documento Google copiado para uma pasta virou atalho (cópia de Documento
    /// Google não é suportada).
    CopiedAsShortcut {
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_state_follows_the_precedence() {
        let base = SyncSummary::default();
        let with = |f: fn(&mut SyncSummary)| {
            let mut s = base;
            f(&mut s);
            s.state()
        };
        assert_eq!(
            with(|s| {
                s.paused = true;
                s.offline = true;
            }),
            SyncState::Paused
        );
        assert_eq!(
            with(|s| {
                s.offline = true;
                s.pending_uploads = 2;
                s.syncing = 2;
            }),
            SyncState::Offline
        );
        assert_eq!(
            with(|s| {
                s.quota_full = true;
                s.syncing = 1;
            }),
            SyncState::QuotaFull
        );
        assert_eq!(with(|s| s.syncing = 3), SyncState::Syncing);
        assert_eq!(with(|s| s.errors = 1), SyncState::Error);
        assert_eq!(base.state(), SyncState::Synced);
        assert_eq!(SyncState::QuotaFull.as_str(), "quota-full");
    }
}
