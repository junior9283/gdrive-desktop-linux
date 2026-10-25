//! Tipos guardados no banco.

use rusqlite::types::{FromSql, FromSqlError, FromSqlResult, ToSql, ToSqlOutput, ValueRef};

/// Enum gravado como inteiro, com o número de cada variante fixo no código: reordenar as
/// variantes não muda o formato do banco.
macro_rules! stored_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident = $n:literal),* $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name {
            $($variant = $n),*
        }

        impl ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<ToSqlOutput<'_>> {
                Ok((*self as i64).into())
            }
        }

        impl FromSql for $name {
            fn column_result(value: ValueRef<'_>) -> FromSqlResult<Self> {
                match value.as_i64()? {
                    $($n => Ok($name::$variant),)*
                    other => Err(FromSqlError::OutOfRange(other)),
                }
            }
        }
    };
}

/// ID do item no Drive.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ItemId(pub String);

/// Número do inode na montagem. O 1 é a raiz; `Inode(0)` pede ao banco que aloque um.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Inode(pub u64);

impl Inode {
    pub const ROOT: Inode = Inode(1);
    pub const UNASSIGNED: Inode = Inode(0);
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemKind {
    Folder,
    File,
    GoogleDoc {
        mime: String,
    },
    Shortcut {
        target_id: ItemId,
        target_mime: String,
    },
}

impl ItemKind {
    /// Extensão do arquivo-ponteiro local; só Documentos Google têm.
    pub fn pointer_extension(&self) -> Option<&'static str> {
        match self {
            ItemKind::GoogleDoc { mime } => gdl_common::gdocs::pointer_extension(mime),
            _ => None,
        }
    }

    /// Colunas `kind`, `mime` e `target_id` do banco.
    pub(crate) fn to_columns(&self) -> (&'static str, Option<&str>, Option<&str>) {
        match self {
            ItemKind::Folder => ("folder", None, None),
            ItemKind::File => ("file", None, None),
            ItemKind::GoogleDoc { mime } => ("gdoc", Some(mime), None),
            ItemKind::Shortcut {
                target_id,
                target_mime,
            } => ("shortcut", Some(target_mime), Some(&target_id.0)),
        }
    }

    pub(crate) fn from_columns(
        kind: &str,
        mime: Option<String>,
        target_id: Option<String>,
    ) -> Option<ItemKind> {
        Some(match kind {
            "folder" => ItemKind::Folder,
            "file" => ItemKind::File,
            "gdoc" => ItemKind::GoogleDoc { mime: mime? },
            "shortcut" => ItemKind::Shortcut {
                target_id: ItemId(target_id?),
                target_mime: mime?,
            },
            _ => return None,
        })
    }
}

stored_enum! {
    /// Situação do conteúdo local em relação à nuvem.
    LocalState { Clean = 0, Dirty = 1, Uploading = 2, Conflict = 3 }
}

stored_enum! {
    /// "Disponível off-line": marcado no próprio item ou herdado de uma pasta.
    PinState { None = 0, Pinned = 1, Inherited = 2 }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    pub id: ItemId,
    pub inode: Inode,
    pub parent: Option<ItemId>,
    pub remote_name: String,
    pub local_name: String,
    pub kind: ItemKind,
    pub size: u64,
    pub md5: Option<String>,
    pub head_revision: Option<String>,
    /// Revisão e md5 da última sincronização, para detectar conflitos.
    pub base_revision: Option<String>,
    pub base_md5: Option<String>,
    pub modified_ms: i64,
    pub created_ms: i64,
    pub trashed: bool,
    pub can_edit: bool,
    pub local_state: LocalState,
    pub pin: PinState,
}

stored_enum! {
    UploadKind { Create = 0, Update = 1, Metadata = 2 }
}

/// Envio pendente na fila persistente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploadJob {
    pub job_id: i64,
    pub item: ItemId,
    pub kind: UploadKind,
    pub attempts: u32,
    pub next_try_ms: i64,
    /// Sessão de upload retomável, quando já aberta.
    pub session_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUploadJob {
    pub item: ItemId,
    pub kind: UploadKind,
    pub next_try_ms: i64,
}
