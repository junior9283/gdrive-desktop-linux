//! Estado do serviço: a conta conectada (motor e montagem), o login e a saída da conta.

use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use futures_util::StreamExt;
use gdl_common::browser;
use gdl_common::config::{ConfigError, Preferences, load_preferences};
use gdl_common::i18n::I18n;
use gdl_common::names::{create_unique, split_ext};
use gdl_common::paths::Paths;
use gdl_drive::client::{Backoff, DriveClient};
use gdl_drive::oauth::{AccessTokens, LoginFlow, OAuthEndpoints};
use gdl_drive::token_store::{ClientCredentials, TokenSet, TokenStore};
use gdl_engine::engine::{Engine, Intervals};
use gdl_engine::events::EngineEvent;
use gdl_fs::DriveFs;
use gdl_store::{Inode, ItemId, ItemKind};
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::{CancellationToken, DropGuard};
use url::Url;
use zbus::Connection;
use zbus::object_server::SignalEmitter;

use crate::dbus::{OBJECT_PATH, Service, ServiceError, Status};
use crate::mount;
use crate::notify::{self, Notification, Notifier, OPEN_LOST_FOUND};

/// Quanto o login espera o retorno do navegador.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Arquivo (em `data_dir`) com a conta conectada, para reconectar na próxima partida.
const CURRENT_ACCOUNT: &str = "current-account";
/// De quanto em quanto tempo `logout finish` confere se os envios terminaram.
const FINISH_POLL: Duration = Duration::from_secs(1);

/// Endereços da Drive API.
#[derive(Debug, Clone)]
pub struct Api {
    pub base: Url,
    pub upload: Url,
}

impl Api {
    pub fn google() -> Api {
        Api {
            base: "https://www.googleapis.com/drive/v3/".parse().unwrap(),
            upload: "https://www.googleapis.com/upload/drive/v3/"
                .parse()
                .unwrap(),
        }
    }
}

pub struct Config {
    pub paths: Paths,
    /// Idioma no formato do `LANG`.
    pub lang: String,
    /// Onde fica `~/Google Drive` quando `DefaultMountPoint` não está definido.
    pub home: PathBuf,
    pub oauth: OAuthEndpoints,
    pub api: Api,
    pub tokens: Arc<dyn TokenStore>,
    /// Lançador do navegador; sem ele, `DefaultWebBrowser` ou `xdg-open`.
    pub browser: Option<PathBuf>,
    pub backoff: Backoff,
    pub intervals: Intervals,
}

/// A conta conectada. Ao sair, desmonta e depois para o motor (os campos caem nessa ordem).
struct Account {
    email: String,
    engine: Arc<Engine>,
    mount_point: PathBuf,
    shutdown: CancellationToken,
    _session: fuser::BackgroundSession,
    _stop: DropGuard,
}

fn failed(e: impl std::fmt::Display) -> ServiceError {
    ServiceError::Failed(e.to_string())
}

pub struct Daemon {
    config: Config,
    prefs: Preferences,
    i18n: I18n,
    account: tokio::sync::Mutex<Option<Account>>,
    conn: OnceLock<Connection>,
    notifier: OnceLock<Notifier>,
    login: Mutex<Option<JoinHandle<()>>>,
}

impl Daemon {
    pub fn new(config: Config) -> Result<Arc<Daemon>, ConfigError> {
        let (prefs, _) = load_preferences(&config.paths)?;
        Ok(Arc::new(Daemon {
            i18n: I18n::new(&config.lang),
            prefs,
            config,
            account: tokio::sync::Mutex::default(),
            conn: OnceLock::new(),
            notifier: OnceLock::new(),
            login: Mutex::default(),
        }))
    }

