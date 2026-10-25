//! Estado do serviço: a conta conectada (motor e montagem), o login e a saída da conta.

use std::collections::{BTreeSet, HashMap};
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Duration;

use futures_util::StreamExt;
use gdl_common::browser;
use gdl_common::config::{
    ConfigError, Preferences, load_preferences, needs_restart, pref_values, reset_user_preferences,
    save_user_preference, set_user_preferences,
};
use gdl_common::i18n::I18n;
use gdl_common::names::{create_unique, split_ext};
use gdl_common::paths::Paths;
use gdl_drive::Network;
use gdl_drive::client::{Backoff, Bandwidth, DriveClient, DriveError, FOLDER_MIME, SHORTCUT_MIME};
use gdl_drive::oauth::{AccessTokens, LoginFlow, OAuthEndpoints};
use gdl_drive::token_store::{ClientCredentials, TokenSet, TokenStore};
use gdl_engine::EngineError;
use gdl_engine::engine::{Engine, Intervals, pending_cache_move};
use gdl_engine::errors::ErrorSource;
use gdl_engine::events::{EngineEvent, SyncState};
use gdl_engine::gdocs::web_url;
use gdl_engine::kept::KeptFolder;
use gdl_fs::DriveFs;
use gdl_store::{Inode, Item, ItemId, ItemKind, PinState};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::{CancellationToken, DropGuard};
use url::Url;
use zbus::Connection;
use zbus::object_server::SignalEmitter;

use gdl_ipc::{
    AccountInfo, ActivityItem, NotificationItem, NotificationSetting, OBJECT_PATH, OfflineEntry,
    OfflineSummary, Pref, SearchResult, ServiceError, SharedItem, Status, SyncErrorItem,
};

use crate::dbus::Service;
use crate::desktop;
use crate::history::History;
use crate::metered::{NetworkManagerProxy, is_metered};
use crate::mount;
use crate::notify::{
    self, Notification, Notifier, OPEN_CANCELED, OPEN_LOST_FOUND, OPTIONAL_KINDS, RESUME,
};

/// Quanto o login espera o retorno do navegador.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Arquivo (em `data_dir`) com a conta conectada, para reconectar na próxima partida.
const CURRENT_ACCOUNT: &str = "current-account";
/// Intervalo mínimo entre dois sinais `ActivityChanged`.
const ACTIVITY_SIGNAL_GAP: Duration = Duration::from_millis(250);
/// Foto da conta, no diretório de dados dela.
const PHOTO_FILE: &str = "photo.png";
/// Histórico das notificações, na pasta de estado.
const HISTORY_FILE: &str = "notifications.json";
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

/// `Managed` para a chave da política, `InvalidArgs` para chave ou valor inválido.
fn config_error(e: ConfigError) -> ServiceError {
    match e {
        ConfigError::Managed(key) => ServiceError::Managed(key),
        e @ (ConfigError::UnknownKey(_) | ConfigError::InvalidValue { .. }) => {
            ServiceError::InvalidArgs(e.to_string())
        }
        e => failed(e),
    }
}

/// `NotFound` para o item `id` que não existe (aqui ou no Drive); `Failed` para o resto.
fn engine_error(e: EngineError, id: &str) -> ServiceError {
    match e {
        EngineError::NotFound | EngineError::Drive(DriveError::NotFound) => {
            ServiceError::NotFound(id.into())
        }
        e => failed(e),
    }
}

/// O item `id`, fora da lixeira.
fn live_item(engine: &Engine, id: &str) -> Result<Item, ServiceError> {
    engine
        .store()
        .get(&ItemId(id.into()))
        .map_err(failed)?
        .filter(|i| !i.trashed)
        .ok_or_else(|| ServiceError::NotFound(id.into()))
}

/// Caminho do item a partir do Meu Drive (`Pasta/arquivo.txt`).
fn path_in_drive(engine: &Engine, item: &Item) -> Result<String, ServiceError> {
    engine
        .store()
        .path_of(&item.id)
        .map_err(failed)?
        .ok_or_else(|| ServiceError::NotFound(item.id.0.clone()))
}

/// Caminhos completos na montagem para uma lista de itens. O caminho de cada pasta é consultado
/// uma vez só (numa pasta da lista de off-line, todos têm o mesmo pai).
struct DrivePaths<'a> {
    engine: &'a Engine,
    my_drive: &'a Path,
    folders: HashMap<ItemId, Option<String>>,
}

