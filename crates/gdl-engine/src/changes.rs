//! Consulta periódica de mudanças no Drive (`changes.list`) e aplicação no banco.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;

use gdl_drive::client::{Change, DriveClient, DriveError};
use gdl_store::{Item, ItemId, LocalState, Store};
use tokio::sync::broadcast;

use crate::events::EngineEvent;
use crate::indexer::{ACCOUNT_EMAIL, START_PAGE_TOKEN, item_from_remote};
use crate::{EngineError, Result};

pub struct ChangePoller {
    store: Arc<Store>,
    client: Arc<DriveClient>,
    events: broadcast::Sender<EngineEvent>,
    interval: Duration,
}

/// Junta o item do banco com a versão nova da nuvem. Devolve o item e se o cache deve ser
/// descartado (a revisão mudou e não há edição local).
fn merge(old: Item, mut new: Item) -> (Item, bool) {
    new.inode = old.inode;
    new.pin = old.pin;
    if old.local_state == LocalState::Clean {
        let revised = new.head_revision != old.head_revision || new.md5 != old.md5;
        return (new, revised);
    }
    // Edição local pendente: tamanho, data e revisão-base continuam os da edição até a fila
    // decidir o que fazer (T9). Se a nuvem também mudou, é conflito.
    let remote_moved_on = new.trashed || new.head_revision != old.base_revision;
    let item = Item {
        size: old.size,
        modified_ms: old.modified_ms,
        base_revision: old.base_revision,
        base_md5: old.base_md5,
        local_state: if remote_moved_on {
            LocalState::Conflict
        } else {
            old.local_state
        },
        ..new
    };
    (item, false)
}

/// Item resultante de uma mudança (`new` é `None` quando o item foi apagado de vez ou deixou de
/// ser acessível) e se o cache cai. Anota as pastas cujos nomes locais precisam ser refeitos.
fn resolve(
    old: Option<Item>,
    new: Option<Item>,
    parents: &mut BTreeSet<ItemId>,
) -> Option<(Item, bool)> {
    let Some(old) = old else {
        // Apagado sem nunca ter sido visto: nada a fazer.
        let new = new?;
        parents.extend(new.parent.clone());
        return Some((new, false));
    };
    // Apagado de vez conta como lixeira: o item fica na lixeira local.
    let new = new.unwrap_or_else(|| Item {
        trashed: true,
        ..old.clone()
    });
    // Só nome, pasta e lixeira mexem nos nomes locais; uma edição de conteúdo não.
    if (&old.remote_name, &old.parent, old.trashed) != (&new.remote_name, &new.parent, new.trashed)
    {
        parents.extend(old.parent.clone());
        parents.extend(new.parent.clone());
    }
    Some(merge(old, new))
}

impl ChangePoller {
    pub fn new(
        store: Arc<Store>,
        client: Arc<DriveClient>,
        events: broadcast::Sender<EngineEvent>,
        interval: Duration,
    ) -> ChangePoller {
        ChangePoller {
            store,
            client,
            events,
            interval,
        }
    }

    /// Busca e aplica as mudanças desde a última consulta. Devolve quantas mudaram o banco.
    pub async fn run_once(&self) -> Result<usize> {
        let token = self
            .store
            .kv_get(START_PAGE_TOKEN)?
            .ok_or(EngineError::NotIndexed)?;
        let email = self.store.kv_get(ACCOUNT_EMAIL)?.unwrap_or_default();
        let (changes, next) = self.client.changes(&token).await?;
        // Itens alterados nesta rodada (e se o cache deles cai), gravados juntos no fim. Uma
        // segunda mudança do mesmo item parte do resultado da primeira.
        let mut pending: BTreeMap<ItemId, (Item, bool)> = BTreeMap::new();
        let mut parents = BTreeSet::new();
        let mut applied = 0;
        for change in changes {
            let (id, new) = match change {
                Change::Upsert(r) => (
                    ItemId(r.id.clone()),
                    Some(item_from_remote(*r, email.as_str())),
                ),
                Change::Removed(id) => (ItemId(id), None),
            };
            let (old, cleared) = match pending.remove(&id) {
                Some((item, clear)) => (Some(item), clear),
                None => (self.store.get(&id)?, false),
            };
            if let Some((item, clear)) = resolve(old, new, &mut parents) {
                pending.insert(id, (item, clear || cleared));
                applied += 1;
            }
        }
        let items: Vec<Item> = pending.values().map(|(item, _)| item.clone()).collect();
        self.store.upsert_items(&items)?;
        for (id, _) in pending.iter().filter(|(_, (_, clear))| *clear) {
            self.store.ranges_clear(id)?;
        }
        self.store.recompute_local_names(&parents)?;
        if next != token {
            self.store.kv_set(START_PAGE_TOKEN, &next)?;
        }
        Ok(applied)
    }

