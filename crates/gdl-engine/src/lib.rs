//! Motor de sincronização: índice, mudanças, cache, envio e off-line.

pub mod activity;
pub mod cache;
pub mod changes;
pub mod engine;
pub mod events;
pub mod gdocs;
pub mod ids;
pub mod indexer;
pub mod kept;
pub mod ops;
pub mod pins;
#[cfg(test)]
mod testutil;
pub mod uploads;

use gdl_drive::client::DriveError;
use gdl_store::StoreError;

use crate::cache::CacheError;

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
}

pub type Result<T> = std::result::Result<T, EngineError>;