impl<'a> DrivePaths<'a> {
    fn new(engine: &'a Engine, my_drive: &'a Path) -> DrivePaths<'a> {
        DrivePaths {
            engine,
            my_drive,
            folders: HashMap::new(),
        }
    }

    /// Sem o caminho da pasta, só o nome, direto no Meu Drive.
    fn of(&mut self, item: &Item) -> String {
        let store = self.engine.store();
        let folder = item.parent.as_ref().and_then(|parent| {
            self.folders
                .entry(parent.clone())
                .or_insert_with(|| store.path_of(parent).ok().flatten())
                .clone()
        });
        let folder = self.my_drive.join(folder.unwrap_or_default());
        folder.join(&item.local_name).display().to_string()
    }
}

fn web_link(item: &Item) -> Result<String, ServiceError> {
    web_url(item).ok_or_else(|| ServiceError::NotFound(item.id.0.clone()))
}

fn search_results(engine: &Engine, my_drive: &Path, items: Vec<Item>) -> Vec<SearchResult> {
    let mut paths = DrivePaths::new(engine, my_drive);
    items
        .into_iter()
        .map(|item| SearchResult {
            path: paths.of(&item),
            mime: match &item.kind {
                ItemKind::Folder => FOLDER_MIME.into(),
                ItemKind::GoogleDoc { mime } => mime.clone(),
                ItemKind::Shortcut { .. } => SHORTCUT_MIME.into(),
                ItemKind::File => String::new(),
            },
            is_folder: item.kind == ItemKind::Folder,
            id: item.id.0,
            name: item.local_name,
        })
        .collect()
}

/// Como o serviço termina, pedido pelo D-Bus (`Quit`, `Restart`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Exit {
    Quit,
    /// O `main` sobe o serviço de novo no mesmo processo, relendo as preferências.
    Restart,
}

pub struct Daemon {
    config: Config,
    /// Recarregadas a cada `SetPreferences`; o que foi lido na partida (cache, montagem, rede)
    /// só muda ao reiniciar.
    prefs: RwLock<Arc<Preferences>>,
    /// Serializa ler-modificar-gravar das preferências do usuário.
    saving: tokio::sync::Mutex<()>,
    exit: watch::Sender<Option<Exit>>,
    i18n: I18n,
    account: tokio::sync::Mutex<Option<Account>>,
    conn: OnceLock<Connection>,
    notifier: OnceLock<Notifier>,
    history: History,
    login: Mutex<Option<JoinHandle<()>>>,
    /// Limites de banda, trocados na hora pelas preferências.
    bandwidth: Arc<Bandwidth>,
    /// A conexão atual é limitada (NetworkManager).
    metered: AtomicBool,
}

/// `DirectConnection` vence `ProxyServer`; sem nenhum, o proxy do sistema.
fn network_of(prefs: &Preferences) -> Network {
    match (prefs.direct_connection, prefs.manual_proxy()) {
        (true, _) => Network::Direct,
        (false, Some(url)) => Network::Proxy(url.into()),
        (false, None) => Network::System,
    }
}

