//! Operações pedidas pela montagem: pastas, lixeira, mover, atalhos, datas, salvar por cima e
//! trazer de volta do `Lost and Found`. As que mexem só em metadados vão direto ao Drive e a
//! resposta entra no banco pelo mesmo caminho das mudanças.

use std::io::Read;
use std::path::Path;

use gdl_common::names::{split_ext, to_remote};
use gdl_common::temp_files::is_local_only;
use gdl_common::time::now_ms;
use gdl_drive::client::{Change, DriveError};
use gdl_drive::model::RemoteFile;
use gdl_store::{Item, ItemId, ItemKind, LocalState};

use crate::activity::ActivityState;
use crate::changes::apply_changes;
use crate::engine::Engine;
use crate::events::EngineEvent;
use crate::gdocs::{is_pointer_extension, pointer_doc_id};
use crate::uploads::{
    UPLOAD_DELAY_MS, commit_local_write_at, create_local_item, enqueue_upload, is_local_id,
};
use crate::{EngineError, Result};

/// Maior ponteiro de Documento Google que vale a pena ler para ver se é cópia de um.
const POINTER_MAX_BYTES: u64 = 64 * 1024;

/// Quando o envio de uma gravação começa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Upload {
    /// Depois de `UPLOAD_DELAY_MS`: o temporário que o editor apaga logo em seguida nunca sobe.
    Soon,
    /// Na hora (`fsync`).
    Now,
}

/// Nunca chegou ao Drive: ID temporário ou trava/temporário de editor.
fn never_uploaded(item: &Item) -> bool {
    is_local_id(&item.id) || item.local_state == LocalState::LocalOnly
}

impl Engine {
    fn item(&self, id: &ItemId) -> Result<Item> {
        self.store.get(id)?.ok_or(EngineError::NotFound)
    }

    /// Grava no banco o item como o Drive o devolveu.
    fn applied(&self, remote: RemoteFile) -> Result<Item> {
        let id = ItemId(remote.id.clone());
        apply_changes(&self.store, vec![Change::Upsert(Box::new(remote))])?;
        self.item(&id)
    }

    /// Cria a pasta no Drive na hora; sem internet (ou dentro de uma pasta que ainda não subiu),
    /// cria só aqui e enfileira.
    pub async fn create_folder(&self, parent: &ItemId, name: &str) -> Result<Item> {
        // Com o ID reservado, a pasta cuja resposta se perdeu não é criada de novo pela fila.
        let mut file_id = None;
        if !is_local_id(parent) && !self.poller.is_offline() {
            let created = match self.uploads.reserve_id().await {
                Ok(id) => {
                    self.client
                        .create_folder(&parent.0, &to_remote(name), Some(file_id.insert(id)))
                        .await
                }
                Err(e) => Err(e),
            };
            match created {
                Ok(remote) => return self.applied(remote),
                Err(DriveError::Offline) => {}
                Err(e) => return Err(e.into()),
            }
        }
        let item = create_local_item(&self.store, parent, name, ItemKind::Folder)?;
        enqueue_upload(&self.store, &item.id, 0, file_id)?;
        Ok(item)
    }

    /// Arquivo novo, vazio, com ID temporário e o conteúdo local já criado.
    pub fn create_file(&self, parent: &ItemId, name: &str) -> Result<Item> {
        let item = create_local_item(&self.store, parent, name, ItemKind::File)?;
        self.cache.open_local(&item.id, true)?;
        Ok(item)
    }

    /// Registra o fim de uma gravação e agenda o envio. Cópia de Documento Google não existe no
    /// Drive: um arquivo novo com o conteúdo de um ponteiro vira atalho para o documento.
    pub async fn finish_write(
        &self,
        id: &ItemId,
        size: u64,
        modified_ms: i64,
        upload: Upload,
    ) -> Result<()> {
        let item = self.item(id)?;
        if let Some(target) = self.copied_pointer(&item) {
            let parent = item.parent.as_ref().ok_or(EngineError::NotFound)?;
            let (stem, _) = split_ext(&item.local_name);
            match self.create_shortcut(parent, stem, &target).await {
                Ok(shortcut) => {
                    // Com o atalho criado, o ponteiro nunca sobe (senão ficaria duplicado).
                    self.forget_local(&item).await?;
                    let _ = self.events.send(EngineEvent::CopiedAsShortcut {
                        name: shortcut.local_name,
                    });
                    return Ok(());
                }
                Err(e) => {
                    tracing::warn!(erro = %e, "não deu para criar o atalho; o ponteiro sobe como arquivo");
                }
            }
        }
        if self.commit_write(id, size, modified_ms, upload)? {
            self.note_saved_while_paused(&item.local_name);
        }
        Ok(())
    }

