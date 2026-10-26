//! Semeadura das miniaturas de uma pasta: as imagens que o Google já gerou vão para o cache do
//! freedesktop, e o Dolphin não precisa baixar os arquivos.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock, RwLockWriteGuard};

use gdl_drive::client::{DriveClient, DriveError, ThumbLink};
use gdl_store::{Item, ItemId, ItemKind, LocalState, Store, StoreError, ThumbRec};
use tokio::sync::Semaphore;
use tokio::task::{JoinHandle, JoinSet};
use tokio_util::sync::CancellationToken;

use super::freedesktop::{
    Stamp, ThumbError, ThumbSize, file_for, remove_all, stored_stamp, uri_of, write_all,
};
use super::mtime_secs;

/// Downloads (com a gravação) de miniatura em paralelo, somando todas as pastas.
const PARALLEL: usize = 8;
/// Lado pedido ao Google: o do maior tamanho gravado.
const FETCH_SIZE: u32 = 512;

/// Erro de uma etapa que mistura arquivo e banco; só vai para o log.
type AnyError = Box<dyn std::error::Error + Send + Sync>;

/// Arquivo comum sem edição local: o que dá para decidir sem consultar o banco.
fn seedable(item: &Item) -> bool {
    item.kind == ItemKind::File && item.local_state == LocalState::Clean
}

/// Um item `seedable` precisa de miniatura se não tem semeadura em dia (mesma revisão, mesma
/// data e mesmo caminho) e não está inteiro no cache local. `complete` (que consulta o banco)
/// só roda quando a memória não basta para decidir.
fn needs_seed(
    item: &Item,
    rec: Option<&ThumbRec>,
    uri: &str,
    complete: impl FnOnce() -> Result<bool, StoreError>,
) -> Result<bool, StoreError> {
    let up_to_date = rec.is_some_and(|r| {
        r.revision == item.head_revision && r.modified_ms == item.modified_ms && r.uri == uri
    });
    Ok(!up_to_date && !complete()?)
}

/// Falha de download que não passa esperando (link que não é de imagem do Google, item sumido,
/// sem permissão, redirecionamento): o Google responderia igual na próxima abertura. Rede (até
/// a queda no meio da imagem), erro do servidor e limite de taxa ou de cota são passageiros.
fn permanent(e: &DriveError) -> bool {
    matches!(
        e,
        DriveError::RefusedLink(_)
            | DriveError::NotFound
            | DriveError::PermissionDenied
            | DriveError::Http(300..=399)
    )
}

/// Um arquivo a semear: o registro que fica no banco depois de gravado e o tamanho do arquivo.
struct Job {
    rec: ThumbRec,
    size: u64,
}

pub(crate) struct ThumbSeeder {
    store: Arc<Store>,
    client: Arc<DriveClient>,
    pub(crate) dir: PathBuf,
    /// Token da geração atual. Cada gravação roda com a trava de leitura e confere antes o seu
    /// token; a limpeza cancela com a de escrita, então nada grava depois dela.
    generation: RwLock<CancellationToken>,
    /// Pastas em semeadura, com o número de cada semeadura.
    running: Mutex<HashMap<ItemId, u64>>,
    next_id: AtomicU64,
    /// Limite de downloads e gravações em curso, para o motor inteiro.
    permits: Arc<Semaphore>,
    /// Downloads e gravações em curso (para os testes conferirem o limite).
    #[cfg(test)]
    busy: tests::Busy,
}

/// Ao fim da semeadura (inclusive por pânico): sai da lista.
struct Finish {
    seeder: Arc<ThumbSeeder>,
    folder: ItemId,
    id: u64,
}

impl Drop for Finish {
    fn drop(&mut self) {
        let mut running = self.seeder.running();
        // A limpeza esvazia a lista: a pasta pode já ser de uma semeadura nova.
        if running.get(&self.folder) == Some(&self.id) {
            running.remove(&self.folder);
        }
    }
}

impl ThumbSeeder {
    pub(crate) fn new(store: Arc<Store>, client: Arc<DriveClient>, dir: PathBuf) -> ThumbSeeder {
        ThumbSeeder {
            store,
            client,
            dir,
            generation: RwLock::default(),
            running: Mutex::default(),
            next_id: AtomicU64::new(0),
            permits: Arc::new(Semaphore::new(PARALLEL)),
            #[cfg(test)]
            busy: tests::Busy::default(),
        }
    }