impl Daemon {
    pub fn new(config: Config) -> Result<Arc<Daemon>, ConfigError> {
        let (prefs, _) = load_preferences(&config.paths)?;
        gdl_drive::set_network(network_of(&prefs));
        let bandwidth = Arc::new(Bandwidth::default());
        apply_bandwidth(&bandwidth, &prefs);
        Ok(Arc::new(Daemon {
            bandwidth,
            metered: AtomicBool::new(false),
            i18n: I18n::new(&config.lang),
            history: History::load(config.paths.state_dir.join(HISTORY_FILE)),
            prefs: RwLock::new(Arc::new(prefs)),
            saving: tokio::sync::Mutex::default(),
            exit: watch::Sender::new(None),
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
        self.apply_autostart();
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            while let Some(signal) = actions.next().await {
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                if let Ok(args) = signal.args() {
                    daemon.on_desktop_action(args.id, &args.action_key).await;
                }
            }
        });
        Ok(())
    }

    fn prefs(&self) -> Arc<Preferences> {
        self.prefs.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// A família de aviso está em `DoNotShowNotifications`?
    fn muted(&self, kind: &str) -> bool {
        self.prefs()
            .do_not_show_notifications
            .iter()
            .any(|k| k == kind)
    }

    /// Espera alguém pedir `Quit` ou `Restart`.
    pub async fn exit_requested(&self) -> Exit {
        let mut exit = self.exit.subscribe();
        let wanted = exit.wait_for(Option::is_some).await;
        wanted.ok().and_then(|e| *e).unwrap_or(Exit::Quit)
    }

    pub async fn quit(&self) {
        if let Some(emitter) = self.emitter() {
            let _ = Service::quitting(&emitter).await;
        }
        self.exit.send_replace(Some(Exit::Quit));
    }

    pub fn restart(&self) {
        self.exit.send_replace(Some(Exit::Restart));
    }

    /// As preferências com o valor efetivo, em JSON.
    pub fn preferences(&self) -> Result<Vec<Pref>, ServiceError> {
        let values = pref_values(&self.config.paths).map_err(failed)?;
        values
            .into_iter()
            .map(|v| {
                Ok(Pref {
                    key: v.key.into(),
                    value: serde_json::to_string(&v.value).map_err(failed)?,
                    managed: v.managed,
                    restart: needs_restart(v.key),
                })
            })
            .collect()
    }

    /// Grava todas ou nenhuma (valor em JSON) e aplica o que vale na hora. `true` se alguma só
    /// vale depois de reiniciar.
    pub async fn set_preferences(
        &self,
        values: Vec<(String, String)>,
    ) -> Result<bool, ServiceError> {
        let mut parsed = Vec::with_capacity(values.len());
        for (key, json) in values {
            let invalid =
                |e: &dyn std::fmt::Display| ServiceError::InvalidArgs(format!("{key}: {e}"));
            let json: serde_json::Value = serde_json::from_str(&json).map_err(|e| invalid(&e))?;
            let value = toml::Value::try_from(json).map_err(|e| invalid(&e))?;
            parsed.push((key, value));
        }
        let _saving = self.saving.lock().await;
        set_user_preferences(&self.config.paths, &parsed).map_err(config_error)?;
        self.preferences_saved().await?;
        Ok(parsed.iter().any(|(key, _)| needs_restart(key)))
    }

    pub async fn reset_preferences(&self) -> Result<(), ServiceError> {
        let _saving = self.saving.lock().await;
        reset_user_preferences(&self.config.paths).map_err(failed)?;
        self.preferences_saved().await
    }

    /// Relê as preferências, aplica as da área de trabalho e avisa quem mostra.
    async fn preferences_saved(&self) -> Result<(), ServiceError> {
        let (prefs, _) = load_preferences(&self.config.paths).map_err(failed)?;
        apply_bandwidth(&self.bandwidth, &prefs);
        // Só a mudança desta preferência reavalia a pausa: mexer em outra não desfaz uma
        // retomada do usuário.
        let metered_changed = self.prefs().pause_syncing_on_metered_connection
            != prefs.pause_syncing_on_metered_connection;
        *self.prefs.write().unwrap_or_else(|e| e.into_inner()) = Arc::new(prefs);
        if metered_changed {
            self.apply_metered(self.is_metered()).await;
        }
        self.apply_autostart();
        if let Some(mount_point) = self.with_account(|a| a.mount_point.clone()).await {
            self.apply_sidebar(&mount_point);
        }
        if let Some(emitter) = self.emitter() {
            let _ = Service::preferences_changed(&emitter).await;
        }
        Ok(())
    }

    /// Observa a conexão limitada no NetworkManager de `conn` (o barramento do sistema). Sem
    /// NetworkManager, só registra no log.
    pub fn watch_metered(self: &Arc<Self>, conn: Connection) {
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            let watch = async {
                let proxy = NetworkManagerProxy::new(&conn).await?;
                let mut changes = proxy.receive_metered_changed().await;
                let initial = proxy.metered().await?;
                if let Some(daemon) = daemon.upgrade() {
                    daemon.set_metered(is_metered(initial)).await;
                }
                while let Some(change) = changes.next().await {
                    let (Some(daemon), Ok(value)) = (daemon.upgrade(), change.get().await) else {
                        break;
                    };
                    daemon.set_metered(is_metered(value)).await;
                }
                zbus::Result::Ok(())
            };
            if let Err(e) = watch.await {
                tracing::info!(erro = %e, "sem NetworkManager: a conexão limitada não pausa");
            }
        });
    }

    pub(crate) fn is_metered(&self) -> bool {
        self.metered.load(Ordering::SeqCst)
    }

