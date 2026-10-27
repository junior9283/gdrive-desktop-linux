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
use gdl_common::links::DRIVE_TRASH;
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
use gdl_engine::gdocs::{OfficeEditor, web_url};
use gdl_engine::kept::{Kept, KeptFolder};
use gdl_fs::{DriveFs, Layout};
use gdl_store::{DrivePath, Inode, Item, ItemId, ItemKind, PinState};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;
use tokio_util::sync::{CancellationToken, DropGuard};
use url::Url;
use zbus::Connection;
use zbus::object_server::SignalEmitter;

use gdl_ipc::launcher::Launcher;
use gdl_ipc::{
    AccountInfo, AccountSummary, ActivityItem, MenuEntry, NotificationItem, NotificationSetting,
    OBJECT_PATH, OfflineEntry, OfflineSummary, Pref, SearchResult, ServiceError, SharedItem,
    Status, SyncErrorItem,
};

use crate::cosmic;
use crate::dbus::Service;
use crate::desktop;
use crate::history::History;
use crate::menu::{self, Action};
use crate::metered::{NetworkManagerProxy, is_metered};
use crate::mount;
use crate::notify::{
    self, Notification, Notifier, OPEN_CANCELED, OPEN_LOST_FOUND, OPTIONAL_KINDS, RESUME, SIGN_IN,
};

/// Quanto o login espera o retorno do navegador.
const LOGIN_TIMEOUT: Duration = Duration::from_secs(300);
/// Contas conectadas, um e-mail por linha, na ordem em que foram adicionadas.
const ACCOUNTS: &str = "accounts";
/// Quantas contas ficam conectadas ao mesmo tempo.
pub const MAX_ACCOUNTS: usize = 4;
/// Arquivo da 0.7 (uma só conta); migrado para `accounts` ao iniciar.
const CURRENT_ACCOUNT: &str = "current-account";
/// Intervalo mínimo entre dois sinais `ActivityChanged`.
const ACTIVITY_SIGNAL_GAP: Duration = Duration::from_millis(250);
/// De quanto em quanto tempo o serviço recolhe os itens que mudaram de estado (`StatesChanged`).
const STATES_SIGNAL_GAP: Duration = Duration::from_millis(300);
/// Caminhos por sinal `StatesChanged` (fixar uma pasta grande muda milhares de itens).
const STATES_PER_SIGNAL: usize = 500;
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
    /// O programa da janela, para abrir telas (`Compartilhar`, `Gerenciar versões`) pelo menu.
    pub ui: PathBuf,
    pub backoff: Backoff,
    pub intervals: Intervals,
}

/// A conta conectada. Ao sair, desmonta e depois para o motor (os campos caem nessa ordem).
struct Account {
    email: String,
    engine: Arc<Engine>,
    /// Os tokens do motor: o login novo da conta entra aqui, sem desmontar nada.
    access: Arc<AccessTokens>,
    mount_point: PathBuf,
    shutdown: CancellationToken,
    _session: fuser::BackgroundSession,
    _stop: DropGuard,
}

mod sharing;

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

/// Onde o item está no Drive: a raiz e o caminho a partir dela (`Pasta/arquivo.txt`). Itens que
/// a montagem não mostra (de pastas compartilhadas por outras pessoas, de Drive desligado) não
/// têm.
fn path_in_drive(engine: &Engine, item: &Item) -> Result<DrivePath, ServiceError> {
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
    mount: &'a Path,
    layout: &'a Layout,
    folders: HashMap<ItemId, Option<DrivePath>>,
}