    /// Consulta a cada `interval` até o processo acabar, avisando quando a rede cai e volta.
    pub async fn run(&self) {
        let mut offline = false;
        loop {
            let result = self.run_once().await;
            let now_offline = matches!(result, Err(EngineError::Drive(DriveError::Offline)));
            if now_offline != offline {
                offline = now_offline;
                // Sem ninguém ouvindo, o aviso pode se perder.
                let _ = self.events.send(EngineEvent::Offline(offline));
            }
            if let Err(e) = result
                && !now_offline
            {
                tracing::warn!(erro = %e, "falha ao consultar mudanças no Drive");
            }
            tokio::time::sleep(self.interval).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gdl_fake_drive::Fault;

    use super::*;
    use crate::indexer::initial_index;
    use crate::testutil::Env;

    async fn indexed() -> (Env, ChangePoller) {
        let env = Env::new().await;
        let (tx, _rx) = broadcast::channel(16);
        let poller = ChangePoller::new(
            env.store.clone(),
            env.client.clone(),
            tx,
            Duration::from_secs(15),
        );
        (env, poller)
    }

    #[tokio::test]
    async fn remote_edit_updates_metadata_and_clears_cache() {
        let (env, poller) = indexed().await;
        let id = env
            .fake
            .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec());
        initial_index(&env.store, &env.client).await.unwrap();
        let before = env.item(&id);
        env.store.ranges_add(&ItemId(id.clone()), 0, 5).unwrap();

        env.fake.modify_remote(&id, b"conteudo novo".to_vec());
        assert_eq!(poller.run_once().await.unwrap(), 1);

        let after = env.item(&id);
        assert_eq!(after.size, 13);
        assert_eq!(after.inode, before.inode);
        assert_ne!(after.head_revision, before.head_revision);
        assert_eq!(after.base_revision, after.head_revision);
        assert_eq!(after.local_state, LocalState::Clean);
        assert_eq!(
            env.store.ranges_missing(&ItemId(id), 0, 5).unwrap(),
            [(0, 5)],
            "o cache da revisão antiga foi descartado"
        );
        assert_eq!(poller.run_once().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn remote_edit_over_dirty_item_marks_conflict() {
        let (env, poller) = indexed().await;
        let id = env
            .fake
            .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec());
        initial_index(&env.store, &env.client).await.unwrap();
        let before = env.item(&id);
        env.store.ranges_add(&ItemId(id.clone()), 0, 99).unwrap();
        env.make_dirty(&id, 99);

        env.fake.modify_remote(&id, b"da nuvem".to_vec());
        poller.run_once().await.unwrap();

        let after = env.item(&id);
        assert_eq!(after.local_state, LocalState::Conflict);
        assert_eq!(after.size, 99, "o tamanho é o da edição local");
        assert_eq!(after.base_revision, before.base_revision);
        assert_ne!(after.head_revision, before.head_revision);
        assert!(
            env.store
                .ranges_missing(&ItemId(id), 0, 99)
                .unwrap()
                .is_empty(),
            "a edição local continua no cache"
        );
    }

    #[tokio::test]
    async fn remote_trash_marks_item_trashed() {
        let (env, poller) = indexed().await;
        let root = env.fake.root_id();
        let clean = env.fake.add_file(&root, "limpo.txt", b"1".to_vec());
        let dirty = env.fake.add_file(&root, "editado.txt", b"2".to_vec());
        initial_index(&env.store, &env.client).await.unwrap();
        env.make_dirty(&dirty, 10);

        env.fake.trash_remote(&clean);
        env.fake.trash_remote(&dirty);
        assert_eq!(poller.run_once().await.unwrap(), 2);

        assert!(env.item(&clean).trashed);
        assert_eq!(env.item(&clean).local_state, LocalState::Clean);
        assert!(
            env.store
                .lookup(&env.root(), "limpo.txt")
                .unwrap()
                .is_none()
        );
        assert!(env.item(&dirty).trashed);
        assert_eq!(env.item(&dirty).local_state, LocalState::Conflict);
    }

    #[tokio::test]
    async fn remote_rename_and_new_items_update_local_names() {
        let (env, poller) = indexed().await;
        let root = env.fake.root_id();
        let old = env.fake.add_file(&root, "nota.txt", b"1".to_vec());
        initial_index(&env.store, &env.client).await.unwrap();

        let newer = env.fake.add_file(&root, "nota.txt", b"2".to_vec());
        poller.run_once().await.unwrap();
        let name = |id: &str| env.item(id).local_name;
        assert_eq!(
            (name(&old), name(&newer)),
            ("nota.txt".into(), "nota (1).txt".into())
        );
    }

    #[tokio::test]
    async fn offline_is_announced_once_and_cleared() {
        let env = Env::new().await;
        initial_index(&env.store, &env.client).await.unwrap();
        let (tx, mut rx) = broadcast::channel(16);
        let poller = Arc::new(ChangePoller::new(
            env.store.clone(),
            env.client.clone(),
            tx,
            Duration::from_millis(5),
        ));
        env.fake.inject(Fault::Offline);
        let task = tokio::spawn({
            let poller = poller.clone();
            async move { poller.run().await }
        });
        let mut next = async || {
            tokio::time::timeout(Duration::from_secs(5), rx.recv())
                .await
                .expect("evento dentro do prazo")
                .unwrap()
        };
        assert_eq!(next().await, EngineEvent::Offline(true));
        env.fake.clear_faults();
        assert_eq!(next().await, EngineEvent::Offline(false));
        task.abort();
    }
}
