//! O que a montagem mostra: as pastas da raiz, o modo de cada item, o estado de sincronização e o
//! endereço na web (atributos estendidos).

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use gdl_common::i18n::I18n;
use gdl_engine::gdocs::web_url;
use gdl_engine::kept::KeptFolder;
use gdl_store::{DrivePath, DriveRoot, Inode, Item, ItemKind, LocalState, PinState, Store};

use crate::inodes::Node;

/// Atributos estendidos somente leitura de cada item, com o valor de cada um.
type XattrValue = fn(&Item, &Store) -> Option<String>;

pub(crate) const XATTRS: [(&str, XattrValue); 3] = [
    ("user.gdl.id", |item, _| Some(item.id.0.clone())),
    ("user.gdl.state", |item, store| {
        let state = item_state(item, store);
        let state = state.inspect_err(|e| tracing::warn!(erro = %e, "estado do item"));
        Some(state.ok()?.into())
    }),
    ("user.gdl.web_url", |item, _| web_url(item)),
];

/// Nomes das pastas da raiz no idioma do sistema.
#[derive(Clone)]
pub struct Layout {
    pub my_drive: String,
    pub shared_drives: String,
    pub computers: String,
    pub trash: String,
    pub lost_found: String,
    pub canceled_uploads: String,
    pub shortcut_targets: String,
}

impl Layout {
    pub fn new(i18n: &I18n) -> Layout {
        Layout {
            my_drive: i18n.tr("folder-my-drive", &[]),
            shared_drives: i18n.tr("folder-shared-drives", &[]),
            computers: i18n.tr("computers-folder", &[]),
            trash: i18n.tr("folder-trash", &[]),
            lost_found: i18n.tr("folder-lost-found", &[]),
            canceled_uploads: i18n.tr("folder-canceled-uploads", &[]),
            shortcut_targets: i18n.tr("folder-shortcut-targets", &[]),
        }
    }

    /// As pastas da raiz; sem "Meu Drive" no modo Espelho (`mirrored`), quando ele fica numa
    /// pasta do computador.
    pub(crate) fn root_entries(&self, mirrored: bool) -> Vec<(&str, Node)> {
        let my_drive = (!mirrored).then_some((self.my_drive.as_str(), Node::Item(Inode::ROOT)));
        my_drive
            .into_iter()
            .chain([
                (self.shared_drives.as_str(), Node::SharedDrives),
                (&self.trash, Node::Trash),
                (&self.lost_found, Node::Kept(KeptFolder::LostAndFound)),
                (
                    &self.canceled_uploads,
                    Node::Kept(KeptFolder::CanceledUploads),
                ),
                (&self.shortcut_targets, Node::ShortcutTargets),
            ])
            .collect()
    }

    /// A pasta da raiz chamada `name`, com "Meu Drive" (o chamador confere o modo Espelho).
    pub(crate) fn lookup_root(&self, name: &OsStr) -> Option<Node> {
        self.root_entries(false)
            .into_iter()
            .find(|(n, _)| OsStr::new(n) == name)
            .map(|(_, node)| node)
    }

    /// A raiz do Drive compartilhado ligado cuja pasta se chama `dir`.
    pub fn shared_drive_root(store: &Store, dir: &str) -> gdl_store::Result<Option<Item>> {
        let roots = store.enabled_drive_roots()?;
        Ok(roots
            .into_iter()
            .find_map(|(name, root)| (name == dir).then_some(root)))
    }

    /// O caminho de `at` na montagem em `mount`: `Meu Drive/...` ou
    /// `Drives compartilhados/<nome>/...`.
    pub fn mounted(&self, mount: &Path, at: &DrivePath) -> PathBuf {
        let root = match &at.root {
            DriveRoot::MyDrive => PathBuf::from(&self.my_drive),
            DriveRoot::Computers => PathBuf::from(&self.computers),
            DriveRoot::Shared { name, .. } => PathBuf::from(&self.shared_drives).join(name),
        };
        let mut path = mount.join(root);
        if !at.path.is_empty() {
            path.push(&at.path);
        }
        path
    }
}

/// Permissões: pasta `0o755` e arquivo `0o644`, sem escrita quando a conta não pode editar ou o
/// item está na lixeira.
pub(crate) fn item_perm(item: &Item) -> u16 {
    let read_only = !item.can_edit || item.trashed;
    match (&item.kind, read_only) {
        (ItemKind::Shortcut { .. }, _) => 0o777,
        // Pasta na lixeira continua gravável para o dono: esvaziar a lixeira do Dolphin apaga o
        // conteúdo dela (o que só esquece a entrada); criar nela continua recusado.
        (ItemKind::Folder, false) => 0o755,
        (ItemKind::Folder, true) if item.can_edit => 0o755,
        (ItemKind::Folder, true) => 0o555,
        (_, false) => 0o644,
        (_, true) => 0o444,
    }
}

/// `user.gdl.state`: `conflict`, `error` (na lista de erros), `syncing` (edição local a enviar),
/// `offline` (fixado ou só local) ou `cloud`.
pub fn item_state(item: &Item, store: &Store) -> gdl_store::Result<&'static str> {
    Ok(match item.local_state {
        LocalState::Conflict => "conflict",
        _ if store.has_upload_error(&item.id)? => "error",
        LocalState::Dirty | LocalState::Uploading => "syncing",
        LocalState::LocalOnly => "offline",
        LocalState::Clean if item.pin != PinState::None => "offline",
        LocalState::Clean => "cloud",
    })
}

pub(crate) fn xattr_value(item: &Item, store: &Store, name: &OsStr) -> Option<String> {
    let (_, value) = XATTRS.iter().find(|(n, _)| OsStr::new(n) == name)?;
    value(item, store)
}
