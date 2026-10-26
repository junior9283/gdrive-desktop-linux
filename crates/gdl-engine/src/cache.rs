//! Cache do conteúdo em blocos de 1 MiB: um arquivo esparso por item, baixado sob demanda.

use std::collections::HashMap;
use std::io::SeekFrom;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use bytes::{Bytes, BytesMut};
use gdl_common::paths::create_private_dir;
use gdl_common::time::now_ms;
use gdl_drive::client::{DriveClient, DriveError};
use gdl_store::{Item, ItemId, LocalState, Store, StoreError};
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio::sync::{Notify, broadcast, futures::OwnedNotified};

use crate::events::EngineEvent;

/// Unidade de download e de controle do cache.
pub const BLOCK: u64 = 1024 * 1024;
/// Primeira janela de leitura antecipada; dobra a cada janela enquanto a leitura segue sequencial.
const READ_AHEAD: u64 = 8 * 1024 * 1024;
/// Maior janela de leitura antecipada.
const MAX_READ_AHEAD: u64 = 64 * 1024 * 1024;
/// Maior pedido de download: quem espera um bloco no meio de um pedido não espera mais que isto.
const MAX_REQUEST: u64 = 16 * 1024 * 1024;
/// O último acesso de um item é gravado no banco no máximo uma vez neste intervalo.
const TOUCH_EVERY_MS: i64 = 60_000;

#[derive(Debug, thiserror::Error)]
pub enum CacheError {
    #[error("sem conexão e o trecho ainda não foi baixado")]
    Offline,
    #[error("pouco espaço livre em disco para o cache")]
    DiskFull,
    #[error("o item não existe")]
    NotFound,
    #[error(transparent)]
    Drive(DriveError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("erro no arquivo do cache: {0}")]
    Io(#[from] std::io::Error),
}

impl CacheError {
    /// Sem rede ou sem espaço: passa sozinho, e quem chamou só precisa tentar depois.
    pub fn is_transient(&self) -> bool {
        matches!(self, CacheError::Offline | CacheError::DiskFull)
    }
}

impl From<DriveError> for CacheError {
    fn from(e: DriveError) -> CacheError {
        match e {
            DriveError::Offline => CacheError::Offline,
            DriveError::NotFound => CacheError::NotFound,
            e => CacheError::Drive(e),
        }
    }
}

pub type Result<T> = std::result::Result<T, CacheError>;

/// Subpasta do cache com o conteúdo de cada item.
pub(crate) const CONTENT_DIR: &str = "content";

/// Espaço livre no disco do cache (injetável nos testes).
pub trait FreeSpace: Send + Sync {
    fn free_kbytes(&self, dir: &Path) -> std::io::Result<u64>;
}

/// Espaço livre real, pelo `statvfs` do sistema de arquivos.
pub struct StatvfsFreeSpace;

impl FreeSpace for StatvfsFreeSpace {
    fn free_kbytes(&self, dir: &Path) -> std::io::Result<u64> {
        let st = rustix::fs::statvfs(dir)?;
        Ok(st.f_bavail.saturating_mul(st.f_frsize) / 1024)
    }
}

/// Cache compartilhável: os clones apontam para o mesmo estado.
#[derive(Clone)]
pub struct BlockCache(Arc<Inner>);

struct Inner {
    store: Arc<Store>,
    client: Arc<DriveClient>,
    /// Pasta do cache da conta (medida para o espaço livre) e a subpasta dos conteúdos.
    dir: PathBuf,
    content_dir: PathBuf,
    max_bytes: Option<u64>,
    min_free_kbytes: u64,
    events: broadcast::Sender<EngineEvent>,
    free: Arc<dyn FreeSpace>,
    /// Já avisamos que o disco está cheio (o aviso sai só na passagem para cheio).
    disk_low: AtomicBool,
    items: Mutex<HashMap<ItemId, ItemState>>,
}

/// Estado em memória de um item lido; sai do mapa quando o item sai do cache.
#[derive(Default)]
struct ItemState {
    /// Segurada a cada bloco gravado: quem passa a editar o item, ou o despeja, espera a gravação.
    lock: Arc<tokio::sync::Mutex<()>>,
    /// Trechos sendo baixados agora; quem precisa de um deles espera em `landed`.
    fetching: Vec<(u64, u64)>,
    /// Avisa a cada bloco que chega e a cada download que termina.
    landed: Arc<Notify>,
    /// Fim da última leitura, para reconhecer leitura sequencial.
    last_end: Option<u64>,
    /// Até onde a leitura antecipada já foi pedida, e o tamanho da próxima janela.
    ahead_until: u64,
    window: u64,
    /// Último acesso gravado no banco.
    touched_ms: i64,
}

/// O que falta de um trecho, para quem pede (`BlockCache::claim`).
enum Claim {
    /// Já está todo no cache.
    Done,
    /// O que falta já está sendo baixado: espera o próximo bloco chegar e olha de novo.
    Wait(Pin<Box<OwnedNotified>>),
    /// Este trecho ficou com quem pediu.
    Fetch(Fetching),
}

/// Trecho reservado para um download; ao sair (fim, erro ou cancelamento), libera a reserva e
/// acorda quem espera.
struct Fetching {
    cache: BlockCache,
    id: ItemId,
    start: u64,
    end: u64,
    landed: Arc<Notify>,
}

impl Drop for Fetching {
    fn drop(&mut self) {
        // O item pode ter saído do cache e voltado com outro estado: a reserva era do velho.
        if let Some(state) = self.cache.items().get_mut(&self.id)
            && Arc::ptr_eq(&state.landed, &self.landed)
        {
            state.fetching.retain(|&r| r != (self.start, self.end));
        }
        self.landed.notify_waiters();
    }
}

/// Arquivo que já não existe conta como sucesso ao apagar ou renomear.
fn ignore_not_found(result: std::io::Result<()>) -> Result<()> {
    match result {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.into()),
        _ => Ok(()),
    }
}

