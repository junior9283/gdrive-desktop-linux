//! Números de inode da montagem: os fixos da raiz virtual, os itens do banco (com deslocamento,
//! porque o banco usa o 1 para "Meu Drive") e os nós criados sob demanda.

use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::Mutex;

use gdl_store::{Inode, ItemId};

use crate::fs::lock;

const MOUNT_ROOT: u64 = 1;
const TRASH: u64 = 2;
const LOST_FOUND: u64 = 3;
const SHORTCUT_TARGETS: u64 = 4;
const KIO_TRASH: u64 = 5;
const KIO_FILES: u64 = 6;
const KIO_INFO: u64 = 7;
/// Item do banco com inode `n` aparece como `n + ITEM_BASE`; o intervalo abaixo fica para os
/// nós fixos.
const ITEM_BASE: u64 = 16;
/// Nós sob demanda (pastas de alvo de atalho, arquivos do Lost and Found) começam aqui, longe
/// de qualquer inode do banco.
const DYNAMIC_BASE: u64 = 1 << 56;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Node {
    MountRoot,
    Trash,
    LostFound,
    ShortcutTargets,
    /// `.shortcut-targets-by-id/<id>`: contém só o item alvo.
    TargetDir(ItemId),
    /// Arquivo dentro de `.Lost and Found`, pelo nome.
    LostFile(OsString),
    /// `.Trash-<uid>`, a lixeira do KIO na raiz da montagem, e as pastas `files/` e `info/`.
    KioTrash,
    KioFiles,
    KioInfo,
    /// Um `.trashinfo` dentro de `info/`, pelo nome.
    KioInfoFile(OsString),
    Item(Inode),
}

#[derive(Default)]
pub(crate) struct Inodes {
    dynamic: Mutex<Dynamic>,
}

#[derive(Default)]
struct Dynamic {
    nodes: Vec<Node>,
    numbers: HashMap<Node, u64>,
}

impl Inodes {
    pub fn node(&self, ino: u64) -> Option<Node> {
        Some(match ino {
            MOUNT_ROOT => Node::MountRoot,
            TRASH => Node::Trash,
            LOST_FOUND => Node::LostFound,
            SHORTCUT_TARGETS => Node::ShortcutTargets,
            KIO_TRASH => Node::KioTrash,
            KIO_FILES => Node::KioFiles,
            KIO_INFO => Node::KioInfo,
            n if (ITEM_BASE..DYNAMIC_BASE).contains(&n) => Node::Item(Inode(n - ITEM_BASE)),
            n if n >= DYNAMIC_BASE => {
                let index = usize::try_from(n - DYNAMIC_BASE).ok()?;
                lock(&self.dynamic).nodes.get(index)?.clone()
            }
            _ => return None,
        })
    }

    /// Número do nó; os sob demanda ganham um na primeira vez e o mantêm enquanto o FUSE roda.
    pub fn ino(&self, node: &Node) -> u64 {
        match node {
            Node::MountRoot => MOUNT_ROOT,
            Node::Trash => TRASH,
            Node::LostFound => LOST_FOUND,
            Node::ShortcutTargets => SHORTCUT_TARGETS,
            Node::KioTrash => KIO_TRASH,
            Node::KioFiles => KIO_FILES,
            Node::KioInfo => KIO_INFO,
            Node::Item(inode) => inode.0 + ITEM_BASE,
            Node::TargetDir(_) | Node::LostFile(_) | Node::KioInfoFile(_) => {
                let mut dynamic = lock(&self.dynamic);
                if let Some(&n) = dynamic.numbers.get(node) {
                    return n;
                }
                let n = DYNAMIC_BASE + dynamic.nodes.len() as u64;
                dynamic.nodes.push(node.clone());
                dynamic.numbers.insert(node.clone(), n);
                n
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_round_trip_and_never_collide() {
        let inodes = Inodes::default();
        let nodes = [
            Node::MountRoot,
            Node::Trash,
            Node::LostFound,
            Node::ShortcutTargets,
            Node::Item(Inode::ROOT),
            Node::Item(Inode(2)),
            Node::TargetDir(ItemId("alvo".into())),
            Node::LostFile("a.txt".into()),
            Node::KioTrash,
            Node::KioFiles,
            Node::KioInfo,
            Node::KioInfoFile("a.txt".into()),
        ];
        let numbers: Vec<u64> = nodes.iter().map(|n| inodes.ino(n)).collect();
        for (node, &n) in nodes.iter().zip(&numbers) {
            assert_eq!(inodes.node(n).as_ref(), Some(node));
        }
        let mut unique = numbers.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), numbers.len());
        assert_eq!(
            inodes.ino(&Node::TargetDir(ItemId("alvo".into()))),
            numbers[6]
        );
        assert_eq!(inodes.node(0), None);
    }
}