    /// `false` se o arquivo nunca sobe (ver `commit_local_write_at`).
    fn commit_write(
        &self,
        id: &ItemId,
        size: u64,
        modified_ms: i64,
        upload: Upload,
    ) -> Result<bool> {
        let next_try_ms = match upload {
            Upload::Soon => now_ms() + UPLOAD_DELAY_MS,
            Upload::Now => 0,
        };
        commit_local_write_at(&self.store, id, size, modified_ms, next_try_ms)
    }

    /// Documento de que `item` (arquivo novo com extensão de ponteiro) é cópia.
    fn copied_pointer(&self, item: &Item) -> Option<ItemId> {
        let (_, ext) = split_ext(&item.local_name);
        if !is_local_id(&item.id) || !is_pointer_extension(ext.trim_start_matches('.')) {
            return None;
        }
        let mut json = Vec::new();
        std::fs::File::open(self.cache.local_path(&item.id))
            .ok()?
            .take(POINTER_MAX_BYTES)
            .read_to_end(&mut json)
            .ok()?;
        pointer_doc_id(&json).map(ItemId)
    }

    /// Tira daqui um item que nunca subiu, com o conteúdo local.
    async fn forget_local(&self, item: &Item) -> Result<()> {
        self.cache.discard(&item.id).await?;
        self.store.remove_item(&item.id)?;
        self.store.recompute_local_names(item.parent.as_ref())?;
        Ok(())
    }

    /// Manda para a lixeira do Drive (nunca apaga de vez). O arquivo que ainda não subiu vai
    /// para a lixeira daqui (o envio espera; restaurar volta a enviar); travas, temporários e
    /// pastas que nunca subiram somem só daqui.
    pub async fn trash(&self, id: &ItemId) -> Result<()> {
        let item = self.item(id)?;
        if never_uploaded(&item) {
            return if item.local_state == LocalState::LocalOnly || item.kind == ItemKind::Folder {
                self.forget_local(&item).await
            } else {
                self.set_local_trashed(id, true)
            };
        }
        self.note(&item, ActivityState::Trashing);
        self.applied(self.client.set_trashed(&id.0, true).await?)?;
        self.note(&item, ActivityState::Trashed);
        Ok(())
    }

    pub async fn untrash(&self, id: &ItemId) -> Result<Item> {
        if is_local_id(id) {
            self.set_local_trashed(id, false)?;
            enqueue_upload(&self.store, id, now_ms() + UPLOAD_DELAY_MS, None)?;
            return self.item(id);
        }
        if let Ok(item) = self.item(id) {
            self.note(&item, ActivityState::Restoring);
        }
        let restored = self.applied(self.client.set_trashed(&id.0, false).await?)?;
        self.note(&restored, ActivityState::Restored);
        Ok(restored)
    }

    fn note(&self, item: &Item, state: ActivityState) {
        self.activity.note(&self.store, item, state, None);
    }

    /// Lixeira daqui, para o que ainda não existe no Drive.
    fn set_local_trashed(&self, id: &ItemId, trashed: bool) -> Result<()> {
        let parent = self.store.modify(|batch| -> Result<Option<ItemId>> {
            let item = batch.get(id)?.ok_or(EngineError::NotFound)?;
            let parent = item.parent.clone();
            batch.put(&Item { trashed, ..item })?;
            Ok(parent)
        })?;
        self.store.recompute_local_names(parent.as_ref())?;
        Ok(())
    }

    /// Esvaziar a lixeira: o que só estava na lixeira daqui (nunca subiu) é apagado; o que está
    /// na lixeira do Drive continua lá.
    pub async fn empty_from_trash(&self, id: &ItemId) -> Result<()> {
        match self.store.get(id)? {
            Some(item) if is_local_id(id) && item.trashed => self.forget_local(&item).await,
            _ => Ok(()),
        }
    }

