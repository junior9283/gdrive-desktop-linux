//! "Disponível off-line": itens fixados ficam baixados por inteiro e fora do despejo do cache.

use std::sync::Arc;

use gdl_store::{Item, ItemId, ItemKind, PinState, Store};

use crate::activity::{ActivityLog, ActivityState};
use crate::cache::BlockCache;
use crate::{EngineError, Result};

pub struct PinManager {
    store: Arc<Store>,
    cache: BlockCache,
    activity: Arc<ActivityLog>,
}

/// Recalcula a fixação de `id` e de tudo o que está dentro dele a partir da posição na árvore:
/// quem foi fixado à parte continua fixado; os demais herdam se alguma pasta acima estiver
/// fixada. Vale para fixar, desfixar e para itens que chegaram ou mudaram de pasta.
/// Devolve os itens que perderam a fixação.
pub(crate) fn reconcile(store: &Store, id: &ItemId) -> Result<Vec<ItemId>> {
    let item = store.get(id)?.ok_or(EngineError::NotFound)?;
    let mut inherited = Vec::new();
    let mut released = Vec::new();
    let mut stack = vec![(item, ancestor_pinned(store, id)?)];
    while let Some((item, from_above)) = stack.pop() {
        let pin = match item.pin {
            PinState::Pinned => PinState::Pinned,
            _ if from_above => PinState::Inherited,
            _ => PinState::None,
        };
        if pin != item.pin {
            if pin == PinState::Inherited {
                inherited.push(item.id.clone());
            } else {
                released.push(item.id.clone());
            }
        }
        if item.kind == ItemKind::Folder {
            let pinned = pin != PinState::None;
            stack.extend(store.children(&item.id)?.into_iter().map(|c| (c, pinned)));
        }
    }
    store.set_pins(&inherited, PinState::Inherited)?;
    store.set_pins(&released, PinState::None)?;
    Ok(released)
}

/// Alguma pasta acima de `id` está fixada (por conta própria ou por herança)?
fn ancestor_pinned(store: &Store, id: &ItemId) -> Result<bool> {
    let mut parent = store.get(id)?.and_then(|i| i.parent);
    while let Some(id) = parent {
        let Some(Item {
            pin, parent: up, ..
        }) = store.get(&id)?
        else {
            break;
        };
        if pin != PinState::None {
            return Ok(true);
        }
        parent = up;
    }
    Ok(false)
}

impl PinManager {
    pub fn new(store: Arc<Store>, cache: BlockCache) -> PinManager {
        PinManager {
            store,
            cache,
            activity: Arc::default(),
        }
    }

    /// Registra os downloads em `activity`.
    pub fn with_activity(self, activity: Arc<ActivityLog>) -> PinManager {
        PinManager { activity, ..self }
    }

    fn note(&self, id: &ItemId, state: ActivityState) {
        if let Ok(Some(item)) = self.store.get(id) {
            self.activity.note(&self.store, &item, state, None);
        }
    }

    /// Fixa o item; numa pasta, tudo o que está dentro passa a herdar a fixação. O download
    /// acontece na próxima manutenção.
    pub fn pin(&self, id: &ItemId) -> Result<()> {
        self.set(id, PinState::Pinned).map(drop)
    }

    /// "Somente on-line": desfixa o item e libera o espaço. Ele continua herdando de uma pasta
    /// acima ainda fixada; o que está dentro perde a fixação herdada, salvo o que foi fixado à
    /// parte (e o conteúdo dele).
    pub async fn unpin(&self, id: &ItemId) -> Result<()> {
        let mut released = self.set(id, PinState::None)?;
        released.push(id.clone());
        Ok(self.cache.release(&released).await?)
    }

    /// "Remover arquivos off-line": desfixa tudo e libera o espaço.
    pub async fn unpin_all(&self) -> Result<()> {
        let released = self.store.unpin_all()?;
        Ok(self.cache.release(&released).await?)
    }

    /// Devolve os itens que perderam a fixação.
    fn set(&self, id: &ItemId, pin: PinState) -> Result<Vec<ItemId>> {
        self.store.get(id)?.ok_or(EngineError::NotFound)?;
        self.store.set_pins(std::slice::from_ref(id), pin)?;
        reconcile(&self.store, id)
    }

    /// Baixa os arquivos fixados que ainda não estão inteiros no cache. Devolve quantos baixou.
    pub async fn maintain_once(&self) -> Result<usize> {
        let mut done = 0;
        for id in self.store.incomplete_pinned_files()? {
            self.note(&id, ActivityState::Downloading);
            match self.cache.ensure_full(&id).await {
                Ok(()) => {
                    self.note(&id, ActivityState::Downloaded);
                    done += 1
                }
                // Sem rede ou sem espaço, nenhum outro vai conseguir agora.
                Err(e) if e.is_transient() => return Err(e.into()),
                Err(e) => tracing::warn!(item = %id.0, erro = %e, "falha ao baixar item fixado"),
            }
        }
        Ok(done)
    }
}

