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

/// Disco sempre com espaço.
struct Plenty;

impl FreeSpace for Plenty {
    fn free_kbytes(&self, _dir: &Path) -> std::io::Result<u64> {
        Ok(u64::MAX)
    }
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
        let client = DriveClient::new(fake.base_url(), fake.upload_url(), Arc::new(TestToken))
            .with_backoff(Backoff {
                base: Duration::from_millis(1),
                max: Duration::from_millis(2),
                tries,
            });
        Env {
            fake,
            store,
            client: Arc::new(client),
            _dir: dir,
        }
    }

    pub fn dir(&self) -> &Path {
        self._dir.path()
    }

    pub fn db_path(&self) -> PathBuf {
        self.dir().join("conta.db")
    }

    /// Cache sem limite de tamanho sobre `store`, na pasta do ambiente.
    pub fn cache_for(&self, store: Arc<Store>) -> BlockCache {
        let (tx, _) = broadcast::channel(16);
        BlockCache::with_free_space(
            store,
            self.client.clone(),
            self.dir().join("cache"),
            None,
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
