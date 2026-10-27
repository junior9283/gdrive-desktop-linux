//! Consulta periódica de mudanças no Drive (`changes.list`) e aplicação no banco.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use gdl_common::time::now_ms;
use gdl_drive::client::{Change, DriveClient, DriveError};
use gdl_store::{Item, ItemId, LocalState, Store};
use tokio::sync::broadcast;

use crate::cache::BlockCache;
use crate::drives::sync_drive_list;
use crate::events::EngineEvent;
use crate::indexer::{ACCOUNT_EMAIL, START_PAGE_TOKEN, item_from_remote};
use crate::pins::reconcile;
use crate::{EngineError, Result};

pub struct ChangePoller {
    store: Arc<Store>,
    client: Arc<DriveClient>,
    cache: BlockCache,
    events: broadcast::Sender<EngineEvent>,
    offline: AtomicBool,
    /// Hora da última consulta bem-sucedida; 0 enquanto não houve nenhuma.
    last_sync_ms: AtomicI64,
    /// Drives das mudanças que a lista de Drives não trouxe: sem procurar de novo a cada
    /// consulta enquanto a mudança se repete (limpa em `forget_searched_drives`).
    searched_drives: Mutex<BTreeSet<String>>,
    /// A lista de Drives já foi consultada nesta partida. A primeira consulta de mudanças a
    /// consulta também: pega o que mudou com o app fechado e o banco de quem veio da 0.7, que
    /// foi indexado sem Drives compartilhados.
    drives_listed: AtomicBool,
}

