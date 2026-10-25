//! Fila de envio persistente: sobe as edições locais, detecta conflitos e guarda no
//! `Lost and Found` o que não pode subir.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use gdl_common::names;
use gdl_common::temp_files::is_local_only;
use gdl_common::time::now_ms;
use gdl_drive::client::{Backoff, DriveClient, DriveError, RemoteFile, SessionSlot};
use gdl_store::{
    Inode, Item, ItemId, ItemKind, LocalState, NewUploadJob, PinState, Store, UploadJob, UploadKind,
};
use tokio::sync::broadcast;

use crate::activity::{ActivityLog, ActivityState};
use crate::cache::BlockCache;
use crate::changes::merge;
use crate::events::{EngineEvent, LostReason};
use crate::ids::IdPool;
use crate::indexer::{ACCOUNT_EMAIL, item_from_remote};
use crate::lost_found::LostAndFound;
use crate::{EngineError, Result};

/// Prefixo dos IDs de itens criados no computador que ainda não existem no Drive.
pub const LOCAL_ID_PREFIX: &str = "local:";
/// Sem internet, a fila tenta de novo depois disto (sem contar tentativa).
const OFFLINE_WAIT_MS: i64 = 30_000;
/// Pasta-mãe ainda não criada no Drive: espera por ela.
const PARENT_WAIT_MS: i64 = 5_000;
/// Janela dos envios que entram na estimativa de tempo restante.
const RATE_WINDOW_MS: i64 = 30_000;

/// O que a fila de envio informa ao resumo: cota cheia e o ritmo recente dos envios.
#[derive(Default)]
pub(crate) struct UploadStats {
    quota_full: AtomicBool,
    /// Envios concluídos na janela: quando e quantos bytes.
    recent: Mutex<VecDeque<(i64, u64)>>,
}

impl UploadStats {
    pub fn quota_full(&self) -> bool {
        self.quota_full.load(Ordering::Relaxed)
    }

    fn mark_quota_full(&self) {
        self.quota_full.store(true, Ordering::Relaxed);
    }

