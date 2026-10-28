//! Fachada do motor: junta banco, cache, fixados, fila de envio e mudanças, e roda os laços do
//! serviço com pausa.

use std::collections::HashSet;
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use gdl_common::config::Preferences;
use gdl_common::fs::write_atomic;
use gdl_common::paths::{Paths, create_private_dir};
use gdl_common::time::now_ms;
use gdl_drive::client::DriveClient;
use gdl_drive::model::About;
use gdl_store::Store;
use tokio::sync::{Notify, broadcast, watch};
use tokio_util::sync::CancellationToken;

use crate::Result;
use crate::activity::{Activity, ActivityLog, ActivityRow, ActivityState, MAX_ROWS};
use crate::cache::{BlockCache, CONTENT_DIR};
use crate::changes::ChangePoller;
use crate::errors::{self, SyncError};
use crate::events::{EngineEvent, SyncSummary};
use crate::indexer::{ACCOUNT_EMAIL, START_PAGE_TOKEN, initial_index};
use crate::kept::{Kept, KeptCopies, KeptFolder};
use crate::pins::PinManager;
use crate::thumbs::seeder::ThumbSeeder;
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
    /// O Google recusa o login da conta (vem do provedor de tokens do cliente).
    signed_out: watch::Receiver<bool>,
    /// Por que está pausado; `paused` é a soma dos motivos.
    pause_reasons: std::sync::Mutex<PauseReasons>,
    /// Já avisou desta pausa que uma gravação espera a sincronização voltar.
    saved_while_paused: AtomicBool,
    pub(crate) activity: Arc<ActivityLog>,
    pub(crate) thumbs: Arc<ThumbSeeder>,
}

/// Motivos da pausa da sincronização.
#[derive(Debug, Default)]
struct PauseReasons {
    user: bool,
    /// `PauseSyncingOnMeteredConnection` numa conexão limitada.
    metered: bool,
}

/// Arquivo (na pasta de dados da conta) com a pasta de cache da última abertura, para saber se
/// `ContentCachePath` mudou.
const CACHE_DIR_FILE: &str = "cache_dir";

/// Pasta de dados da conta (banco, `Lost and Found` e, por padrão, o cache).
fn data_dir_of(paths: &Paths, account: &str) -> PathBuf {
    paths.data_dir.join(account)
}

/// Pasta do cache da conta: `ContentCachePath/<conta>` quando configurada, senão a dos dados.
fn cache_dir_of(paths: &Paths, prefs: &Preferences, account: &str) -> PathBuf {
    prefs
        .content_cache_path
        .as_ref()
        .map_or_else(|| data_dir_of(paths, account), |p| p.join(account))
}

/// A pasta de cache gravada na última abertura (a dos dados, se nunca foi gravada).
fn last_cache_dir(data_dir: &Path) -> PathBuf {
    std::fs::read(data_dir.join(CACHE_DIR_FILE))
        .map(|bytes| PathBuf::from(std::ffi::OsString::from_vec(bytes)))
        .unwrap_or_else(|_| data_dir.to_path_buf())
}

/// A mudança de lugar do cache que a próxima abertura do motor fará (de, para), quando
/// `ContentCachePath` mudou desde a última e o cache antigo tem conteúdo.
pub fn pending_cache_move(
    paths: &Paths,
    prefs: &Preferences,
    account: &str,
) -> Option<(PathBuf, PathBuf)> {
    let old = last_cache_dir(&data_dir_of(paths, account));
    let new = cache_dir_of(paths, prefs, account);
    (old != new && old.join(CONTENT_DIR).is_dir()).then_some((old, new))
}

/// Move o conteúdo do cache: `rename` no mesmo disco; entre discos (ou com o destino já
/// ocupado), cada arquivo que não pode ser renomeado é copiado e só então apagado da origem.
fn move_content(from: &Path, to: &Path) -> std::io::Result<()> {
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    create_private_dir(to)?;
    for entry in std::fs::read_dir(from)? {
        let (src, dst) = {
            let entry = entry?;
            (entry.path(), to.join(entry.file_name()))
        };
        if std::fs::rename(&src, &dst).is_err() {
            std::fs::copy(&src, &dst)?;
            std::fs::remove_file(&src)?;
        }
    }
    std::fs::remove_dir(from)
}