    /// Liga o serviço à conexão D-Bus onde ele foi publicado: sinais saem por ela e as
    /// notificações vão ao servidor de notificações do outro lado.
    pub async fn attach(self: &Arc<Self>, conn: Connection) -> zbus::Result<()> {
        let notifier = Notifier::new(&conn, &self.i18n).await?;
        let mut actions = notifier.proxy().receive_action_invoked().await?;
        let _ = self.notifier.set(notifier);
        let _ = self.conn.set(conn);
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            while let Some(signal) = actions.next().await {
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                if signal.args().is_ok_and(|a| a.action_key == OPEN_LOST_FOUND) {
                    daemon.open_lost_and_found().await;
                }
            }
        });
        Ok(())
    }

    /// O motor da conta conectada.
    pub async fn engine(&self) -> Option<Arc<Engine>> {
        self.with_account(|a| a.engine.clone()).await
    }

    fn current_account_file(&self) -> PathBuf {
        self.config.paths.data_dir.join(CURRENT_ACCOUNT)
    }

    /// Reconecta a conta da execução anterior, se houver.
    pub async fn resume_saved_account(self: &Arc<Self>) -> Result<(), ServiceError> {
        let Ok(email) = std::fs::read_to_string(self.current_account_file()) else {
            return Ok(());
        };
        let saved = self
            .config
            .tokens
            .load(email.trim())
            .await
            .map_err(failed)?;
        match saved {
            Some((creds, tokens)) => self.start_account(creds, tokens).await,
            None => Ok(()),
        }
    }

    /// Desmonta e para o motor (o serviço está saindo).
    pub async fn shutdown(&self) {
        self.stop_account().await;
    }

    /// Abre no navegador sem esperar o lançador terminar.
    fn open(&self, target: String) {
        let launcher = self
            .config
            .browser
            .clone()
            .or_else(|| self.prefs.default_web_browser.clone());
        tokio::task::spawn_blocking(move || {
            if let Err(e) = browser::open(&target, launcher.as_deref()) {
                tracing::warn!(erro = %e, "não deu para abrir o navegador");
            }
        });
    }

    async fn notify(&self, note: &Notification) {
        if let Some(notifier) = self.notifier.get() {
            notifier.send(note).await;
        }
    }

    async fn open_lost_and_found(&self) {
        if let Some(engine) = self.engine().await {
            let dir = engine.lost_and_found().dir().display().to_string();
            self.open(dir);
        }
    }

    fn emitter(&self) -> Option<SignalEmitter<'static>> {
        SignalEmitter::new(self.conn.get()?, OBJECT_PATH)
            .ok()
            .map(SignalEmitter::into_owned)
    }

    async fn emit_status_changed(&self) {
        if let Some(emitter) = self.emitter() {
            let _ = Service::status_changed(&emitter).await;
        }
    }

    /// Começa o login no navegador e devolve o endereço (para quem não tem navegador aqui). O
    /// resultado chega pelo sinal `LoginFinished`.
    pub async fn begin_login(
        self: &Arc<Self>,
        creds: ClientCredentials,
    ) -> Result<String, ServiceError> {
        let flow = LoginFlow::start(creds.clone(), self.config.oauth.clone())
            .await
            .map_err(failed)?;
        let url = flow.auth_url().to_string();
        self.open(url.clone());
        let daemon = self.clone();
        let task = tokio::spawn(async move {
            let (ok, email, error) = match daemon.finish_login(flow, creds).await {
                Ok(email) => (true, email, String::new()),
                Err((email, error)) => (false, email, error),
            };
            if let Some(emitter) = daemon.emitter() {
                let _ = Service::login_finished(&emitter, ok, &email, &error).await;
            }
        });
        // Um login novo substitui o que ficou esperando o navegador.
        let previous = self
            .login
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .replace(task);
        if let Some(previous) = previous {
            previous.abort();
        }
        Ok(url)
    }

    /// Espera o navegador, confere se a conta é permitida, guarda os tokens e monta a conta.
    /// O erro vem com o e-mail (se já conhecido) e a mensagem.
    async fn finish_login(
        self: &Arc<Self>,
        flow: LoginFlow,
        creds: ClientCredentials,
    ) -> Result<String, (String, String)> {
        let tokens = flow
            .wait(LOGIN_TIMEOUT)
            .await
            .map_err(|e| (String::new(), e.to_string()))?;
        let email = tokens.email.clone();
        let fail = |e: &dyn std::fmt::Display| (email.clone(), e.to_string());
        if !self.allowed(&email) {
            let message = self.i18n.tr("login-rejected", &[("email", &email)]);
            return Err((email, message));
        }
        self.config
            .tokens
            .save(&email, &creds, &tokens)
            .await
            .map_err(|e| fail(&e))?;
        self.start_account(creds, tokens)
            .await
            .map_err(|e| fail(&e))?;
        Ok(email)
    }

    /// `AllowedAccountsPattern` (expressão regular sobre o e-mail inteiro); sem padrão, toda
    /// conta é permitida. Um padrão inválido não deixa ninguém entrar.
    fn allowed(&self, email: &str) -> bool {
        match self.prefs.allowed_accounts_pattern.as_deref() {
            None | Some("") => true,
            Some(pattern) => regex::Regex::new(&format!("^(?:{pattern})$"))
                .inspect_err(|e| tracing::warn!(erro = %e, "AllowedAccountsPattern inválido"))
                .is_ok_and(|re| re.is_match(email)),
        }
    }

    async fn start_account(
        self: &Arc<Self>,
        creds: ClientCredentials,
        tokens: TokenSet,
    ) -> Result<(), ServiceError> {
        self.stop_account().await;
        let email = tokens.email.clone();
        let config = &self.config;
        let access = AccessTokens::new(creds, tokens, config.oauth.token.clone());
        let client = DriveClient::new(
            config.api.base.clone(),
            config.api.upload.clone(),
            Arc::new(access),
        )
        .with_backoff(config.backoff);
        let engine = Engine::open(&config.paths, &self.prefs, &email, client).map_err(failed)?;
        let engine = Arc::new(engine.with_intervals(config.intervals));
        engine.ensure_indexed().await.map_err(failed)?;

        let preferred = self
            .prefs
            .default_mount_point
            .clone()
            .unwrap_or_else(|| config.home.join("Google Drive"));
        let mount_point = mount::choose(&preferred).map_err(failed)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let fs = DriveFs::new(engine.clone(), I18n::new(&config.lang), uid, gid);
        let session = gdl_fs::mount(fs, &mount_point).map_err(failed)?;
        gdl_common::paths::create_private_dir(&config.paths.data_dir)
            .and_then(|()| std::fs::write(self.current_account_file(), &email))
            .map_err(failed)?;

        // Daqui em diante nada falha: o motor só roda com a conta guardada em `account`.
        let shutdown = CancellationToken::new();
        tokio::spawn({
            let (engine, shutdown) = (engine.clone(), shutdown.clone());
            async move { engine.run(shutdown).await }
        });
        self.forward_events(engine.subscribe(), shutdown.clone());
        let moved = mount_point != preferred;
        *self.account.lock().await = Some(Account {
            email,
            engine,
            mount_point: mount_point.clone(),
            _stop: shutdown.clone().drop_guard(),
            shutdown,
            _session: session,
        });
        if moved {
            let path = mount_point.display().to_string();
            self.notify(&Notification {
                summary: self.i18n.tr("notify-title", &[]),
                body: self.i18n.tr("mount-moved", &[("path", &path)]),
                action: None,
                kind: None,
            })
            .await;
        }
        self.emit_status_changed().await;
        Ok(())
    }

    /// Avisos e `StatusChanged` a partir dos eventos do motor, até a conta sair.
    fn forward_events(
        self: &Arc<Self>,
        mut events: broadcast::Receiver<EngineEvent>,
        shutdown: CancellationToken,
    ) {
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                let event = tokio::select! {
                    () = shutdown.cancelled() => break,
                    event = events.recv() => event,
                };
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                match event {
                    Ok(event) => daemon.on_event(&event).await,
                    Err(broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        });
    }

    async fn on_event(&self, event: &EngineEvent) {
        if matches!(
            event,
            EngineEvent::SyncStateChanged(_) | EngineEvent::Offline(_)
        ) {
            self.emit_status_changed().await;
        }
        if let Some(note) = notify::for_event(event, &self.i18n) {
            self.notify(&note).await;
        }
    }

    async fn stop_account(&self) {
        let account = self.account.lock().await.take();
        if let Some(account) = account {
            // Desmontar espera as threads do FUSE terminarem.
            let _ = tokio::task::spawn_blocking(move || drop(account)).await;
        }
    }

    fn not_logged_in(&self) -> ServiceError {
        ServiceError::NotLoggedIn(self.i18n.tr("not-logged-in", &[]))
    }

    pub async fn status(&self) -> Result<Status, ServiceError> {
        let Some((engine, account, mount_point)) = self
            .with_account(|a| {
                let mount_point = a.mount_point.display().to_string();
                (a.engine.clone(), a.email.clone(), mount_point)
            })
            .await
        else {
            return Ok(Status::default());
        };
        let summary = engine.summary().map_err(failed)?;
        Ok(Status {
            account,
            mount_point,
            pending_uploads: summary.pending_uploads,
            paused: summary.paused,
            offline: summary.offline,
            last_sync_ms: summary.last_sync_ms.unwrap_or(0),
            errors: summary.errors,
        })
    }

    /// `f` sobre a conta conectada, soltando a trava antes de o chamador consultar o banco.
    async fn with_account<T>(&self, f: impl FnOnce(&Account) -> T) -> Option<T> {
        self.account.lock().await.as_ref().map(f)
    }

    async fn connected_engine(&self) -> Result<Arc<Engine>, ServiceError> {
        self.engine().await.ok_or_else(|| self.not_logged_in())
    }

    pub async fn set_paused(&self, paused: bool) -> Result<(), ServiceError> {
        let engine = self.connected_engine().await?;
        // O motor avisa a mudança (`SyncStateChanged`), que vira `StatusChanged`.
        match paused {
            true => engine.pause(),
            false => engine.resume(),
        }
        Ok(())
    }

    pub async fn set_pinned(&self, path: &str, pinned: bool) -> Result<(), ServiceError> {
        let (engine, id) = self.item_at(path).await?;
        let pins = engine.pins();
        match pinned {
            true => pins.pin(&id),
            false => pins.unpin(&id),
        }
        .map_err(failed)
    }

    pub async fn lost_and_found(&self) -> Result<Vec<String>, ServiceError> {
        let engine = self.connected_engine().await?;
        let files = engine.lost_and_found().list();
        Ok(files.iter().map(|p| p.display().to_string()).collect())
    }

    /// Item do Meu Drive no caminho `path` da montagem.
    async fn item_at(&self, path: &str) -> Result<(Arc<Engine>, ItemId), ServiceError> {
        let (engine, mount_point) = self
            .with_account(|a| (a.engine.clone(), a.mount_point.clone()))
            .await
            .ok_or_else(|| self.not_logged_in())?;
        let not_found =
            || ServiceError::NotFound(self.i18n.tr("path-not-in-drive", &[("path", path)]));
        let relative = relative_to_mount(Path::new(path), &mount_point).ok_or_else(not_found)?;
        let mut parts = relative.components();
        let my_drive = self.i18n.tr("folder-my-drive", &[]);
        if parts.next() != Some(Component::Normal(my_drive.as_ref())) {
            return Err(not_found());
        }
        let store = engine.store();
        let mut item = store
            .by_inode(Inode::ROOT)
            .map_err(failed)?
            .ok_or_else(not_found)?;
        for part in parts {
            let name = match part {
                Component::Normal(name) => name.to_str().ok_or_else(not_found)?,
                _ => return Err(not_found()),
            };
            item = store
                .lookup(&item.id, name)
                .map_err(failed)?
                .ok_or_else(not_found)?;
        }
        Ok((engine, item.id))
    }

    /// Sai da conta. Com envios pendentes, `mode` diz o que fazer com eles: `finish` (sai quando
    /// terminarem), `move` (copia para a pasta `dir` antes) ou `discard`.
    pub async fn logout(self: &Arc<Self>, mode: &str, dir: &str) -> Result<(), ServiceError> {
        let (engine, email, shutdown) = self
            .with_account(|a| (a.engine.clone(), a.email.clone(), a.shutdown.clone()))
            .await
            .ok_or_else(|| self.not_logged_in())?;
        match mode {
            "" => {
                let pending = unsynced(&engine).map_err(failed)?;
                if pending > 0 {
                    let count = pending.to_string();
                    return Err(ServiceError::PendingUploads(
                        self.i18n.tr("logout-pending", &[("count", &count)]),
                    ));
                }
                self.sign_out(&engine, &email).await
            }
            "discard" => self.sign_out(&engine, &email).await,
            "move" if !dir.is_empty() => {
                let (copier, dir) = (engine.clone(), PathBuf::from(dir));
                tokio::task::spawn_blocking(move || copy_pending(&copier, &dir))
                    .await
                    .map_err(failed)?
                    .map_err(failed)?;
                self.sign_out(&engine, &email).await
            }
            "finish" => {
                engine.resume();
                let daemon = self.clone();
                tokio::spawn(async move {
                    while engine.summary().is_ok_and(|s| s.pending_uploads > 0) {
                        tokio::select! {
                            // A conta saiu por outro caminho: não há mais o que esperar.
                            () = shutdown.cancelled() => return,
                            () = tokio::time::sleep(FINISH_POLL) => {}
                        }
                    }
                    if let Err(e) = daemon.sign_out(&engine, &email).await {
                        tracing::warn!(erro = %e, "falha ao sair da conta");
                    }
                });
                Ok(())
            }
            _ => Err(ServiceError::InvalidArgs(format!("{mode} {dir}"))),
        }
    }

    /// Desmonta, esquece os tokens e apaga os dados locais da conta (banco e cache). O Lost and
    /// Found fica: é a única cópia das edições guardadas nele.
    async fn sign_out(&self, engine: &Engine, email: &str) -> Result<(), ServiceError> {
        self.stop_account().await;
        self.config.tokens.delete(email).await.map_err(failed)?;
        let _ = std::fs::remove_file(self.current_account_file());
        let dirs = engine.local_dirs();
        let keep = engine.lost_and_found().dir().to_path_buf();
        tokio::task::spawn_blocking(move || {
            for dir in dirs {
                remove_all_but(&dir, &keep)?;
            }
            Ok::<_, std::io::Error>(())
        })
        .await
        .map_err(failed)?
        .map_err(failed)?;
        self.emit_status_changed().await;
        Ok(())
    }
}