    /// Os envios da janela que termina em `now` (os mais antigos saem).
    fn recent(&self, now: i64) -> std::sync::MutexGuard<'_, VecDeque<(i64, u64)>> {
        let mut recent = self.recent.lock().unwrap_or_else(|e| e.into_inner());
        while recent
            .front()
            .is_some_and(|(at, _)| now - at > RATE_WINDOW_MS)
        {
            recent.pop_front();
        }
        recent
    }

    fn uploaded(&self, bytes: u64, now: i64) {
        self.quota_full.store(false, Ordering::Relaxed);
        self.recent(now).push_back((now, bytes));
    }

    /// Segundos para enviar `pending` bytes no ritmo dos envios da janela (só envios: os
    /// downloads dos fixados não entram); `None` sem nada pendente ou sem amostras.
    pub fn remaining_secs(&self, pending: u64, now: i64) -> Option<u64> {
        if pending == 0 {
            return None;
        }
        let recent = self.recent(now);
        let first = recent.front()?.0;
        let bytes: u64 = recent.iter().map(|(_, b)| b).sum();
        let elapsed_ms = (now - first).max(1_000) as u64;
        Some(pending * elapsed_ms / bytes.max(1) / 1_000)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadOutcome {
    /// Enviado; o ID é o do Drive (o temporário foi trocado).
    Uploaded(ItemId),
    /// A edição não pôde subir e foi guardada no `Lost and Found`.
    Preserved { path: PathBuf, reason: LostReason },
    /// Sem internet: tenta quando a rede voltar, sem contar tentativa.
    Offline,
    /// Pasta-mãe ainda local: tenta depois, sem contar tentativa.
    Waiting,
    /// Falha passageira: tenta depois, pelo recuo.
    Retrying,
    /// Nada a enviar (item já sincronizado, só local ou removido).
    Skipped,
}

pub fn is_local_id(id: &ItemId) -> bool {
    id.0.starts_with(LOCAL_ID_PREFIX)
}

/// Espera entre fechar um arquivo e enviá-lo: editores gravam um temporário e o renomeiam por
/// cima do original logo em seguida, e o temporário não deve subir.
pub const UPLOAD_DELAY_MS: i64 = 2_000;

/// Enfileira o envio do conteúdo local: `Create` para item só local, `Update` para os do Drive.
/// `file_id`: ID já reservado para a criação.
pub(crate) fn enqueue_upload(
    store: &Store,
    id: &ItemId,
    next_try_ms: i64,
    file_id: Option<String>,
) -> Result<()> {
    let kind = if is_local_id(id) {
        UploadKind::Create
    } else {
        UploadKind::Update
    };
    store.enqueue(NewUploadJob {
        item: id.clone(),
        kind,
        next_try_ms,
        file_id,
    })?;
    Ok(())
}

/// Cria no banco um item novo feito no computador, com ID temporário. Travas e temporários de
/// editores ficam `LocalOnly` e nunca sobem.
pub fn create_local_item(
    store: &Store,
    parent: &ItemId,
    name: &str,
    kind: ItemKind,
) -> Result<Item> {
    let now = now_ms();
    let id = ItemId(format!("{LOCAL_ID_PREFIX}{:032x}", rand::random::<u128>()));
    store.upsert_item(&Item {
        id: id.clone(),
        inode: Inode::UNASSIGNED,
        parent: Some(parent.clone()),
        remote_name: names::to_remote(name),
        local_name: name.into(),
        kind,
        size: 0,
        md5: None,
        head_revision: None,
        base_revision: None,
        base_md5: None,
        modified_ms: now,
        created_ms: now,
        trashed: false,
        can_edit: true,
        local_state: if is_local_only(name) {
            LocalState::LocalOnly
        } else {
            LocalState::Dirty
        },
        pin: PinState::None,
    })?;
    store.recompute_local_names([parent])?;
    store.get(&id)?.ok_or(EngineError::NotFound)
}

/// Registra que o conteúdo local (já gravado em `BlockCache::local_path`) tem `size` bytes e
/// enfileira o envio. Chamado no último fechamento do arquivo ou num `fsync`.
pub fn commit_local_write(store: &Store, id: &ItemId, size: u64, modified_ms: i64) -> Result<()> {
    commit_local_write_at(store, id, size, modified_ms, 0)
}

/// Como `commit_local_write`, com o envio só a partir de `next_try_ms`.
pub fn commit_local_write_at(
    store: &Store,
    id: &ItemId,
    size: u64,
    modified_ms: i64,
    next_try_ms: i64,
) -> Result<()> {
    let local_only = store.modify(|batch| -> Result<bool> {
        let mut item = batch.get(id)?.ok_or(EngineError::NotFound)?;
        item.size = size;
        item.modified_ms = modified_ms;
        let local_only = item.local_state == LocalState::LocalOnly;
        // Um conflito já anotado continua conflito até a fila decidir.
        if !local_only && item.local_state != LocalState::Conflict {
            item.local_state = LocalState::Dirty;
        }
        batch.put(&item)?;
        batch.reset_upload_sessions(id)?;
        batch.ranges_clear(id)?;
        if size > 0 {
            batch.ranges_add(id, 0, size)?;
        }
        Ok(local_only)
    })?;
    if !local_only {
        enqueue_upload(store, id, next_try_ms, None)?;
    }
    Ok(())
}

/// Na partida do serviço: os envios que esperavam a rede tentam de novo na hora, e toda edição
/// pendente sem envio na fila (o serviço caiu entre gravar e enfileirar) ganha um envio. Devolve
/// quantos foram enfileirados.
pub fn recover_on_start(store: &Store, cache: &BlockCache) -> Result<usize> {
    store.wake_network_waiters(now_ms())?;
    let mut queued = 0;
    for id in store.dirty_items_without_job()? {
        if !cache.local_path(&id).exists() {
            tracing::warn!(item = %id.0, "edição pendente sem conteúdo local; nada a enviar");
            continue;
        }
        enqueue_upload(store, &id, 0, None)?;
        queued += 1;
    }
    Ok(queued)
}

/// Sessão retomável guardada no próprio envio da fila, para sobreviver a quedas e reinícios.
struct JobSession<'a> {
    store: &'a Store,
    job_id: i64,
    url: Mutex<Option<String>>,
    activity: &'a ActivityLog,
    inode: Inode,
}

impl SessionSlot for JobSession<'_> {
    fn load(&self) -> Option<String> {
        self.url.load()
    }

    fn store(&self, url: Option<&str>) {
        self.url.store(url);
        if let Err(e) = self.store.set_session_url(self.job_id, url) {
            tracing::warn!(erro = %e, "não deu para guardar a sessão de envio");
        }
    }

    fn progress(&self, sent: u64, total: u64) {
        let percent = (sent * 100 / total.max(1)).min(99) as u8;
        self.activity.progress(self.inode, percent);
    }
}

pub struct UploadWorker {
    store: Arc<Store>,
    client: Arc<DriveClient>,
    cache: BlockCache,
    lost: LostAndFound,
    events: broadcast::Sender<EngineEvent>,
    retry: Backoff,
    ids: IdPool,
    pub(crate) stats: Arc<UploadStats>,
    activity: Arc<ActivityLog>,
}

impl UploadWorker {
    pub fn new(
        store: Arc<Store>,
        client: Arc<DriveClient>,
        cache: BlockCache,
        lost: LostAndFound,
        events: broadcast::Sender<EngineEvent>,
    ) -> UploadWorker {
        UploadWorker {
            store,
            client,
            cache,
            lost,
            events,
            retry: Backoff::default(),
            ids: IdPool::default(),
            stats: Arc::default(),
            activity: Arc::default(),
        }
    }

