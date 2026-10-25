//! O que a montagem mostra: as pastas da raiz, o modo de cada item, o estado de sincronização e o
//! endereço na web (atributos estendidos).

use std::ffi::OsStr;

use gdl_common::i18n::I18n;
use gdl_engine::gdocs::web_url;
use gdl_engine::kept::KeptFolder;
use gdl_store::{Inode, Item, ItemKind, LocalState, PinState};

use crate::inodes::Node;

/// Atributos estendidos somente leitura de cada item, com o valor de cada um.
type XattrValue = fn(&Item) -> Option<String>;

pub(crate) const XATTRS: [(&str, XattrValue); 3] = [
    ("user.gdl.id", |item| Some(item.id.0.clone())),
    ("user.gdl.state", |item| Some(item_state(item).into())),
    ("user.gdl.web_url", web_url),
];

/// Nomes das pastas da raiz no idioma do sistema.
pub(crate) struct Layout {
    pub my_drive: String,
    pub trash: String,
    pub lost_found: String,
    pub canceled_uploads: String,
    pub shortcut_targets: String,
}

impl Layout {
    pub fn new(i18n: &I18n) -> Layout {
        Layout {
            my_drive: i18n.tr("folder-my-drive", &[]),
            trash: i18n.tr("folder-trash", &[]),
            lost_found: i18n.tr("folder-lost-found", &[]),
            canceled_uploads: i18n.tr("folder-canceled-uploads", &[]),
            shortcut_targets: i18n.tr("folder-shortcut-targets", &[]),
        }
    }

    pub fn root_entries(&self) -> [(&str, Node); 5] {
        [
            (&self.my_drive, Node::Item(Inode::ROOT)),
            (&self.trash, Node::Trash),
            (&self.lost_found, Node::Kept(KeptFolder::LostAndFound)),
            (
                &self.canceled_uploads,
                Node::Kept(KeptFolder::CanceledUploads),
            ),
            (&self.shortcut_targets, Node::ShortcutTargets),
        ]
    }

    pub fn lookup_root(&self, name: &OsStr) -> Option<Node> {
        self.root_entries()
            .into_iter()
            .find(|(n, _)| OsStr::new(n) == name)
            .map(|(_, node)| node)
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

/// `user.gdl.state`: `conflict`, `syncing` (edição local a enviar), `offline` (fixado ou só
/// local) ou `cloud`.
pub(crate) fn item_state(item: &Item) -> &'static str {
    match item.local_state {
        LocalState::Conflict => "conflict",
        LocalState::Dirty | LocalState::Uploading => "syncing",
        LocalState::LocalOnly => "offline",
        LocalState::Clean if item.pin != PinState::None => "offline",
        LocalState::Clean => "cloud",
    }
}

pub(crate) fn xattr_value(item: &Item, name: &OsStr) -> Option<String> {
    let (_, value) = XATTRS.iter().find(|(n, _)| OsStr::new(n) == name)?;
    value(item)
}
