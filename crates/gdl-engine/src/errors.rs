//! Lista de erros da sincronização: envios que falharam e cópias no `Lost and Found`, por
//! categoria.

use std::path::PathBuf;

use gdl_drive::client::DriveError;
use gdl_store::{ItemId, Store};

use crate::kept::KeptCopies;
use crate::{EngineError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ErrorCategory {
    /// Armazenamento da conta cheio.
    Quota,
    /// Limite diário de envio.
    UploadLimit,
    NoPermission,
    TooLarge,
    /// A pasta (ou o arquivo) sumiu da nuvem.
    CloudMissing,
    /// O cache não tem o conteúdo a enviar.
    LocalMissing,
    /// Uma cópia no `Lost and Found`.
    LostAndFound,
    Unknown,
}

impl ErrorCategory {
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCategory::Quota => "quota",
            ErrorCategory::UploadLimit => "upload-limit",
            ErrorCategory::NoPermission => "no-permission",
            ErrorCategory::TooLarge => "too-large",
            ErrorCategory::CloudMissing => "cloud-missing",
            ErrorCategory::LocalMissing => "local-missing",
            ErrorCategory::LostAndFound => "lost-and-found",
            ErrorCategory::Unknown => "unknown",
        }
    }

    /// O contrário de `as_str`; o que não reconhece é `Unknown`.
    pub fn parse(s: &str) -> ErrorCategory {
        match s {
            "quota" => ErrorCategory::Quota,
            "upload-limit" => ErrorCategory::UploadLimit,
            "no-permission" => ErrorCategory::NoPermission,
            "too-large" => ErrorCategory::TooLarge,
            "cloud-missing" => ErrorCategory::CloudMissing,
            "local-missing" => ErrorCategory::LocalMissing,
            "lost-and-found" => ErrorCategory::LostAndFound,
            _ => ErrorCategory::Unknown,
        }
    }

    /// A categoria de uma falha de envio.
    pub fn of(e: &DriveError) -> ErrorCategory {
        match e {
            DriveError::QuotaExceeded => ErrorCategory::Quota,
            DriveError::DailyUploadLimit => ErrorCategory::UploadLimit,
            DriveError::PermissionDenied => ErrorCategory::NoPermission,
            DriveError::TooLarge => ErrorCategory::TooLarge,
            DriveError::NotFound => ErrorCategory::CloudMissing,
            DriveError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ErrorCategory::LocalMissing
            }
            _ => ErrorCategory::Unknown,
        }
    }

    /// A categoria de um erro local da fila (banco, disco ou o próprio Drive).
    pub fn of_engine(e: &EngineError) -> ErrorCategory {
        match e {
            EngineError::Drive(e) => ErrorCategory::of(e),
            EngineError::Io(e) if e.kind() == std::io::ErrorKind::NotFound => {
                ErrorCategory::LocalMissing
            }
            _ => ErrorCategory::Unknown,
        }
    }
}

/// De onde vem o erro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorSource {
    /// Item com envio que falhou; o caminho é a partir do Meu Drive.
    Item { id: ItemId, path: String },
    /// Cópia numa pasta de cópias guardadas.
    Kept(PathBuf),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyncError {
    pub category: ErrorCategory,
    pub name: String,
    /// O erro técnico (vazio nas cópias do `Lost and Found`).
    pub message: String,
    pub source: ErrorSource,
}

/// Os erros da conta, por categoria e nome.
pub fn list(store: &Store, lost_and_found: &KeptCopies) -> Result<Vec<SyncError>> {
    let mut errors = vec![];
    for e in store.job_errors()? {
        // Item de Drive compartilhado: o caminho ainda sai sem a raiz (tarefa 9 da 0.8).
        let path = store
            .path_of(&e.item)?
            .map_or_else(|| e.name.clone(), |p| p.path);
        errors.push(SyncError {
            category: ErrorCategory::parse(&e.category),
            name: e.name,
            message: e.message,
            source: ErrorSource::Item { id: e.item, path },
        });
    }
    for copy in lost_and_found.list() {
        errors.push(SyncError {
            category: ErrorCategory::LostAndFound,
            name: copy
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into(),
            message: String::new(),
            source: ErrorSource::Kept(copy),
        });
    }
    errors.sort_by(|a, b| (a.category, &a.name).cmp(&(b.category, &b.name)));
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use gdl_drive::client::DriveError;

    use super::*;

    #[test]
    fn drive_errors_map_to_categories() {
        for (error, category) in [
            (DriveError::QuotaExceeded, ErrorCategory::Quota),
            (DriveError::DailyUploadLimit, ErrorCategory::UploadLimit),
            (DriveError::PermissionDenied, ErrorCategory::NoPermission),
            (DriveError::TooLarge, ErrorCategory::TooLarge),
            (DriveError::NotFound, ErrorCategory::CloudMissing),
            (
                DriveError::Io(std::io::ErrorKind::NotFound.into()),
                ErrorCategory::LocalMissing,
            ),
            (DriveError::Http(400), ErrorCategory::Unknown),
        ] {
            assert_eq!(ErrorCategory::of(&error), category, "{error:?}");
            assert_eq!(ErrorCategory::parse(category.as_str()), category);
        }
        assert_eq!(ErrorCategory::LostAndFound.as_str(), "lost-and-found");
        let missing = EngineError::Io(std::io::ErrorKind::NotFound.into());
        assert_eq!(
            ErrorCategory::of_engine(&missing),
            ErrorCategory::LocalMissing
        );
        let db = EngineError::NotIndexed;
        assert_eq!(ErrorCategory::of_engine(&db), ErrorCategory::Unknown);
        assert_eq!(ErrorCategory::parse("?"), ErrorCategory::Unknown);
    }
}
