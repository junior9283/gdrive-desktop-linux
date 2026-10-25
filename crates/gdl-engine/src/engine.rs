//! Fachada do motor: junta banco, cache, fixados, fila de envio e mudanças, e roda os laços do
//! serviço com pausa.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gdl_common::config::Preferences;
use gdl_common::paths::{Paths, create_private_dir};
use gdl_common::time::now_ms;
use gdl_drive::client::DriveClient;
use gdl_drive::model::About;
use gdl_store::Store;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;

use crate::Result;
use crate::activity::{Activity, ActivityLog, ActivityRow, ActivityState, MAX_ROWS};
use crate::cache::BlockCache;
use crate::changes::ChangePoller;
use crate::errors::{self, SyncError};
use crate::events::{EngineEvent, SyncSummary};
use crate::indexer::{ACCOUNT_EMAIL, START_PAGE_TOKEN, initial_index};
use crate::kept::{Kept, KeptCopies, KeptFolder};
use crate::pins::PinManager;
use crate::uploads::{CancelOutcome, UploadOutcome, UploadWorker, recover_on_start};

/// De quanto em quanto tempo cada laço do serviço roda.
#[derive(Debug, Clone, Copy)]
pub struct Intervals {
    pub changes: Duration,
    /// Espera quando a fila de envio está vazia.
    pub uploads_idle: Duration,
    /// Download dos itens fixados.
    pub maintenance: Duration,
    pub eviction: Duration,
}

impl Default for Intervals {
    fn default() -> Intervals {
        Intervals {
            changes: Duration::from_secs(15),
            uploads_idle: Duration::from_secs(1),
            maintenance: Duration::from_secs(30),
            eviction: Duration::from_secs(60),
        }
    }
}

pub struct Engine {
    pub(crate) store: Arc<Store>,
    pub(crate) client: Arc<DriveClient>,
    pub(crate) cache: BlockCache,
    pins: PinManager,
    pub(crate) uploads: UploadWorker,
    pub(crate) poller: ChangePoller,
    kept: Kept,
    data_dir: PathBuf,
    pub(crate) events: broadcast::Sender<EngineEvent>,
    intervals: Intervals,
    paused: watch::Sender<bool>,
    /// Já avisou desta pausa que uma gravação espera a sincronização voltar.
    saved_while_paused: AtomicBool,
    pub(crate) activity: Arc<ActivityLog>,
}

impl Engine {
    /// Abre o motor da conta `account`: banco, cache e `Lost and Found` ficam em
    /// `<dados>/<conta>` (o cache vai para `ContentCachePath/<conta>` quando configurado), e o
    /// que ficou pela metade antes de um desligamento volta para a fila.
    pub fn open(
        paths: &Paths,
        prefs: &Preferences,
        account: &str,
        client: DriveClient,
    ) -> Result<Engine> {
        let data_dir = paths.data_dir.join(account);
        let cache_dir: PathBuf = prefs
            .content_cache_path
            .as_ref()
            .map_or_else(|| data_dir.clone(), |p| p.join(account));
        create_private_dir(&data_dir)?;
        create_private_dir(&cache_dir)?;
        let store = Arc::new(Store::open(&data_dir.join("metadata.db"))?);
        let client = Arc::new(client);
        let (events, _) = broadcast::channel(64);
        let cache = BlockCache::new(
            store.clone(),
            client.clone(),
            cache_dir,
            prefs.content_cache_max_kbytes,
            prefs.min_free_disk_space_kbytes,
            events.clone(),
        );
        recover_on_start(&store, &cache)?;
        let kept = Kept::in_dir(&data_dir);
        let activity = Arc::new(ActivityLog::default());
        Ok(Engine {
            pins: PinManager::new(store.clone(), cache.clone()).with_activity(activity.clone()),
            uploads: UploadWorker::new(
                store.clone(),
                client.clone(),
                cache.clone(),
                kept.clone(),
                events.clone(),
            )
            .with_activity(activity.clone()),
            activity,
            poller: ChangePoller::new(store.clone(), client.clone(), cache.clone(), events.clone()),
            kept,
            data_dir,
            store,
            client,
            cache,
            events,
            intervals: Intervals::default(),
            paused: watch::Sender::new(false),
            saved_while_paused: AtomicBool::new(false),
        })
    }