fn align_down(n: u64) -> u64 {
    n / BLOCK * BLOCK
}

impl BlockCache {
    pub fn new(
        store: Arc<Store>,
        client: Arc<DriveClient>,
        dir: PathBuf,
        max_kbytes: Option<u64>,
        min_free_kbytes: u64,
        events: broadcast::Sender<EngineEvent>,
    ) -> BlockCache {
        let free = Arc::new(StatvfsFreeSpace);
        BlockCache::with_free_space(
            store,
            client,
            dir,
            max_kbytes,
            min_free_kbytes,
            events,
            free,
        )
    }

    /// Como `new`, com a medição de espaço livre escolhida (nos testes, um valor fixo).
    pub fn with_free_space(
        store: Arc<Store>,
        client: Arc<DriveClient>,
        dir: PathBuf,
        max_kbytes: Option<u64>,
        min_free_kbytes: u64,
        events: broadcast::Sender<EngineEvent>,
        free: Arc<dyn FreeSpace>,
    ) -> BlockCache {
        BlockCache(Arc::new(Inner {
            store,
            client,
            content_dir: dir.join(CONTENT_DIR),
            dir,
            max_bytes: max_kbytes.map(|kb| kb * 1024),
            min_free_kbytes,
            events,
            free,
            disk_low: AtomicBool::new(false),
            items: Mutex::default(),
        }))
    }

    /// Espaço livre no disco do cache, em bytes.
    pub fn free_bytes(&self) -> std::io::Result<u64> {
        Ok(self.0.free.free_kbytes(&self.0.dir)?.saturating_mul(1024))
    }

    pub fn dir(&self) -> &Path {
        &self.0.dir
    }

    /// Arquivo esparso com o conteúdo do item; é onde a gravação local acontece.
    pub fn local_path(&self, id: &ItemId) -> PathBuf {
        self.0.content_dir.join(&id.0)
    }

