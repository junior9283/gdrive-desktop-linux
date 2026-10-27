//! Ambiente de teste: Drive simulado, banco temporário e cliente ligado aos dois.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gdl_drive::client::{Backoff, DriveClient};
use gdl_drive::oauth::{AccessTokenProvider, AuthError};
use gdl_fake_drive::{FakeDrive, TEST_ACCESS_TOKEN};
use gdl_store::{Item, ItemId, LocalState, Store};
use tokio::sync::broadcast;

use crate::cache::{BlockCache, FreeSpace};
use crate::changes::ChangePoller;
use crate::events::EngineEvent;

struct TestToken;

#[async_trait::async_trait]
impl AccessTokenProvider for TestToken {
    async fn access_token(&self) -> Result<String, AuthError> {
        Ok(TEST_ACCESS_TOKEN.into())
    }

    async fn refresh_now(&self, _stale: &str) -> Result<String, AuthError> {
        Ok(TEST_ACCESS_TOKEN.into())
    }
}

/// Acesso que o Google recusou: a renovação dá `invalid_grant`.
struct RevokedToken;

#[async_trait::async_trait]
impl AccessTokenProvider for RevokedToken {
    async fn access_token(&self) -> Result<String, AuthError> {
        Err(AuthError::Revoked)
    }

    async fn refresh_now(&self, _stale: &str) -> Result<String, AuthError> {
        Err(AuthError::Revoked)
    }

    fn signed_out(&self) -> tokio::sync::watch::Receiver<bool> {
        tokio::sync::watch::channel(true).1
    }
}

/// Disco sempre com espaço.
struct Plenty;

impl FreeSpace for Plenty {
    fn free_kbytes(&self, _dir: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
}

/// Cliente do Drive simulado com recuo de milissegundos.
pub(crate) fn fast_client(fake: &FakeDrive, tries: u32) -> DriveClient {
    DriveClient::new(fake.base_url(), fake.upload_url(), Arc::new(TestToken)).with_backoff(
        Backoff {
            base: Duration::from_millis(1),
            max: Duration::from_millis(2),
            tries,
        },
    )
}

/// Cliente cujo login o Google já recusou.
pub(crate) fn revoked_client(fake: &FakeDrive) -> Arc<DriveClient> {
    Arc::new(DriveClient::new(
        fake.base_url(),
        fake.upload_url(),
        Arc::new(RevokedToken),
    ))
}

pub(crate) struct Env {
    pub fake: FakeDrive,
    pub store: Arc<Store>,
    pub client: Arc<DriveClient>,
    _dir: tempfile::TempDir,
}

impl Env {
    pub async fn new() -> Env {
        Env::with_tries(3).await
    }

    /// Ambiente cujo cliente desiste depois de `tries` tentativas.
    pub async fn with_tries(tries: u32) -> Env {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&dir.path().join("conta.db")).unwrap());
        let client = Arc::new(fast_client(&fake, tries));
        Env {
            fake,
            store,
            client,
            _dir: dir,
        }
    }

    pub fn dir(&self) -> &Path {
        self._dir.path()
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir().join("conta.db")
    }

    /// Consulta de mudanças sobre o banco do ambiente, com o receptor dos avisos.
    pub fn poller(&self) -> (ChangePoller, broadcast::Receiver<EngineEvent>) {
        let (tx, rx) = broadcast::channel(16);
        let cache = self.cache_for(self.store.clone());
        let poller = ChangePoller::new(self.store.clone(), self.client.clone(), cache, tx);
        (poller, rx)
    }

    /// Cache sem limite de tamanho sobre `store`, na pasta do ambiente.
    pub fn cache_for(&self, store: Arc<Store>) -> BlockCache {
        self.cache_limited(store, None)
    }

    /// Cache com limite de `max_kbytes`.
    pub fn cache_limited(&self, store: Arc<Store>, max_kbytes: Option<u64>) -> BlockCache {
        let (tx, _) = broadcast::channel(16);
        BlockCache::with_free_space(
            store,
            self.client.clone(),
            self.dir().join("cache"),
            max_kbytes,
            512_000,
            tx,
            Arc::new(Plenty),
        )
    }

    pub fn item(&self, id: &str) -> Item {
        self.store.get(&ItemId(id.into())).unwrap().unwrap()
    }

    pub fn root(&self) -> ItemId {
        ItemId(self.fake.root_id())
    }

    /// Simula uma edição local ainda não enviada.
    pub fn make_dirty(&self, id: &str, local_size: u64) {
        let mut item = self.item(id);
        item.local_state = LocalState::Dirty;
        item.size = local_size;
        self.store.upsert_item(&item).unwrap();
    }
}

/// Motor completo sobre o Drive simulado, já indexado, com as preferências padrão.
pub(crate) async fn indexed_engine(fake: &FakeDrive) -> (crate::engine::Engine, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let paths = gdl_common::paths::Paths::for_test(tmp.path());
    let (prefs, _) = gdl_common::config::load_preferences(&paths).unwrap();
    let engine =
        crate::engine::Engine::open(&paths, &prefs, "teste@example.com", fast_client(fake, 3))
            .unwrap();
    engine.ensure_indexed().await.unwrap();
    (engine, tmp)
}

/// Espera até `done` ficar verdadeiro, falhando o teste depois de 10 s.
pub(crate) async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(10), async {
        while !done() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("esperando: {what}"));
}