    /// Renomeia e/ou move para outra pasta do Meu Drive.
    pub async fn move_item(&self, id: &ItemId, parent: &ItemId, name: &str) -> Result<Item> {
        let item = self.item(id)?;
        let remote_name = to_remote(name);
        if never_uploaded(&item) {
            // O envio pendente usa o nome e a pasta da hora em que subir. O nome decide se sobe:
            // o download terminado (`.crdownload` → nome final) passa a subir, e o que virou
            // nome de trava deixa de subir (a fila pula o que não está pendente).
            let old_parent = item.parent.clone();
            let starts_uploading = self.store.modify(|batch| -> Result<bool> {
                let item = batch.get(id)?.ok_or(EngineError::NotFound)?;
                let was_local_only = item.local_state == LocalState::LocalOnly;
                let local_only = is_local_only(name);
                let local_state = match (was_local_only, local_only) {
                    (true, false) => LocalState::Dirty,
                    (false, true) => LocalState::LocalOnly,
                    _ => item.local_state,
                };
                batch.put(&Item {
                    parent: Some(parent.clone()),
                    remote_name,
                    local_state,
                    ..item
                })?;
                Ok(was_local_only && !local_only)
            })?;
            if starts_uploading {
                enqueue_upload(&self.store, id, now_ms() + UPLOAD_DELAY_MS, None)?;
            }
            self.store
                .recompute_local_names(old_parent.iter().chain([parent]))?;
            crate::pins::reconcile(&self.store, id)?;
            return self.item(id);
        }
        if is_local_id(parent) {
            // A pasta destino ainda não existe no Drive.
            return Err(EngineError::Drive(DriveError::Offline));
        }
        let new_name = (remote_name != item.remote_name).then_some(remote_name.as_str());
        let old_parent = item.parent.as_ref().filter(|p| *p != parent);
        let add = old_parent.map(|_| parent.0.as_str());
        let remote = self
            .client
            .rename_move(&id.0, new_name, add, old_parent.map(|p| p.0.as_str()))
            .await?;
        self.applied(remote)
    }

    /// Atalho do Drive para `target` dentro de `parent`.
    pub async fn create_shortcut(
        &self,
        parent: &ItemId,
        name: &str,
        target: &ItemId,
    ) -> Result<Item> {
        let remote = self
            .client
            .create_shortcut(&parent.0, &to_remote(name), &target.0)
            .await?;
        self.applied(remote)
    }

    /// Documento Google vazio (`mime` do Google) dentro de `parent`.
    pub async fn create_google_file(
        &self,
        parent: &ItemId,
        name: &str,
        mime: &str,
    ) -> Result<Item> {
        let remote = self
            .client
            .create_google_file(&parent.0, &to_remote(name), mime)
            .await?;
        self.applied(remote)
    }

    /// Data de modificação escolhida pelo app. Com envio pendente, vai junto com o conteúdo;
    /// num item já sincronizado, vai direto ao Drive.
    pub async fn set_modified(&self, id: &ItemId, modified_ms: i64) -> Result<Item> {
        let item = self.item(id)?;
        if never_uploaded(&item) || item.local_state != LocalState::Clean {
            self.store.upsert_item(&Item {
                modified_ms,
                ..item
            })?;
            return self.item(id);
        }
        self.applied(self.client.set_modified_time(&id.0, modified_ms).await?)
    }

    /// Salvar por `rename` por cima: `dst` mantém o ID do Drive e recebe o conteúdo e o inode de
    /// `src`. A origem nunca vai ao Drive; se já tinha ido, vai para a lixeira.
    pub async fn save_over(&self, src: &ItemId, dst: &ItemId) -> Result<()> {
        let source = self.item(src)?;
        if !never_uploaded(&source) {
            self.client.set_trashed(&src.0, true).await?;
        }
        // Com a trava de `dst`: um download dele em andamento não grava no conteúdo novo.
        self.cache
            .locked(dst, || -> Result<()> {
                std::fs::rename(self.cache.local_path(src), self.cache.local_path(dst))?;
                self.store.take_over(src, dst)?;
                self.commit_write(dst, source.size, source.modified_ms, Upload::Soon)
                    .map(drop)
            })
            .await?;
        self.store.recompute_local_names(source.parent.as_ref())?;
        Ok(())
    }

    /// Traz um arquivo do `Lost and Found` de volta para uma pasta do Meu Drive como arquivo novo.
    pub async fn restore_lost(&self, file: &Path, parent: &ItemId, name: &str) -> Result<Item> {
        let item = create_local_item(&self.store, parent, name, ItemKind::File)?;
        let mut dst = self.cache.open_local(&item.id, true)?;
        let size = std::io::copy(&mut std::fs::File::open(file)?, &mut dst)?;
        self.commit_write(&item.id, size, now_ms(), Upload::Soon)?;
        tokio::fs::remove_file(file).await?;
        self.item(&item.id)
    }
}