/// Junta o item do banco com a versão nova da nuvem. Devolve o item e se o cache deve ser
/// descartado (a revisão mudou e não há edição local).
pub(crate) fn merge(old: Item, mut new: Item) -> (Item, bool) {
    new.inode = old.inode;
    new.pin = old.pin;
    if old.local_state == LocalState::Clean {
        let revised = new.head_revision != old.head_revision || new.md5 != old.md5;
        return (new, revised);
    }
    // Edição local pendente: tamanho, data e revisão-base continuam os da edição até a fila
    // decidir o que fazer (T9). Se a nuvem também mudou, é conflito; durante o envio, não: a
    // revisão nova pode ser a do próprio envio, e o resultado dele decide (já conferiu antes).
    let remote_moved_on = old.local_state != LocalState::Uploading
        && (new.trashed || new.head_revision != old.base_revision);
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
    // Apagado de vez com edição local: conta como lixeira, e o merge decide (conflito).
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

/// Aplica ao banco mudanças vindas do Drive (da consulta periódica ou da resposta de uma operação
/// feita pela montagem). Devolve quantas mudaram o banco e os itens que saíram dele (apagados de
/// vez no Drive, sem edição local), cujo conteúdo em cache quem chamou descarta.
pub fn apply_changes(store: &Store, changes: Vec<Change>) -> Result<(usize, Vec<ItemId>)> {
    let email = store.kv_get(ACCOUNT_EMAIL)?.unwrap_or_default();
    let enabled_drives: BTreeSet<String> = store
        .shared_drives()?
        .into_iter()
        .filter(|d| d.enabled)
        .map(|d| d.id.0)
        .collect();
    let mut parents = BTreeSet::new();
    // Itens novos ou que mudaram de pasta: a fixação deles segue a pasta nova.
    let mut moved = BTreeSet::new();
    // Ler e gravar na mesma transação: uma gravação local no meio não é desfeita.
    let mut removed = vec![];
    let applied = store.modify(|batch| -> gdl_store::Result<usize> {
        // Itens alterados nesta rodada (e se o cache deles cai), gravados juntos no fim. Uma
        // segunda mudança do mesmo item parte do resultado da primeira.
        let mut pending: BTreeMap<ItemId, (Item, bool)> = BTreeMap::new();
        let mut applied = 0;
        for change in changes {
            let (id, new) = match change {
                // Item de Drive desligado (ou que foi para um): sai do banco, se estava nele.
                Change::Upsert(r)
                    if r.drive_id
                        .as_ref()
                        .is_some_and(|d| !enabled_drives.contains(d)) =>
                {
                    (ItemId(r.id), None)
                }
                Change::Upsert(r) => (
                    ItemId(r.id.clone()),
                    Some(item_from_remote(*r, email.as_str())),
                ),
                Change::Removed(id) => (ItemId(id), None),
                // A lista de Drives já foi consultada antes (`register_new_drives`).
                Change::Drive(_) => continue,
            };
            let (old, cleared) = match pending.remove(&id) {
                Some((item, clear)) => (Some(item), clear),
                None => (batch.get(&id)?, false),
            };
            // Apagado de vez sem edição local: sai do banco (e da lixeira daqui).
            if new.is_none()
                && let Some(gone) = old.as_ref().filter(|o| o.local_state == LocalState::Clean)
            {
                parents.extend(gone.parent.clone());
                batch.remove_item(&id)?;
                removed.push(id);
                applied += 1;
                continue;
            }
            let old_parent = old.as_ref().map(|o| o.parent.clone());
            if let Some((item, clear)) = resolve(old, new, &mut parents) {
                if old_parent.as_ref() != Some(&item.parent) {
                    moved.insert(id.clone());
                }
                pending.insert(id, (item, clear || cleared));
                applied += 1;
            }
        }
        let items: Vec<Item> = pending.values().map(|(item, _)| item.clone()).collect();
        batch.put_all(&items)?;
        for (id, _) in pending.iter().filter(|(_, (_, clear))| *clear) {
            batch.ranges_clear(id)?;
        }
        Ok(applied)
    })?;
    store.recompute_local_names(&parents)?;
    for id in &moved {
        reconcile(store, id)?;
    }
    Ok((applied, removed))
}

impl ChangePoller {
    pub fn new(
        store: Arc<Store>,
        client: Arc<DriveClient>,
        cache: BlockCache,
        events: broadcast::Sender<EngineEvent>,
    ) -> ChangePoller {
        ChangePoller {
            store,
            client,
            cache,
            events,
            offline: AtomicBool::new(false),
            last_sync_ms: AtomicI64::new(0),
            searched_drives: Mutex::default(),
            drives_listed: AtomicBool::new(false),
        }
    }

    pub fn is_offline(&self) -> bool {
        self.offline.load(Ordering::Relaxed)
    }

    /// Um envio deu com o Drive inacessível: sem internet até a próxima consulta bem-sucedida,
    /// que libera os envios que ficaram esperando a rede.
    pub fn report_offline(&self) {
        self.set_offline(true);
    }

    /// Avisa quando a rede cai e volta; na volta, libera os envios que esperavam por ela.
    fn set_offline(&self, now_offline: bool) {
        if self.offline.swap(now_offline, Ordering::Relaxed) == now_offline {
            return;
        }
        if !now_offline && let Err(e) = self.store.wake_network_waiters(now_ms()) {
            tracing::warn!(erro = %e, "falha ao retomar os envios que esperavam a rede");
        }
        // Sem ninguém ouvindo, o aviso pode se perder.
        let _ = self.events.send(EngineEvent::Offline(now_offline));
    }

    pub fn last_sync_ms(&self) -> Option<i64> {
        Some(self.last_sync_ms.load(Ordering::Relaxed)).filter(|&ms| ms != 0)
    }

    /// Busca e aplica as mudanças desde a última consulta. Devolve quantas mudaram o banco.
    pub async fn run_once(&self) -> Result<usize> {
        let token = self
            .store
            .kv_get(START_PAGE_TOKEN)?
            .ok_or(EngineError::NotIndexed)?;
        let (changes, next) = self.client.changes(&token).await?;
        self.register_new_drives(&changes).await?;
        let (applied, removed) = apply_changes(&self.store, changes)?;
        for id in &removed {
            self.cache.discard(id).await?;
        }
        if next != token {
            self.store.kv_set(START_PAGE_TOKEN, &next)?;
        }
        Ok(applied)
    }

    /// Antes de aplicar as mudanças, a lista de Drives é consultada quando um Drive mudou
    /// (criado, renomeado, dado ou tirado da conta) ou quando um item vem de um Drive que o banco
    /// não conhece: os novos entram ligados e indexados. Um que a lista não traz não é procurado
    /// de novo até `forget_searched_drives`. Sem nada disso, a primeira consulta da partida
    /// consulta a lista também, mas a falha dela só vai para o log (nenhuma mudança depende
    /// dela) e ela é tentada de novo na próxima.
    async fn register_new_drives(&self, changes: &[Change]) -> Result<()> {
        let known = self.store.shared_drives()?;
        let unknown: BTreeSet<String> = {
            let searched = self.searched();
            changes
                .iter()
                .filter_map(|c| match c {
                    Change::Upsert(r) => r.drive_id.clone(),
                    Change::Removed(_) | Change::Drive(_) => None,
                })
                .filter(|d| !searched.contains(d) && known.iter().all(|k| k.id.0 != *d))
                .collect()
        };
        let drive_changed = changes.iter().any(|c| matches!(c, Change::Drive(_)));
        let needed = drive_changed || !unknown.is_empty();
        if !needed && self.drives_listed.load(Ordering::Relaxed) {
            return Ok(());
        }
        let email = self.store.kv_get(ACCOUNT_EMAIL)?.unwrap_or_default();
        match sync_drive_list(&self.store, &self.client, Some(&self.cache), &email).await {
            Ok(_) => self.drives_listed.store(true, Ordering::Relaxed),
            Err(e) if !needed => {
                tracing::warn!(erro = %e, "falha ao consultar a lista de Drives compartilhados");
                return Ok(());
            }
            Err(e) => return Err(e),
        }
        let known = self.store.shared_drives()?;
        self.searched().extend(
            unknown
                .into_iter()
                .filter(|d| known.iter().all(|k| k.id.0 != *d)),
        );
        Ok(())
    }

    /// A lista de Drives foi consultada por outro caminho: os que as mudanças trazem e ela não
    /// tinha voltam a ser procurados.
    pub(crate) fn forget_searched_drives(&self) {
        self.drives_listed.store(true, Ordering::Relaxed);
        self.searched().clear();
    }

    fn searched(&self) -> std::sync::MutexGuard<'_, BTreeSet<String>> {
        self.searched_drives
            .lock()
            .unwrap_or_else(|e| e.into_inner())
    }

    /// Uma consulta, avisando quando a rede cai e volta e anotando a hora da última que deu certo.
    pub async fn poll(&self) {
        let result = self.run_once().await;
        let now_offline = matches!(result, Err(EngineError::Drive(DriveError::Offline)));
        self.set_offline(now_offline);
        match result {
            Ok(_) => self.last_sync_ms.store(now_ms(), Ordering::Relaxed),
            Err(_) if now_offline => {}
            Err(e) => tracing::warn!(erro = %e, "falha ao consultar mudanças no Drive"),
        }
    }
}

