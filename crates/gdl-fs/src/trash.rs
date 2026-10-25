//! Lixeira do KIO na raiz da montagem (`.Trash-<uid>`, padrão freedesktop para lixeira por
//! volume). Mandar para `files/` é mandar para a lixeira do Drive; `info/` guarda os
//! `.trashinfo`, que são só locais. Esvaziar a lixeira do Dolphin esquece a entrada local, e o
//! item continua na lixeira do Drive (que o apaga depois de 30 dias): o app nunca exclui de vez.

use std::collections::{BTreeSet, HashMap};
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use gdl_store::ItemId;

use crate::fs::lock;

const TRASHINFO: &str = ".trashinfo";

pub(crate) struct KioTrash {
    /// `.Trash-<uid>`.
    pub name: String,
    info_dir: PathBuf,
    /// Nome em `files/` de cada item mandado para a lixeira por ela.
    files: Mutex<HashMap<OsString, ItemId>>,
}

impl KioTrash {
    pub fn new(uid: u32, data_dir: &Path) -> KioTrash {
        let info_dir = data_dir.join("kio-trash-info");
        if let Err(e) = std::fs::create_dir_all(&info_dir) {
            tracing::warn!(erro = %e, "não deu para criar a pasta dos .trashinfo");
        }
        KioTrash {
            name: format!(".Trash-{uid}"),
            info_dir,
            files: Mutex::default(),
        }
    }

    pub fn info_path(&self, name: &OsStr) -> PathBuf {
        self.info_dir.join(name)
    }

    pub fn info_dir(&self) -> &Path {
        &self.info_dir
    }

    pub fn remember(&self, name: &OsStr, id: ItemId) {
        lock(&self.files).insert(name.into(), id);
    }

    /// Esquece a entrada de `files/`; devolve o item que ela apontava.
    pub fn forget(&self, name: &OsStr) -> Option<ItemId> {
        lock(&self.files).remove(name)
    }

    /// Item lembrado para o nome; sem lembrança (depois de reiniciar), quem decide é o
    /// `.trashinfo` correspondente.
    pub fn remembered(&self, name: &OsStr) -> Option<ItemId> {
        lock(&self.files).get(name).cloned()
    }

    pub fn has_info(&self, name: &OsStr) -> bool {
        let mut info = name.to_owned();
        info.push(TRASHINFO);
        self.info_path(&info).is_file()
    }

    /// Nomes que podem estar em `files/`: os lembrados e os que têm `.trashinfo`.
    pub fn candidate_names(&self) -> BTreeSet<OsString> {
        let mut names: BTreeSet<OsString> = lock(&self.files).keys().cloned().collect();
        if let Ok(entries) = std::fs::read_dir(&self.info_dir) {
            names.extend(entries.filter_map(|e| {
                let name = e.ok()?.file_name();
                Some(name.to_str()?.strip_suffix(TRASHINFO)?.into())
            }));
        }
        names
    }
}