/// `path` relativo à montagem, aceitando o caminho real (com links simbólicos resolvidos) da
/// pasta onde ela fica.
fn relative_to_mount(path: &Path, mount_point: &Path) -> Option<PathBuf> {
    if let Ok(relative) = path.strip_prefix(mount_point) {
        return Some(relative.into());
    }
    let real = mount_point
        .parent()?
        .canonicalize()
        .ok()?
        .join(mount_point.file_name()?);
    path.strip_prefix(real).ok().map(Into::into)
}

/// Quantos itens ainda não estão no Drive: os pendentes (o que `copy_pending` copia) e as
/// cópias no Lost and Found.
fn unsynced(engine: &Engine) -> gdl_engine::Result<usize> {
    Ok(engine.store().pending_items()?.len() + engine.lost_and_found().list().len())
}

/// Apaga `dir` inteira, menos `keep` (e as pastas no caminho até ela) se `keep` tiver algo.
fn remove_all_but(dir: &Path, keep: &Path) -> std::io::Result<()> {
    let keep_it =
        keep.starts_with(dir) && std::fs::read_dir(keep).is_ok_and(|mut d| d.next().is_some());
    if !keep_it {
        return match std::fs::remove_dir_all(dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        };
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if path == keep {
            continue;
        }
        if keep.starts_with(&path) {
            remove_all_but(&path, keep)?;
        } else if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Copia o conteúdo local dos arquivos pendentes e o Lost and Found para `dir`, sem sobrescrever
/// nada.
fn copy_pending(engine: &Engine, dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let pending = engine
        .store()
        .pending_items()
        .map_err(std::io::Error::other)?;
    for item in pending.iter().filter(|i| i.kind == ItemKind::File) {
        let mut src = match std::fs::File::open(engine.cache().local_path(&item.id)) {
            Ok(src) => src,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        copy_unique(&mut src, dir, &item.local_name)?;
    }
    for saved in engine.lost_and_found().list() {
        let name = saved.file_name().unwrap_or_default().to_string_lossy();
        copy_unique(&mut std::fs::File::open(&saved)?, dir, &name)?;
    }
    Ok(())
}

/// Copia `src` para `dir/name`, numerando o nome se já existir.
fn copy_unique(src: &mut std::fs::File, dir: &Path, name: &str) -> std::io::Result<()> {
    let (stem, ext) = split_ext(name);
    let (_, mut dst) = create_unique(dir, stem, |n| match n {
        0 => ext.to_string(),
        n => format!(" ({n}){ext}"),
    })?;
    std::io::copy(src, &mut dst)?;
    Ok(())
}