#[cfg(test)]
mod tests {
    use gdl_fake_drive::Fault;
    use gdl_store::{ItemId, LocalState, PinState};

    use super::*;
    use crate::cache::BlockCache;
    use crate::indexer::initial_index;
    use crate::testutil::Env;

    const MIB: usize = 1024 * 1024;

    struct Setup {
        env: Env,
        cache: BlockCache,
        pins: PinManager,
    }

    async fn setup(env: Env, max_kbytes: Option<u64>) -> Setup {
        initial_index(&env.store, &env.client).await.unwrap();
        let cache = env.cache_limited(env.store.clone(), max_kbytes);
        let pins = PinManager::new(env.store.clone(), cache.clone());
        Setup { env, cache, pins }
    }

    impl Setup {
        fn pin_of(&self, id: &str) -> PinState {
            self.env.item(id).pin
        }

        fn complete(&self, id: &str) -> bool {
            let size = self.env.item(id).size;
            self.env
                .store
                .ranges_missing(&ItemId(id.into()), 0, size)
                .unwrap()
                .is_empty()
        }

        async fn poll_changes(&self) {
            self.env.poller().0.run_once().await.unwrap();
        }
    }

    fn id(s: &str) -> ItemId {
        ItemId(s.into())
    }

    #[tokio::test]
    async fn pinning_folder_downloads_all_descendants() {
        let env = Env::new().await;
        let root = env.fake.root_id();
        let folder = env.fake.add_folder(&root, "Projetos");
        let a = env.fake.add_file(&folder, "a.bin", vec![1; 2 * MIB]);
        let sub = env.fake.add_folder(&folder, "Sub");
        let b = env.fake.add_file(&sub, "b.txt", vec![2; 1000]);
        let outside = env.fake.add_file(&root, "fora.bin", vec![3; MIB]);
        let s = setup(env, None).await;

        s.pins.pin(&id(&folder)).unwrap();

        assert_eq!(s.pin_of(&folder), PinState::Pinned);
        for inner in [&a, &sub, &b] {
            assert_eq!(s.pin_of(inner), PinState::Inherited);
        }
        assert_eq!(s.pin_of(&outside), PinState::None);
        assert_eq!(s.pins.maintain_once().await.unwrap(), 2);
        assert!(s.complete(&a) && s.complete(&b));
        assert!(!s.complete(&outside));
        assert_eq!(s.env.fake.downloaded_bytes(), (2 * MIB + 1000) as u64);
        assert_eq!(
            s.pins.maintain_once().await.unwrap(),
            0,
            "nada a baixar de novo"
        );
    }

    #[tokio::test]
    async fn pinning_shows_downloads() {
        let env = Env::new().await;
        let folder = env.fake.add_folder(&env.fake.root_id(), "Fotos");
        env.fake.add_file(&folder, "a.jpg", vec![1; 1000]);
        env.fake.add_file(&folder, "b.jpg", vec![2; 1000]);
        let s = setup(env, None).await;
        let log = Arc::new(ActivityLog::default());
        let pins = PinManager::new(s.env.store.clone(), s.cache.clone()).with_activity(log.clone());

        pins.pin(&id(&folder)).unwrap();
        pins.maintain_once().await.unwrap();

        let rows: Vec<_> = log
            .rows()
            .into_iter()
            .map(|r| (r.path.path.clone(), r.state))
            .collect();
        assert_eq!(rows.len(), 2, "{rows:?}");
        for (path, state) in &rows {
            assert!(path.starts_with("Fotos/"), "{path}");
            assert_eq!(*state, ActivityState::Downloaded);
        }
    }

    #[tokio::test]
    async fn new_child_of_pinned_folder_is_downloaded() {
        let env = Env::new().await;
        let folder = env.fake.add_folder(&env.fake.root_id(), "Fixada");
        let s = setup(env, None).await;
        s.pins.pin(&id(&folder)).unwrap();

        let child = s.env.fake.add_file(&folder, "novo.txt", b"chegou".to_vec());
        s.poll_changes().await;

        assert_eq!(s.pin_of(&child), PinState::Inherited);
        assert_eq!(s.pins.maintain_once().await.unwrap(), 1);
        assert!(s.complete(&child));
    }