#[cfg(test)]
mod tests {

    use gdl_fake_drive::Fault;

    use super::*;
    use crate::indexer::initial_index;
    use crate::testutil::Env;

    async fn indexed() -> (Env, ChangePoller) {
        let env = Env::new().await;
        let (poller, _) = env.poller();
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

    /// Bug: a aplicação das mudanças lia o item e gravava depois; uma gravação local que
    /// terminava no meio voltava ao tamanho antigo (e podia voltar a `Clean` e se perder).
    #[tokio::test]
    async fn local_write_during_apply_changes_is_not_overwritten() {
        let (env, _poller) = indexed().await;
        let id = env
            .fake
            .add_file(&env.fake.root_id(), "a.txt", b"v".to_vec());
        initial_index(&env.store, &env.client).await.unwrap();
        let remote = env.client.get(&id).await.unwrap();
        let item = ItemId(id);
        let stop = Arc::new(AtomicBool::new(false));
        let applier = {
            let (store, stop) = (env.store.clone(), stop.clone());
            std::thread::spawn(move || {
                while !stop.load(Ordering::Relaxed) {
                    let change = Change::Upsert(Box::new(remote.clone()));
                    apply_changes(&store, vec![change]).unwrap();
                }
            })
        };
        let mut reverted = 0;
        for size in 1..=2000 {
            crate::uploads::commit_local_write(&env.store, &item, size, 0).unwrap();
            std::thread::yield_now();
            let now = env.store.get(&item).unwrap().unwrap();
            if now.size != size || now.local_state != LocalState::Dirty {
                reverted += 1;
            }
        }
        stop.store(true, Ordering::Relaxed);
        applier.join().unwrap();
        assert_eq!(reverted, 0, "gravações locais desfeitas pela consulta");
    }

    /// Bug: a consulta que trazia a revisão do próprio envio antes do fim dele marcava conflito,
    /// e o item ficava preso em `Conflict` sem envio na fila.
    #[tokio::test]
    async fn remote_revision_during_own_upload_is_not_a_conflict() {
        let (env, poller) = indexed().await;
        let id = env
            .fake
            .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec());
        initial_index(&env.store, &env.client).await.unwrap();
        env.make_dirty(&id, 4);
        env.store
            .set_local_state(&ItemId(id.clone()), LocalState::Uploading)
            .unwrap();

        // A revisão nova chega pela consulta antes da resposta do envio.
        env.fake.modify_remote(&id, b"novo".to_vec());
        poller.run_once().await.unwrap();

        assert_eq!(env.item(&id).local_state, LocalState::Uploading);
    }

