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
