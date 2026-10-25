//! Ambiente de teste: Drive simulado, banco temporário e cliente ligado aos dois.

use std::sync::Arc;
use std::time::Duration;

use gdl_drive::client::{Backoff, DriveClient};
use gdl_drive::oauth::{AccessTokenProvider, AuthError};
use gdl_fake_drive::{FakeDrive, TEST_ACCESS_TOKEN};
use gdl_store::{Item, ItemId, LocalState, Store};

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

pub(crate) struct Env {
    pub fake: FakeDrive,
    pub store: Arc<Store>,
    pub client: Arc<DriveClient>,
    _dir: tempfile::TempDir,
}

impl Env {
    pub async fn new() -> Env {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(Store::open(&dir.path().join("conta.db")).unwrap());
        let client = DriveClient::new(fake.base_url(), fake.upload_url(), Arc::new(TestToken))
            .with_backoff(Backoff {
                base: Duration::from_millis(1),
                max: Duration::from_millis(2),
                tries: 3,
            });
        Env {
            fake,
            store,
            client: Arc::new(client),
            _dir: dir,
        }
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
