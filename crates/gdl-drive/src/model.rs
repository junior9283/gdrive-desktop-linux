//! Tipos da Drive API v3 usados pelo app.

use gdl_common::time::parse_rfc3339;
use serde::Deserialize;

pub const FOLDER_MIME: &str = "application/vnd.google-apps.folder";
pub const SHORTCUT_MIME: &str = "application/vnd.google-apps.shortcut";

/// Campos pedidos à API para cada arquivo (sempre os mesmos, para o banco ficar completo).
pub const FILE_FIELDS: &str = "id,name,mimeType,parents,size,md5Checksum,headRevisionId,modifiedTime,\
                               createdTime,trashed,capabilities/canEdit,shortcutDetails,webViewLink";

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "RawFile")]
pub struct RemoteFile {
    pub id: String,
    pub name: String,
    pub mime_type: String,
    pub parents: Vec<String>,
    /// Só arquivos com conteúdo binário têm tamanho, md5 e revisão.
    pub size: Option<u64>,
    pub md5: Option<String>,
    pub head_revision: Option<String>,
    pub modified_ms: i64,
    pub created_ms: i64,
    pub trashed: bool,
    pub can_edit: bool,
    /// Atalho: ID e tipo do item apontado.
    pub shortcut_target: Option<(String, String)>,
    pub web_view_link: Option<String>,
}

/// Endereço da miniatura que o Google gera para um arquivo.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ThumbLink {
    pub id: String,
    #[serde(rename = "thumbnailLink")]
    pub link: String,
}

/// Item de "Compartilhados comigo", com quem compartilhou e quando.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(from = "RawShared")]
pub struct SharedFile {
    pub file: RemoteFile,
    /// Nome de quem é o dono.
    pub owner: String,
    pub shared_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Upsert(Box<RemoteFile>),
    /// Item apagado de vez ou que deixou de ser acessível.
    Removed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct About {
    pub email: String,
    pub display_name: String,
    /// Endereço da foto da conta; vazio quando não há.
    pub photo_link: String,
    /// `None` quando o armazenamento é ilimitado.
    pub quota_total: Option<u64>,
    pub quota_used: u64,
}

fn parse_ms(text: Option<&str>) -> i64 {
    text.and_then(parse_rfc3339).unwrap_or(0)
}

/// Número que a API manda como texto (`"size": "123"`).
pub(crate) fn parse_u64(text: Option<&str>) -> Option<u64> {
    text.and_then(|t| t.parse().ok())
}

/// Forma crua do arquivo na API (números e datas como texto), convertida em `RemoteFile`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFile {
    id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    mime_type: String,
    #[serde(default)]
    parents: Vec<String>,
    size: Option<String>,
    md5_checksum: Option<String>,
    head_revision_id: Option<String>,
    modified_time: Option<String>,
    created_time: Option<String>,
    #[serde(default)]
    trashed: bool,
    capabilities: Option<Capabilities>,
    shortcut_details: Option<ShortcutDetails>,
    web_view_link: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawShared {
    #[serde(flatten)]
    file: RemoteFile,
    shared_with_me_time: Option<String>,
    #[serde(default)]
    owners: Vec<Owner>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Owner {
    #[serde(default)]
    display_name: String,
}

impl From<RawShared> for SharedFile {
    fn from(raw: RawShared) -> SharedFile {
        SharedFile {
            file: raw.file,
            owner: raw
                .owners
                .into_iter()
                .next()
                .map(|o| o.display_name)
                .unwrap_or_default(),
            shared_ms: parse_ms(raw.shared_with_me_time.as_deref()),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Capabilities {
    #[serde(default)]
    can_edit: bool,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ShortcutDetails {
    target_id: String,
    #[serde(default)]
    target_mime_type: String,
}

impl From<RawFile> for RemoteFile {
    fn from(raw: RawFile) -> RemoteFile {
        RemoteFile {
            size: parse_u64(raw.size.as_deref()),
            modified_ms: parse_ms(raw.modified_time.as_deref()),
            created_ms: parse_ms(raw.created_time.as_deref()),
            can_edit: raw.capabilities.is_some_and(|c| c.can_edit),
            shortcut_target: raw
                .shortcut_details
                .map(|s| (s.target_id, s.target_mime_type)),
            id: raw.id,
            name: raw.name,
            mime_type: raw.mime_type,
            parents: raw.parents,
            md5: raw.md5_checksum,
            head_revision: raw.head_revision_id,
            trashed: raw.trashed,
            web_view_link: raw.web_view_link,
        }
    }
}