    /// Troca os intervalos dos laços (nos testes, milissegundos).
    pub fn with_intervals(self, intervals: Intervals) -> Engine {
        Engine { intervals, ..self }
    }

    /// Indexa o Meu Drive se a conta ainda não foi indexada.
    pub async fn ensure_indexed(&self) -> Result<()> {
        if self.store.kv_get(START_PAGE_TOKEN)?.is_none() {
            initial_index(&self.store, &self.client).await?;
        }
        Ok(())
    }

    pub fn pins(&self) -> &PinManager {
        &self.pins
    }

    pub fn cache(&self) -> &BlockCache {
        &self.cache
    }

    pub fn store(&self) -> &Arc<Store> {
        &self.store
    }

    /// Pasta de dados da conta (banco, cópias guardadas, arquivos só locais).
    pub fn data_dir(&self) -> &std::path::Path {
        &self.data_dir
    }

    /// Pastas com os dados locais da conta: banco e cópias guardadas, e o cache.
    pub fn local_dirs(&self) -> [PathBuf; 2] {
        [self.data_dir.clone(), self.cache.dir().to_path_buf()]
    }

    pub fn lost_and_found(&self) -> &KeptCopies {
        self.kept.get(KeptFolder::LostAndFound)
    }

    /// As pastas de cópias guardadas (`Lost and Found` e `Uploads cancelados`).
    pub fn kept(&self) -> &Kept {
        &self.kept
    }

    /// Os erros da sincronização, por categoria e nome.
    pub fn errors(&self) -> Result<Vec<SyncError>> {
        errors::list(&self.store, self.lost_and_found())
    }

    /// "Tentar de novo": o envio do item vai para agora.
    pub fn retry(&self, id: &gdl_store::ItemId) -> Result<()> {
        Ok(self.store.retry_now(id)?)
    }

    /// Cancela o envio do arquivo (ver `UploadWorker::cancel`).
    pub async fn cancel_upload(&self, id: &gdl_store::ItemId) -> Result<CancelOutcome> {
        self.uploads.cancel(id).await
    }

    pub fn account_email(&self) -> Result<String> {
        Ok(self.store.kv_get(ACCOUNT_EMAIL)?.unwrap_or_default())
    }

    /// Conta e cota de armazenamento, direto do Drive.
    pub async fn about(&self) -> Result<About> {
        Ok(self.client.about().await?)
    }

    /// A foto da conta (`About::photo_link`).
    pub async fn fetch_photo(&self, link: &str) -> Result<bytes::Bytes> {
        Ok(self.client.fetch_photo(link).await?)
    }

    /// A atividade de sincronização, a mais recente primeiro. O que está na fila e ainda não
    /// começou aparece como `UploadQueued`, no topo.
    pub fn activity(&self) -> Result<Vec<ActivityRow>> {
        let mut rows = self.activity.rows();
        let next_seq = rows.first().map_or(1, |r| r.seq + 1);
        let shown: HashSet<_> = rows.iter().map(|r| r.inode).collect();
        let queued: Vec<ActivityRow> = self
            .store
            .pending_items()?
            .iter()
            .filter(|item| !shown.contains(&item.inode))
            .take(MAX_ROWS)
            .map(|item| ActivityRow {
                seq: next_seq,
                time_ms: item.modified_ms,
                activity: Activity::of(&self.store, item, ActivityState::UploadQueued, None),
            })
            .collect();
        rows.splice(0..0, queued);
        Ok(rows)
    }