    /// Registra os envios em `activity` (sem isto, num registro só desta fila).
    pub fn with_activity(self, activity: Arc<ActivityLog>) -> UploadWorker {
        UploadWorker { activity, ..self }
    }

    fn note(&self, item: &Item, state: ActivityState, percent: Option<u8>) {
        self.activity.note(&self.store, item, state, percent);
    }

    /// Processa o próximo envio pronto; `None` quando a fila não tem nada para agora.
    pub async fn run_once(&self) -> Result<Option<UploadOutcome>> {
        let Some(job) = self.store.next_job(now_ms())? else {
            return Ok(None);
        };
        let item = self.store.get(&job.item)?;
        let result = match item {
            // Na lixeira daqui antes de subir: restaurar enfileira de novo.
            Some(item) if item.trashed && is_local_id(&item.id) => self.skip(&job),
            Some(item)
                if matches!(
                    item.local_state,
                    LocalState::Dirty | LocalState::Uploading | LocalState::Conflict
                ) =>
            {
                match job.kind {
                    // Criar ou atualizar se decide agora: um envio enfileirado quando o item
                    // ainda era só local atualiza depois que outro envio o criou.
                    UploadKind::Create | UploadKind::Update if is_local_id(&item.id) => {
                        self.create(&job, item).await
                    }
                    UploadKind::Create | UploadKind::Update => self.update(&job, item).await,
                    // Renomear e mover sem internet chegam com o FUSE (T12).
                    UploadKind::Metadata => self.skip(&job),
                }
            }
            _ => self.skip(&job),
        };
        if result.is_err() {
            // Erro daqui (disco, banco): este envio vai para depois, pelo recuo, e a fila segue.
            let delay = self.retry.delay(job.attempts).as_millis() as i64;
            self.store.retry_job(job.job_id, now_ms() + delay)?;
            self.store.transition_local_state(
                &job.item,
                LocalState::Uploading,
                LocalState::Dirty,
            )?;
        }
        result.map(Some)
    }

    fn skip(&self, job: &UploadJob) -> Result<UploadOutcome> {
        self.store.finish_job(job.job_id)?;
        Ok(UploadOutcome::Skipped)
    }

