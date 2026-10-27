//! O Espelho do Meu Drive: o Meu Drive sai da montagem e é sincronizado numa pasta do computador
//! (um par `Mirror` com a raiz do Meu Drive, que roda como as pastas do computador).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use gdl_engine::cache::{FreeSpace, StatvfsFreeSpace};
use gdl_engine::engine::Engine;
use gdl_folders::guard::{self, Refusal};
use gdl_folders::sync::remote_tree;
use gdl_ipc::{MirrorCheck, ServiceError};
use gdl_store::folders::{Direction, Pair, PairKind};
use gdl_store::{Inode, Item, ItemKind};

use super::folders::refusal_text;
use super::{Daemon, failed, unsynced};

impl Daemon {
    /// A política do administrador desliga o Espelho.
    pub(super) fn mirror_refused(&self) -> Result<(), ServiceError> {
        if self.prefs().disable_mirrored_my_drive {
            return Err(ServiceError::Refused("mirror-disabled".into()));
        }
        Ok(())
    }

    pub async fn mirror(&self, account: &str) -> Result<String, ServiceError> {
        let engine = self.account_engine(account).await?;
        let mirror = engine.store().mirror().map_err(failed)?;
        Ok(mirror.map_or_else(String::new, |p| p.local.display().to_string()))
    }

    pub async fn mirror_check(
        &self,
        account: &str,
        path: &str,
    ) -> Result<MirrorCheck, ServiceError> {
        let engine = self.account_engine(account).await?;
        let refusal = match self.mirror_place(path).await {
            Ok(_) => String::new(),
            Err(ServiceError::Refused(reason)) => reason,
            Err(e) => return Err(e),
        };
        let root = my_drive(&engine)?;
        let tree = remote_tree(engine.store(), &root.id, PairKind::Mirror).map_err(failed)?;
        let size = tree
            .iter()
            .filter(|(_, i)| i.kind == ItemKind::File)
            .map(|(_, i)| i.remote_size)
            .sum();
        let free = nearest_existing(Path::new(path))
            .and_then(|p| StatvfsFreeSpace.free_kbytes(&p).ok())
            .map_or(0, |kb| kb.saturating_mul(1024));
        Ok(MirrorCheck {
            refusal,
            size,
            free,
            pending: unsynced(&engine).map_err(failed)? as u64,
        })
    }

    pub async fn set_mirror(
        self: &Arc<Self>,
        account: &str,
        path: &str,
    ) -> Result<(), ServiceError> {
        let account = self.folder_account(account).await?;
        let store = account.engine.store();
        if store.mirror().map_err(failed)?.is_some() {
            return Err(ServiceError::InvalidArgs("mirror-exists".into()));
        }
        let local = self.mirror_place(path).await?;
        let made = local.clone();
        tokio::task::spawn_blocking(move || std::fs::create_dir_all(made))
            .await
            .map_err(failed)?
            .map_err(failed)?;
        let root = my_drive(&account.engine)?;
        let id = store
            .add_pair(&local, &root.id, PairKind::Mirror, Direction::TwoWay)
            .map_err(failed)?;
        self.restart_folder(&account, id).await
    }

    pub async fn set_stream(&self, account: &str, remove_old: bool) -> Result<(), ServiceError> {
        let account = self.folder_account(account).await?;
        let mirror = account.engine.store().mirror().map_err(failed)?;
        let Some(Pair { id, local, .. }) = mirror else {
            return Err(ServiceError::InvalidArgs("no-mirror".into()));
        };
        if let Some(sync) = account.runners.get(id)
            && sync.pending_uploads().await.map_err(failed)? > 0
        {
            return Err(ServiceError::Refused("mirror-pending".into()));
        }
        self.drop_pair(&account, id).await?;
        if remove_old {
            let trash = self.system_trash();
            tokio::task::spawn_blocking(move || match trash.put(&local) {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
                _ => Ok(()),
            })
            .await
            .map_err(failed)?
            .map_err(failed)?;
        }
        Ok(())
    }

    /// Onde o Espelho fica, se serve (senão `Refused` com o motivo): a política deixa, e a pasta
    /// (existente ou a criar) está fora das montagens, das pastas do computador e dos dados do
    /// app, e não num volume de rede.
    async fn mirror_place(&self, path: &str) -> Result<PathBuf, ServiceError> {
        self.mirror_refused()?;
        let taken = self.taken(None).await;
        let path = PathBuf::from(path);
        let place = tokio::task::spawn_blocking(move || {
            let path = planned(&path).ok_or(Refusal::Missing)?;
            if path.exists() && !path.is_dir() {
                return Err(Refusal::Missing);
            }
            if let Some(refusal) = guard::overlap(&path, &taken) {
                return Err(refusal);
            }
            let existing = nearest_existing(&path).ok_or(Refusal::Missing)?;
            if guard::network_fs(&existing) {
                return Err(Refusal::Network);
            }
            Ok(path)
        });
        place
            .await
            .unwrap_or(Err(Refusal::Unreadable))
            .map_err(|r| ServiceError::Refused(refusal_text(r).into()))
    }
}

/// A raiz do Meu Drive da conta.
fn my_drive(engine: &Engine) -> Result<Item, ServiceError> {
    let root = engine.store().by_inode(Inode::ROOT).map_err(failed)?;
    root.ok_or_else(|| ServiceError::NotFound("root".into()))
}

/// O caminho canônico de `path`, que pode ainda não existir (a pasta pai precisa existir).
fn planned(path: &Path) -> Option<PathBuf> {
    if let Ok(path) = path.canonicalize() {
        return Some(path);
    }
    Some(path.parent()?.canonicalize().ok()?.join(path.file_name()?))
}

/// A pasta mais próxima de `path` (ela mesma ou acima) que existe.
fn nearest_existing(path: &Path) -> Option<PathBuf> {
    path.ancestors().find(|p| p.exists()).map(Path::to_path_buf)
}