    fn running(&self) -> MutexGuard<'_, HashMap<ItemId, u64>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Ver `Engine::seed_thumbnails`.
    pub(crate) fn seed(
        self: &Arc<Self>,
        folder: &ItemId,
        folder_path: PathBuf,
    ) -> Option<JoinHandle<()>> {
        // Travas sempre na ordem `generation` → `running`, como na limpeza. Com a de leitura
        // presa até registrar a pasta, uma limpeza vem inteira antes (e esta semeadura nasce na
        // geração nova) ou inteira depois (e cancela esta, mesmo antes de começar).
        let generation = self
            .generation
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        let mut running = self.running();
        if running.contains_key(folder) {
            return None;
        }
        let finish = Finish {
            seeder: self.clone(),
            folder: folder.clone(),
            id: self.next_id.fetch_add(1, Ordering::Relaxed),
        };
        running.insert(folder.clone(), finish.id);
        let cancel = generation.child_token();
        drop(running);
        drop(generation);
        Some(tokio::spawn(async move {
            let seeder = finish.seeder.clone();
            tokio::select! {
                () = cancel.cancelled() => {}
                () = seeder.seed_folder(&finish.folder, folder_path, &cancel) => {}
            }
            drop(finish);
        }))
    }

    async fn seed_folder(
        self: &Arc<Self>,
        folder: &ItemId,
        folder_path: PathBuf,
        cancel: &CancellationToken,
    ) {
        let pending = async {
            let (this, folder, cancel) = (self.clone(), folder.clone(), cancel.clone());
            let jobs =
                tokio::task::spawn_blocking(move || this.pending(&folder, &folder_path, &cancel))
                    .await??;
            Ok::<_, AnyError>(jobs)
        };
        let pending = match pending.await {
            Ok(jobs) if jobs.is_empty() => return,
            Ok(jobs) => jobs,
            Err(e) => return tracing::debug!(erro = %e, "falha ao ler a pasta para miniaturas"),
        };
        let mut links: HashMap<String, ThumbLink> =
            match self.client.thumbnail_links(&folder.0).await {
                Ok(links) => links.into_iter().map(|l| (l.id.clone(), l)).collect(),
                Err(e) => {
                    return tracing::debug!(erro = %e, "falha ao listar as miniaturas da pasta");
                }
            };
        let mut tasks = JoinSet::new();
        let mut without_link = Vec::new();
        for job in pending {
            let Some(link) = links.remove(&job.rec.id.0) else {
                without_link.push(job.rec);
                continue;
            };
            // O Google já tem outra versão, que o feed de mudanças ainda não trouxe: a miniatura
            // seria dela. Sem memória, para tentar de novo quando o banco alcançar.
            if link.modified_ms != job.rec.modified_ms {
                continue;
            }
            let (this, cancel) = (self.clone(), cancel.clone());
            tasks.spawn(async move {
                // Presa até o fim da gravação.
                let Ok(_permit) = this.permits.clone().acquire_owned().await else {
                    return;
                };
                #[cfg(test)]
                let _busy = this.busy.enter();
                this.seed_one(job, &link.link, cancel).await;
            });
        }
        // Sem miniatura no Google: lembra mesmo assim, para não listar a pasta a cada abertura;
        // uma versão nova do arquivo faz tentar de novo.
        if !without_link.is_empty() {
            let (this, cancel) = (self.clone(), cancel.clone());
            tasks.spawn_blocking(move || this.remember(&without_link, &cancel));
        }
        while let Some(result) = tasks.join_next().await {
            if let Err(e) = result {
                tracing::debug!(erro = %e, "falha ao semear uma miniatura");
            }
        }
    }

    /// Os arquivos da pasta que precisam de miniatura. Os sem memória que já têm os três PNGs em
    /// dia (de uma semeadura cuja memória se perdeu) só voltam para ela.
    fn pending(
        &self,
        folder: &ItemId,
        folder_path: &Path,
        cancel: &CancellationToken,
    ) -> Result<Vec<Job>, StoreError> {
        let mut recs = self.store.thumbs_of_children(folder)?;
        let mut jobs = Vec::new();
        // Já com os três PNGs em dia: só voltam para a memória, todos de uma vez no fim.
        let mut current = Vec::new();
        for item in self.store.children(folder)?.into_iter().filter(seedable) {
            let uri = uri_of(&folder_path.join(&item.local_name));
            let rec = recs.remove(&item.id);
            let remembered = rec.is_some();
            if !needs_seed(&item, rec.as_ref(), &uri, || {
                self.store.is_complete(&item.id, item.size)
            })? {
                continue;
            }
            // Arquivo renomeado ou movido: os PNGs do caminho antigo sairiam da memória.
            if let Some(old) = rec.filter(|r| r.uri != uri) {
                self.unless_cancelled(cancel, || remove_all(&self.dir, &old.uri));
            }
            let job = Job {
                rec: ThumbRec {
                    id: item.id,
                    revision: item.head_revision,
                    modified_ms: item.modified_ms,
                    uri,
                },
                size: item.size,
            };
            // Só sem memória: com a de outra versão, a mesma data não prova que a imagem é desta.
            let stamp = Stamp {
                mtime: Some(mtime_secs(job.rec.modified_ms)),
                size: Some(job.size),
            };
            if !remembered
                && ThumbSize::ALL
                    .iter()
                    .all(|&s| stored_stamp(&file_for(&self.dir, s, &job.rec.uri)) == Some(stamp))
            {
                current.push(job.rec);
                continue;
            }
            jobs.push(job);
        }
        if !current.is_empty()
            && let Some(result) = self.unless_cancelled(cancel, || self.store.put_thumbs(&current))
        {
            result?;
        }
        Ok(jobs)
    }

