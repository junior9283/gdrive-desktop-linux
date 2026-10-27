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
    /// Situação do conteúdo local em relação à nuvem. `LocalOnly`: temporário de editor, que
    /// nunca sobe.
    LocalState { Clean = 0, Dirty = 1, Uploading = 2, Conflict = 3, LocalOnly = 4 }
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
    /// Tamanho da versão da nuvem (a do `md5` e da `head_revision`).
    pub remote_size: u64,
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
    /// ID reservado no Drive para criar o item (`files.generateIds`).
    pub file_id: Option<String>,
}

/// O erro mais recente registrado nos envios de um item (`Store::retry_job` com erro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobError {
    pub item: ItemId,
    pub name: String,
    pub category: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewUploadJob {
    pub item: ItemId,
    pub kind: UploadKind,
    pub next_try_ms: i64,
    /// ID já reservado no Drive para a criação, se houver.
    pub file_id: Option<String>,
}

/// Miniatura já gravada no cache do freedesktop para um item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThumbRec {
    pub id: ItemId,
    /// Revisão do arquivo na hora da semeadura.
    pub revision: Option<String>,
    /// `modified_ms` do arquivo na hora da semeadura.
    pub modified_ms: i64,
    /// URI `file://` usada no nome do PNG, para apagá-lo depois.
    pub uri: String,
}

/// Um Drive compartilhado da conta (tabela `shared_drives`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredDrive {
    pub id: ItemId,
    pub name: String,
    /// Só os ligados têm itens no banco e aparecem na montagem.
    pub enabled: bool,
}

/// A raiz de onde um caminho (`DrivePath`) parte.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveRoot {
    MyDrive,
    /// Um Drive compartilhado ligado, com o nome que ele tem agora.
    Shared {
        id: ItemId,
        name: String,
    },
}

/// Caminho de um item dentro do Drive: a raiz e, a partir da pasta de cima dela, os nomes locais
/// (`Pasta/arquivo.txt`; vazio para a própria raiz).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DrivePath {
    pub root: DriveRoot,
    pub path: String,
}

impl DrivePath {
    /// `path` a partir da raiz do Meu Drive.
    pub fn my_drive(path: impl Into<String>) -> DrivePath {
        DrivePath {
            root: DriveRoot::MyDrive,
            path: path.into(),
        }
    }
}
