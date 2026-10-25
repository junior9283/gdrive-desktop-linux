//! Motor de sincronização: índice, mudanças, cache, envio e off-line.

pub mod cache;
pub mod changes;
pub mod events;
pub mod gdocs;
pub mod indexer;
#[cfg(test)]
mod testutil;

use gdl_drive::client::DriveError;
use gdl_store::StoreError;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Drive(#[from] DriveError),
    #[error("a conta ainda não foi indexada")]
    NotIndexed,
}

pub type Result<T> = std::result::Result<T, EngineError>;