    async fn seed_one(self: &Arc<Self>, job: Job, link: &str, cancel: CancellationToken) {
        let image = match self.client.fetch_thumbnail(link, FETCH_SIZE).await {
            Ok(image) => image,
            Err(e) => {
                tracing::debug!(erro = %e, item = %job.rec.id.0, "falha ao baixar a miniatura");
                // Lembrada sem PNG, como o item sem miniatura; uma versão nova tenta de novo. As
                // passageiras (rede, servidor, limite de taxa) ficam sem memória.
                if permanent(&e) {
                    let this = self.clone();
                    let remembered =
                        tokio::task::spawn_blocking(move || this.remember(&[job.rec], &cancel));
                    if let Err(e) = remembered.await {
                        tracing::debug!(erro = %e, "falha ao lembrar a miniatura que falhou");
                    }
                }
                return;
            }
        };
        let this = self.clone();
        let written = tokio::task::spawn_blocking(move || this.write(&job, &image, &cancel)).await;
        if let Err(e) = written {
            tracing::debug!(erro = %e, "falha ao gravar a miniatura");
        }
    }

    /// Grava os três tamanhos e lembra deles. Se algo falha, apaga o que ficou pela metade:
    /// um PNG sem memória não seria apagado na saída da conta. Imagem que não decodifica é
    /// lembrada sem PNG: o Google mandaria a mesma de novo.
    fn write(&self, job: &Job, image: &[u8], cancel: &CancellationToken) {
        let rec = &job.rec;
        let result = self.unless_cancelled(cancel, || {
            let mtime = mtime_secs(rec.modified_ms);
            match write_all(&self.dir, &rec.uri, mtime, job.size, image) {
                Err(ThumbError::Decode(e)) => {
                    tracing::debug!(erro = %e, item = %rec.id.0, "miniatura ilegível; lembrada sem PNG");
                }
                written => written?,
            }
            self.store.put_thumb(rec)?;
            Ok::<_, AnyError>(())
        });
        if let Some(Err(e)) = result {
            remove_all(&self.dir, &rec.uri);
            tracing::debug!(erro = %e, item = %rec.id.0, "falha ao gravar a miniatura");
        }
    }

    fn remember(&self, recs: &[ThumbRec], cancel: &CancellationToken) {
        if let Some(Err(e)) = self.unless_cancelled(cancel, || self.store.put_thumbs(recs)) {
            tracing::debug!(erro = %e, itens = recs.len(), "falha ao lembrar os itens sem miniatura");
        }
    }

    /// Roda `f` com a trava de leitura da geração, se a semeadura não foi cancelada.
    fn unless_cancelled<T>(&self, cancel: &CancellationToken, f: impl FnOnce() -> T) -> Option<T> {
        let _generation = self
            .generation
            .read()
            .unwrap_or_else(PoisonError::into_inner);
        (!cancel.is_cancelled()).then(f)
    }