    /// Guarda o estado novo só depois de aplicá-lo: quem vê `is_metered` já vê a pausa feita.
    async fn set_metered(&self, metered: bool) {
        self.apply_metered(metered).await;
        self.metered.store(metered, Ordering::SeqCst);
    }

    /// Pausa na conexão limitada (com `PauseSyncingOnMeteredConnection`); o motor só retoma ao
    /// sair dela se a pausa não for do usuário.
    async fn apply_metered(&self, metered: bool) {
        if let Some(engine) = self.engine().await {
            engine.set_metered_pause(metered && self.prefs().pause_syncing_on_metered_connection);
        }
    }

    /// `AutoStartOnLogin` em `~/.config/autostart`.
    fn apply_autostart(&self) {
        let enabled = self.prefs().auto_start_on_login;
        if let Err(e) = desktop::set_autostart(&self.config.paths.config_home, enabled) {
            tracing::warn!(erro = %e, "não deu para ajustar o início automático");
        }
    }

    /// `ShowMountPointInSidebar` nos Locais do KDE.
    fn apply_sidebar(&self, mount_point: &Path) {
        let title = mount_point
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let enabled = self.prefs().show_mount_point_in_sidebar;
        let places = self.config.paths.data_home.join("user-places.xbel");
        if let Err(e) = desktop::set_sidebar_place(&places, mount_point, &title, enabled) {
            tracing::warn!(erro = %e, "não deu para ajustar os Locais do Dolphin");
        }
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

    /// Abre o endereço no navegador (`DefaultWebBrowser`, se o administrador definiu).
    fn open(&self, target: String) {
        let launcher = self
            .config
            .browser
            .clone()
            .or_else(|| self.prefs().default_web_browser.clone());
        Self::launch(target, launcher);
    }

    /// Abre o arquivo ou a pasta no programa padrão (`xdg-open`), nunca no navegador da política.
    fn open_local(&self, path: &Path) {
        Self::launch(path.display().to_string(), self.config.browser.clone());
    }

    /// Abre sem esperar o lançador terminar.
    fn launch(target: String, launcher: Option<PathBuf>) {
        tokio::task::spawn_blocking(move || {
            if let Err(e) = browser::open(&target, launcher.as_deref()) {
                tracing::warn!(erro = %e, "não deu para abrir o navegador");
            }
        });
    }

    /// Mostra o aviso e o guarda no histórico, a menos que a família dele esteja desligada.
    async fn notify(&self, note: &Notification) {
        if self.muted(note.kind_id) {
            return;
        }
        let id = self.history.push(note);
        self.emit_notifications_changed().await;
        let Some(notifier) = self.notifier.get() else {
            return;
        };
        if let Some(desktop_id) = notifier.send(note).await {
            self.history.set_desktop_id(id, desktop_id);
        }
    }

    /// Botão de um aviso clicado no servidor de notificações.
    async fn on_desktop_action(&self, desktop_id: u32, key: &str) {
        let id = self.history.by_desktop_id(desktop_id);
        if let Err(e) = self.run_action(key).await {
            tracing::warn!(erro = %e, acao = key, "ação do aviso falhou");
        }
        if let Some(id) = id
            && self.history.mark_read(id)
        {
            self.emit_notifications_changed().await;
        }
    }

    async fn run_action(&self, key: &str) -> Result<(), ServiceError> {
        let folder = match key {
            RESUME => return self.set_paused(false).await,
            OPEN_LOST_FOUND => KeptFolder::LostAndFound,
            OPEN_CANCELED => KeptFolder::CanceledUploads,
            _ => return Err(ServiceError::InvalidArgs(key.into())),
        };
        let engine = self.connected_engine().await?;
        self.open_local(engine.kept().get(folder).dir());
        Ok(())
    }

    pub fn notifications(&self) -> Vec<NotificationItem> {
        self.history.items()
    }

    pub async fn mark_notifications_read(&self) {
        if self.history.mark_all_read() {
            self.emit_notifications_changed().await;
        }
    }

    pub async fn dismiss_notification(&self, id: u64) -> Result<(), ServiceError> {
        if !self.history.dismiss(id) {
            return Err(ServiceError::NotFound(id.to_string()));
        }
        self.emit_notifications_changed().await;
        Ok(())
    }

    /// O botão `key` do aviso `id` do histórico: executa a ação e marca o aviso como lido.
    pub async fn invoke_notification_action(&self, id: u64, key: &str) -> Result<(), ServiceError> {
        let note = self
            .history
            .get(id)
            .ok_or_else(|| ServiceError::NotFound(id.to_string()))?;
        if key.is_empty() || note.action_key != key {
            return Err(ServiceError::InvalidArgs(key.into()));
        }
        self.run_action(key).await?;
        self.history.mark_read(id);
        self.emit_notifications_changed().await;
        Ok(())
    }

    /// Os avisos que o usuário pode desligar, com o estado de cada um.
    pub fn notification_settings(&self) -> Vec<NotificationSetting> {
        OPTIONAL_KINDS
            .iter()
            .map(|kind| NotificationSetting {
                kind: kind.to_string(),
                label: self.i18n.tr(&format!("notify-kind-{kind}"), &[]),
                enabled: !self.muted(kind),
            })
            .collect()
    }

    /// Liga ou desliga uma família de aviso e grava a preferência `DoNotShowNotifications`.
    pub async fn set_notification_enabled(
        &self,
        kind: &str,
        enabled: bool,
    ) -> Result<(), ServiceError> {
        if !OPTIONAL_KINDS.contains(&kind) {
            return Err(ServiceError::InvalidArgs(kind.into()));
        }
        let _saving = self.saving.lock().await;
        let mut muted: BTreeSet<String> = self
            .prefs()
            .do_not_show_notifications
            .iter()
            .cloned()
            .collect();
        match enabled {
            true => muted.remove(kind),
            false => muted.insert(kind.into()),
        };
        let list = toml::Value::try_from(&muted).map_err(failed)?;
        save_user_preference(&self.config.paths, "DoNotShowNotifications", list)
            .map_err(config_error)?;
        self.preferences_saved().await
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

    async fn emit_notifications_changed(&self) {
        if let Some(emitter) = self.emitter() {
            let _ = Service::notifications_changed(&emitter).await;
        }
    }

    async fn emit_activity_changed(&self) {
        if let Some(emitter) = self.emitter() {
            let _ = Service::activity_changed(&emitter).await;
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

    /// Abre o motor fora das threads do runtime: com `ContentCachePath` novo, ele primeiro muda
    /// o cache de lugar (que pode ser uma cópia entre discos), com um aviso no início e no fim.
    async fn open_engine(
        &self,
        prefs: &Preferences,
        email: &str,
        client: DriveClient,
    ) -> Result<Engine, ServiceError> {
        let paths = self.config.paths.clone();
        let moving = pending_cache_move(&paths, prefs, email);
        // `stage`: `moving` (início) ou `moved` (fim, que substitui o do início).
        let note = |stage: &str, to: &Path| Notification {
            kind_id: "cache-moved",
            summary: self.i18n.tr(&format!("notify-cache-{stage}-title"), &[]),
            body: self.i18n.tr(
                &format!("notify-cache-{stage}-body"),
                &[("path", &to.display().to_string())],
            ),
            action: None,
            replaces_key: Some("cache-moved".into()),
        };
        if let Some((_, to)) = &moving {
            self.notify(&note("moving", to)).await;
        }
        let (prefs_owned, email_owned) = (prefs.clone(), email.to_string());
        let engine = tokio::task::spawn_blocking(move || {
            Engine::open(&paths, &prefs_owned, &email_owned, client)
        })
        .await
        .map_err(failed)?
        .map_err(failed)?;
        if let Some((_, to)) = &moving {
            self.notify(&note("moved", to)).await;
        }
        Ok(engine)
    }

    /// `AllowedAccountsPattern` (expressão regular sobre o e-mail inteiro); sem padrão, toda
    /// conta é permitida. Um padrão inválido não deixa ninguém entrar.
    fn allowed(&self, email: &str) -> bool {
        match self.prefs().allowed_accounts_pattern.as_deref() {
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
        .with_backoff(config.backoff)
        .with_bandwidth(self.bandwidth.clone());
        let prefs = self.prefs();
        let engine = self.open_engine(&prefs, &email, client).await?;
        let engine = Arc::new(engine.with_intervals(config.intervals));
        engine.ensure_indexed().await.map_err(failed)?;

        let preferred = prefs
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
        self.forward_activity(engine.activity_changes(), shutdown.clone());
        let moved = mount_point != preferred;
        *self.account.lock().await = Some(Account {
            email,
            engine,
            mount_point: mount_point.clone(),
            _stop: shutdown.clone().drop_guard(),
            shutdown,
            _session: session,
        });
        self.apply_sidebar(&mount_point);
        self.apply_metered(self.is_metered()).await;
        if moved {
            let path = mount_point.display().to_string();
            self.notify(&Notification {
                kind_id: "mount-moved",
                summary: self.i18n.tr("notify-title", &[]),
                body: self.i18n.tr("mount-moved", &[("path", &path)]),
                action: None,
                replaces_key: None,
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

    /// `ActivityChanged` a cada mudança da atividade, no máximo um por `ACTIVITY_SIGNAL_GAP`.
    fn forward_activity(
        self: &Arc<Self>,
        mut changes: tokio::sync::watch::Receiver<u64>,
        shutdown: CancellationToken,
    ) {
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    changed = changes.changed() => if changed.is_err() { break },
                }
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                daemon.emit_activity_changed().await;
                drop(daemon);
                tokio::time::sleep(ACTIVITY_SIGNAL_GAP).await;
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
            return Ok(Status {
                state: SyncState::SignedOut.as_str().into(),
                remaining_secs: -1,
                ..Status::default()
            });
        };
        let summary = engine.summary().map_err(failed)?;
        Ok(Status {
            account,
            mount_point,
            state: summary.state().as_str().into(),
            syncing: summary.syncing,
            remaining_secs: summary.remaining_secs.map_or(-1, |s| s as i64),
            pending_uploads: summary.pending_uploads,
            paused: summary.paused,
            offline: summary.offline,
            last_sync_ms: summary.last_sync_ms.unwrap_or(0),
            errors: summary.errors,
        })
    }

    /// Nome, cota e foto da conta conectada. A foto é baixada uma vez e fica no diretório de
    /// dados da conta.
    pub async fn account_info(&self) -> Result<AccountInfo, ServiceError> {
        let engine = self.connected_engine().await?;
        let about = engine.about().await.map_err(failed)?;
        Ok(AccountInfo {
            photo_png: cached_photo(&engine, &about.photo_link).await,
            email: about.email,
            name: about.display_name,
            quota_used: about.quota_used as i64,
            quota_total: about.quota_total.map_or(-1, |t| t as i64),
        })
    }

    pub async fn activity(&self) -> Result<Vec<ActivityItem>, ServiceError> {
        let (engine, my_drive) = self.engine_and_my_drive().await?;
        let rows = engine.activity().map_err(failed)?;
        Ok(rows
            .into_iter()
            .map(|r| ActivityItem {
                seq: r.seq,
                time_ms: r.time_ms,
                path: my_drive.join(&r.activity.path).display().to_string(),
                size: r.activity.size as i64,
                state: r.activity.state.as_str().into(),
                percent: r.activity.percent.map_or(-1, i32::from),
                item_id: r.activity.item_id,
                name: r.activity.name,
            })
            .collect())
    }

    pub async fn errors(&self) -> Result<Vec<SyncErrorItem>, ServiceError> {
        let engine = self.connected_engine().await?;
        let mount_point = self.mount_point().await?;
        let my_drive = mount_point.join(self.my_drive_name());
        // As cópias aparecem na montagem, na pasta oculta do Lost and Found.
        let lost_found = mount_point.join(self.i18n.tr("folder-lost-found", &[]));
        let errors = engine.errors().map_err(failed)?;
        Ok(errors
            .into_iter()
            .map(|e| {
                let (item_id, path) = match e.source {
                    ErrorSource::Item { id, path } => (id.0, my_drive.join(path)),
                    ErrorSource::Kept(copy) => (
                        String::new(),
                        lost_found.join(copy.file_name().unwrap_or_default()),
                    ),
                };
                SyncErrorItem {
                    item_id,
                    name: e.name,
                    path: path.display().to_string(),
                    category: e.category.as_str().into(),
                    message: e.message,
                }
            })
            .collect())
    }

    /// "Tentar de novo" num envio da lista de erros.
    pub async fn retry(&self, item_id: &str) -> Result<(), ServiceError> {
        if item_id.is_empty() {
            return Err(ServiceError::InvalidArgs("item sem envio".into()));
        }
        let engine = self.connected_engine().await?;
        engine.retry(&ItemId(item_id.into())).map_err(failed)?;
        self.emit_status_changed().await;
        Ok(())
    }

    /// A montagem da conta conectada.
    async fn mount_point(&self) -> Result<PathBuf, ServiceError> {
        self.with_account(|a| a.mount_point.clone())
            .await
            .ok_or_else(|| self.not_logged_in())
    }

    /// Nome da pasta do Meu Drive na montagem (no idioma do sistema, como o FUSE mostra).
    fn my_drive_name(&self) -> String {
        self.i18n.tr("folder-my-drive", &[])
    }

    /// Cancela o envio do arquivo: `canceled` (a versão local foi para `Uploads cancelados`) ou
    /// `too-late` (o envio já terminou).
    pub async fn cancel_upload(&self, item_id: &str) -> Result<String, ServiceError> {
        let engine = self.connected_engine().await?;
        let outcome = engine
            .cancel_upload(&ItemId(item_id.into()))
            .await
            .map_err(failed)?;
        self.emit_status_changed().await;
        Ok(outcome.as_str().into())
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

    pub async fn offline_summary(&self) -> Result<OfflineSummary, ServiceError> {
        let engine = self.connected_engine().await?;
        let store = engine.store();
        Ok(OfflineSummary {
            pinned_bytes: store.pinned_bytes().map_err(failed)?,
            cache_bytes: store.cached_bytes().map_err(failed)?,
            free_bytes: engine.cache().free_bytes().map_err(failed)?,
            pinned_count: store.count_top_pinned().map_err(failed)?,
        })
    }

    /// Ver `Store::pinned_children`; `folder_id` vazio é o topo.
    pub async fn offline_items(&self, folder_id: &str) -> Result<Vec<OfflineEntry>, ServiceError> {
        let (engine, my_drive) = self.engine_and_my_drive().await?;
        let folder = (!folder_id.is_empty()).then(|| ItemId(folder_id.into()));
        let items = engine
            .store()
            .pinned_children(folder.as_ref())
            .map_err(failed)?;
        let mut paths = DrivePaths::new(&engine, &my_drive);
        Ok(items
            .into_iter()
            .map(|item| OfflineEntry {
                path: paths.of(&item),
                is_folder: item.kind == ItemKind::Folder,
                size: item.size,
                pin: match item.pin {
                    PinState::None => "mixed",
                    PinState::Pinned | PinState::Inherited => "on",
                }
                .into(),
                id: item.id.0,
                name: item.local_name,
            })
            .collect())
    }

    pub async fn unpin_item(&self, item_id: &str) -> Result<(), ServiceError> {
        let engine = self.connected_engine().await?;
        engine
            .pins()
            .unpin(&ItemId(item_id.into()))
            .map_err(|e| engine_error(e, item_id))
    }

    pub async fn clear_cache(&self) -> Result<u64, ServiceError> {
        let engine = self.connected_engine().await?;
        engine.cache().clear().await.map_err(failed)
    }

    pub async fn clear_offline_files(&self) -> Result<(), ServiceError> {
        let engine = self.connected_engine().await?;
        engine.pins().unpin_all().map_err(failed)
    }

    pub async fn search(&self, query: &str) -> Result<Vec<SearchResult>, ServiceError> {
        let (engine, my_drive) = self.engine_and_my_drive().await?;
        let items = engine.search(query).await.map_err(failed)?;
        Ok(search_results(&engine, &my_drive, items))
    }

    pub async fn recent_items(&self) -> Result<Vec<SearchResult>, ServiceError> {
        let (engine, my_drive) = self.engine_and_my_drive().await?;
        let items = engine.recent().map_err(failed)?;
        Ok(search_results(&engine, &my_drive, items))
    }

    /// `file` abre o item, `folder` a pasta que o contém (os dois pela montagem) e `web` o
    /// endereço no Drive.
    pub async fn open_item(&self, item_id: &str, target: &str) -> Result<(), ServiceError> {
        if !matches!(target, "file" | "folder" | "web") {
            return Err(ServiceError::InvalidArgs(target.into()));
        }
        let (engine, my_drive) = self.engine_and_my_drive().await?;
        let item = live_item(&engine, item_id)?;
        let path = || Ok::<_, ServiceError>(my_drive.join(path_in_drive(&engine, &item)?));
        match target {
            "web" => self.open(web_link(&item)?),
            "file" => self.open_local(&path()?),
            _ => self.open_local(path()?.parent().unwrap_or(&my_drive)),
        }
        Ok(())
    }

    pub async fn web_link(&self, item_id: &str) -> Result<String, ServiceError> {
        let engine = self.connected_engine().await?;
        web_link(&live_item(&engine, item_id)?)
    }

    pub async fn shared_with_me(&self) -> Result<Vec<SharedItem>, ServiceError> {
        let engine = self.connected_engine().await?;
        let shared = engine.shared_with_me().await.map_err(failed)?;
        Ok(shared
            .into_iter()
            .map(|s| SharedItem {
                id: s.file.id,
                name: s.file.name,
                mime: s.file.mime_type,
                owner: s.owner,
                shared_ms: s.shared_ms,
            })
            .collect())
    }

    pub async fn add_shortcut(&self, item_id: &str) -> Result<(), ServiceError> {
        let engine = self.connected_engine().await?;
        engine
            .add_shortcut(&ItemId(item_id.into()))
            .await
            .map(drop)
            .map_err(|e| engine_error(e, item_id))
    }

    /// O motor da conta conectada e a pasta do Meu Drive na montagem.
    async fn engine_and_my_drive(&self) -> Result<(Arc<Engine>, PathBuf), ServiceError> {
        let engine = self.connected_engine().await?;
        Ok((engine, self.mount_point().await?.join(self.my_drive_name())))
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
        let my_drive = self.my_drive_name();
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

    /// Desmonta, esquece os tokens e apaga os dados locais da conta (banco e cache). As cópias
    /// guardadas (Lost and Found e Uploads cancelados) ficam: são a única cópia dessas versões.
    async fn sign_out(&self, engine: &Engine, email: &str) -> Result<(), ServiceError> {
        self.stop_account().await;
        self.config.tokens.delete(email).await.map_err(failed)?;
        let _ = std::fs::remove_file(self.current_account_file());
        let dirs = engine.local_dirs();
        let keep = engine.kept().dirs();
        tokio::task::spawn_blocking(move || {
            // Só as pastas com alguma cópia; as vazias vão embora com o resto.
            let keep: Vec<PathBuf> = keep
                .into_iter()
                .filter(|k| std::fs::read_dir(k).is_ok_and(|mut d| d.next().is_some()))
                .collect();
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

/// A foto guardada da conta, ou baixada de `link` e guardada; vazia se não houver ou falhar
/// (a foto não é essencial: o erro só vai para o log).
async fn cached_photo(engine: &Engine, link: &str) -> Vec<u8> {
    let saved = engine.data_dir().join(PHOTO_FILE);
    if let Ok(bytes) = tokio::fs::read(&saved).await {
        return bytes;
    }
    if link.is_empty() {
        return Vec::new();
    }
    let bytes = match engine.fetch_photo(link).await {
        Ok(bytes) => bytes.to_vec(),
        Err(e) => {
            tracing::warn!(erro = %e, "não deu para baixar a foto da conta");
            return Vec::new();
        }
    };
    // Grava ao lado e renomeia: uma queda no meio não deixa uma foto pela metade.
    let partial = saved.with_extension("part");
    let stored = match tokio::fs::write(&partial, &bytes).await {
        Ok(()) => tokio::fs::rename(&partial, &saved).await,
        Err(e) => Err(e),
    };
    if let Err(e) = stored {
        tracing::warn!(erro = %e, "não deu para guardar a foto da conta");
    }
    bytes
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
/// cópias guardadas.
fn unsynced(engine: &Engine) -> gdl_engine::Result<usize> {
    Ok(engine.store().pending_items()?.len() + engine.kept().list().len())
}

/// Apaga `dir` inteira, menos as pastas de `keep` (e as pastas no caminho até elas).
fn remove_all_but(dir: &Path, keep: &[PathBuf]) -> std::io::Result<()> {
    if !keep.iter().any(|k| k.starts_with(dir)) {
        return match std::fs::remove_dir_all(dir) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            result => result,
        };
    }
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        if keep.contains(&path) {
            continue;
        }
        if keep.iter().any(|k| k.starts_with(&path)) {
            remove_all_but(&path, keep)?;
        } else if path.is_dir() {
            std::fs::remove_dir_all(&path)?;
        } else {
            std::fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Copia o conteúdo local dos arquivos pendentes e as cópias guardadas para `dir`, sem
/// sobrescrever nada.
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
    for saved in engine.kept().list() {
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

/// `BandwidthRxKBPS`/`BandwidthTxKBPS` nos baldes.
fn apply_bandwidth(bandwidth: &Bandwidth, prefs: &Preferences) {
    bandwidth.rx.set_rate(prefs.bandwidth_rx_kbps);
    bandwidth.tx.set_rate(prefs.bandwidth_tx_kbps);
}