impl<'a> DrivePaths<'a> {
    fn new(engine: &'a Engine, mount: &'a Path, layout: &'a Layout) -> DrivePaths<'a> {
        DrivePaths {
            engine,
            mount,
            layout,
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
        let folder = folder.unwrap_or_else(|| DrivePath::my_drive(""));
        self.layout
            .mounted(self.mount, &folder)
            .join(&item.local_name)
            .display()
            .to_string()
    }
}

/// Os caminhos na montagem dos itens que mudaram de estado desde a última vez (os que foram para
/// a lixeira não aparecem mais lá).
fn changed_paths(engine: &Engine, mount: &Path, layout: &Layout) -> Vec<String> {
    let items = engine.store().take_state_changes().unwrap_or_default();
    let mut paths = DrivePaths::new(engine, mount, layout);
    items
        .iter()
        .filter(|item| !item.trashed)
        .map(|item| paths.of(item))
        .collect()
}

/// O que o menu do botão direito precisa saber do item.
/// `office`: arquivos do Office abrem no editor on-line (`OpenOfficeFilesInDocs`).
fn picked(engine: &Engine, item: &Item, office: bool) -> Result<menu::Picked, ServiceError> {
    let is_folder = item.kind == ItemKind::Folder;
    let offline = match item.pin {
        PinState::Pinned | PinState::Inherited => menu::Offline::On,
        PinState::None
            if is_folder && engine.store().has_pinned_inside(&item.id).map_err(failed)? =>
        {
            menu::Offline::Mixed
        }
        PinState::None => menu::Offline::Off,
    };
    let has_link = web_url(item).is_some();
    let is_file = has_link && item.kind == ItemKind::File;
    Ok(menu::Picked {
        is_folder,
        has_link,
        has_versions: is_file,
        editor: OfficeEditor::for_name(&item.local_name).filter(|_| is_file && office),
        offline,
    })
}

/// A pasta dos programas: a bandeja e a CLI ficam ao lado do serviço (em /usr/bin, ou no prefixo
/// do install-dev.sh).
fn bin_dir() -> PathBuf {
    gdl_common::paths::exe_dir().unwrap_or_else(|| "/usr/bin".into())
}

fn web_link(item: &Item) -> Result<String, ServiceError> {
    web_url(item).ok_or_else(|| ServiceError::NotFound(item.id.0.clone()))
}

fn search_results(
    engine: &Engine,
    mount: &Path,
    layout: &Layout,
    items: Vec<Item>,
) -> Vec<SearchResult> {
    let mut paths = DrivePaths::new(engine, mount, layout);
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
    /// Os nomes das pastas da raiz da montagem, no idioma do sistema (como o FUSE mostra).
    layout: Layout,
    accounts: tokio::sync::Mutex<Vec<Account>>,
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
        let i18n = I18n::new(&config.lang);
        apply_bandwidth(&bandwidth, &prefs);
        Ok(Arc::new(Daemon {
            bandwidth,
            metered: AtomicBool::new(false),
            layout: Layout::new(&i18n),
            i18n,
            history: History::load(config.paths.state_dir.join(HISTORY_FILE)),
            prefs: RwLock::new(Arc::new(prefs)),
            saving: tokio::sync::Mutex::default(),
            exit: watch::Sender::new(None),
            config,
            accounts: tokio::sync::Mutex::default(),
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
        self.apply_cosmic_actions();
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
            // `null` apaga a chave do usuário.
            let value = (!json.is_null())
                .then(|| toml::Value::try_from(json))
                .transpose()
                .map_err(|e| invalid(&e))?;
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
        for mount_point in self.mount_points().await {
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
        let pause = metered && self.prefs().pause_syncing_on_metered_connection;
        for account in self.accounts.lock().await.iter() {
            account.engine.set_metered_pause(pause);
        }
    }

    /// `AutoStartOnLogin` em `~/.config/autostart`.
    fn apply_autostart(&self) {
        let enabled = self.prefs().auto_start_on_login;
        if let Err(e) = desktop::set_autostart(&self.config.paths.config_home, &bin_dir(), enabled)
        {
            tracing::warn!(erro = %e, "não deu para ajustar o início automático");
        }
    }

    /// As ações do Google Drive no menu do botão direito do COSMIC Files (que não aceita
    /// extensões; os outros gerenciadores trazem a sua dos pacotes).
    fn apply_cosmic_actions(&self) {
        let label = |action: menu::Action| self.i18n.tr(action.label_key(), &[]);
        let config_home = &self.config.paths.config_home;
        if let Err(e) = cosmic::add_actions(config_home, &bin_dir(), label) {
            tracing::warn!(erro = %e, "não deu para ajustar as ações do COSMIC Files");
        }
    }

    /// A montagem fora do indexador do KDE (Baloo), que baixaria tudo para ler o conteúdo.
    fn apply_baloo(&self, mount_point: &Path) {
        let config_home = &self.config.paths.config_home;
        if let Err(e) = desktop::set_baloo_exclusion(config_home, mount_point, true) {
            tracing::warn!(erro = %e, "não deu para tirar a montagem do Baloo");
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
        self.account_engine("").await.ok()
    }

    fn accounts_file(&self) -> PathBuf {
        self.config.paths.data_dir.join(ACCOUNTS)
    }

    /// Grava a lista de contas (vazia, o arquivo some), para a próxima execução reconectá-las.
    /// Uma falha só vai para o log: a conta continua conectada nesta execução.
    async fn save_accounts(&self) {
        let emails: Vec<String> = self
            .accounts
            .lock()
            .await
            .iter()
            .map(|a| a.email.clone())
            .collect();
        let path = self.accounts_file();
        let saved = if emails.is_empty() {
            gdl_common::fs::remove_file_if_exists(&path)
        } else {
            gdl_common::paths::create_private_dir(&self.config.paths.data_dir)
                .and_then(|()| gdl_common::fs::write_atomic(&path, emails.join("\n").as_bytes()))
        };
        if let Err(e) = saved {
            tracing::warn!(erro = %e, "não deu para guardar a lista de contas");
        }
    }

    /// Troca o `current-account` da 0.7 (uma conta só) pelo `accounts`, se ainda não houver.
    fn migrate_current_account(&self) {
        let legacy = self.config.paths.data_dir.join(CURRENT_ACCOUNT);
        if self.accounts_file().exists() {
            return;
        }
        let Ok(email) = std::fs::read_to_string(&legacy) else {
            return;
        };
        if gdl_common::fs::write_atomic(&self.accounts_file(), email.trim().as_bytes()).is_ok() {
            let _ = std::fs::remove_file(&legacy);
        }
    }

    /// Os e-mails guardados, na ordem em que as contas foram adicionadas.
    fn saved_emails(&self) -> Vec<String> {
        std::fs::read_to_string(self.accounts_file())
            .unwrap_or_default()
            .lines()
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(String::from)
            .collect()
    }

    /// Reconecta as contas da execução anterior, se houver. Sobem em paralelo, mas entram na
    /// lista na ordem do arquivo, e a primeira do arquivo continua com a pasta preferida.
    pub async fn resume_saved_accounts(self: &Arc<Self>) -> Result<(), ServiceError> {
        self.migrate_current_account();
        let emails = self.saved_emails();
        let starts = emails.iter().enumerate().map(|(index, email)| async move {
            let token_url = self.config.oauth.token.clone();
            // Sem o login no chaveiro, a conta sobe só com o que está no cache e pede para
            // entrar de novo.
            let stored = self.config.tokens.load(email).await.map_err(failed)?;
            let access = match stored {
                Some((creds, tokens)) => AccessTokens::new(creds, tokens, token_url),
                None => AccessTokens::revoked(token_url),
            };
            self.mount_account(email, Arc::new(access), index == 0)
                .await
        });
        // Uma conta que não sobe não impede as outras. O arquivo não é regravado: a que não
        // subiu continua nele.
        for (email, started) in emails
            .iter()
            .zip(futures_util::future::join_all(starts).await)
        {
            match started {
                Ok((account, moved)) => self.register_account(account, moved).await,
                Err(e) => {
                    tracing::warn!(erro = %e, conta = %email, "não deu para reconectar a conta");
                }
            }
        }
        Ok(())
    }

    /// Desmonta e para o motor (o serviço está saindo).
    pub async fn shutdown(&self) {
        self.stop_all_accounts().await;
    }

    /// Mostra a tela `request` (`tela` ou `tela:argumento`) na janela, abrindo-a se preciso.
    fn open_screen(&self, request: String) {
        let Some(conn) = self.conn.get() else {
            return;
        };
        let launcher = Launcher::new(conn.clone(), self.config.ui.clone());
        tokio::spawn(async move { launcher.open(&request).await });
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
        if let Some(desktop_id) = notifier.send(&self.with_account_name(note).await).await {
            self.history.set_desktop_id(id, desktop_id);
        }
    }

    /// Com mais de uma conta, o aviso da área de trabalho diz de qual é; a janela mostra a conta
    /// pelo campo `account` do histórico.
    async fn with_account_name(&self, note: &Notification) -> Notification {
        let mut note = note.clone();
        if !note.account.is_empty() && self.accounts.lock().await.len() > 1 {
            let account = self.i18n.tr("notify-account", &[("email", &note.account)]);
            note.body = format!("{}\n{account}", note.body);
        }
        note
    }

    /// Botão de um aviso clicado no servidor de notificações.
    async fn on_desktop_action(&self, desktop_id: u32, key: &str) {
        let id = self.history.by_desktop_id(desktop_id);
        let account = id
            .and_then(|id| self.history.get(id))
            .map_or_else(String::new, |note| note.account);
        if let Err(e) = self.run_action(&account, key).await {
            tracing::warn!(erro = %e, acao = key, "ação do aviso falhou");
        }
        if let Some(id) = id
            && self.history.mark_read(id)
        {
            self.emit_notifications_changed().await;
        }
    }

    /// A ação `key` de um aviso, na conta `account` dele.
    async fn run_action(&self, account: &str, key: &str) -> Result<(), ServiceError> {
        let folder = match key {
            RESUME => {
                apply_paused(&*self.account_engine(account).await?, false);
                return Ok(());
            }
            // A tela de login da conta é da janela; o serviço só pede para abri-la.
            SIGN_IN => {
                self.account_engine(account).await?;
                self.open_screen(format!("onboarding:resume:{account}"));
                return Ok(());
            }
            OPEN_LOST_FOUND => KeptFolder::LostAndFound,
            OPEN_CANCELED => KeptFolder::CanceledUploads,
            _ => return Err(ServiceError::InvalidArgs(key.into())),
        };
        let engine = self.account_engine(account).await?;
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
        self.run_action(&note.account, key).await?;
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

    /// Por que uma conta nova não pode entrar com `connected` contas (`too-many-accounts` ou
    /// `add-account-disabled`); `None` se pode.
    fn add_account_refusal(&self, connected: usize) -> Option<&'static str> {
        if connected >= MAX_ACCOUNTS {
            Some("too-many-accounts")
        } else if connected > 0 && self.prefs().disable_add_account {
            Some("add-account-disabled")
        } else {
            None
        }
    }

    /// As credenciais do login: as informadas ou, com o Client ID vazio, as já guardadas (da conta
    /// que entra de novo ou, na falta delas, da primeira que as tem), para quem adiciona outra
    /// conta sem digitá-las de novo.
    async fn credentials_for(
        &self,
        given: ClientCredentials,
        expected: &str,
    ) -> Result<ClientCredentials, ServiceError> {
        if !given.client_id.is_empty() {
            return Ok(given);
        }
        let emails: Vec<String> = self
            .accounts
            .lock()
            .await
            .iter()
            .map(|a| a.email.clone())
            .collect();
        for email in std::iter::once(expected.to_string()).chain(emails) {
            if let Some((creds, _)) = self.config.tokens.load(&email).await.map_err(failed)? {
                return Ok(creds);
            }
        }
        Err(ServiceError::InvalidArgs(
            "sem Client ID para entrar".into(),
        ))
    }

    /// Começa o login no navegador e devolve o endereço (para quem não tem navegador aqui). O
    /// resultado chega pelo sinal `LoginFinished`.
    pub async fn begin_login(
        self: &Arc<Self>,
        creds: ClientCredentials,
        expected: &str,
    ) -> Result<String, ServiceError> {
        let (connected, resuming) = {
            let accounts = self.accounts.lock().await;
            let resuming = !expected.is_empty() && accounts.iter().any(|a| a.email == expected);
            (accounts.len(), resuming)
        };
        // Entrar de novo numa conta que já está na lista não cria conta, então não esbarra nos
        // limites.
        if !resuming && let Some(reason) = self.add_account_refusal(connected) {
            return Err(ServiceError::Refused(reason.into()));
        }
        let creds = self.credentials_for(creds, expected).await?;
        let expected = expected.to_string();
        let flow = LoginFlow::start(creds.clone(), self.config.oauth.clone())
            .await
            .map_err(failed)?;
        let url = flow.auth_url().to_string();
        self.open(url.clone());
        let daemon = self.clone();
        let task = tokio::spawn(async move {
            let (ok, email, error) = match daemon.finish_login(flow, creds, &expected).await {
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

    /// Espera o navegador, confere se a conta é permitida, guarda os tokens e monta a conta (ou,
    /// com `expected`, retoma a que precisava entrar de novo, sem montar de novo). O erro vem com
    /// o e-mail (se já conhecido) e a mensagem.
    async fn finish_login(
        self: &Arc<Self>,
        flow: LoginFlow,
        creds: ClientCredentials,
        expected: &str,
    ) -> Result<String, (String, String)> {
        let tokens = flow
            .wait(LOGIN_TIMEOUT)
            .await
            .map_err(|e| (String::new(), e.to_string()))?;
        let email = tokens.email.clone();
        let fail = |e: &dyn std::fmt::Display| (email.clone(), e.to_string());
        let (connected, already) = {
            let accounts = self.accounts.lock().await;
            (accounts.len(), accounts.iter().any(|a| a.email == email))
        };
        if !expected.is_empty() && email != expected {
            return Err((email, "other-account".into()));
        }
        if already && expected.is_empty() {
            return Err((email, "already-connected".into()));
        }
        if !already && connected >= MAX_ACCOUNTS {
            return Err((email, "too-many-accounts".into()));
        }
        if !self.allowed(&email) {
            let message = self.i18n.tr("login-rejected", &[("email", &email)]);
            return Err((email, message));
        }
        self.config
            .tokens
            .save(&email, &creds, &tokens)
            .await
            .map_err(|e| fail(&e))?;
        if already {
            self.resume_account(&email, creds, tokens).await;
            return Ok(email);
        }
        // A pasta preferida é da primeira conta, a que entra com a lista vazia.
        let access = AccessTokens::new(creds, tokens, self.config.oauth.token.clone());
        let (account, moved) = self
            .mount_account(&email, Arc::new(access), connected == 0)
            .await
            .map_err(|e| fail(&e))?;
        self.register_account(account, moved).await;
        self.save_accounts().await;
        Ok(email)
    }

    /// A conta que já está na lista entra de novo: o login novo vale para o motor que está de pé,
    /// que retoma sozinho ao ver o login voltar.
    async fn resume_account(&self, email: &str, creds: ClientCredentials, tokens: TokenSet) {
        let access = {
            let accounts = self.accounts.lock().await;
            accounts
                .iter()
                .find(|a| a.email == email)
                .map(|a| a.access.clone())
        };
        if let Some(access) = access {
            access.replace(creds, tokens).await;
        }
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
            account: email.into(),
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

    /// Abre o motor e monta a conta `email`, sem pô-la na lista. `primary`: a pasta preferida;
    /// as outras contas ficam em `<preferida> (<e-mail>)`. Devolve também se a pasta não é a
    /// preferida (já tinha arquivos), para avisar o usuário.
    async fn mount_account(
        self: &Arc<Self>,
        email: &str,
        access: Arc<AccessTokens>,
        primary: bool,
    ) -> Result<(Account, bool), ServiceError> {
        let config = &self.config;
        let client = DriveClient::new(
            config.api.base.clone(),
            config.api.upload.clone(),
            access.clone(),
        )
        .with_backoff(config.backoff)
        .with_bandwidth(self.bandwidth.clone());
        let prefs = self.prefs();
        let engine = self.open_engine(&prefs, email, client).await?;
        let engine = Arc::new(engine.with_intervals(config.intervals));
        engine.ensure_indexed().await.map_err(failed)?;

        let mut preferred = self.preferred_mount_point(&prefs);
        if !primary {
            preferred = beside(&preferred, email);
        }
        let mount_point = mount::choose(&preferred).map_err(failed)?;
        let uid = rustix::process::getuid().as_raw();
        let gid = rustix::process::getgid().as_raw();
        let fs = DriveFs::new(
            engine.clone(),
            I18n::new(&config.lang),
            uid,
            gid,
            mount_point.clone(),
        );
        let session = gdl_fs::mount(fs, &mount_point).map_err(failed)?;

        // Daqui em diante nada falha: o motor só roda com a conta guardada em `account`.
        let shutdown = CancellationToken::new();
        tokio::spawn({
            let (engine, shutdown) = (engine.clone(), shutdown.clone());
            async move { engine.run(shutdown).await }
        });
        self.forward_events(email.into(), engine.subscribe(), shutdown.clone());
        self.forward_activity(engine.activity_changes(), shutdown.clone());
        self.forward_states(engine.clone(), mount_point.clone(), shutdown.clone());
        let moved = mount_point != preferred;
        let account = Account {
            email: email.into(),
            engine,
            access,
            mount_point,
            _stop: shutdown.clone().drop_guard(),
            shutdown,
            _session: session,
        };
        Ok((account, moved))
    }

    /// Põe a conta montada na lista, com os Locais, o Baloo e o aviso de pasta trocada.
    async fn register_account(&self, account: Account, moved: bool) {
        let mount_point = account.mount_point.clone();
        let email = account.email.clone();
        self.accounts.lock().await.push(account);
        self.apply_sidebar(&mount_point);
        self.apply_baloo(&mount_point);
        self.apply_metered(self.is_metered()).await;
        if moved {
            let path = mount_point.display().to_string();
            self.notify(&Notification {
                kind_id: "mount-moved",
                summary: self.i18n.tr("notify-title", &[]),
                body: self.i18n.tr("mount-moved", &[("path", &path)]),
                action: None,
                replaces_key: None,
                account: email,
            })
            .await;
        }
        self.emit_status_changed().await;
    }

    /// Avisos e `StatusChanged` a partir dos eventos do motor, até a conta sair.
    fn forward_events(
        self: &Arc<Self>,
        email: String,
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
                    Ok(event) => daemon.on_event(&email, &event).await,
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

    /// `StatesChanged` com os caminhos dos itens cujo estado local ou fixação mudou (o banco
    /// anota cada mudança, venha de onde vier), recolhidos a cada `STATES_SIGNAL_GAP`.
    fn forward_states(
        self: &Arc<Self>,
        engine: Arc<Engine>,
        mount: PathBuf,
        shutdown: CancellationToken,
    ) {
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = shutdown.cancelled() => break,
                    () = tokio::time::sleep(STATES_SIGNAL_GAP) => {}
                }
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                let paths = {
                    let (engine, mount) = (engine.clone(), mount.clone());
                    let daemon = daemon.clone();
                    tokio::task::spawn_blocking(move || {
                        changed_paths(&engine, &mount, &daemon.layout)
                    })
                    .await
                    .unwrap_or_default()
                };
                if let Some(emitter) = daemon.emitter() {
                    for chunk in paths.chunks(STATES_PER_SIGNAL) {
                        let _ = Service::states_changed(&emitter, chunk).await;
                    }
                }
            }
        });
    }

    /// O evento do motor da conta `email`: `StatusChanged` e, se couber, um aviso dessa conta.
    async fn on_event(&self, email: &str, event: &EngineEvent) {
        if matches!(
            event,
            EngineEvent::SyncStateChanged(_)
                | EngineEvent::Offline(_)
                | EngineEvent::NeedsSignIn(_)
        ) {
            self.emit_status_changed().await;
        }
        if let Some(note) = notify::for_event(event, &self.i18n) {
            let note = Notification {
                account: email.into(),
                ..note
            };
            self.notify(&note).await;
        }
    }

    /// Desmonta e para todas as contas, sem mexer na lista guardada (o serviço vai voltar a
    /// elas); quem sai de uma conta é `sign_out`, que atualiza a lista.
    async fn stop_all_accounts(&self) {
        let accounts = std::mem::take(&mut *self.accounts.lock().await);
        unmount(accounts).await;
    }

    /// Tira a conta `email` da lista e a desmonta, sem mexer nas outras nem em `accounts`.
    /// Devolve o motor (já parado) e a pasta onde ela estava montada.
    async fn remove_account(&self, email: &str) -> Option<(Arc<Engine>, PathBuf)> {
        let account = {
            let mut accounts = self.accounts.lock().await;
            let index = accounts.iter().position(|a| a.email == email)?;
            accounts.remove(index)
        };
        let removed = (account.engine.clone(), account.mount_point.clone());
        unmount(vec![account]).await;
        Some(removed)
    }

    /// As montagens de todas as contas, na ordem da lista.
    async fn mount_points(&self) -> Vec<PathBuf> {
        let accounts = self.accounts.lock().await;
        accounts.iter().map(|a| a.mount_point.clone()).collect()
    }

    fn not_logged_in(&self) -> ServiceError {
        ServiceError::NotLoggedIn(self.i18n.tr("not-logged-in", &[]))
    }

    /// O estado da conta `account` (vazio: a primeira, com `state` sendo o pior entre todas) e a
    /// lista de contas.
    pub async fn status(&self, account: &str) -> Result<Status, ServiceError> {
        let listed: Vec<_> = self
            .accounts
            .lock()
            .await
            .iter()
            .map(|a| (a.email.clone(), a.mount_point.clone(), a.engine.clone()))
            .collect();
        let can_add_account = self.add_account_refusal(listed.len()).is_none();
        if listed.is_empty() {
            return Ok(Status {
                state: SyncState::SignedOut.as_str().into(),
                remaining_secs: -1,
                can_add_account,
                ..Status::default()
            });
        }
        let mut summaries = Vec::with_capacity(listed.len());
        let mut accounts = Vec::with_capacity(listed.len());
        for (email, mount_point, engine) in listed {
            let summary = engine.summary().map_err(failed)?;
            accounts.push(AccountSummary {
                email,
                mount_point: mount_point.display().to_string(),
                state: summary.state().as_str().into(),
                pending_uploads: summary.pending_uploads,
            });
            summaries.push(summary);
        }
        let index = match account {
            "" => 0,
            email => accounts
                .iter()
                .position(|a| a.email == email)
                .ok_or_else(|| self.not_logged_in())?,
        };
        let summary = &summaries[index];
        let state = match account {
            "" => summaries.iter().map(|s| s.state()).max(),
            _ => None,
        }
        .unwrap_or_else(|| summary.state());
        Ok(Status {
            account: accounts[index].email.clone(),
            mount_point: accounts[index].mount_point.clone(),
            state: state.as_str().into(),
            syncing: summary.syncing,
            remaining_secs: summary.remaining_secs.map_or(-1, |s| s as i64),
            pending_uploads: summary.pending_uploads,
            paused: summary.paused,
            offline: summary.offline,
            last_sync_ms: summary.last_sync_ms.unwrap_or(0),
            errors: summary.errors,
            accounts,
            can_add_account,
        })
    }

    /// Nome, cota e foto da conta conectada. A foto é baixada uma vez e fica no diretório de
    /// dados da conta.
    pub async fn account_info(&self, account: &str) -> Result<AccountInfo, ServiceError> {
        let engine = self.account_engine(account).await?;
        let about = engine.about().await.map_err(failed)?;
        Ok(AccountInfo {
            photo_png: cached_photo(&engine, &about.photo_link).await,
            email: about.email,
            name: about.display_name,
            quota_used: about.quota_used as i64,
            quota_total: about.quota_total.map_or(-1, |t| t as i64),
        })
    }

    pub async fn activity(&self, account: &str) -> Result<Vec<ActivityItem>, ServiceError> {
        let (engine, mount) = self.engine_and_mount(account).await?;
        let rows = engine.activity().map_err(failed)?;
        Ok(rows
            .into_iter()
            .map(|r| ActivityItem {
                seq: r.seq,
                time_ms: r.time_ms,
                path: self
                    .layout
                    .mounted(&mount, &r.activity.path)
                    .display()
                    .to_string(),
                size: r.activity.size as i64,
                state: r.activity.state.as_str().into(),
                percent: r.activity.percent.map_or(-1, i32::from),
                item_id: r.activity.item_id,
                name: r.activity.name,
            })
            .collect())
    }

    pub async fn errors(&self, account: &str) -> Result<Vec<SyncErrorItem>, ServiceError> {
        let (engine, mount_point) = self.engine_and_mount(account).await?;
        // As cópias aparecem na montagem, na pasta oculta do Lost and Found.
        let lost_found = mount_point.join(&self.layout.lost_found);
        let errors = engine.errors().map_err(failed)?;
        Ok(errors
            .into_iter()
            .map(|e| {
                let (item_id, path) = match e.source {
                    ErrorSource::Item { id, path } => {
                        (id.0, self.layout.mounted(&mount_point, &path))
                    }
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
    pub async fn retry(&self, account: &str, item_id: &str) -> Result<(), ServiceError> {
        if item_id.is_empty() {
            return Err(ServiceError::InvalidArgs("item sem envio".into()));
        }
        let engine = self.account_engine(account).await?;
        engine.retry(&ItemId(item_id.into())).map_err(failed)?;
        self.emit_status_changed().await;
        Ok(())
    }

    /// Onde a montagem fica (`DefaultMountPoint`, ou `~/Google Drive`), se estiver livre.
    fn preferred_mount_point(&self, prefs: &Preferences) -> PathBuf {
        prefs
            .default_mount_point
            .clone()
            .unwrap_or_else(|| self.config.home.join("Google Drive"))
    }

    /// A montagem da conta `account`.
    async fn mount_point(&self, account: &str) -> Result<PathBuf, ServiceError> {
        self.with_account(account, |a| a.mount_point.clone()).await
    }

    /// Cancela o envio do arquivo: `canceled` (a versão local foi para `Uploads cancelados`) ou
    /// `too-late` (o envio já terminou) ou `in-use` (o arquivo está aberto para escrita).
    pub async fn cancel_upload(
        &self,
        account: &str,
        item_id: &str,
    ) -> Result<String, ServiceError> {
        let engine = self.account_engine(account).await?;
        let outcome = engine
            .cancel_upload(&ItemId(item_id.into()))
            .await
            .map_err(failed)?;
        self.emit_status_changed().await;
        Ok(outcome.as_str().into())
    }

    /// `f` sobre a conta `account` (e-mail; vazio, a primeira), soltando a trava antes de o
    /// chamador consultar o banco. Sem a conta, `NotLoggedIn`.
    async fn with_account<T>(
        &self,
        account: &str,
        f: impl FnOnce(&Account) -> T,
    ) -> Result<T, ServiceError> {
        let accounts = self.accounts.lock().await;
        let found = if account.is_empty() {
            accounts.first()
        } else {
            accounts.iter().find(|a| a.email == account)
        };
        found.map(f).ok_or_else(|| self.not_logged_in())
    }

    async fn account_engine(&self, account: &str) -> Result<Arc<Engine>, ServiceError> {
        self.with_account(account, |a| a.engine.clone()).await
    }

    /// O e-mail da conta dona do motor (vazio se ela já saiu).
    async fn email_of(&self, engine: &Arc<Engine>) -> String {
        let accounts = self.accounts.lock().await;
        let found = accounts.iter().find(|a| Arc::ptr_eq(&a.engine, engine));
        found.map(|a| a.email.clone()).unwrap_or_default()
    }

    /// O motor da conta cuja montagem é o prefixo mais longo de `path`, e o caminho relativo a ela.
    async fn engine_at(&self, path: &Path) -> Option<(Arc<Engine>, PathBuf)> {
        self.accounts
            .lock()
            .await
            .iter()
            .filter_map(|a| Some((a, relative_to_mount(path, &a.mount_point)?)))
            .max_by_key(|(a, _)| a.mount_point.components().count())
            .map(|(a, relative)| (a.engine.clone(), relative))
    }

    /// Pausa ou retoma todas as contas.
    pub async fn set_paused(&self, paused: bool) -> Result<(), ServiceError> {
        let accounts = self.accounts.lock().await;
        if accounts.is_empty() {
            return Err(self.not_logged_in());
        }
        for account in accounts.iter() {
            apply_paused(&account.engine, paused);
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

    pub async fn lost_and_found(&self, account: &str) -> Result<Vec<String>, ServiceError> {
        let engine = self.account_engine(account).await?;
        let files = engine.lost_and_found().list();
        Ok(files.iter().map(|p| p.display().to_string()).collect())
    }

    pub async fn offline_summary(&self, account: &str) -> Result<OfflineSummary, ServiceError> {
        let engine = self.account_engine(account).await?;
        let store = engine.store();
        Ok(OfflineSummary {
            pinned_bytes: store.pinned_bytes().map_err(failed)?,
            cache_bytes: store.cached_bytes().map_err(failed)?,
            free_bytes: engine.cache().free_bytes().map_err(failed)?,
            pinned_count: store.count_top_pinned().map_err(failed)?,
        })
    }

    /// Ver `Store::pinned_children`; `folder_id` vazio é o topo.
    pub async fn offline_items(
        &self,
        account: &str,
        folder_id: &str,
    ) -> Result<Vec<OfflineEntry>, ServiceError> {
        let (engine, mount) = self.engine_and_mount(account).await?;
        let folder = (!folder_id.is_empty()).then(|| ItemId(folder_id.into()));
        let items = engine
            .store()
            .pinned_children(folder.as_ref())
            .map_err(failed)?;
        let mut paths = DrivePaths::new(&engine, &mount, &self.layout);
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

    pub async fn unpin_item(&self, account: &str, item_id: &str) -> Result<(), ServiceError> {
        let engine = self.account_engine(account).await?;
        engine
            .pins()
            .unpin(&ItemId(item_id.into()))
            .map_err(|e| engine_error(e, item_id))
    }

    pub async fn clear_cache(&self, account: &str) -> Result<u64, ServiceError> {
        let engine = self.account_engine(account).await?;
        engine.cache().clear().await.map_err(failed)
    }

    pub async fn clear_offline_files(&self, account: &str) -> Result<(), ServiceError> {
        let engine = self.account_engine(account).await?;
        engine.pins().unpin_all().map_err(failed)
    }

    pub async fn search(
        &self,
        account: &str,
        query: &str,
    ) -> Result<Vec<SearchResult>, ServiceError> {
        let (engine, mount) = self.engine_and_mount(account).await?;
        let items = engine.search(query).await.map_err(failed)?;
        Ok(search_results(&engine, &mount, &self.layout, items))
    }

    pub async fn recent_items(&self, account: &str) -> Result<Vec<SearchResult>, ServiceError> {
        let (engine, mount) = self.engine_and_mount(account).await?;
        let items = engine.recent().map_err(failed)?;
        Ok(search_results(&engine, &mount, &self.layout, items))
    }

    /// `file` abre o item, `folder` a pasta que o contém (os dois pela montagem) e `web` o
    /// endereço no Drive.
    pub async fn open_item(
        &self,
        account: &str,
        item_id: &str,
        target: &str,
    ) -> Result<(), ServiceError> {
        if !matches!(target, "file" | "folder" | "web") {
            return Err(ServiceError::InvalidArgs(target.into()));
        }
        let (engine, mount) = self.engine_and_mount(account).await?;
        let item = live_item(&engine, item_id)?;
        let path = || {
            let at = path_in_drive(&engine, &item)?;
            Ok::<_, ServiceError>(self.layout.mounted(&mount, &at))
        };
        match target {
            "web" => self.open(web_link(&item)?),
            "file" => self.open_local(&path()?),
            _ => self.open_local(path()?.parent().unwrap_or(&mount)),
        }
        Ok(())
    }

    pub async fn web_link(&self, account: &str, item_id: &str) -> Result<String, ServiceError> {
        let engine = self.account_engine(account).await?;
        web_link(&live_item(&engine, item_id)?)
    }

    pub async fn shared_with_me(&self, account: &str) -> Result<Vec<SharedItem>, ServiceError> {
        let engine = self.account_engine(account).await?;
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

    pub async fn add_shortcut(&self, account: &str, item_id: &str) -> Result<(), ServiceError> {
        let engine = self.account_engine(account).await?;
        engine
            .add_shortcut(&ItemId(item_id.into()))
            .await
            .map(drop)
            .map_err(|e| engine_error(e, item_id))
    }

    /// O menu do botão direito para `paths`: vazio se algum não for do Meu Drive (ou sem conta),
    /// para o gerenciador de arquivos não mostrar o submenu.
    pub async fn menu(&self, paths: &[String]) -> Vec<MenuEntry> {
        let Ok(selection) = self.selection(paths).await else {
            return Vec::new();
        };
        let office = self.prefs().open_office_files_in_docs;
        let picked: Result<Vec<_>, _> = selection
            .iter()
            .map(|(engine, item)| picked(engine, item, office))
            .collect();
        let Ok(picked) = picked else {
            return Vec::new();
        };
        menu::entries(&picked)
            .into_iter()
            .map(|(action, state)| MenuEntry {
                id: action.map_or(menu::SEPARATOR, Action::id).into(),
                label: action.map_or_else(String::new, |a| self.i18n.tr(a.label_key(), &[])),
                state: state.into(),
            })
            .collect()
    }

    /// Faz a ação `action` do menu nos `paths`; devolve o link em `copy-link` e vazio nas outras.
    pub async fn menu_action(
        self: &Arc<Self>,
        action: &str,
        paths: &[String],
    ) -> Result<String, ServiceError> {
        let invalid = || ServiceError::InvalidArgs(action.into());
        let action = Action::parse(action).ok_or_else(invalid)?;
        let selection = self.selection(paths).await?;
        let single = || match selection.as_slice() {
            [(engine, item)] => Ok((engine, item)),
            _ => Err(invalid()),
        };
        let folder = || {
            single().and_then(|f| match f.1.kind {
                ItemKind::Folder => Ok(f),
                _ => Err(invalid()),
            })
        };
        match action {
            Action::OpenWeb => self.open(web_link(single()?.1)?),
            Action::OpenDocs | Action::OpenSheets | Action::OpenSlides => {
                // Só o editor do tipo do arquivo (a ação vem de fora, pelo D-Bus).
                let item = single()?.1;
                let editor = OfficeEditor::for_name(&item.local_name)
                    .filter(|e| action.editor() == Some(*e))
                    .ok_or_else(invalid)?;
                self.open(editor.url(&item.id.0));
            }
            Action::Share | Action::Versions => {
                let (engine, item) = single()?;
                let screen = match action {
                    Action::Share => "share",
                    _ => "versions",
                };
                let account = self.email_of(engine).await;
                self.open_screen(format!("{screen}:{account}/{}", item.id.0));
            }
            Action::CopyLink => {
                let (engine, item) = single()?;
                let link = web_link(item)?;
                // O aviso diz quem pode abrir o link; ele sai depois, sem atrasar a cópia.
                let (daemon, engine, id) = (self.clone(), engine.clone(), item.id.clone());
                let account = self.email_of(&engine).await;
                tokio::spawn(
                    async move { daemon.notify_link_copied(&account, &engine, &id).await },
                );
                return Ok(link);
            }
            Action::Offline | Action::OnlineOnly => {
                for (engine, item) in &selection {
                    let pins = engine.pins();
                    match action {
                        Action::Offline => pins.pin(&item.id),
                        _ => pins.unpin(&item.id),
                    }
                    .map_err(|e| engine_error(e, &item.id.0))?;
                }
            }
            Action::Refresh => folder()?.0.refresh().await,
            Action::Trash => self.open(DRIVE_TRASH.into()),
            Action::NewDocument | Action::NewSpreadsheet | Action::NewPresentation => {
                let new = action.new_file().ok_or_else(invalid)?;
                let (engine, parent) = folder()?;
                let name = self.i18n.tr(new.name_key, &[]);
                let created = engine
                    .create_google_file(&parent.id, &name, new.mime)
                    .await
                    .map_err(failed)?;
                self.open(web_link(&created)?);
            }
        }
        Ok(String::new())
    }

    /// Os itens do Meu Drive em `paths`.
    async fn selection(&self, paths: &[String]) -> Result<Vec<(Arc<Engine>, Item)>, ServiceError> {
        let mut selection = Vec::with_capacity(paths.len());
        for path in paths {
            let (engine, id) = self.item_at(path).await?;
            let item = live_item(&engine, &id.0)?;
            selection.push((engine, item));
        }
        Ok(selection)
    }

    /// O motor da conta `account` e a montagem dela.
    async fn engine_and_mount(
        &self,
        account: &str,
    ) -> Result<(Arc<Engine>, PathBuf), ServiceError> {
        let engine = self.account_engine(account).await?;
        let mount_point = self.mount_point(account).await?;
        Ok((engine, mount_point))
    }

    /// Item do Meu Drive ou de um Drive compartilhado ligado no caminho `path` da montagem.
    async fn item_at(&self, path: &str) -> Result<(Arc<Engine>, ItemId), ServiceError> {
        let not_found =
            || ServiceError::NotFound(self.i18n.tr("path-not-in-drive", &[("path", path)]));
        let (engine, relative) = self
            .engine_at(Path::new(path))
            .await
            .ok_or_else(not_found)?;
        let names: Vec<&str> = relative
            .components()
            .map(|part| match part {
                Component::Normal(name) => name.to_str().ok_or_else(not_found),
                _ => Err(not_found()),
            })
            .collect::<Result<_, _>>()?;
        let store = engine.store();
        let (root, inside) = match names.split_first() {
            Some((first, rest)) if *first == self.layout.my_drive => {
                (store.by_inode(Inode::ROOT), rest)
            }
            Some((first, [drive, rest @ ..])) if *first == self.layout.shared_drives => {
                (Layout::shared_drive_root(store, drive), rest)
            }
            _ => return Err(not_found()),
        };
        let mut item = root.map_err(failed)?.ok_or_else(not_found)?;
        for name in inside {
            item = store
                .lookup(&item.id, name)
                .map_err(failed)?
                .ok_or_else(not_found)?;
        }
        Ok((engine, item.id))
    }

    /// Sai da conta. Com envios pendentes, `mode` diz o que fazer com eles: `finish` (sai quando
    /// terminarem), `move` (copia para a pasta `dir` antes) ou `discard`.
    pub async fn logout(
        self: &Arc<Self>,
        account: &str,
        mode: &str,
        dir: &str,
    ) -> Result<(), ServiceError> {
        let (engine, email, shutdown) = self
            .with_account(account, |a| {
                (a.engine.clone(), a.email.clone(), a.shutdown.clone())
            })
            .await?;
        match mode {
            "" => {
                let pending = unsynced(&engine).map_err(failed)?;
                if pending > 0 {
                    let count = pending.to_string();
                    return Err(ServiceError::PendingUploads(
                        self.i18n.tr("logout-pending", &[("count", &count)]),
                    ));
                }
                self.sign_out(&email).await
            }
            "discard" => self.sign_out(&email).await,
            "move" if !dir.is_empty() => {
                let (copier, dir) = (engine.clone(), PathBuf::from(dir));
                tokio::task::spawn_blocking(move || copy_pending(&copier, &dir))
                    .await
                    .map_err(failed)?
                    .map_err(failed)?;
                self.sign_out(&email).await
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
                    if let Err(e) = daemon.sign_out(&email).await {
                        tracing::warn!(erro = %e, "falha ao sair da conta");
                    }
                });
                Ok(())
            }
            _ => Err(ServiceError::InvalidArgs(format!("{mode} {dir}"))),
        }
    }

    /// Sai de todas as contas, cada uma como em `logout`. Se alguma recusar (envios pendentes), as
    /// anteriores já saíram e o erro é o da primeira que recusou.
    pub async fn logout_all(self: &Arc<Self>, mode: &str, dir: &str) -> Result<(), ServiceError> {
        let emails: Vec<String> = {
            let accounts = self.accounts.lock().await;
            accounts.iter().map(|a| a.email.clone()).collect()
        };
        if emails.is_empty() {
            return Err(self.not_logged_in());
        }
        for email in emails {
            self.logout(&email, mode, dir).await?;
        }
        Ok(())
    }

    /// Desmonta, esquece os tokens e apaga os dados locais da conta (banco e cache). As cópias
    /// guardadas (Lost and Found e Uploads cancelados) ficam: são a única cópia dessas versões.
    pub(crate) async fn sign_out(&self, email: &str) -> Result<(), ServiceError> {
        let (engine, mount_point) = self
            .remove_account(email)
            .await
            .ok_or_else(|| self.not_logged_in())?;
        self.save_accounts().await;
        let places = self.config.paths.data_home.join("user-places.xbel");
        if let Err(e) = desktop::set_sidebar_place(&places, &mount_point, "", false) {
            tracing::warn!(erro = %e, "não deu para tirar a conta dos Locais do Dolphin");
        }
        // Com a montagem já desfeita (nenhum `opendir` semeia PNGs novos) e antes de apagar o
        // banco, que guarda a memória das miniaturas semeadas. Melhor esforço: uma falha aqui
        // não impede a saída da conta.
        let thumbs = engine.clone();
        if let Err(e) = tokio::task::spawn_blocking(move || thumbs.purge_thumbnails()).await {
            tracing::warn!(erro = %e, "falha ao limpar as miniaturas");
        }
        self.config.tokens.delete(email).await.map_err(failed)?;
        let dirs = engine.local_dirs();
        let keep = engine.kept().dirs();
        tokio::task::spawn_blocking(move || {
            let keep = not_empty(keep);
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

    /// Antes de os programas saírem: desmonta, tira o início automático e o lugar nos Locais e,
    /// com `purge`, apaga os dados locais do app; depois encerra o serviço. `purge` só vale sem
    /// conta conectada: a saída da conta é que decide o que fazer com os envios pendentes.
    pub async fn uninstall(&self, purge: bool) -> Result<(), ServiceError> {
        let mut mount_points = self.mount_points().await;
        if purge && !mount_points.is_empty() {
            return Err(ServiceError::Failed(
                self.i18n.tr("uninstall-signed-in", &[]),
            ));
        }
        if mount_points.is_empty() {
            mount_points.push(self.preferred_mount_point(&self.prefs()));
        }
        self.stop_all_accounts().await;
        let paths = self.config.paths.clone();
        tokio::task::spawn_blocking(move || {
            desktop::remove_autostart(&paths.config_home)?;
            cosmic::remove_actions(&paths.config_home)?;
            let places = paths.data_home.join("user-places.xbel");
            for mount_point in &mount_points {
                desktop::set_sidebar_place(&places, mount_point, "", false)?;
                desktop::set_baloo_exclusion(&paths.config_home, mount_point, false)?;
                // Desmontada, a pasta fica vazia; com algo dentro, fica.
                let _ = std::fs::remove_dir(mount_point);
            }
            if purge {
                purge_local_data(&paths)?;
            }
            Ok::<_, std::io::Error>(())
        })
        .await
        .map_err(failed)?
        .map_err(failed)?;
        self.quit().await;
        Ok(())
    }
}

/// Desmonta e para as contas; desmontar espera as threads do FUSE terminarem.
async fn unmount(accounts: Vec<Account>) {
    let _ = tokio::task::spawn_blocking(move || drop(accounts)).await;
}

/// A pasta de uma conta que não é a primeira: `<preferida> (<e-mail>)`.
fn beside(preferred: &Path, email: &str) -> PathBuf {
    let name = preferred.file_name().unwrap_or_default().to_string_lossy();
    preferred.with_file_name(format!("{name} ({email})"))
}

/// O motor avisa a mudança (`SyncStateChanged`), que vira `StatusChanged`.
fn apply_paused(engine: &Engine, paused: bool) {
    match paused {
        true => engine.pause(),
        false => engine.resume(),
    }
}

/// As pastas de `dirs` com alguma coisa dentro.
fn not_empty(dirs: Vec<PathBuf>) -> Vec<PathBuf> {
    dirs.into_iter()
        .filter(|k| std::fs::read_dir(k).is_ok_and(|mut d| d.next().is_some()))
        .collect()
}

/// Apaga a configuração, o estado e os dados do app. As cópias guardadas de cada conta (`Lost
/// and Found` e `Uploads cancelados`) ficam: são a única cópia dessas versões.
fn purge_local_data(paths: &Paths) -> std::io::Result<()> {
    let accounts = match std::fs::read_dir(&paths.data_dir) {
        Ok(entries) => entries.collect::<std::io::Result<Vec<_>>>()?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e),
    };
    let keep = not_empty(
        accounts
            .iter()
            .flat_map(|account| Kept::in_dir(&account.path()).dirs())
            .collect(),
    );
    remove_all_but(&paths.data_dir, &keep)?;
    remove_all_but(&paths.config_dir, &[])?;
    remove_all_but(&paths.state_dir, &[])
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