    /// Cancela as semeaduras em curso e passa para uma geração nova. Devolve a trava de escrita,
    /// que esperou as gravações em curso: enquanto ela vive, nada grava.
    fn cancel_all(&self) -> RwLockWriteGuard<'_, CancellationToken> {
        // Ordem das travas: `generation` → `running`, a mesma de `seed`.
        let mut generation = self
            .generation
            .write()
            .unwrap_or_else(PoisonError::into_inner);
        generation.cancel();
        *generation = CancellationToken::new();
        // Uma semeadura nova da mesma pasta não deve esbarrar na cancelada.
        self.running().clear();
        generation
    }

    /// O motor parou: cancela as semeaduras em curso, sem apagar nada.
    pub(crate) fn stop(&self) {
        drop(self.cancel_all());
    }

    /// Ver `Engine::purge_thumbnails`.
    pub(crate) fn purge(&self) {
        // Com a trava presa até o fim, nenhuma gravação antiga volta depois da limpeza.
        let _generation = self.cancel_all();
        match self.store.all_thumbs() {
            Ok(recs) => recs.iter().for_each(|r| remove_all(&self.dir, &r.uri)),
            Err(e) => tracing::warn!(erro = %e, "falha ao listar as miniaturas semeadas"),
        }
        if let Err(e) = self.store.clear_thumbs() {
            tracing::warn!(erro = %e, "falha ao esquecer as miniaturas semeadas");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use gdl_fake_drive::{FakeDrive, Fault};
    use gdl_store::{Inode, ItemKind, LocalState, PinState};

    use super::*;
    use crate::engine::Engine;
    use crate::testutil::{indexed_engine, wait_until};
    use crate::thumbs::freedesktop::{
        ThumbSize, file_for, stored_mtime, stored_stamp, uri_of, write_all,
    };
    use crate::thumbs::mtime_secs;

    /// Conta o trabalho em curso e guarda o máximo simultâneo.
    #[derive(Default)]
    pub(super) struct Busy {
        now: std::sync::atomic::AtomicUsize,
        max: std::sync::atomic::AtomicUsize,
    }

    pub(super) struct BusyGuard<'a>(&'a Busy);

    impl Busy {
        pub(super) fn enter(&self) -> BusyGuard<'_> {
            let now = self.now.fetch_add(1, Ordering::SeqCst) + 1;
            self.max.fetch_max(now, Ordering::SeqCst);
            BusyGuard(self)
        }
    }

    impl Drop for BusyGuard<'_> {
        fn drop(&mut self) {
            self.0.now.fetch_sub(1, Ordering::SeqCst);
        }
    }

    const FOTO: &str = "Fotos de Março #1 (100%).jpg";
    const LIST: &str = "/drive/v3/files";

    fn file(name: &str) -> Item {
        Item {
            id: ItemId("f1".into()),
            inode: Inode(7),
            parent: Some(ItemId("root".into())),
            remote_name: name.into(),
            local_name: name.into(),
            kind: ItemKind::File,
            size: 10,
            remote_size: 10,
            md5: None,
            head_revision: Some("r1".into()),
            base_revision: Some("r1".into()),
            base_md5: None,
            modified_ms: 1_700_000_000_123,
            created_ms: 0,
            trashed: false,
            can_edit: true,
            local_state: LocalState::Clean,
            pin: PinState::None,
        }
    }

    fn rec_of(item: &Item, uri: &str) -> ThumbRec {
        ThumbRec {
            id: item.id.clone(),
            revision: item.head_revision.clone(),
            modified_ms: item.modified_ms,
            uri: uri.into(),
        }
    }

    #[test]
    fn needs_seed_rules() {
        let item = file("a.jpg");
        let uri = "file:///m/a.jpg";
        let needs = |item: &Item, rec: Option<&ThumbRec>, uri: &str, complete: bool| {
            seedable(item) && needs_seed(item, rec, uri, || Ok(complete)).unwrap()
        };
        assert!(needs(&item, None, uri, false));
        assert!(!needs(&item, None, uri, true), "completo no cache");

        for state in [
            LocalState::Dirty,
            LocalState::Uploading,
            LocalState::Conflict,
            LocalState::LocalOnly,
        ] {
            let item = Item {
                local_state: state,
                ..file("a.jpg")
            };
            assert!(!needs(&item, None, uri, false), "{state:?}");
        }
        for kind in [
            ItemKind::Folder,
            ItemKind::GoogleDoc {
                mime: "application/vnd.google-apps.document".into(),
            },
            ItemKind::Shortcut {
                target_id: ItemId("x".into()),
                target_mime: "image/jpeg".into(),
            },
        ] {
            let item = Item {
                kind: kind.clone(),
                ..file("a.jpg")
            };
            assert!(!needs(&item, None, uri, false), "{kind:?}");
        }

        let rec = rec_of(&item, uri);
        assert!(!needs(&item, Some(&rec), uri, false), "já em dia");
        // Memória em dia: decide sem consultar o cache local.
        let decided = needs_seed(&item, Some(&rec), uri, || {
            panic!("consultou o cache com a memória em dia")
        });
        assert!(!decided.unwrap());
        let other_time = ThumbRec {
            modified_ms: item.modified_ms + 1,
            ..rec.clone()
        };
        assert!(needs(&item, Some(&other_time), uri, false));
        assert!(
            !needs(&item, Some(&other_time), uri, true),
            "completo no cache"
        );
        let other_rev = ThumbRec {
            revision: Some("r0".into()),
            ..rec.clone()
        };
        assert!(needs(&item, Some(&other_rev), uri, false));
        assert!(needs(&item, Some(&rec), "file:///m/b.jpg", false));
    }

    fn tiny_png() -> Vec<u8> {
        let img = image::RgbaImage::from_pixel(4, 4, image::Rgba([0, 0, 0, 255]));
        let mut bytes = std::io::Cursor::new(Vec::new());
        img.write_to(&mut bytes, image::ImageFormat::Png).unwrap();
        bytes.into_inner()
    }

    struct Setup {
        fake: FakeDrive,
        engine: Arc<Engine>,
        folder: ItemId,
        folder_path: PathBuf,
        _tmp: tempfile::TempDir,
    }

    /// Motor indexado com a pasta `Fotos` (duas imagens e um texto).
    async fn setup() -> (Setup, [String; 3]) {
        let fake = FakeDrive::start().await;
        let folder = fake.add_folder(&fake.root_id(), "Fotos");
        let a = fake.add_file_with_mime(&folder, FOTO, "image/jpeg", vec![1; 10]);
        let b = fake.add_file_with_mime(&folder, "b.png", "image/png", vec![2; 10]);
        let c = fake.add_file(&folder, "c.txt", b"texto".to_vec());
        let (engine, tmp) = indexed_engine(&fake).await;
        let folder_path = tmp.path().join("montagem/Meu Drive/Fotos");
        let s = Setup {
            fake,
            engine: Arc::new(engine),
            folder: ItemId(folder),
            folder_path,
            _tmp: tmp,
        };
        (s, [a, b, c])
    }

    impl Setup {
        fn dir(&self) -> &Path {
            &self.engine.thumbs.dir
        }

        async fn seed(&self) {
            self.engine
                .seed_thumbnails(&self.folder, self.folder_path.clone())
                .expect("nenhuma semeadura em andamento")
                .await
                .unwrap();
        }

        fn uri(&self, name: &str) -> String {
            uri_of(&self.folder_path.join(name))
        }

        fn mtimes(&self, name: &str) -> Vec<Option<i64>> {
            let uri = self.uri(name);
            ThumbSize::ALL
                .iter()
                .map(|&s| stored_mtime(&file_for(self.dir(), s, &uri)))
                .collect()
        }

        fn item(&self, id: &str) -> Item {
            self.engine.store.get(&ItemId(id.into())).unwrap().unwrap()
        }

        fn pngs(&self) -> usize {
            ThumbSize::ALL
                .iter()
                .filter_map(|s| std::fs::read_dir(self.dir().join(s.dir_name())).ok())
                .flatten()
                .count()
        }
    }

    #[tokio::test]
    async fn seeds_folder_without_downloading_content() {
        let (s, [a, b, c]) = setup().await;
        s.seed().await;

        for (id, name) in [(&a, FOTO), (&b, "b.png")] {
            let mtime = mtime_secs(s.item(id).modified_ms);
            assert_eq!(s.mtimes(name), vec![Some(mtime); 3], "{name}");
            let rec = s.engine.store.thumb(&ItemId(id.clone())).unwrap().unwrap();
            assert_eq!(rec.uri, s.uri(name));
        }
        assert!(
            s.uri(FOTO)
                .contains("Fotos%20de%20Mar%C3%A7o%20%231%20(100%25).jpg")
        );
        assert_eq!(s.mtimes("c.txt"), vec![None; 3]);
        // Sem miniatura no Google: só lembrado, para a próxima abertura não chamar a API.
        assert!(s.engine.store.thumb(&ItemId(c)).unwrap().is_some());
        assert_eq!(s.pngs(), 6);
        assert_eq!(s.fake.downloaded_bytes(), 0);
        assert_eq!(s.fake.requests("/thumb"), 2);
    }

    #[tokio::test]
    async fn second_seed_makes_no_requests() {
        let (s, _) = setup().await;
        s.seed().await;
        let (list, thumbs) = (s.fake.requests(LIST), s.fake.requests("/thumb"));
        s.seed().await;
        assert_eq!(s.fake.requests(LIST), list);
        assert_eq!(s.fake.requests("/thumb"), thumbs);
    }

    #[tokio::test]
    async fn remote_change_reseeds_with_new_mtime() {
        let (s, [a, ..]) = setup().await;
        s.seed().await;
        let old = mtime_secs(s.item(&a).modified_ms);

        // No segundo seguinte: o `Thumb::MTime` só muda de segundo em segundo.
        let ms = gdl_common::time::now_ms();
        tokio::time::sleep(std::time::Duration::from_millis((1005 - ms % 1000) as u64)).await;
        s.fake.modify_remote(&a, vec![3; 20]);
        s.engine.poller.run_once().await.unwrap();
        let new = mtime_secs(s.item(&a).modified_ms);
        assert!(new > old);

        s.seed().await;
        assert_eq!(s.mtimes(FOTO), vec![Some(new); 3]);
        assert_eq!(s.fake.requests("/thumb"), 3);
        let rec = s.engine.store.thumb(&ItemId(a.clone())).unwrap().unwrap();
        assert_eq!(rec.revision, s.item(&a).head_revision);
    }

    #[tokio::test]
    async fn folder_with_only_docs_does_not_call_api() {
        let fake = FakeDrive::start().await;
        let folder = fake.add_folder(&fake.root_id(), "Docs");
        fake.add_gdoc(&folder, "Texto", "application/vnd.google-apps.document");
        fake.add_folder(&folder, "Sub");
        let (engine, tmp) = indexed_engine(&fake).await;
        let engine = Arc::new(engine);
        let list = fake.requests(LIST);
        engine
            .seed_thumbnails(&ItemId(folder), tmp.path().join("m/Docs"))
            .unwrap()
            .await
            .unwrap();
        assert_eq!(fake.requests(LIST), list);
    }

    #[tokio::test]
    async fn concurrent_seeds_of_same_folder_fetch_once() {
        // Referência: uma semeadura sozinha, numa pasta igual.
        let (alone, _) = setup().await;
        let list = alone.fake.requests(LIST);
        alone.seed().await;
        let alone_lists = alone.fake.requests(LIST) - list;
        assert_eq!(alone_lists, 1);

        let (s, _) = setup().await;
        let list = s.fake.requests(LIST);
        let first = s.engine.seed_thumbnails(&s.folder, s.folder_path.clone());
        let second = s.engine.seed_thumbnails(&s.folder, s.folder_path.clone());
        // Quem chega durante a semeadura não recebe o que esperar: só quem a iniciou espera.
        assert!(second.is_none());
        first.expect("a primeira inicia").await.unwrap();
        assert_eq!(s.fake.requests("/thumb"), 2);
        assert_eq!(s.fake.requests(LIST) - list, alone_lists, "uma só listagem");
        assert_eq!(s.pngs(), 6);
    }

    /// O limite de 8 vale para o motor inteiro, não para cada pasta.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn parallel_limit_is_global_across_folders() {
        let fake = FakeDrive::start().await;
        let folders: Vec<String> = (0..2)
            .map(|f| {
                let folder = fake.add_folder(&fake.root_id(), &format!("Pasta {f}"));
                for i in 0..12 {
                    fake.add_file_with_mime(&folder, &format!("{i}.png"), "image/png", vec![1; 10]);
                }
                folder
            })
            .collect();
        let (engine, tmp) = indexed_engine(&fake).await;
        let engine = Arc::new(engine);
        let handles: Vec<_> = folders
            .iter()
            .map(|f| {
                engine
                    .seed_thumbnails(&ItemId(f.clone()), tmp.path().join("m").join(f))
                    .unwrap()
            })
            .collect();
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(fake.requests("/thumb"), 24);
        let max = engine.thumbs.busy.max.load(Ordering::SeqCst);
        assert!(max <= PARALLEL, "{max} em voo ao mesmo tempo");
    }

    #[tokio::test]
    async fn api_error_is_swallowed() {
        let (s, _) = setup().await;
        s.fake.inject(Fault::Status {
            code: 500,
            times: 100,
        });
        s.seed().await;
        assert_eq!(s.pngs(), 0);
        assert!(s.engine.store.all_thumbs().unwrap().is_empty());
    }

    /// Falha que não passa esperando (redirecionamento, 403, 404, 410): lembrada sem PNG, como o item
    /// sem miniatura; a próxima abertura não chama a API. Antes, nada ficava na memória e cada
    /// abertura da pasta repetia a listagem e os downloads condenados.
    #[tokio::test]
    async fn permanent_thumbnail_errors_are_remembered() {
        for fault in ["redirecionamento", "403", "404", "410"] {
            let (s, [a, b, _]) = setup().await;
            match fault {
                "redirecionamento" => s.fake.redirect_thumbs_to("http://exemplo.invalid/x"),
                code => s.fake.inject(Fault::ThumbStatus(code.parse().unwrap())),
            }
            s.seed().await;
            assert_eq!(s.pngs(), 0, "{fault}");
            for id in [&a, &b] {
                let rec = s.engine.store.thumb(&ItemId(id.clone())).unwrap();
                assert_eq!(
                    rec.map(|r| r.revision),
                    Some(s.item(id).head_revision),
                    "{fault}"
                );
            }
            assert_eq!(s.fake.requests("/thumb"), 2, "{fault}");
            let list = s.fake.requests(LIST);
            s.seed().await;
            assert_eq!(s.fake.requests("/thumb"), 2, "{fault}: baixou de novo");
            assert_eq!(s.fake.requests(LIST), list, "{fault}: listou de novo");
        }
    }

    /// Falha passageira (servidor, limite de taxa): sem memória, a próxima abertura tenta de novo.
    #[tokio::test]
    async fn transient_thumbnail_errors_are_not_remembered() {
        for code in [500, 429] {
            let (s, [a, b, _]) = setup().await;
            s.fake.inject(Fault::ThumbStatus(code));
            s.seed().await;
            assert_eq!(s.pngs(), 0, "{code}");
            for id in [&a, &b] {
                let rec = s.engine.store.thumb(&ItemId(id.clone())).unwrap();
                assert!(rec.is_none(), "{code}");
            }
            let thumbs = s.fake.requests("/thumb");
            s.fake.clear_faults();
            s.seed().await;
            assert_eq!(s.fake.requests("/thumb"), thumbs + 2, "{code}");
            assert_eq!(s.pngs(), 6, "{code}");
        }
    }

    /// Bug: a conexão que caía no meio do corpo virava `Protocol` (o reqwest chama de erro de
    /// decodificação) e a miniatura ficava lembrada como inexistente até o arquivo mudar.
    #[tokio::test]
    async fn thumbnail_body_cut_is_not_remembered() {
        let (s, [a, b, _]) = setup().await;
        s.fake.inject(Fault::CutThumbBody);
        s.seed().await;
        assert_eq!(s.pngs(), 0);
        for id in [&a, &b] {
            assert!(s.engine.store.thumb(&ItemId(id.clone())).unwrap().is_none());
        }
        let thumbs = s.fake.requests("/thumb");
        s.fake.clear_faults();
        s.seed().await;
        assert_eq!(s.fake.requests("/thumb"), thumbs + 2);
        assert_eq!(s.pngs(), 6);
    }

    /// Imagem que não decodifica: o Google mandaria a mesma de novo; lembrada sem PNG.
    #[tokio::test]
    async fn undecodable_thumbnail_is_remembered_without_png() {
        let (s, [a, ..]) = setup().await;
        let item = s.item(&a);
        let job = Job {
            rec: rec_of(&item, &s.uri(FOTO)),
            size: item.size,
        };
        s.engine
            .thumbs
            .write(&job, b"nao e imagem", &CancellationToken::new());
        assert_eq!(s.pngs(), 0);
        assert_eq!(s.engine.store.thumb(&ItemId(a)).unwrap(), Some(job.rec));
    }

    /// Memória de outra revisão: baixa de novo, mesmo com PNGs do mesmo `Thumb::MTime` (a data
    /// sozinha não prova que a imagem é desta versão).
    #[tokio::test]
    async fn rec_of_old_revision_redownloads_despite_current_pngs() {
        let (s, [a, ..]) = setup().await;
        s.seed().await;
        let old = ThumbRec {
            revision: Some("r-antiga".into()),
            ..s.engine.store.thumb(&ItemId(a.clone())).unwrap().unwrap()
        };
        s.engine.store.put_thumb(&old).unwrap();
        let thumbs = s.fake.requests("/thumb");

        s.seed().await;
        assert_eq!(s.fake.requests("/thumb"), thumbs + 1);
        let rec = s.engine.store.thumb(&ItemId(a.clone())).unwrap().unwrap();
        assert_eq!(rec.revision, s.item(&a).head_revision);
    }

    /// Sem memória, os PNGs só são adotados se também o `Thumb::Size` bate com o arquivo.
    #[tokio::test]
    async fn pngs_of_other_size_are_not_adopted() {
        let (s, [a, ..]) = setup().await;
        s.seed().await;
        s.engine.store.clear_thumbs().unwrap();
        let mtime = mtime_secs(s.item(&a).modified_ms);
        write_all(s.dir(), &s.uri(FOTO), mtime, 999, &tiny_png()).unwrap();
        let thumbs = s.fake.requests("/thumb");

        s.seed().await;
        assert_eq!(s.fake.requests("/thumb"), thumbs + 1, "só a foto é baixada");
        let size = stored_stamp(&file_for(s.dir(), ThumbSize::Normal, &s.uri(FOTO)))
            .and_then(|st| st.size);
        assert_eq!(size, Some(s.item(&a).size));
    }

    /// O Google já tem uma versão mais nova que o banco (o feed de mudanças ainda não chegou):
    /// a miniatura seria da outra versão. Pula sem memória e semeia depois do feed.
    #[tokio::test]
    async fn remote_version_ahead_of_local_is_skipped_without_memory() {
        let (s, [a, b, _]) = setup().await;
        s.fake.modify_remote(&a, vec![3; 20]);

        s.seed().await;
        assert_eq!(s.mtimes(FOTO), vec![None; 3]);
        assert!(s.engine.store.thumb(&ItemId(a.clone())).unwrap().is_none());
        assert!(s.engine.store.thumb(&ItemId(b)).unwrap().is_some());
        assert_eq!(s.fake.requests("/thumb"), 1);

        s.engine.poller.run_once().await.unwrap();
        s.seed().await;
        let mtime = mtime_secs(s.item(&a).modified_ms);
        assert_eq!(s.mtimes(FOTO), vec![Some(mtime); 3]);
        assert_eq!(s.fake.requests("/thumb"), 2);
    }

    #[tokio::test]
    async fn renamed_file_drops_old_pngs() {
        let (s, [a, ..]) = setup().await;
        s.seed().await;
        let mut item = s.item(&a);
        item.local_name = "renomeada.jpg".into();
        s.engine.store.upsert_item(&item).unwrap();

        s.seed().await;
        assert_eq!(s.mtimes(FOTO), vec![None; 3]);
        assert_eq!(
            s.mtimes("renomeada.jpg"),
            vec![Some(mtime_secs(item.modified_ms)); 3]
        );
        assert_eq!(s.pngs(), 6);
    }

    #[tokio::test]
    async fn partial_write_is_redone() {
        let (s, [a, ..]) = setup().await;
        s.seed().await;
        // Gravação interrompida no meio (disco cheio): falta o `x-large` e a memória.
        std::fs::remove_file(file_for(s.dir(), ThumbSize::XLarge, &s.uri(FOTO))).unwrap();
        s.engine.store.clear_thumbs().unwrap();

        s.seed().await;
        let mtime = mtime_secs(s.item(&a).modified_ms);
        assert_eq!(s.mtimes(FOTO), vec![Some(mtime); 3]);
        assert_eq!(
            s.fake.requests("/thumb"),
            3,
            "só a foto incompleta é baixada"
        );
        assert_eq!(s.engine.store.all_thumbs().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn current_pngs_are_remembered_without_api() {
        let (s, _) = setup().await;
        s.seed().await;
        s.engine.store.clear_thumbs().unwrap();
        let (list, thumbs) = (s.fake.requests(LIST), s.fake.requests("/thumb"));
        s.seed().await;
        assert_eq!(s.fake.requests("/thumb"), thumbs);
        assert_eq!(s.engine.store.all_thumbs().unwrap().len(), 3);
        // Só o `c.txt` (sem PNG) ainda pede a listagem.
        assert_eq!(s.fake.requests(LIST), list + 1);
    }

    #[tokio::test]
    async fn stale_png_with_other_mtime_is_rewritten() {
        let (s, [a, ..]) = setup().await;
        let uri = s.uri(FOTO);
        // Miniatura antiga, de outra versão do arquivo.
        write_all(s.dir(), &uri, 1, 10, &tiny_png()).unwrap();

        s.seed().await;
        let mtime = mtime_secs(s.item(&a).modified_ms);
        assert_eq!(s.mtimes(FOTO), vec![Some(mtime); 3]);
    }

    #[tokio::test]
    async fn dropped_handle_keeps_seeding() {
        let (s, _) = setup().await;
        let first = s.engine.seed_thumbnails(&s.folder, s.folder_path.clone());
        let timed_out = tokio::time::timeout(
            std::time::Duration::from_millis(1),
            first.expect("a primeira inicia"),
        )
        .await;
        drop(timed_out);
        wait_until("semeadura de fundo", || {
            s.engine.store.all_thumbs().unwrap().len() == 3
        })
        .await;
        assert_eq!(s.pngs(), 6);
    }

    /// Parar o motor (desmontagem) cancela as semeaduras em curso, sem apagar o que já foi feito.
    #[tokio::test]
    async fn engine_stop_cancels_running_seeds_without_purging() {
        let (s, [a, ..]) = setup().await;
        s.seed().await;
        s.fake.modify_remote(&a, vec![3; 20]);
        s.engine.poller.run_once().await.unwrap();
        s.fake
            .inject(Fault::DelayThumbs(std::time::Duration::from_secs(10)));
        let running = s
            .engine
            .seed_thumbnails(&s.folder, s.folder_path.clone())
            .unwrap();

        let stop = CancellationToken::new();
        let run = tokio::spawn({
            let (engine, stop) = (s.engine.clone(), stop.clone());
            async move { engine.run(stop).await }
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        stop.cancel();
        run.await.unwrap();
        tokio::time::timeout(std::time::Duration::from_secs(2), running)
            .await
            .expect("a semeadura parou com o motor")
            .unwrap();
        // Nada foi apagado: a memória e os PNGs da semeadura anterior ficam.
        assert_eq!(s.engine.store.all_thumbs().unwrap().len(), 3);
        assert_eq!(s.pngs(), 6);
    }

    #[tokio::test]
    async fn purge_removes_files_and_memory() {
        let (s, _) = setup().await;
        s.seed().await;
        assert_eq!(s.pngs(), 6);
        // PNG de outro caminho, que não foi semeado por nós: fica.
        write_all(s.dir(), "file:///outro.jpg", 1, 10, &tiny_png()).unwrap();

        s.engine.purge_thumbnails();
        assert_eq!(s.pngs(), 3);
        assert_eq!(s.mtimes(FOTO), vec![None; 3]);
        assert!(s.engine.store.all_thumbs().unwrap().is_empty());
    }

    #[tokio::test]
    async fn purge_cancels_running_seed_and_seeding_works_after() {
        let (s, _) = setup().await;
        let running = s.engine.seed_thumbnails(&s.folder, s.folder_path.clone());
        s.engine.purge_thumbnails();
        running.unwrap().await.unwrap();
        assert_eq!(s.pngs(), 0, "nada gravado depois da limpeza");
        assert!(s.engine.store.all_thumbs().unwrap().is_empty());

        s.seed().await;
        assert_eq!(s.pngs(), 6);
        assert_eq!(s.engine.store.all_thumbs().unwrap().len(), 3);
    }

    /// Limpeza no meio dos downloads e gravações, em várias threads: nada sobra depois dela,
    /// nem quando uma gravação já estava em curso.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn purge_during_seeding_leaves_nothing() {
        use std::time::Instant;
        let (s, _) = setup().await;
        let start = Instant::now();
        s.seed().await;
        let full = start.elapsed();
        s.engine.purge_thumbnails();
        // A gravação ocupa a maior parte da semeadura: limpa em vários pontos dela.
        for i in 1..=4u32 {
            let running = s.engine.seed_thumbnails(&s.folder, s.folder_path.clone());
            tokio::time::sleep(full * i / 8).await;
            s.engine.purge_thumbnails();
            running.unwrap().await.unwrap();
            // Uma gravação que escapasse da limpeza terminaria aqui.
            tokio::time::sleep(full).await;
            assert_eq!(s.pngs(), 0, "rodada {i}");
            assert!(
                s.engine.store.all_thumbs().unwrap().is_empty(),
                "rodada {i}"
            );
        }
    }
}