    #[tokio::test]
    async fn folders_moved_in_or_out_of_a_pinned_folder_carry_their_contents() {
        let env = Env::new().await;
        let root = env.fake.root_id();
        let pinned = env.fake.add_folder(&root, "Fixada");
        let moved = env.fake.add_folder(&root, "Movida");
        let inner = env.fake.add_file(&moved, "dentro.txt", b"oi".to_vec());
        let s = setup(env, None).await;
        s.pins.pin(&id(&pinned)).unwrap();

        s.env.fake.move_remote(&moved, &pinned);
        s.poll_changes().await;
        assert_eq!(s.pin_of(&moved), PinState::Inherited);
        assert_eq!(s.pin_of(&inner), PinState::Inherited);

        s.env.fake.move_remote(&moved, &root);
        s.poll_changes().await;
        assert_eq!(s.pin_of(&moved), PinState::None);
        assert_eq!(s.pin_of(&inner), PinState::None);
    }

    #[tokio::test]
    async fn remote_update_of_pinned_file_is_redownloaded() {
        let env = Env::new().await;
        let file = env
            .fake
            .add_file(&env.fake.root_id(), "a.txt", b"velho".to_vec());
        let s = setup(env, None).await;
        s.pins.pin(&id(&file)).unwrap();
        s.pins.maintain_once().await.unwrap();

        s.env.fake.modify_remote(&file, b"versao nova".to_vec());
        s.poll_changes().await;

        assert_eq!(s.pins.maintain_once().await.unwrap(), 1);
        s.env.fake.inject(Fault::Offline);
        assert_eq!(
            s.cache.read(&id(&file), 0, 100).await.unwrap().as_ref(),
            b"versao nova"
        );
    }

    #[tokio::test]
    async fn unpin_keeps_ancestor_pins() {
        let env = Env::new().await;
        let folder = env.fake.add_folder(&env.fake.root_id(), "Pasta");
        let a = env.fake.add_file(&folder, "a.bin", vec![1; MIB]);
        let b = env.fake.add_file(&folder, "b.bin", vec![2; MIB]);
        let s = setup(env, Some(1)).await;
        s.pins.pin(&id(&folder)).unwrap();
        s.pins.pin(&id(&b)).unwrap();
        s.pins.maintain_once().await.unwrap();

        s.pins.unpin(&id(&a)).await.unwrap();
        assert_eq!(s.pin_of(&a), PinState::Inherited, "a pasta continua fixada");
        assert!(s.complete(&a), "herdado da pasta, continua baixado");

        s.pins.unpin(&id(&folder)).await.unwrap();
        assert_eq!(s.pin_of(&folder), PinState::None);
        assert_eq!(s.pin_of(&a), PinState::None);
        assert_eq!(
            s.pin_of(&b),
            PinState::Pinned,
            "fixado à parte continua fixado"
        );
        assert!(!s.complete(&a) && s.complete(&b));
    }

    // "Somente on-line" libera o espaço na hora, sem esperar o cache passar do limite.
    #[tokio::test]
    async fn unpin_frees_the_cached_content() {
        let env = Env::new().await;
        let folder = env.fake.add_folder(&env.fake.root_id(), "Pasta");
        let a = env.fake.add_file(&folder, "a.bin", vec![1; MIB]);
        let s = setup(env, None).await;
        s.pins.pin(&id(&folder)).unwrap();
        s.pins.maintain_once().await.unwrap();
        assert!(s.complete(&a));

        s.pins.unpin(&id(&folder)).await.unwrap();

        assert!(!s.complete(&a));
        assert_eq!(s.env.store.cached_bytes().unwrap(), 0);
    }

    // O que tem edição ainda não enviada continua no cache: só sai o conteúdo limpo.
    #[tokio::test]
    async fn unpin_keeps_dirty_and_open_content() {
        let env = Env::new().await;
        let root = env.fake.root_id();
        let clean = env.fake.add_file(&root, "limpo.bin", vec![1; MIB]);
        let dirty = env.fake.add_file(&root, "editado.bin", vec![2; MIB]);
        let s = setup(env, None).await;
        for f in [&clean, &dirty] {
            s.pins.pin(&id(f)).unwrap();
        }
        s.pins.maintain_once().await.unwrap();
        s.env
            .store
            .set_local_state(&id(&dirty), LocalState::Dirty)
            .unwrap();

        s.pins.unpin_all().await.unwrap();

        assert!(!s.complete(&clean));
        assert!(s.complete(&dirty), "a edição pendente fica");
    }

    #[tokio::test]
    async fn pinned_files_stay_readable_offline() {
        let env = Env::new().await;
        let data: Vec<u8> = (0..3 * MIB).map(|n| (n % 251) as u8).collect();
        let file = env
            .fake
            .add_file(&env.fake.root_id(), "viagem.pdf", data.clone());
        let s = setup(env, None).await;
        s.pins.pin(&id(&file)).unwrap();
        s.pins.maintain_once().await.unwrap();

        s.env.fake.inject(Fault::Offline);

        let got = s.cache.read(&id(&file), 0, 3 * MIB as u32).await.unwrap();
        assert_eq!(got.as_ref(), &data[..]);
    }
}