    fn session<'a>(&'a self, job: &UploadJob, item: &Item) -> JobSession<'a> {
        JobSession {
            store: &self.store,
            job_id: job.job_id,
            url: Mutex::new(job.session_url.clone()),
            activity: &self.activity,
            inode: item.inode,
        }
    }

    async fn create(&self, job: &UploadJob, item: Item) -> Result<UploadOutcome> {
        let Some(parent) = item.parent.clone() else {
            return self.skip(job);
        };
        if is_local_id(&parent) {
            self.store
                .postpone_job(job.job_id, now_ms() + PARENT_WAIT_MS)?;
            return Ok(UploadOutcome::Waiting);
        }
        let file_id = match self.file_id(job).await {
            Ok(id) => id,
            Err(EngineError::Drive(e)) => return self.failed(job, item, e).await,
            Err(e) => return Err(e),
        };
        self.store
            .set_local_state(&item.id, LocalState::Uploading)?;
        self.note(&item, ActivityState::Uploading, Some(0));
        let result = match item.kind {
            ItemKind::Folder => {
                self.client
                    .create_folder(&parent.0, &item.remote_name, Some(&file_id))
                    .await
            }
            _ => {
                let src = self.cache.local_path(&item.id);
                let session = self.session(job, &item);
                self.client
                    .upload_new(
                        &parent.0,
                        &item.remote_name,
                        &src,
                        item.modified_ms,
                        &session,
                        Some(&file_id),
                    )
                    .await
            }
        };
        match result {
            Ok(remote) => {
                let id = ItemId(remote.id.clone());
                self.store.replace_id(&item.id, &id)?;
                self.cache.rename_local(&item.id, &id).await?;
                self.uploaded(job, &id, remote)
            }
            Err(e) => self.failed(job, item, e).await,
        }
    }

    /// ID reservado no Drive para um item novo.
    pub async fn reserve_id(&self) -> std::result::Result<String, DriveError> {
        self.ids.take(&self.client).await
    }

    /// O ID reservado para criar o item; o primeiro fica guardado na fila, para a criação
    /// repetida (por este ou outro envio do item, mesmo depois de reiniciar) não duplicar.
    async fn file_id(&self, job: &UploadJob) -> Result<String> {
        if let Some(id) = &job.file_id {
            return Ok(id.clone());
        }
        let id = match self.store.reserved_file_id(&job.item)? {
            Some(id) => id,
            None => self.reserve_id().await?,
        };
        self.store.set_job_file_id(job.job_id, &id)?;
        Ok(id)
    }

    async fn update(&self, job: &UploadJob, item: Item) -> Result<UploadOutcome> {
        // Antes de mandar, confere se a nuvem mudou desde a última sincronização.
        let remote = match self.client.get(&item.id.0).await {
            Ok(remote) => remote,
            Err(e) => return self.failed(job, item, e).await,
        };
        if remote.trashed {
            return self
                .preserve(job, item, LostReason::RemoteDeleted, Some(remote))
                .await;
        }
        if remote.head_revision != item.base_revision && remote.md5 != item.base_md5 {
            return self
                .preserve(job, item, LostReason::RemoteChanged, Some(remote))
                .await;
        }
        self.store
            .set_local_state(&item.id, LocalState::Uploading)?;
        self.note(&item, ActivityState::Uploading, Some(0));
        let src = self.cache.local_path(&item.id);
        let session = self.session(job, &item);
        match self
            .client
            .upload_update(&item.id.0, &src, item.modified_ms, &session)
            .await
        {
            Ok(remote) => self.uploaded(job, &item.id, remote),
            Err(e) => self.failed(job, item, e).await,
        }
    }

    /// Envio aceito: a revisão da nuvem vira a nova revisão-base.
    fn uploaded(&self, job: &UploadJob, id: &ItemId, remote: RemoteFile) -> Result<UploadOutcome> {
        let item = self.store.modify(|batch| -> Result<Item> {
            let mut item = batch.get(id)?.ok_or(EngineError::NotFound)?;
            item.md5 = remote.md5.clone();
            item.base_md5 = remote.md5;
            item.head_revision = remote.head_revision.clone();
            item.base_revision = remote.head_revision;
            item.modified_ms = remote.modified_ms;
            // Gravado de novo durante o envio: continua pendente (outro envio já está na fila).
            if item.local_state == LocalState::Uploading {
                item.local_state = LocalState::Clean;
            }
            batch.put(&item)?;
            Ok(item)
        })?;
        self.store.finish_job(job.job_id)?;
        self.stats.uploaded(item.size, now_ms());
        self.note(&item, ActivityState::Uploaded, Some(100));
        Ok(UploadOutcome::Uploaded(id.clone()))
    }

    async fn failed(&self, job: &UploadJob, item: Item, e: DriveError) -> Result<UploadOutcome> {
        let lost = match e {
            DriveError::PermissionDenied => Some(LostReason::NoPermission),
            DriveError::NotFound => Some(LostReason::RemoteDeleted),
            _ => None,
        };
        if let Some(reason) = lost {
            return self.preserve(job, item, reason, None).await;
        }
        self.store
            .transition_local_state(&item.id, LocalState::Uploading, LocalState::Dirty)?;
        let state = match e {
            DriveError::Offline => ActivityState::UploadQueued,
            _ => ActivityState::UploadFailed,
        };
        self.note(&item, state, None);
        let now = now_ms();
        if matches!(e, DriveError::Offline) {
            self.store
                .wait_for_network(job.job_id, now + OFFLINE_WAIT_MS)?;
            return Ok(UploadOutcome::Offline);
        }
        let delay = self.retry.delay(job.attempts).as_millis() as i64;
        self.store.retry_job(job.job_id, now + delay)?;
        if !e.is_retryable() {
            let event = match e {
                DriveError::QuotaExceeded => {
                    self.stats.mark_quota_full();
                    EngineEvent::QuotaFull
                }
                _ => EngineEvent::UploadFailed {
                    name: item.local_name.clone(),
                    error: e.to_string(),
                },
            };
            // Sem ninguém ouvindo, o aviso pode se perder.
            let _ = self.events.send(event);
        }
        Ok(UploadOutcome::Retrying)
    }

    /// Guarda a edição local no `Lost and Found`, descarta-a e deixa o item como está na nuvem.
    async fn preserve(
        &self,
        job: &UploadJob,
        item: Item,
        reason: LostReason,
        remote: Option<RemoteFile>,
    ) -> Result<UploadOutcome> {
        self.note(&item, ActivityState::UploadFailed, None);
        let (lost, src) = (self.lost.clone(), self.cache.local_path(&item.id));
        let name = item.local_name.clone();
        let path = tokio::task::spawn_blocking(move || lost.preserve(&src, &name, reason))
            .await
            .map_err(std::io::Error::other)??;
        self.store.finish_job(job.job_id)?;
        self.cache.discard(&item.id).await?;
        if is_local_id(&item.id) {
            // Nunca chegou ao Drive: some do computador (a cópia está no Lost and Found).
            self.store.remove_item(&item.id)?;
        } else {
            self.restore(item, reason, remote).await?;
        }
        // Sem ninguém ouvindo, o aviso pode se perder.
        let _ = self.events.send(EngineEvent::LostAndFound {
            path: path.clone(),
            reason,
        });
        Ok(UploadOutcome::Preserved { path, reason })
    }

    /// Deixa o item como está na nuvem, sem a edição local descartada.
    async fn restore(
        &self,
        item: Item,
        reason: LostReason,
        remote: Option<RemoteFile>,
    ) -> Result<()> {
        // Sem permissão, a versão da nuvem ainda é legível; apagado, não há o que buscar.
        let remote = match remote {
            None if reason == LostReason::NoPermission => self.client.get(&item.id.0).await.ok(),
            remote => remote,
        };
        let old_parent = item.parent.clone();
        let clean = Item {
            local_state: LocalState::Clean,
            ..item
        };
        let mut restored = match remote {
            Some(remote) => {
                let email = self.store.kv_get(ACCOUNT_EMAIL)?.unwrap_or_default();
                merge(clean, item_from_remote(remote, &email)).0
            }
            None => clean,
        };
        restored.trashed |= reason == LostReason::RemoteDeleted;
        self.store.upsert_item(&restored)?;
        self.store
            .recompute_local_names(restored.parent.iter().chain(old_parent.iter()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use gdl_common::time::now_ms;
    use gdl_fake_drive::Fault;
    use gdl_store::{ItemId, ItemKind, LocalState, Store};
    use tokio::sync::broadcast;

    use super::*;
    use crate::cache::BlockCache;
    use crate::events::{EngineEvent, LostReason};
    use crate::indexer::initial_index;
    use crate::lost_found::LostAndFound;
    use crate::testutil::Env;

    const MIB: usize = 1024 * 1024;

    struct Setup {
        env: Env,
        cache: BlockCache,
        worker: UploadWorker,
        lost: LostAndFound,
        events: broadcast::Receiver<EngineEvent>,
    }

    fn worker_for(
        env: &Env,
        store: Arc<Store>,
        cache: BlockCache,
        lost: LostAndFound,
    ) -> (UploadWorker, broadcast::Receiver<EngineEvent>) {
        let (tx, events) = broadcast::channel(16);
        (
            UploadWorker::new(store, env.client.clone(), cache, lost, tx),
            events,
        )
    }

    /// Indexa o que o teste já pôs no Drive simulado e monta cache e fila.
    async fn setup(env: Env) -> Setup {
        initial_index(&env.store, &env.client).await.unwrap();
        let cache = env.cache_for(env.store.clone());
        let lost = LostAndFound::new(env.dir().join("lost_and_found"));
        let (worker, events) = worker_for(&env, env.store.clone(), cache.clone(), lost.clone());
        Setup {
            env,
            cache,
            worker,
            lost,
            events,
        }
    }

    impl Setup {
        /// Cria um arquivo novo no computador, como o FUSE faria.
        fn write_new(&self, parent: &ItemId, name: &str, bytes: &[u8]) -> ItemId {
            let item = create_local_item(&self.env.store, parent, name, ItemKind::File).unwrap();
            self.write(&item.id, bytes);
            item.id
        }

        /// Edita um arquivo que já está no Drive.
        async fn edit(&self, id: &ItemId, bytes: &[u8]) {
            self.cache.ensure_full(id).await.unwrap();
            self.write(id, bytes);
        }

        fn write(&self, id: &ItemId, bytes: &[u8]) {
            let path = self.cache.local_path(id);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            commit_local_write(&self.env.store, id, bytes.len() as u64, now_ms()).unwrap();
        }

        async fn run(&self) -> Option<UploadOutcome> {
            self.worker.run_once().await.unwrap()
        }

        fn preserved(&self) -> Vec<(PathBuf, Vec<u8>)> {
            self.lost
                .list()
                .into_iter()
                .map(|p| {
                    let bytes = std::fs::read(&p).unwrap();
                    (p, bytes)
                })
                .collect()
        }
    }

    #[tokio::test]
    async fn new_file_is_created_then_clean() {
        let s = setup(Env::new().await).await;
        let local = s.write_new(&s.env.root(), "novo.txt", b"ola");
        assert!(local.0.starts_with("local:"));
        let inode = s.env.item(&local.0).inode;

        let Some(UploadOutcome::Uploaded(id)) = s.run().await else {
            panic!("esperava envio");
        };

        assert!(!id.0.starts_with("local:"));
        assert_eq!(s.env.fake.content(&id.0), b"ola");
        assert_eq!(s.env.fake.meta(&id.0)["name"], "novo.txt");
        let item = s.env.item(&id.0);
        assert_eq!((item.inode, item.local_state), (inode, LocalState::Clean));
        assert!(item.head_revision.is_some());
        assert_eq!(item.base_revision, item.head_revision);
        assert!(s.env.store.get(&local).unwrap().is_none());
        assert_eq!(s.cache.read(&id, 0, 10).await.unwrap().as_ref(), b"ola");
        assert!(s.run().await.is_none(), "fila vazia");
    }

    #[tokio::test]
    async fn update_keeps_same_drive_id() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let s = setup(env).await;
        s.edit(&id, b"conteudo novo").await;

        assert_eq!(s.run().await, Some(UploadOutcome::Uploaded(id.clone())));

        assert_eq!(s.env.fake.content(&id.0), b"conteudo novo");
        let item = s.env.item(&id.0);
        assert_eq!(item.local_state, LocalState::Clean);
        assert_eq!(
            item.head_revision.as_deref(),
            s.env.fake.meta(&id.0)["headRevisionId"].as_str()
        );
        assert_eq!(item.base_revision, item.head_revision);
        assert_eq!(item.size, 13);
    }

    #[tokio::test]
    async fn dirty_content_survives_restart_and_uploads() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let s = setup(env).await;
        // O serviço morre depois de gravar e antes de enfileirar: só o estado ficou no banco.
        s.cache.ensure_full(&id).await.unwrap();
        std::fs::write(s.cache.local_path(&id), b"editado antes da queda").unwrap();
        let mut item = s.env.item(&id.0);
        item.local_state = LocalState::Dirty;
        item.size = 22;
        s.env.store.upsert_item(&item).unwrap();
        s.env.store.ranges_add(&id, 0, 22).unwrap();
        let Setup { env, .. } = s;

        let store = Arc::new(Store::open(&env.db_path()).unwrap());
        let cache = env.cache_for(store.clone());
        assert_eq!(recover_on_start(&store, &cache).unwrap(), 1);
        let lost = LostAndFound::new(env.dir().join("lost_and_found"));
        let (worker, _events) = worker_for(&env, store, cache, lost);

        assert_eq!(
            worker.run_once().await.unwrap(),
            Some(UploadOutcome::Uploaded(id.clone()))
        );
        assert_eq!(env.fake.content(&id.0), b"editado antes da queda");
    }

    #[tokio::test]
    async fn remote_change_during_pending_edit_goes_to_lost_and_found() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let mut s = setup(env).await;
        s.edit(&id, b"minha edicao local").await;
        s.env.fake.modify_remote(&id.0, b"da nuvem".to_vec());

        let Some(UploadOutcome::Preserved { path, reason }) = s.run().await else {
            panic!("esperava conflito");
        };

        assert_eq!(reason, LostReason::RemoteChanged);
        assert_eq!(
            s.preserved(),
            [(path.clone(), b"minha edicao local".to_vec())]
        );
        assert_eq!(
            s.env.fake.content(&id.0),
            b"da nuvem",
            "a nuvem não foi sobrescrita"
        );
        assert_eq!(s.env.item(&id.0).local_state, LocalState::Clean);
        assert_eq!(
            s.cache.read(&id, 0, 100).await.unwrap().as_ref(),
            b"da nuvem",
            "o caminho original fica com a versão da nuvem"
        );
        assert_eq!(
            s.events.try_recv().unwrap(),
            EngineEvent::LostAndFound {
                path,
                reason: LostReason::RemoteChanged
            }
        );
    }

    #[tokio::test]
    async fn remote_delete_during_pending_edit_goes_to_lost_and_found_and_is_not_recreated() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let s = setup(env).await;
        s.edit(&id, b"edicao").await;
        s.env.fake.trash_remote(&id.0);

        let outcome = s.run().await;

        assert!(matches!(
            outcome,
            Some(UploadOutcome::Preserved {
                reason: LostReason::RemoteDeleted,
                ..
            })
        ));
        assert_eq!(s.preserved()[0].1, b"edicao");
        assert!(s.env.item(&id.0).trashed);
        assert_eq!(s.env.fake.meta(&id.0)["trashed"], true);
        assert_eq!(s.env.fake.requests("/upload"), 0, "nada foi enviado");
        assert!(s.run().await.is_none());
    }

    #[tokio::test]
    async fn full_storage_reports_quota_full_and_keeps_the_job() {
        let env = Env::new().await;
        let mut s = setup(env).await;
        let root = ItemId(s.env.fake.root_id());
        s.write_new(&root, "grande.bin", b"dados");
        s.env.fake.inject(Fault::QuotaExceeded { times: 1 });

        let outcome = s.run().await;

        assert!(matches!(outcome, Some(UploadOutcome::Retrying)));
        assert_eq!(s.events.try_recv().unwrap(), EngineEvent::QuotaFull);
        assert_eq!(s.env.store.queue_stats().unwrap().pending, 1);
    }

    #[tokio::test]
    async fn permission_denied_goes_to_lost_and_found() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let s = setup(env).await;
        s.edit(&id, b"sem permissao").await;
        s.env.fake.inject(Fault::Status {
            code: 403,
            times: 1,
        });

        let outcome = s.run().await;

        assert!(matches!(
            outcome,
            Some(UploadOutcome::Preserved {
                reason: LostReason::NoPermission,
                ..
            })
        ));
        assert_eq!(s.preserved()[0].1, b"sem permissao");
        assert_eq!(s.env.fake.content(&id.0), b"velho");
        assert_eq!(s.env.item(&id.0).local_state, LocalState::Clean);
        assert_eq!(s.cache.read(&id, 0, 100).await.unwrap().as_ref(), b"velho");
    }

    #[tokio::test]
    async fn offline_queue_waits_without_burning_attempts() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let s = setup(env).await;
        s.edit(&id, b"novo").await;
        s.env.fake.inject(Fault::Offline);

        assert_eq!(s.run().await, Some(UploadOutcome::Offline));

        let job = s.env.store.next_job(i64::MAX).unwrap().unwrap();
        assert_eq!(job.attempts, 0);
        assert!(job.next_try_ms > now_ms(), "espera antes de tentar de novo");
        assert_eq!(s.env.item(&id.0).local_state, LocalState::Dirty);

        s.env.fake.clear_faults();
        s.env.store.postpone_job(job.job_id, 0).unwrap();
        assert_eq!(s.run().await, Some(UploadOutcome::Uploaded(id.clone())));
        assert_eq!(s.env.fake.content(&id.0), b"novo");
    }

    /// Bug: a resposta da criação se perdia, a nova tentativa criava o arquivo de novo e o Drive
    /// ficava com duas cópias.
    #[tokio::test]
    async fn lost_create_response_does_not_duplicate_the_file() {
        let s = setup(Env::new().await).await;
        s.write_new(&s.env.root(), "novo.txt", b"uma vez");
        s.env.fake.inject(Fault::LoseCreateResponses { times: 1 });

        let Some(UploadOutcome::Uploaded(id)) = s.run().await else {
            panic!("esperava envio");
        };

        assert_eq!(s.env.fake.find_live("novo.txt"), vec![id.0.clone()]);
        assert_eq!(s.env.fake.content(&id.0), b"uma vez");
    }

    /// O ID reservado fica no job: a criação que perdeu a resposta e só é repetida depois (rede
    /// fora, serviço reiniciado) também não duplica.
    #[tokio::test]
    async fn reserved_id_survives_in_the_queue_between_attempts() {
        let s = setup(Env::with_tries(1).await).await;
        s.write_new(&s.env.root(), "novo.txt", b"uma vez");
        s.env.fake.inject(Fault::LoseCreateResponses { times: 1 });
        assert_eq!(s.run().await, Some(UploadOutcome::Offline));
        let job = s.env.store.next_job(i64::MAX).unwrap().unwrap();
        s.env.store.postpone_job(job.job_id, 0).unwrap();

        let Some(UploadOutcome::Uploaded(id)) = s.run().await else {
            panic!("esperava envio");
        };

        assert_eq!(s.env.fake.find_live("novo.txt"), vec![id.0]);
    }

    /// Bug: com a rede de volta, o envio ainda esperava até 30 s.
    #[tokio::test]
    async fn network_return_wakes_uploads_waiting_for_it() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let s = setup(env).await;
        let (poller, _events) = s.env.poller();
        s.edit(&id, b"novo").await;
        s.env.fake.inject(Fault::Offline);
        assert_eq!(s.run().await, Some(UploadOutcome::Offline));
        poller.poll().await;

        s.env.fake.clear_faults();
        poller.poll().await;

        assert_eq!(s.run().await, Some(UploadOutcome::Uploaded(id.clone())));
        assert_eq!(s.env.fake.content(&id.0), b"novo");
    }

    /// Bug: depois de reiniciar, o envio que esperava a rede ainda esperava até 30 s.
    #[tokio::test]
    async fn restart_wakes_uploads_waiting_for_network() {
        let env = Env::new().await;
        let id = ItemId(
            env.fake
                .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec()),
        );
        let s = setup(env).await;
        s.edit(&id, b"novo").await;
        s.env.fake.inject(Fault::Offline);
        assert_eq!(s.run().await, Some(UploadOutcome::Offline));
        s.env.fake.clear_faults();

        recover_on_start(&s.env.store, &s.cache).unwrap();

        assert_eq!(s.run().await, Some(UploadOutcome::Uploaded(id.clone())));
    }

    #[tokio::test]
    async fn drop_during_resumable_upload_resumes() {
        let s = setup(Env::with_tries(1).await).await;
        let data: Vec<u8> = (0..20 * MIB).map(|n| (n % 251) as u8).collect();
        s.write_new(&s.env.root(), "grande.bin", &data);
        s.env
            .fake
            .inject(Fault::DropConnectionAfterBytes(10 * MIB as u64));

        assert_eq!(s.run().await, Some(UploadOutcome::Offline));
        let job = s.env.store.next_job(i64::MAX).unwrap().unwrap();
        assert!(job.session_url.is_some(), "a sessão ficou guardada na fila");

        s.env.fake.clear_faults();
        s.env.store.postpone_job(job.job_id, 0).unwrap();
        let Some(UploadOutcome::Uploaded(id)) = s.run().await else {
            panic!("esperava envio");
        };
        assert_eq!(s.env.fake.content(&id.0), data);
        assert_eq!(
            s.env
                .fake
                .requests("/upload/drive/v3/files?uploadType=resumable"),
            1,
            "continuou a mesma sessão"
        );
    }

    /// Bug: o segundo envio de um arquivo novo continuava `Create` depois que o primeiro criou o
    /// arquivo, e uma nova edição fazia ele criar outra cópia no Drive.
    #[tokio::test]
    async fn rewrite_during_create_upload_makes_one_drive_file() {
        let s = setup(Env::new().await).await;
        let local = s.write_new(&s.env.root(), "novo.txt", b"v1");
        s.write(&local, b"v2");
        let Some(UploadOutcome::Uploaded(id)) = s.run().await else {
            panic!("esperava envio");
        };

        s.write(&id, b"v3");
        while s.run().await.is_some() {}

        assert_eq!(s.env.fake.find_live("novo.txt"), vec![id.0.clone()]);
        assert_eq!(s.env.fake.content(&id.0), b"v3");
    }

    /// Bug: a sessão retomável interrompida continuava com o conteúdo gravado depois, e o Drive
    /// ficava com o começo da versão velha e o fim da nova.
    #[tokio::test]
    async fn rewrite_after_interrupted_resumable_restarts_session() {
        let s = setup(Env::with_tries(1).await).await;
        let old: Vec<u8> = (0..20 * MIB).map(|n| (n % 251) as u8).collect();
        let local = s.write_new(&s.env.root(), "grande.bin", &old);
        s.env
            .fake
            .inject(Fault::DropConnectionAfterBytes(10 * MIB as u64));
        assert_eq!(s.run().await, Some(UploadOutcome::Offline));
        s.env.fake.clear_faults();
        let interrupted = s.env.store.next_job(i64::MAX).unwrap().unwrap();
        assert!(interrupted.session_url.is_some());

        let new: Vec<u8> = (0..20 * MIB).map(|n| (n % 241) as u8).collect();
        s.write(&local, &new);
        s.env.store.postpone_job(interrupted.job_id, 0).unwrap();
        while s.run().await.is_some() {}

        let ids = s.env.fake.find_live("grande.bin");
        assert_eq!(ids.len(), 1);
        assert!(s.env.fake.content(&ids[0]) == new, "conteúdo misturado");
    }

    /// Bug: um erro local num envio (aqui, o Lost and Found sem onde gravar) deixava o envio
    /// sempre como o primeiro da fila, e nenhum outro saía.
    #[tokio::test]
    async fn local_error_in_one_upload_does_not_block_the_queue() {
        let env = Env::new().await;
        let root = env.fake.root_id();
        let broken = ItemId(env.fake.add_file(&root, "a.txt", b"v1".to_vec()));
        let fine = ItemId(env.fake.add_file(&root, "b.txt", b"v1".to_vec()));
        let s = setup(env).await;
        s.edit(&broken, b"local").await;
        s.edit(&fine, b"novo").await;
        s.env.fake.modify_remote(&broken.0, b"nuvem".to_vec());
        std::fs::write(s.env.dir().join("lost_and_found"), b"").unwrap();

        assert!(s.worker.run_once().await.is_err());

        assert_eq!(s.run().await, Some(UploadOutcome::Uploaded(fine.clone())));
        assert_eq!(s.env.fake.content(&fine.0), b"novo");
    }

    #[tokio::test]
    async fn editor_lock_files_never_upload() {
        let s = setup(Env::new().await).await;
        let root = s.env.root();
        let lock = s.write_new(&root, ".~lock.relatorio.odt#", b"trava");
        let swap = s.write_new(&root, "foo.swp", b"swap");

        assert!(s.run().await.is_none());

        assert_eq!(s.env.fake.requests("/upload"), 0);
        for id in [lock, swap] {
            assert_eq!(s.env.item(&id.0).local_state, LocalState::LocalOnly);
        }
        assert!(s.env.store.lookup(&root, "foo.swp").unwrap().is_some());
    }

    #[test]
    fn remaining_time_uses_the_recent_rate() {
        let stats = UploadStats::default();
        assert_eq!(stats.remaining_secs(1_000, 0), None);
        // 2 MB em 2 s: 1 MB/s.
        stats.uploaded(1_000_000, 0);
        stats.uploaded(1_000_000, 2_000);
        assert_eq!(stats.remaining_secs(10_000_000, 2_000), Some(10));
        // Envios fora da janela não contam mais.
        assert_eq!(
            stats.remaining_secs(10_000_000, 2_000 + RATE_WINDOW_MS + 1),
            None
        );
    }
}
