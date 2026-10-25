//! Fachada do motor: junta banco, cache, fixados, fila de envio e mudanças, e roda os laços do
//! serviço com pausa.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gdl_common::config::Preferences;
use gdl_common::paths::Paths;
use gdl_drive::client::DriveClient;
use gdl_store::Store;
use tokio::sync::{broadcast, watch};
use tokio_util::sync::CancellationToken;

use crate::Result;
use crate::cache::BlockCache;
use crate::changes::ChangePoller;
use crate::events::{EngineEvent, SyncSummary};
use crate::indexer::{START_PAGE_TOKEN, initial_index};
use crate::lost_found::LostAndFound;
use crate::pins::PinManager;
use crate::uploads::{UploadWorker, recover_on_start};

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
    store: Arc<Store>,
    client: Arc<DriveClient>,
    cache: BlockCache,
    pins: PinManager,
    uploads: UploadWorker,
    poller: ChangePoller,
    events: broadcast::Sender<EngineEvent>,
    intervals: Intervals,
    paused: watch::Sender<bool>,
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
        std::fs::create_dir_all(&data_dir)?;
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
        let lost = LostAndFound::new(data_dir.join("lost_and_found"));
        Ok(Engine {
            pins: PinManager::new(store.clone(), cache.clone()),
            uploads: UploadWorker::new(
                store.clone(),
                client.clone(),
                cache.clone(),
                lost,
                events.clone(),
            ),
            poller: ChangePoller::new(store.clone(), client.clone(), events.clone()),
            store,
            client,
            cache,
            events,
            intervals: Intervals::default(),
            paused: watch::Sender::new(false),
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
        if self.paused.send_replace(paused) != paused
            && let Ok(summary) = self.summary()
        {
            let _ = self.events.send(EngineEvent::SyncStateChanged(summary));
        }
    }

    pub fn summary(&self) -> Result<SyncSummary> {
        let (pending_uploads, errors) = self.store.queue_stats()?;
        Ok(SyncSummary {
            pending_uploads,
            paused: self.is_paused(),
            offline: self.poller.is_offline(),
            last_sync_ms: self.poller.last_sync_ms(),
            errors,
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
            if !self.is_paused() {
                match self.uploads.run_once().await {
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