    /// Abre o conteúdo local para leitura e escrita, criando o arquivo (e a pasta) se preciso.
    pub fn open_local(&self, id: &ItemId, truncate: bool) -> std::io::Result<std::fs::File> {
        let open = || {
            std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(truncate)
                .mode(0o600)
                .open(self.local_path(id))
        };
        match open() {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                create_private_dir(&self.0.content_dir)?;
                open()
            }
            result => result,
        }
    }

    fn items(&self) -> MutexGuard<'_, HashMap<ItemId, ItemState>> {
        self.0.items.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn item_lock(&self, id: &ItemId) -> Arc<tokio::sync::Mutex<()>> {
        self.items().entry(id.clone()).or_default().lock.clone()
    }

    fn size_of(&self, id: &ItemId) -> Result<u64> {
        Ok(self.0.store.get(id)?.ok_or(CacheError::NotFound)?.size)
    }

    /// Lê até `len` bytes a partir de `offset`, baixando só os blocos que faltam.
    pub async fn read(&self, id: &ItemId, offset: u64, len: u32) -> Result<Bytes> {
        self.read_hinted(id, offset, len, None).await
    }

    /// Como `read`, com a leitura sequencial já decidida por quem lê (o FUSE acompanha cada
    /// arquivo aberto); `None` deixa o cache decidir pelo item.
    pub async fn read_hinted(
        &self,
        id: &ItemId,
        offset: u64,
        len: u32,
        sequential: Option<bool>,
    ) -> Result<Bytes> {
        let size = self.size_of(id)?;
        let end = size.min(offset.saturating_add(len.into()));
        if offset >= end {
            return Ok(Bytes::new());
        }
        let now = now_ms();
        let (prefetch, touch) = {
            let mut items = self.items();
            let state = items.entry(id.clone()).or_default();
            let follows = state.last_end.replace(end) == Some(offset);
            let sequential = sequential.unwrap_or(follows);
            if !sequential {
                state.window = READ_AHEAD;
            }
            // Pede a próxima janela, maior, quando a anterior está na metade.
            let window = state.window.max(READ_AHEAD);
            let prefetch = (sequential && end + window / 2 > state.ahead_until).then(|| {
                state.ahead_until = size.min(end + window);
                state.window = (window * 2).min(MAX_READ_AHEAD);
                state.ahead_until
            });
            let touch = now - state.touched_ms >= TOUCH_EVERY_MS;
            if touch {
                state.touched_ms = now;
            }
            (prefetch, touch)
        };
        if touch {
            self.0.store.touch_access(id, now)?;
        }
        if let Some(ahead_end) = prefetch {
            let (cache, id) = (self.clone(), id.clone());
            tokio::spawn(async move {
                if let Err(e) = cache.ensure_range(&id, end, ahead_end, size).await {
                    tracing::debug!(erro = %e, "leitura antecipada interrompida");
                }
            });
        }
        self.ensure_range(id, offset, end, size).await?;
        let path = self.local_path(id);
        let buf = tokio::task::spawn_blocking(move || -> std::io::Result<Vec<u8>> {
            let mut buf = vec![0; (end - offset) as usize];
            std::fs::File::open(path)?.read_exact_at(&mut buf, offset)?;
            Ok(buf)
        })
        .await
        .map_err(std::io::Error::other)??;
        Ok(buf.into())
    }

    /// Baixa o conteúdo inteiro (off-line e edição local).
    pub async fn ensure_full(&self, id: &ItemId) -> Result<()> {
        let size = self.size_of(id)?;
        self.ensure_range(id, 0, size, size).await
    }

    /// Garante no cache os blocos que cobrem `[start, end)` de um item de `size` bytes: baixa o
    /// que ninguém está baixando e espera o resto chegar.
    async fn ensure_range(&self, id: &ItemId, start: u64, end: u64, size: u64) -> Result<()> {
        let (start, end) = (align_down(start), end.next_multiple_of(BLOCK).min(size));
        loop {
            let item = self.0.store.get(id)?.ok_or(CacheError::NotFound)?;
            // Com edição local, o arquivo é dela e nada da nuvem entra nele.
            if item.local_state != LocalState::Clean {
                return Ok(());
            }
            match self.claim(id, start, end)? {
                Claim::Done => return Ok(()),
                Claim::Wait(landed) => landed.await,
                Claim::Fetch(fetching) => {
                    self.check_disk(id).await?;
                    self.fetch(&item, &fetching).await?;
                }
            }
        }
    }

    /// Reserva o primeiro trecho de `[start, end)` que falta e que ninguém está baixando (até
    /// `MAX_REQUEST`); sem nenhum, devolve a espera pelo próximo bloco que chegar.
    fn claim(&self, id: &ItemId, start: u64, end: u64) -> Result<Claim> {
        // O banco é consultado com o mapa travado: um download que termina no meio não escapa.
        let mut items = self.items();
        let gaps = self.0.store.ranges_missing(id, start, end)?;
        if gaps.is_empty() {
            return Ok(Claim::Done);
        }
        let state = items.entry(id.clone()).or_default();
        for (mut at, gap_end) in gaps {
            while at < gap_end {
                if let Some(&(_, busy_end)) =
                    state.fetching.iter().find(|&&(s, e)| s <= at && at < e)
                {
                    at = busy_end;
                    continue;
                }
                let next_busy = state
                    .fetching
                    .iter()
                    .map(|&(s, _)| s)
                    .filter(|&s| s > at)
                    .min();
                let to = gap_end
                    .min(at + MAX_REQUEST)
                    .min(next_busy.unwrap_or(u64::MAX));
                state.fetching.push((at, to));
                return Ok(Claim::Fetch(Fetching {
                    cache: self.clone(),
                    id: id.clone(),
                    start: at,
                    end: to,
                    landed: state.landed.clone(),
                }));
            }
        }
        // Inscrito antes de soltar o mapa: um bloco que chega logo depois não se perde.
        let mut landed = Box::pin(state.landed.clone().notified_owned());
        landed.as_mut().enable();
        Ok(Claim::Wait(landed))
    }

    /// Baixa o trecho reservado num pedido só, gravando cada bloco assim que chega; se a conexão
    /// cai depois de algum progresso, continua de onde parou.
    async fn fetch(&self, item: &Item, fetching: &Fetching) -> Result<()> {
        let mut at = fetching.start;
        loop {
            let before = at;
            match self.stream(item, fetching, &mut at).await {
                Err(e) if at == before => return Err(e),
                Err(e) => tracing::debug!(erro = %e, "download interrompido; segue de onde parou"),
                Ok(()) => return Ok(()),
            }
        }
    }

    /// Um pedido de `[at, fim do trecho)`, avançando `at` a cada bloco gravado. Termina sem
    /// erro também quando o item muda no meio: o resto não é gravado.
    async fn stream(&self, item: &Item, fetching: &Fetching, at: &mut u64) -> Result<()> {
        let mut body = self
            .0
            .client
            .download_stream(&item.id.0, *at, fetching.end)
            .await?;
        let mut block = BytesMut::new();
        while *at < fetching.end {
            let len = BLOCK.min(fetching.end - *at) as usize;
            while block.len() < len {
                let piece = body.chunk().await?.ok_or_else(|| {
                    DriveError::Protocol("o download terminou antes do fim do trecho".into())
                })?;
                block.extend_from_slice(&piece);
            }
            if !self.land(item, *at, &block.split_to(len)).await? {
                return Ok(());
            }
            *at += len as u64;
            fetching.landed.notify_waiters();
        }
        Ok(())
    }

    /// Grava um bloco baixado e o marca no cache. Devolve `false`, sem gravar nada, se o item
    /// passou a ser editado aqui ou ganhou revisão nova durante o download.
    async fn land(&self, item: &Item, at: u64, bytes: &[u8]) -> Result<bool> {
        let id = &item.id;
        let same =
            |c: &Item| c.head_revision == item.head_revision && c.local_state == LocalState::Clean;
        // Quem passa a editar o item segura esta trava (`locked`): a edição não é sobrescrita.
        let lock = self.item_lock(id);
        let _guard = lock.lock().await;
        if !self.0.store.get(id)?.is_some_and(|c| same(&c)) {
            return Ok(false);
        }
        let mut file = tokio::fs::File::from_std(self.open_local(id, false)?);
        file.seek(SeekFrom::Start(at)).await?;
        file.write_all(bytes).await?;
        file.flush().await?;
        // A troca de revisão (pelas mudanças da nuvem) não passa pela trava: confere de novo.
        Ok(self.0.store.modify(|batch| -> gdl_store::Result<bool> {
            let same = batch.get(id)?.is_some_and(|c| same(&c));
            if same {
                batch.ranges_add(id, at, at + bytes.len() as u64)?;
            }
            Ok(same)
        })?)
    }

    /// Roda `f` com a trava do item: nenhum download grava no conteúdo local enquanto isso.
    /// Quem passa a editar o item (marca a edição ou troca o arquivo) faz isso aqui dentro.
    pub async fn locked<T>(&self, id: &ItemId, f: impl FnOnce() -> T) -> T {
        let lock = self.item_lock(id);
        let _guard = lock.lock().await;
        f()
    }

    /// Com o disco abaixo do mínimo livre, libera o que estiver em cache há mais tempo (menos o
    /// item `reading`); se não bastar, recusa gravar no cache e avisa na passagem para cheio.
    async fn check_disk(&self, reading: &ItemId) -> Result<()> {
        let mut free_kb = self.0.free.free_kbytes(&self.0.dir)?;
        if free_kb < self.0.min_free_kbytes {
            // Libera os de acesso mais antigo até voltar ao mínimo.
            for (id, _) in self.0.store.eviction_candidates(64)? {
                if free_kb >= self.0.min_free_kbytes {
                    break;
                }
                if id != *reading && self.try_evict(&id).await? {
                    free_kb = self.0.free.free_kbytes(&self.0.dir)?;
                }
            }
        }
        let low = free_kb < self.0.min_free_kbytes;
        if self.0.disk_low.swap(low, Ordering::SeqCst) != low && low {
            // Sem ninguém ouvindo, o aviso pode se perder.
            let _ = self.0.events.send(EngineEvent::DiskLow { free_kb });
        }
        if low {
            return Err(CacheError::DiskFull);
        }
        Ok(())
    }

    /// Apaga o conteúdo local do item e esquece o que estava no cache. Quem chama segura a trava.
    async fn drop_content(&self, id: &ItemId) -> Result<()> {
        ignore_not_found(tokio::fs::remove_file(self.local_path(id)).await)?;
        self.0.store.ranges_clear(id)?;
        self.items().remove(id);
        Ok(())
    }

    /// Descarta o conteúdo local (uma edição que foi para o `Lost and Found`); a próxima leitura
    /// baixa de novo da nuvem.
    pub async fn discard(&self, id: &ItemId) -> Result<()> {
        let lock = self.item_lock(id);
        let _guard = lock.lock().await;
        self.drop_content(id).await
    }

    /// Primeiro passo da troca do ID temporário pelo do Drive: o conteúdo ganha o nome novo, ainda
    /// com o velho. Entre `Store::replace_id` e `unlink_old_id`, quem lê por qualquer um dos dois
    /// IDs acha o arquivo; uma queda no meio só deixa um nome a mais.
    pub async fn link_local(&self, old: &ItemId, new: &ItemId) -> Result<()> {
        ignore_not_found(tokio::fs::hard_link(self.local_path(old), self.local_path(new)).await)?;
        Ok(())
    }

    /// Último passo da troca de ID (depois de `link_local` e de `Store::replace_id`): tira o nome
    /// velho do conteúdo e passa o estado do item para o ID novo.
    pub async fn unlink_old_id(&self, old: &ItemId, new: &ItemId) -> Result<()> {
        ignore_not_found(tokio::fs::remove_file(self.local_path(old)).await)?;
        let mut items = self.items();
        if let Some(state) = items.remove(old) {
            items.insert(new.clone(), state);
        }
        Ok(())
    }

    /// Libera o item se ninguém o estiver usando agora. Devolve se liberou.
    async fn try_evict(&self, id: &ItemId) -> Result<bool> {
        let lock = self.item_lock(id);
        let Ok(_guard) = lock.try_lock() else {
            return Ok(false);
        };
        if self.items().get(id).is_some_and(|s| !s.fetching.is_empty()) {
            return Ok(false);
        }
        self.drop_content(id).await?;
        Ok(true)
    }

    /// Libera os itens de acesso mais antigo até o cache caber no limite. Nunca toca em itens
    /// fixados, com edição local ou sendo baixados agora. Devolve os bytes liberados.
    pub async fn evict_if_needed(&self) -> Result<u64> {
        match self.0.max_bytes {
            Some(max) => self.evict_down_to(max).await,
            None => Ok(0),
        }
    }

    /// "Limpar cache": libera tudo o que `evict_if_needed` poderia liberar. Devolve os bytes.
    pub async fn clear(&self) -> Result<u64> {
        self.evict_down_to(0).await
    }

    async fn evict_down_to(&self, max: u64) -> Result<u64> {
        let mut total = self.0.store.cached_bytes()?;
        let mut freed = 0;
        while total > max {
            let before = freed;
            for (id, bytes) in self.0.store.eviction_candidates(64)? {
                if total <= max {
                    break;
                }
                if self.try_evict(&id).await? {
                    total -= bytes;
                    freed += bytes;
                }
            }
            // Só restam itens em uso: tenta de novo na próxima vez.
            if freed == before {
                break;
            }
        }
        Ok(freed)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::Duration;

    use gdl_fake_drive::Fault;
    use gdl_store::{ItemId, LocalState, PinState};
    use tokio::sync::broadcast;

    use super::*;
    use crate::events::EngineEvent;
    use crate::indexer::initial_index;
    use crate::testutil::{Env, wait_until};

    const MIB: usize = 1024 * 1024;

    struct FakeFree(AtomicU64);

    impl FreeSpace for FakeFree {
        fn free_kbytes(&self, _dir: &std::path::Path) -> std::io::Result<u64> {
            Ok(self.0.load(Ordering::SeqCst))
        }
    }

    fn data(len: usize) -> Vec<u8> {
        (0..len).map(|n| (n % 251) as u8).collect()
    }

    struct Setup {
        env: Env,
        cache: BlockCache,
        events: broadcast::Receiver<EngineEvent>,
        free: Arc<FakeFree>,
        _dir: tempfile::TempDir,
    }

    /// Indexa os arquivos `(nome, tamanho)` e monta o cache sobre eles.
    async fn setup(files: &[(&str, usize)], max_kbytes: Option<u64>) -> (Setup, Vec<ItemId>) {
        let env = Env::new().await;
        let root = env.fake.root_id();
        let ids = files
            .iter()
            .map(|(name, len)| ItemId(env.fake.add_file(&root, name, data(*len))))
            .collect();
        initial_index(&env.store, &env.client).await.unwrap();
        let dir = tempfile::tempdir().unwrap();
        let (tx, events) = broadcast::channel(16);
        let free = Arc::new(FakeFree(AtomicU64::new(u64::MAX)));
        let cache = BlockCache::with_free_space(
            env.store.clone(),
            env.client.clone(),
            dir.path().to_path_buf(),
            max_kbytes,
            512_000,
            tx,
            free.clone(),
        );
        let setup = Setup {
            env,
            cache,
            events,
            free,
            _dir: dir,
        };
        (setup, ids)
    }

    /// Bug: um download (leitura antecipada, por exemplo) que alcançava o item depois de ele
    /// passar a ser editado aqui gravava os blocos da nuvem por cima da edição.
    #[tokio::test]
    async fn download_never_writes_over_a_local_edit() {
        let (s, ids) = setup(&[("a.bin", 2 * MIB)], None).await;
        let mut file = s.cache.open_local(&ids[0], true).unwrap();
        std::io::Write::write_all(&mut file, b"LOCAL").unwrap();
        s.env
            .store
            .set_local_state(&ids[0], LocalState::Dirty)
            .unwrap();

        s.cache.ensure_full(&ids[0]).await.unwrap();

        assert_eq!(
            std::fs::read(s.cache.local_path(&ids[0])).unwrap(),
            b"LOCAL"
        );
    }

    #[tokio::test]
    async fn reading_twice_downloads_once() {
        let (s, ids) = setup(&[("a.bin", 3 * MIB)], None).await;
        let first = s.cache.read(&ids[0], 100, 4096).await.unwrap();
        let again = s.cache.read(&ids[0], 100, 4096).await.unwrap();
        assert_eq!(first.as_ref(), &data(3 * MIB)[100..4196]);
        assert_eq!(first, again);
        assert_eq!(s.env.fake.downloaded_bytes(), MIB as u64);
    }

    #[tokio::test]
    async fn reads_stop_at_the_end_of_the_file() {
        let (s, ids) = setup(&[("a.bin", 1000)], None).await;
        assert_eq!(s.cache.read(&ids[0], 900, 4096).await.unwrap().len(), 100);
        assert!(s.cache.read(&ids[0], 1000, 10).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn random_seek_downloads_only_nearby_blocks() {
        let (s, ids) = setup(&[("video.mkv", 64 * MIB)], None).await;
        let at = 40 * MIB;
        let got = s.cache.read(&ids[0], at as u64, 4096).await.unwrap();
        assert_eq!(got.as_ref(), &data(64 * MIB)[at..at + 4096]);
        assert!(s.env.fake.downloaded_bytes() <= 2 * MIB as u64);
    }

    #[tokio::test]
    async fn sequential_read_prefetches_8_mib() {
        let (s, ids) = setup(&[("filme.mp4", 32 * MIB)], None).await;
        let id = &ids[0];
        s.cache.read(id, 0, 65536).await.unwrap();
        s.cache.read(id, 65536, 65536).await.unwrap();
        let wanted = (131072 + 8 * MIB) as u64;
        wait_until("leitura antecipada dos próximos 8 MiB", || {
            s.env
                .store
                .ranges_missing(id, 0, wanted)
                .unwrap()
                .is_empty()
        })
        .await;
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            s.env.fake.downloaded_bytes() <= 10 * MIB as u64,
            "não baixa o arquivo todo"
        );
    }

    /// Pedidos de conteúdo (`alt=media`) feitos ao Drive simulado.
    fn media_requests(s: &Setup, id: &ItemId) -> u64 {
        s.env
            .fake
            .requests(&format!("/drive/v3/files/{}?alt=media", id.0))
    }

    /// Lê o arquivo inteiro em pedaços de 128 KiB, como o FUSE numa cópia.
    async fn read_sequentially(s: &Setup, id: &ItemId, len: usize) -> Vec<u8> {
        let mut got = Vec::with_capacity(len);
        while got.len() < len {
            let piece = s
                .cache
                .read_hinted(id, got.len() as u64, 128 * 1024, Some(!got.is_empty()))
                .await
                .unwrap();
            got.extend_from_slice(&piece);
        }
        got
    }

    /// Desempenho: a leitura sequencial pedia 8 MiB de cada vez, um pedido depois do outro
    /// (~1,2 MB/s contra ~2,5 MB/s do rclone). As janelas crescem enquanto a leitura segue.
    #[tokio::test]
    async fn sequential_read_uses_growing_windows() {
        let (s, ids) = setup(&[("filme.mp4", 64 * MIB)], None).await;

        let got = read_sequentially(&s, &ids[0], 64 * MIB).await;

        assert!(got == data(64 * MIB), "conteúdo diferente do da nuvem");
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(
            s.env.fake.downloaded_bytes(),
            64 * MIB as u64,
            "nada baixado duas vezes"
        );
        let requests = media_requests(&s, &ids[0]);
        assert!(requests <= 7, "{requests} pedidos para 64 MiB");
    }

    /// Saltos (um player de vídeo procurando a cena) baixam só os blocos lidos, como antes.
    #[tokio::test]
    async fn random_reads_download_only_the_blocks_read() {
        let (s, ids) = setup(&[("video.mkv", 64 * MIB)], None).await;
        let all = data(64 * MIB);
        for n in [50, 3, 24, 11, 62, 1, 40, 30] {
            let at = n * MIB + 4096;
            let got = s
                .cache
                .read_hinted(&ids[0], at as u64, 128 * 1024, Some(false))
                .await
                .unwrap();
            assert!(got.as_ref() == &all[at..at + 128 * 1024]);
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert_eq!(s.env.fake.downloaded_bytes(), 8 * MIB as u64);
        assert_eq!(media_requests(&s, &ids[0]), 8);
    }

    /// O bloco que alguém espera fica pronto assim que chega, sem esperar o resto do pedido:
    /// antes, o pedido inteiro ficava na memória e só então ia para o cache.
    #[tokio::test]
    async fn a_block_is_readable_as_soon_as_it_arrives() {
        let (s, ids) = setup(&[("grande.bin", 32 * MIB)], None).await;
        let id = ids[0].clone();
        s.env
            .fake
            .inject(Fault::StallConnectionAfterBytes(3 * MIB as u64));
        let cache = s.cache.clone();
        let full = tokio::spawn({
            let id = id.clone();
            async move { cache.ensure_full(&id).await }
        });
        wait_until("o download empacar", || s.env.fake.byte_limit_reached()).await;

        let got = tokio::time::timeout(Duration::from_secs(5), s.cache.read(&id, MIB as u64, 10))
            .await
            .expect("o bloco já chegou e a leitura esperou o pedido inteiro")
            .unwrap();

        assert_eq!(got.as_ref(), &data(MIB + 10)[MIB..]);
        full.abort();
    }

    /// Caça a bugs: um download cancelado no meio (Ctrl+C num `cp`, leitura interrompida)
    /// solta a reserva; o próximo leitor baixa o trecho em vez de esperar para sempre.
    #[tokio::test]
    async fn canceled_download_releases_its_range() {
        let (s, ids) = setup(&[("grande.bin", 8 * MIB)], None).await;
        let id = ids[0].clone();
        s.env
            .fake
            .inject(Fault::StallConnectionAfterBytes(MIB as u64));
        let cache = s.cache.clone();
        let stuck = tokio::spawn({
            let id = id.clone();
            async move { cache.ensure_full(&id).await }
        });
        wait_until("o download empacar", || s.env.fake.byte_limit_reached()).await;
        stuck.abort();
        let _ = stuck.await;
        s.env.fake.clear_faults();

        let at = 6 * MIB;
        let got = tokio::time::timeout(Duration::from_secs(5), s.cache.read(&id, at as u64, 10))
            .await
            .expect("a reserva do download cancelado ficou presa")
            .unwrap();

        assert_eq!(got.as_ref(), &data(8 * MIB)[at..at + 10]);
    }

    #[tokio::test]
    async fn offline_with_missing_block_is_offline_error_but_cached_block_still_reads() {
        let (s, ids) = setup(&[("a.bin", 4 * MIB)], None).await;
        let id = &ids[0];
        s.cache.read(id, 0, 10).await.unwrap();
        s.env.fake.inject(Fault::Offline);
        assert!(matches!(
            s.cache.read(id, 3 * MIB as u64, 10).await,
            Err(CacheError::Offline)
        ));
        assert_eq!(
            s.cache.read(id, 0, 10).await.unwrap().as_ref(),
            &data(10)[..]
        );
    }

    /// A troca do ID temporário pelo do Drive tem um instante em que o banco já tem o ID novo e o
    /// arquivo ainda não foi renomeado: uma leitura nesse instante não pode dar erro.
    #[tokio::test]
    async fn read_between_id_swap_and_rename_still_finds_the_content() {
        let (s, ids) = setup(&[("a.bin", 10)], None).await;
        let old = &ids[0];
        s.cache.read(old, 0, 10).await.unwrap();
        let new = ItemId("id-do-drive".into());

        s.cache.link_local(old, &new).await.unwrap();
        s.env.store.replace_id(old, &new).unwrap();
        let during = s.cache.read(&new, 0, 10).await;
        s.cache.unlink_old_id(old, &new).await.unwrap();
        let after = s.cache.read(&new, 0, 10).await;

        assert_eq!(during.unwrap().as_ref(), &data(10)[..]);
        assert_eq!(after.unwrap().as_ref(), &data(10)[..]);
        assert!(!s.cache.local_path(old).exists(), "o nome velho some");
    }

    /// "Limpar cache" das Preferências: sai tudo o que não é off-line nem edição por enviar.
    #[tokio::test]
    async fn clear_cache_keeps_pinned_and_pending() {
        let names = [("a", MIB), ("b", MIB), ("c", MIB), ("d", 10)];
        let (s, ids) = setup(&names, None).await;
        for id in &ids {
            s.cache.ensure_full(id).await.unwrap();
        }
        let store = &s.env.store;
        let mut pinned = store.get(&ids[1]).unwrap().unwrap();
        pinned.pin = PinState::Pinned;
        let mut dirty = store.get(&ids[2]).unwrap().unwrap();
        dirty.local_state = LocalState::Dirty;
        store.upsert_items(&[pinned, dirty]).unwrap();

        assert_eq!(s.cache.clear().await.unwrap(), MIB as u64 + 10);

        assert_eq!(store.cached_bytes().unwrap(), 2 * MIB as u64);
        assert!(!s.cache.local_path(&ids[0]).exists());
        assert!(!s.cache.local_path(&ids[3]).exists());
        assert!(s.cache.local_path(&ids[1]).exists() && s.cache.local_path(&ids[2]).exists());
        assert_eq!(s.cache.clear().await.unwrap(), 0);
    }

    #[tokio::test]
    async fn eviction_respects_limit_and_skips_pinned_and_dirty() {
        let names = [("a", MIB), ("b", MIB), ("c", MIB), ("d", MIB)];
        let (s, ids) = setup(&names, Some(2048)).await;
        for id in &ids {
            s.cache.ensure_full(id).await.unwrap();
        }
        let store = &s.env.store;
        let mut pinned = store.get(&ids[1]).unwrap().unwrap();
        pinned.pin = PinState::Pinned;
        let mut dirty = store.get(&ids[2]).unwrap().unwrap();
        dirty.local_state = LocalState::Dirty;
        store.upsert_items(&[pinned, dirty]).unwrap();

        assert_eq!(s.cache.evict_if_needed().await.unwrap(), 2 * MIB as u64);

        let cached = |id: &ItemId| store.ranges_missing(id, 0, MIB as u64).unwrap().is_empty();
        assert!(
            !cached(&ids[0]) && !cached(&ids[3]),
            "a e d saíram do cache"
        );
        assert!(cached(&ids[1]) && cached(&ids[2]), "fixado e editado ficam");
        assert!(!s.cache.local_path(&ids[0]).exists());
        assert!(s.cache.local_path(&ids[2]).exists());
        assert_eq!(s.cache.evict_if_needed().await.unwrap(), 0);
    }

    /// Espaço livre que só volta quando o arquivo dado sai do disco.
    struct FreedBy(std::path::PathBuf);

    impl FreeSpace for FreedBy {
        fn free_kbytes(&self, _dir: &std::path::Path) -> std::io::Result<u64> {
            Ok(if self.0.exists() { 100_000 } else { u64::MAX })
        }
    }

    /// Bug: sem limite de cache configurado, nada era liberado; com o disco no mínimo livre,
    /// todo arquivo ainda não baixado dava erro.
    #[tokio::test]
    async fn low_disk_frees_the_oldest_cached_item_before_refusing() {
        let (s, ids) = setup(&[("velho.bin", MIB), ("novo.bin", MIB)], None).await;
        let dir = tempfile::tempdir().unwrap();
        let old_path = dir.path().join("content").join(&ids[0].0);
        let (tx, _events) = broadcast::channel(16);
        let cache = BlockCache::with_free_space(
            s.env.store.clone(),
            s.env.client.clone(),
            dir.path().to_path_buf(),
            None,
            512_000,
            tx,
            Arc::new(FreedBy(old_path.clone())),
        );
        cache.ensure_full(&ids[0]).await.unwrap();
        assert!(old_path.exists());

        let got = cache.read(&ids[1], 0, 10).await.unwrap();

        assert_eq!(got.as_ref(), &data(MIB)[..10]);
        assert!(!old_path.exists(), "o mais antigo saiu do cache");
    }

    #[tokio::test]
    async fn low_disk_refuses_cache_writes_and_emits_event() {
        let (mut s, ids) = setup(&[("a.bin", 2 * MIB)], None).await;
        s.cache.read(&ids[0], 0, 10).await.unwrap();
        s.free.0.store(100_000, Ordering::SeqCst);
        assert!(matches!(
            s.cache.read(&ids[0], MIB as u64, 10).await,
            Err(CacheError::DiskFull)
        ));
        assert_eq!(
            s.events.try_recv().unwrap(),
            EngineEvent::DiskLow { free_kb: 100_000 }
        );
        assert!(
            s.cache.read(&ids[0], 0, 10).await.is_ok(),
            "o que já está no cache ainda lê"
        );
    }
}
