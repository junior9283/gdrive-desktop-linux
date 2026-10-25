//! Fila de envio persistente: sobe as edições locais, detecta conflitos e guarda no
//! `Lost and Found` o que não pode subir.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use gdl_common::names;
use gdl_common::temp_files::is_local_only;
use gdl_common::time::now_ms;
use gdl_drive::client::{Backoff, DriveClient, DriveError, RemoteFile, SessionSlot};
use gdl_store::{
    Inode, Item, ItemId, ItemKind, LocalState, NewUploadJob, PinState, Store, UploadJob, UploadKind,
};
use tokio::sync::broadcast;

use crate::cache::BlockCache;
use crate::changes::merge;
use crate::events::{EngineEvent, LostReason};
use crate::indexer::{ACCOUNT_EMAIL, item_from_remote};
use crate::lost_found::LostAndFound;
use crate::{EngineError, Result};

/// Prefixo dos IDs de itens criados no computador que ainda não existem no Drive.
pub const LOCAL_ID_PREFIX: &str = "local:";
/// Sem internet, a fila tenta de novo depois disto (sem contar tentativa).
const OFFLINE_WAIT_MS: i64 = 30_000;
/// Pasta-mãe ainda não criada no Drive: espera por ela.
const PARENT_WAIT_MS: i64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UploadOutcome {
    /// Enviado; o ID é o do Drive (o temporário foi trocado).
    Uploaded(ItemId),
    /// A edição não pôde subir e foi guardada no `Lost and Found`.
    Preserved { path: PathBuf, reason: LostReason },
    /// Sem internet, ou pasta-mãe ainda local: tenta depois, sem contar tentativa.
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
pub(crate) fn enqueue_upload(store: &Store, id: &ItemId, next_try_ms: i64) -> Result<()> {
    let kind = if is_local_id(id) {
        UploadKind::Create
    } else {
        UploadKind::Update
    };
    store.enqueue(NewUploadJob {
        item: id.clone(),
        kind,
        next_try_ms,
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
    let mut item = store.get(id)?.ok_or(EngineError::NotFound)?;
    item.size = size;
    item.modified_ms = modified_ms;
    let local_only = item.local_state == LocalState::LocalOnly;
    // Um conflito já anotado continua conflito até a fila decidir.
    if !local_only && item.local_state != LocalState::Conflict {
        item.local_state = LocalState::Dirty;
    }
    store.upsert_item(&item)?;
    store.ranges_clear(id)?;
    if size > 0 {
        store.ranges_add(id, 0, size)?;
    }
    if !local_only {
        enqueue_upload(store, id, next_try_ms)?;
    }
    Ok(())
}

/// Na partida do serviço: toda edição pendente sem envio na fila (o serviço caiu entre gravar e
/// enfileirar) ganha um envio. Devolve quantos foram enfileirados.
pub fn recover_on_start(store: &Store, cache: &BlockCache) -> Result<usize> {
    let mut queued = 0;
    for id in store.dirty_items_without_job()? {
        if !cache.local_path(&id).exists() {
            tracing::warn!(item = %id.0, "edição pendente sem conteúdo local; nada a enviar");
            continue;
        }
        enqueue_upload(store, &id, 0)?;
        queued += 1;
    }
    Ok(queued)
}

/// Sessão retomável guardada no próprio envio da fila, para sobreviver a quedas e reinícios.
struct JobSession<'a> {
    store: &'a Store,
    job_id: i64,
    url: Mutex<Option<String>>,
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
}

pub struct UploadWorker {
    store: Arc<Store>,
    client: Arc<DriveClient>,
    cache: BlockCache,
    lost: LostAndFound,
    events: broadcast::Sender<EngineEvent>,
    retry: Backoff,
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
        }
    }

    /// Processa o próximo envio pronto; `None` quando a fila não tem nada para agora.
    pub async fn run_once(&self) -> Result<Option<UploadOutcome>> {
        let Some(job) = self.store.next_job(now_ms())? else {
            return Ok(None);
        };
        let item = self.store.get(&job.item)?;
        let outcome = match item {
            Some(item)
                if matches!(
                    item.local_state,
                    LocalState::Dirty | LocalState::Uploading | LocalState::Conflict
                ) =>
            {
                match job.kind {
                    UploadKind::Create => self.create(&job, item).await?,
                    UploadKind::Update => self.update(&job, item).await?,
                    // Renomear e mover sem internet chegam com o FUSE (T12).
                    UploadKind::Metadata => self.skip(&job)?,
                }
            }
            _ => self.skip(&job)?,
        };
        Ok(Some(outcome))
    }

    fn skip(&self, job: &UploadJob) -> Result<UploadOutcome> {
        self.store.finish_job(job.job_id)?;
        Ok(UploadOutcome::Skipped)
    }

    fn session<'a>(&'a self, job: &UploadJob) -> JobSession<'a> {
        JobSession {
            store: &self.store,
            job_id: job.job_id,
            url: Mutex::new(job.session_url.clone()),
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
        self.store
            .set_local_state(&item.id, LocalState::Uploading)?;
        let result = match item.kind {
            ItemKind::Folder => {
                self.client
                    .create_folder(&parent.0, &item.remote_name)
                    .await
            }
            _ => {
                let src = self.cache.local_path(&item.id);
                let session = self.session(job);
                self.client
                    .upload_new(
                        &parent.0,
                        &item.remote_name,
                        &src,
                        item.modified_ms,
                        &session,
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
        let src = self.cache.local_path(&item.id);
        let session = self.session(job);
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
        let mut item = self.store.get(id)?.ok_or(EngineError::NotFound)?;
        item.md5 = remote.md5.clone();
        item.base_md5 = remote.md5;
        item.head_revision = remote.head_revision.clone();
        item.base_revision = remote.head_revision;
        item.modified_ms = remote.modified_ms;
        // Gravado de novo durante o envio: continua pendente (outro envio já está na fila).
        if item.local_state == LocalState::Uploading {
            item.local_state = LocalState::Clean;
        }
        self.store.upsert_item(&item)?;
        self.store.finish_job(job.job_id)?;
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
        let now = now_ms();
        if matches!(e, DriveError::Offline) {
            self.store.postpone_job(job.job_id, now + OFFLINE_WAIT_MS)?;
            return Ok(UploadOutcome::Waiting);
        }
        let delay = self.retry.delay(job.attempts).as_millis() as i64;
        self.store.retry_job(job.job_id, now + delay)?;
        if !e.is_retryable() {
            let event = match e {
                DriveError::QuotaExceeded => EngineEvent::QuotaFull,
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
        assert_eq!(s.env.store.queue_stats().unwrap().0, 1);
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

        assert_eq!(s.run().await, Some(UploadOutcome::Waiting));

        let job = s.env.store.next_job(i64::MAX).unwrap().unwrap();
        assert_eq!(job.attempts, 0);
        assert!(job.next_try_ms > now_ms(), "espera antes de tentar de novo");
        assert_eq!(s.env.item(&id.0).local_state, LocalState::Dirty);

        s.env.fake.clear_faults();
        s.env.store.postpone_job(job.job_id, 0).unwrap();
        assert_eq!(s.run().await, Some(UploadOutcome::Uploaded(id.clone())));
        assert_eq!(s.env.fake.content(&id.0), b"novo");
    }

    #[tokio::test]
    async fn drop_during_resumable_upload_resumes() {
        let s = setup(Env::with_tries(1).await).await;
        let data: Vec<u8> = (0..20 * MIB).map(|n| (n % 251) as u8).collect();
        s.write_new(&s.env.root(), "grande.bin", &data);
        s.env
            .fake
            .inject(Fault::DropConnectionAfterBytes(10 * MIB as u64));

        assert_eq!(s.run().await, Some(UploadOutcome::Waiting));
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
}
