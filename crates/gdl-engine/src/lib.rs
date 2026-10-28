//! Motor de sincronização: índice, mudanças, cache, envio e off-line.

pub mod activity;
pub mod cache;
pub mod changes;
pub mod drives;
pub mod engine;
pub mod errors;
pub mod events;
pub mod gdocs;
pub mod health;
pub mod ids;
pub mod indexer;
pub mod kept;
pub mod ops;
pub mod pins;
pub mod ransomware;
pub mod search;
pub mod sharing;
#[cfg(test)]
mod testutil;
pub mod thumbs;
pub mod uploads;

use gdl_drive::client::DriveError;
use gdl_store::StoreError;

use crate::cache::CacheError;

/// Uma trava envenenada (pânico com ela tomada) continua usável: o conjunto segue consistente.
pub(crate) fn lock<T>(mutex: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Drive(#[from] DriveError),
    #[error(transparent)]
    Cache(#[from] CacheError),
    #[error("erro de arquivo local: {0}")]
    Io(#[from] std::io::Error),
    #[error("a conta ainda não foi indexada")]
    NotIndexed,
    #[error("o item não existe")]
    NotFound,
    #[error("o arquivo tem alterações locais que ainda não foram enviadas")]
    LocalEdits,
    #[error("o arquivo ainda não foi enviado para o Google Drive")]
    NotUploaded,
    #[error("o Drive compartilhado tem arquivos que ainda não foram enviados")]
    PendingUploads,
    #[error("o nome do Drive compartilhado está vazio")]
    EmptyName,
    #[error("a API não move pastas entre o Meu Drive e os Drives compartilhados")]
    FolderCrossDrive,
}

pub type Result<T> = std::result::Result<T, EngineError>;