/// Espera `rx` mudar; se o provedor de tokens foi embora, não muda mais.
async fn changed(rx: &mut watch::Receiver<bool>) {
    if rx.changed().await.is_err() {
        std::future::pending::<()>().await;
    }
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
        let data_dir = data_dir_of(paths, account);
        let cache_dir = cache_dir_of(paths, prefs, account);
        create_private_dir(&data_dir)?;
        create_private_dir(&cache_dir)?;
        if let Some((from, _)) = pending_cache_move(paths, prefs, account) {
            move_content(&from.join(CONTENT_DIR), &cache_dir.join(CONTENT_DIR))?;
        }
        let marker = data_dir.join(CACHE_DIR_FILE);
        let cache_bytes = cache_dir.as_os_str().as_bytes();
        if std::fs::read(&marker).ok().as_deref() != Some(cache_bytes) {
            write_atomic(&marker, cache_bytes)?;
        }
        let store = Arc::new(Store::open(&data_dir.join("metadata.db"))?);
        let client = Arc::new(client);
        let signed_out = client.signed_out();
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
            thumbs: Arc::new(ThumbSeeder::new(
                store.clone(),
                client.clone(),
                paths.thumbnails_dir.clone(),
            )),
            poller: ChangePoller::new(store.clone(), client.clone(), cache.clone(), events.clone()),
            kept,
            data_dir,
            store,
            client,
            cache,
            events,
            intervals: Intervals::default(),
            paused: watch::Sender::new(false),
            signed_out,
            pause_reasons: Default::default(),
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

    pub fn client(&self) -> &Arc<DriveClient> {
        &self.client
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

    /// O arquivo está aberto para escrita (ver `UploadWorker::mark_writing`).
    pub fn mark_writing(&self, inode: gdl_store::Inode) -> crate::uploads::WritingMark {
        self.uploads.mark_writing(inode)
    }

    /// Semeia, em segundo plano, as miniaturas dos arquivos da pasta `folder` (montada em
    /// `folder_path`). Com a mesma pasta já em semeadura, devolve `None`: só quem a iniciou
    /// espera, e reabrir a pasta não espera de novo. Descartar o handle (ou desistir de esperar)
    /// não interrompe o trabalho; erros só vão para o log.
    pub fn seed_thumbnails(
        &self,
        folder: &gdl_store::ItemId,
        folder_path: PathBuf,
    ) -> Option<tokio::task::JoinHandle<()>> {
        self.thumbs.seed(folder, folder_path)
    }

    /// Cancela as semeaduras e apaga as miniaturas semeadas e a memória delas (saída da conta).
    /// Espera as gravações em curso terminarem, para nenhuma recriar um PNG depois.
    pub fn purge_thumbnails(&self) {
        self.thumbs.purge();
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
        self.update_pause(|r| r.user = true);
    }

    /// Retoma, inclusive o que a conexão limitada pausou: a decisão do usuário vale mais.
    pub fn resume(&self) {
        self.update_pause(|r| *r = PauseReasons::default());
    }

    /// Pausa (ou desfaz) pela conexão limitada; desfazer não retoma uma pausa do usuário.
    pub fn set_metered_pause(&self, on: bool) {
        self.update_pause(|r| r.metered = on);
    }

    fn update_pause(&self, change: impl FnOnce(&mut PauseReasons)) {
        let mut reasons = self.pause_reasons.lock().unwrap_or_else(|e| e.into_inner());
        change(&mut reasons);
        // Ainda com a trava: duas mudanças seguidas não publicam fora de ordem.
        self.set_paused(reasons.user || reasons.metered);
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

    /// A última consulta de mudanças não alcançou o Drive.
    pub fn is_offline(&self) -> bool {
        self.poller.is_offline()
    }

    /// O Google recusou o login da conta: ela fica só com o que está no cache, e os envios
    /// esperam, até o login voltar.
    pub fn needs_sign_in(&self) -> bool {
        *self.signed_out.borrow()
    }

    pub fn summary(&self) -> Result<SyncSummary> {
        let queue = self.store.queue_stats()?;
        let downloads = self.store.count_incomplete_pinned()?;
        let stats = &self.uploads.stats;
        Ok(SyncSummary {
            pending_uploads: queue.pending,
            paused: self.is_paused(),
            offline: self.poller.is_offline(),
            needs_sign_in: self.needs_sign_in(),
            last_sync_ms: self.poller.last_sync_ms(),
            // O mesmo que `errors().len()`, sem montar os caminhos.
            errors: queue.failed + self.lost_and_found().count() as u32,
            // Sem nada na fila, a cota cheia não impede nada.
            quota_full: queue.pending > 0 && stats.quota_full(),
            syncing: queue.pending + downloads,
            remaining_secs: stats.remaining_secs(queue.bytes, now_ms()),
        })
    }

    /// Busca agora as mudanças no Drive ("Atualizar pasta"), sem esperar a próxima consulta.
    pub async fn refresh(&self) {
        self.poller.poll().await;
    }

    /// Roda os laços do serviço até `shutdown` ser cancelado.
    pub async fn run(&self, shutdown: CancellationToken) {
        tokio::select! {
            () = shutdown.cancelled() => {}
            () = self.announce_sign_in() => {}
            () = self.every(self.intervals.changes, true, || self.poller.poll()) => {}
            () = self.upload_loop() => {}
            () = self.every(self.intervals.maintenance, true, || self.maintain()) => {}
            () = self.every(self.intervals.eviction, false, || self.evict()) => {}
        }
        // Desmontagem: as semeaduras em curso param junto, sem apagar nada.
        self.thumbs.stop();
    }

    /// Avisa (`NeedsSignIn`) quando o Google recusa o login da conta e quando ele volta; na
    /// volta, libera os envios que esperavam por ele.
    async fn announce_sign_in(&self) {
        let mut signed_out = self.signed_out.clone();
        let mut announced = false;
        loop {
            let now = *signed_out.borrow_and_update();
            if now != announced {
                announced = now;
                if !now && let Err(e) = self.store.wake_network_waiters(now_ms()) {
                    tracing::warn!(erro = %e, "falha ao retomar os envios que esperavam o login");
                }
                // Sem ninguém ouvindo, o aviso pode se perder.
                let _ = self.events.send(EngineEvent::NeedsSignIn(now));
            }
            changed(&mut signed_out).await;
        }
    }

    /// Roda `work` a cada `interval`; com `pausable`, não roda enquanto pausado nem sem login.
    /// Pausar, retomar ou o login voltar interrompe a espera.
    async fn every<F: Future<Output = ()>>(
        &self,
        interval: Duration,
        pausable: bool,
        mut work: impl FnMut() -> F,
    ) {
        loop {
            if !(pausable && (self.is_paused() || self.needs_sign_in())) {
                work().await;
            }
            self.sleep(interval).await;
        }
    }

    async fn sleep(&self, interval: Duration) {
        let mut paused = self.paused.subscribe();
        let mut signed_out = self.signed_out.clone();
        tokio::select! {
            () = tokio::time::sleep(interval) => {}
            _ = paused.changed() => {}
            () = changed(&mut signed_out) => {}
        }
    }

    /// Quatro envios ao mesmo tempo: arquivo pequeno sobe no ritmo da espera de cada pedido,
    /// não da banda, e um de cada vez deixava a fila lenta (~2 s por arquivo).
    /// Fila parada custa uma consulta por intervalo. Com envio pronto, quatro rodam juntos:
    /// arquivo pequeno sobe no ritmo da espera de cada pedido, não da banda, e um de cada vez
    /// deixava a fila lenta (~2 s por arquivo).
    async fn upload_loop(&self) {
        loop {
            match self.uploads.has_ready() {
                Ok(true) if self.may_upload() => {
                    let crew = Crew::default();
                    tokio::join!(
                        self.upload_burst(&crew),
                        self.upload_burst(&crew),
                        self.upload_burst(&crew),
                        self.upload_burst(&crew)
                    );
                }
                Ok(_) => {}
                Err(e) => tracing::warn!(erro = %e, "falha na fila de envio"),
            }
            self.sleep(self.intervals.uploads_idle).await;
        }
    }

    /// Sem pausa, com rede e com login: sem rede, nenhum envio tenta, e a próxima consulta
    /// bem-sucedida libera a fila; sem login, ela espera o login novo.
    fn may_upload(&self) -> bool {
        !self.is_paused() && !self.poller.is_offline() && !self.needs_sign_in()
    }

    /// Envia um atrás do outro enquanto houver envio pronto que nenhum outro esteja fazendo. Sem
    /// nenhum livre, espera enquanto outro da equipe ainda envia: o que ele termina pode liberar
    /// mais (uma pasta criada libera os arquivos dela), e um só não sobe o resto sozinho.
    async fn upload_burst(&self, crew: &Crew) {
        while self.may_upload() {
            crew.busy.fetch_add(1, Ordering::SeqCst);
            let outcome = self.uploads.run_once().await;
            crew.busy.fetch_sub(1, Ordering::SeqCst);
            crew.done.notify_waiters();
            match outcome {
                // O envio é o primeiro a ver a rede cair; a consulta de mudanças vê a volta.
                Ok(Some(UploadOutcome::Offline)) => return self.poller.report_offline(),
                Ok(Some(_)) => {}
                Ok(None) => {
                    // Inscrito antes de olhar o contador: um aviso entre os dois não se perde.
                    let mut done = std::pin::pin!(crew.done.notified());
                    done.as_mut().enable();
                    if crew.busy.load(Ordering::SeqCst) == 0 {
                        return;
                    }
                    tokio::select! {
                        () = done => {}
                        () = self.sleep(self.intervals.uploads_idle) => {}
                    }
                }
                Err(e) => return tracing::warn!(erro = %e, "falha na fila de envio"),
            }
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

/// Os envios que rodam juntos: quantos estão enviando agora e o aviso de cada um que termina.
#[derive(Default)]
struct Crew {
    busy: AtomicUsize,
    done: Notify,
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

    /// Com o login recusado, os laços param; quando o login volta, a consulta de mudanças volta
    /// na hora, sem esperar o intervalo (aqui, 60 s).
    #[tokio::test]
    async fn signing_in_again_wakes_the_loops_without_waiting_the_interval() {
        use gdl_drive::oauth::AccessTokens;
        use gdl_drive::token_store::{ClientCredentials, TokenSet};

        let fake = gdl_fake_drive::FakeDrive::start().await;
        let creds = ClientCredentials {
            client_id: "id".into(),
            client_secret: "segredo".into(),
        };
        let tokens = |access: &str, expires_at_ms: i64| TokenSet {
            access_token: access.into(),
            refresh_token: "recusado".into(),
            expires_at_ms,
            email: "teste@example.com".into(),
        };
        let valid = || tokens(gdl_fake_drive::TEST_ACCESS_TOKEN, i64::MAX / 2);
        let access = Arc::new(AccessTokens::new(creds.clone(), valid(), fake.token_url()));
        let client = DriveClient::new(fake.base_url(), fake.upload_url(), access.clone());
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let (prefs, _) = load_preferences(&paths).unwrap();
        let engine = Arc::new(
            Engine::open(&paths, &prefs, "teste@example.com", client)
                .unwrap()
                .with_intervals(Intervals {
                    changes: Duration::from_secs(60),
                    ..fast()
                }),
        );
        engine.ensure_indexed().await.unwrap();
        // O token venceu e o Google não aceita renová-lo.
        access.replace(creds.clone(), tokens("velho", 0)).await;
        let mut events = engine.subscribe();
        let shutdown = CancellationToken::new();
        tokio::spawn({
            let (engine, shutdown) = (engine.clone(), shutdown.clone());
            async move { engine.run(shutdown).await }
        });
        // O aviso sai de uma tarefa à parte: espera cada um antes de seguir.
        let mut announced = async |want: bool| {
            let wait = async {
                loop {
                    if let Ok(EngineEvent::NeedsSignIn(on)) = events.recv().await {
                        assert_eq!(on, want);
                        return;
                    }
                }
            };
            tokio::time::timeout(Duration::from_secs(10), wait)
                .await
                .unwrap_or_else(|_| panic!("esperando NeedsSignIn({want})"));
        };
        announced(true).await;
        assert!(engine.summary().unwrap().needs_sign_in);
        let id = fake.add_file(&fake.root_id(), "novo.txt", b"x".to_vec());

        access.replace(creds, valid()).await;

        announced(false).await;
        let id = gdl_store::ItemId(id);
        wait_until("consulta depois do login", || {
            engine.store.get(&id).unwrap().is_some()
        })
        .await;
        assert!(!engine.needs_sign_in());
        shutdown.cancel();
    }

    #[tokio::test]
    async fn metered_pause_keeps_user_pause_and_yields_to_user_resume() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let (engine, _tmp) = crate::testutil::indexed_engine(&fake).await;

        engine.pause();
        engine.set_metered_pause(true);
        engine.set_metered_pause(false);
        assert!(engine.is_paused(), "a pausa do usuário continua");

        engine.resume();
        engine.set_metered_pause(true);
        assert!(engine.is_paused());
        engine.resume();
        assert!(
            !engine.is_paused(),
            "retomar pelo usuário vence a conexão limitada"
        );

        engine.set_metered_pause(true);
        engine.set_metered_pause(false);
        assert!(!engine.is_paused());
    }

    /// Uma pasta nova com vários arquivos sobe com envios em paralelo, a pasta antes deles.
    #[tokio::test]
    async fn many_new_files_upload_in_parallel_after_their_folder() {
        new_folder_with_files_uploads_in_parallel(None).await;
    }

    /// Bug (teste instável "1 envios juntos"): com a pasta demorando mais para ser criada que a
    /// espera dos arquivos por ela, os outros envios já tinham desistido e um só subia os 8.
    #[tokio::test]
    async fn slow_folder_creation_still_uploads_its_files_in_parallel() {
        let slow = Duration::from_millis(crate::uploads::PARENT_WAIT_MS as u64 + 500);
        new_folder_with_files_uploads_in_parallel(Some(slow)).await;
    }

    async fn new_folder_with_files_uploads_in_parallel(create_delay: Option<Duration>) {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let (engine, _tmp) = crate::testutil::indexed_engine(&fake).await;
        let engine = Arc::new(engine.with_intervals(fast()));
        let root = gdl_store::ItemId(fake.root_id());
        let folder = create_local_item(&engine.store, &root, "fotos", ItemKind::Folder).unwrap();
        // Criada sem rede: sobe pela fila, como os arquivos.
        crate::uploads::enqueue_upload(&engine.store, &folder.id, 0, None).unwrap();
        for n in 0..8 {
            let name = format!("{n}.txt");
            let item = create_local_item(&engine.store, &folder.id, &name, ItemKind::File).unwrap();
            let path = engine.cache.local_path(&item.id);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, name.as_bytes()).unwrap();
            commit_local_write(&engine.store, &item.id, name.len() as u64, now_ms()).unwrap();
        }
        fake.inject(gdl_fake_drive::Fault::DelayUploads(Duration::from_millis(
            100,
        )));
        if let Some(delay) = create_delay {
            fake.inject(gdl_fake_drive::Fault::DelayCreates(delay));
        }

        let shutdown = CancellationToken::new();
        let task = tokio::spawn({
            let (engine, shutdown) = (engine.clone(), shutdown.clone());
            async move { engine.run(shutdown).await }
        });
        wait_until("fila vazia", || {
            engine.summary().unwrap().pending_uploads == 0
        })
        .await;
        shutdown.cancel();
        task.await.unwrap();

        let folders = fake.find_live("fotos");
        assert_eq!(folders.len(), 1);
        for n in 0..8 {
            let ids = fake.find_live(&format!("{n}.txt"));
            assert_eq!(ids.len(), 1, "{n}.txt subiu uma vez");
            assert_eq!(fake.meta(&ids[0])["parents"][0], folders[0]);
        }
        let parallel = fake.max_concurrent_uploads();
        assert!((2..=4).contains(&parallel), "{parallel} envios juntos");
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