    /// Bug: o item apagado de vez no Drive (lixeira esvaziada pela web) ficava para sempre na
    /// lixeira daqui, e restaurar dava erro.
    #[tokio::test]
    async fn permanently_deleted_clean_item_leaves_the_local_trash() {
        let (env, poller) = indexed().await;
        let id = env
            .fake
            .add_file(&env.fake.root_id(), "velho.txt", b"x".to_vec());
        initial_index(&env.store, &env.client).await.unwrap();
        let cache = env.cache_for(env.store.clone());
        cache.ensure_full(&ItemId(id.clone())).await.unwrap();
        env.fake.trash_remote(&id);
        poller.run_once().await.unwrap();

        env.fake.delete_remote(&id);
        poller.run_once().await.unwrap();

        assert!(env.store.get(&ItemId(id.clone())).unwrap().is_none());
        assert!(env.store.trashed().unwrap().is_empty());
        assert!(
            !cache.local_path(&ItemId(id)).exists(),
            "o conteúdo saiu do disco"
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
        let (poller, mut rx) = env.poller();
        env.fake.inject(Fault::Offline);
        poller.poll().await;
        poller.poll().await;
        assert_eq!(rx.try_recv().unwrap(), EngineEvent::Offline(true));
        assert!(rx.try_recv().is_err(), "avisa uma vez só");
        assert!(poller.is_offline() && poller.last_sync_ms().is_none());

        env.fake.clear_faults();
        poller.poll().await;
        assert_eq!(rx.try_recv().unwrap(), EngineEvent::Offline(false));
        assert!(!poller.is_offline() && poller.last_sync_ms().is_some());
    }

    /// Bug: um envio que dava sem rede logo depois de a consulta ver a rede voltar ficava 30 s
    /// parado; o aviso do envio faz a próxima consulta bem-sucedida liberá-lo.
    #[tokio::test]
    async fn upload_reporting_offline_is_woken_by_the_next_good_poll() {
        let env = Env::new().await;
        initial_index(&env.store, &env.client).await.unwrap();
        let (poller, mut rx) = env.poller();
        poller.poll().await;
        let job = env
            .store
            .enqueue(gdl_store::NewUploadJob {
                item: env.root(),
                kind: gdl_store::UploadKind::Update,
                next_try_ms: 0,
                file_id: None,
            })
            .unwrap();
        env.store.wait_for_network(job, i64::MAX).unwrap();

        poller.report_offline();
        poller.poll().await;

        assert!(env.store.next_job(now_ms()).unwrap().is_some());
        assert_eq!(rx.try_recv().unwrap(), EngineEvent::Offline(true));
        assert_eq!(rx.try_recv().unwrap(), EngineEvent::Offline(false));
    }

    #[tokio::test]
    async fn drive_missing_from_the_list_is_searched_once() {
        let (env, poller) = indexed().await;
        initial_index(&env.store, &env.client).await.unwrap();
        let ghost = Change::Upsert(Box::new(gdl_drive::model::RemoteFile {
            id: "x".into(),
            name: "x".into(),
            mime_type: "text/plain".into(),
            parents: vec!["fantasma".into()],
            size: Some(0),
            md5: None,
            head_revision: None,
            modified_ms: 0,
            created_ms: 0,
            trashed: false,
            can_edit: true,
            shortcut_target: None,
            web_view_link: None,
            drive_id: Some("fantasma".into()),
        }));
        let lists = || env.fake.requests("/drive/v3/drives");
        let before = lists();

        poller
            .register_new_drives(std::slice::from_ref(&ghost))
            .await
            .unwrap();
        poller
            .register_new_drives(std::slice::from_ref(&ghost))
            .await
            .unwrap();
        assert_eq!(lists() - before, 1, "a segunda consulta não foi à rede");

        poller.forget_searched_drives();
        poller.register_new_drives(&[ghost]).await.unwrap();
        assert_eq!(lists() - before, 2);
    }
}