    /// Muda a cada registro novo na atividade.
    pub fn activity_changes(&self) -> watch::Receiver<u64> {
        self.activity.subscribe()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<EngineEvent> {
        self.events.subscribe()
    }

    /// Pausa a sincronização: mudanças, envios e downloads de fixados param; abrir arquivos
    /// continua baixando sob demanda.
    pub fn pause(&self) {
        self.set_paused(true);
    }

    pub fn resume(&self) {
        self.set_paused(false);
    }

    pub fn is_paused(&self) -> bool {
        *self.paused.borrow()
    }

    fn set_paused(&self, paused: bool) {
        // `send_replace` avisa os laços, que acordam na hora em vez de no fim do intervalo.
        if self.paused.send_replace(paused) == paused {
            return;
        }
        self.saved_while_paused.store(false, Ordering::Relaxed);
        if let Ok(summary) = self.summary() {
            let _ = self.events.send(EngineEvent::SyncStateChanged(summary));
        }
    }

    /// Avisa (uma vez por pausa) que `name` foi gravado e só sobe quando a sincronização voltar.
    pub(crate) fn note_saved_while_paused(&self, name: &str) {
        if self.is_paused() && !self.saved_while_paused.swap(true, Ordering::Relaxed) {
            let _ = self
                .events
                .send(EngineEvent::SavedWhilePaused { name: name.into() });
        }
    }

    pub fn summary(&self) -> Result<SyncSummary> {
        let queue = self.store.queue_stats()?;
        let downloads = self.store.count_incomplete_pinned()?;
        let stats = &self.uploads.stats;
        Ok(SyncSummary {
            pending_uploads: queue.pending,
            paused: self.is_paused(),
            offline: self.poller.is_offline(),
            last_sync_ms: self.poller.last_sync_ms(),
            // O mesmo que `errors().len()`, sem montar os caminhos.
            errors: queue.failed + self.lost_and_found().count() as u32,
            // Sem nada na fila, a cota cheia não impede nada.
            quota_full: queue.pending > 0 && stats.quota_full(),
            syncing: queue.pending + downloads,
            remaining_secs: stats.remaining_secs(queue.bytes, now_ms()),
        })
    }

    /// Roda os laços do serviço até `shutdown` ser cancelado.
    pub async fn run(&self, shutdown: CancellationToken) {
        tokio::select! {
            () = shutdown.cancelled() => {}
            () = self.every(self.intervals.changes, true, || self.poller.poll()) => {}
            () = self.upload_loop() => {}
            () = self.every(self.intervals.maintenance, true, || self.maintain()) => {}
            () = self.every(self.intervals.eviction, false, || self.evict()) => {}
        }
    }

    /// Roda `work` a cada `interval`; com `pausable`, não roda enquanto pausado. Pausar ou
    /// retomar interrompe a espera.
    async fn every<F: Future<Output = ()>>(
        &self,
        interval: Duration,
        pausable: bool,
        mut work: impl FnMut() -> F,
    ) {
        loop {
            if !(pausable && self.is_paused()) {
                work().await;
            }
            self.sleep(interval).await;
        }
    }

    async fn sleep(&self, interval: Duration) {
        let mut paused = self.paused.subscribe();
        tokio::select! {
            () = tokio::time::sleep(interval) => {}
            _ = paused.changed() => {}
        }
    }

    async fn upload_loop(&self) {
        loop {
            // Sem rede, nenhum envio tenta: a próxima consulta bem-sucedida libera a fila.
            if !self.is_paused() && !self.poller.is_offline() {
                match self.uploads.run_once().await {
                    // O envio é o primeiro a ver a rede cair; a consulta de mudanças vê a volta.
                    Ok(Some(UploadOutcome::Offline)) => self.poller.report_offline(),
                    // Tinha trabalho: tenta o próximo sem esperar.
                    Ok(Some(_)) => continue,
                    Ok(None) => {}
                    Err(e) => tracing::warn!(erro = %e, "falha na fila de envio"),
                }
            }
            self.sleep(self.intervals.uploads_idle).await;
        }
    }

    async fn maintain(&self) {
        match self.pins.maintain_once().await {
            Err(crate::EngineError::Cache(e)) if e.is_transient() => {}
            Err(e) => tracing::warn!(erro = %e, "falha ao baixar itens fixados"),
            Ok(_) => {}
        }
    }

    async fn evict(&self) {
        if let Err(e) = self.cache.evict_if_needed().await {
            tracing::warn!(erro = %e, "falha ao liberar espaço do cache");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gdl_common::config::load_preferences;
    use gdl_common::paths::Paths;
    use gdl_common::time::now_ms;
    use gdl_store::ItemKind;

    use super::*;
    use crate::testutil::{fast_client, wait_until};
    use crate::uploads::{commit_local_write, create_local_item};

    fn fast() -> Intervals {
        Intervals {
            changes: Duration::from_millis(20),
            uploads_idle: Duration::from_millis(10),
            maintenance: Duration::from_millis(20),
            eviction: Duration::from_millis(50),
        }
    }

    /// Bug: banco, cache e Lost and Found eram criados com o modo padrão (0755/0644), legíveis
    /// por outros usuários num disco compartilhado.
    #[tokio::test]
    async fn account_data_is_private_to_the_user() {
        use std::os::unix::fs::PermissionsExt;
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a.txt", b"segredo".to_vec());
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let (mut prefs, _) = load_preferences(&paths).unwrap();
        let shared_disk = tmp.path().join("disco");
        prefs.content_cache_path = Some(shared_disk.clone());
        let account = paths.data_dir.join("teste@example.com");
        std::fs::create_dir_all(&account).unwrap();
        std::fs::set_permissions(&account, std::fs::Permissions::from_mode(0o755)).unwrap();

        let engine =
            Engine::open(&paths, &prefs, "teste@example.com", fast_client(&fake, 3)).unwrap();
        engine.ensure_indexed().await.unwrap();
        let id = gdl_store::ItemId(id);
        engine.cache.ensure_full(&id).await.unwrap();
        engine
            .lost_and_found()
            .preserve(&engine.cache.local_path(&id), "a.txt")
            .unwrap();

        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        for dir in [
            account.clone(),
            shared_disk.join("teste@example.com"),
            account.join("lost_and_found"),
        ] {
            assert_eq!(mode(&dir), 0o700, "{}", dir.display());
        }
        assert_eq!(mode(&engine.cache.local_path(&id)), 0o600);
    }

    #[tokio::test]
    async fn pause_stops_uploads_and_resume_continues() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let (prefs, _) = load_preferences(&paths).unwrap();
        let engine = Engine::open(&paths, &prefs, "teste@example.com", fast_client(&fake, 3))
            .unwrap()
            .with_intervals(fast());
        engine.ensure_indexed().await.unwrap();
        assert!(paths.data_dir.join("teste@example.com").is_dir());

        let root = gdl_store::ItemId(fake.root_id());
        let item = create_local_item(&engine.store, &root, "nota.txt", ItemKind::File).unwrap();
        let path = engine.cache.local_path(&item.id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"escrito pausado").unwrap();
        commit_local_write(&engine.store, &item.id, 15, now_ms()).unwrap();

        engine.pause();
        let engine = Arc::new(engine);
        let shutdown = CancellationToken::new();
        let task = tokio::spawn({
            let (engine, shutdown) = (engine.clone(), shutdown.clone());
            async move { engine.run(shutdown).await }
        });
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert_eq!(fake.requests("/upload"), 0, "pausado não envia");
        let summary = engine.summary().unwrap();
        assert!(summary.paused && engine.is_paused());
        assert_eq!(summary.pending_uploads, 1);

        engine.resume();
        wait_until("envio depois de retomar", || fake.requests("/upload") > 0).await;
        wait_until("fila vazia", || {
            engine.summary().unwrap().pending_uploads == 0
        })
        .await;
        assert!(engine.summary().unwrap().last_sync_ms.is_some());

        shutdown.cancel();
        tokio::time::timeout(Duration::from_secs(5), task)
            .await
            .expect("os laços param no cancelamento")
            .unwrap();
    }
}
