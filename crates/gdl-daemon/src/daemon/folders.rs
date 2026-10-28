//! As pastas do computador ("Computadores"): o serviço guarda os pares no banco de cada conta,
//! roda um laço por par enquanto a conta está conectada e cria as pastas de apoio no Drive.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gdl_drive::client::RemoteFile;
use gdl_drive::model::Change;
use gdl_engine::engine::Engine;
use gdl_folders::guard::{self, Refusal};
use gdl_folders::sync::{FolderSync, Links};
use gdl_folders::trash::Trash;
use gdl_ipc::{FolderCheck, LocalFolder, ServiceError};
use gdl_store::ItemId;
use gdl_store::folders::{Direction, Pair, PairKind, PairState, StopReason};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::{Daemon, failed};
use crate::dbus::Service;
use crate::file_name;
use crate::notify::{self, Notification};

/// `appProperties` das pastas de apoio: achadas por elas, não pelo nome (que o usuário pode
/// trocar na web, e que muda com o idioma).
const ROLE: (&str, &str) = ("gdl.role", "computers");
const COMPUTER: &str = "gdl.computer";
/// Onde o banco da conta guarda o ID da pasta deste computador.
const COMPUTER_KEY: &str = "folders.computer";

/// Este computador no Drive.
#[derive(Debug, Clone)]
pub struct Computer {
    /// 16 hexadecimais do SHA-256 do `machine-id`.
    pub id: String,
    /// O nome "bonito" do systemd, senão o hostname.
    pub name: String,
}

impl Computer {
    pub async fn detect() -> Computer {
        let machine = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
        let digest = <sha2::Sha256 as sha2::Digest>::digest(machine.trim().as_bytes());
        let id = digest[..8].iter().map(|b| format!("{b:02x}")).collect();
        let name = pretty_hostname()
            .await
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| {
                rustix::system::uname()
                    .nodename()
                    .to_string_lossy()
                    .into_owned()
            });
        Computer { id, name }
    }
}

/// `PrettyHostname` do `org.freedesktop.hostname1` (barramento do sistema).
async fn pretty_hostname() -> Option<String> {
    // Com o serviço do sistema fora do ar, a partida não espera por ele.
    tokio::time::timeout(std::time::Duration::from_secs(2), async {
        let conn = zbus::Connection::system().await.ok()?;
        let proxy = zbus::Proxy::new(
            &conn,
            "org.freedesktop.hostname1",
            "/org/freedesktop/hostname1",
            "org.freedesktop.hostname1",
        )
        .await
        .ok()?;
        proxy.get_property("PrettyHostname").await.ok()
    })
    .await
    .ok()
    .flatten()
}

/// O que o serviço guarda de cada conta: um laço por par e os pares mudando de lugar.
#[derive(Default)]
pub(super) struct Runners {
    running: Mutex<RunnerMap>,
    moving: Mutex<HashSet<i64>>,
}

/// O que os pares de uma conta precisam: o e-mail (para os avisos), o motor, os laços que rodam e
/// o cancelamento da conta.
pub(super) struct FolderAccount {
    pub email: String,
    pub engine: Arc<Engine>,
    pub runners: Arc<Runners>,
    pub shutdown: CancellationToken,
    /// A montagem: os atalhos do Espelho apontam para ela.
    pub mount: PathBuf,
}

/// Enquanto existe, o par consta como mudando de lugar (mesmo se a troca entrar em pânico).
struct Moving {
    runners: Arc<Runners>,
    id: i64,
}

impl Drop for Moving {
    fn drop(&mut self) {
        crate::lock(&self.runners.moving).remove(&self.id);
    }
}

type RunnerMap = HashMap<i64, (Arc<FolderSync>, CancellationToken, JoinHandle<()>)>;

impl Runners {
    fn lock(&self) -> std::sync::MutexGuard<'_, RunnerMap> {
        crate::lock(&self.running)
    }

    /// Marca o par como mudando de lugar até a guarda sair; nada se ele já estava.
    fn begin_move(self: &Arc<Self>, id: i64) -> Option<Moving> {
        crate::lock(&self.moving).insert(id).then(|| Moving {
            runners: self.clone(),
            id,
        })
    }

    /// Pede ao laço do par para parar; ele termina a passada em andamento, e quem vai subir
    /// outro no mesmo par espera o que sai daqui.
    fn stop(&self, id: i64) -> Option<JoinHandle<()>> {
        let (_, stop, running) = self.lock().remove(&id)?;
        stop.cancel();
        Some(running)
    }

    pub(super) fn get(&self, id: i64) -> Option<Arc<FolderSync>> {
        self.lock().get(&id).map(|(sync, ..)| sync.clone())
    }
}

impl Daemon {
    /// Sobe os pares guardados da conta (na partida dela); param com `shutdown`.
    pub(super) fn start_folders(self: &Arc<Self>, account: &FolderAccount) {
        match account.engine.store().pairs() {
            Ok(pairs) => {
                // Com a política, os pares ficam guardados mas não rodam.
                let allowed = |kind| match kind {
                    PairKind::Folder => self.folders_refused().is_ok(),
                    PairKind::Mirror => self.mirror_refused().is_ok(),
                    PairKind::Device => false,
                };
                for pair in pairs.into_iter().filter(|p| allowed(p.kind)) {
                    self.start_folder(account, pair);
                }
            }
            Err(e) => tracing::warn!(erro = %e, "falha ao ler as pastas do computador"),
        }
    }

    fn start_folder(self: &Arc<Self>, account: &FolderAccount, pair: Pair) {
        let FolderAccount {
            engine,
            runners,
            shutdown,
            mount,
            ..
        } = account;
        let (id, name, kind) = (pair.id, file_name(&pair.local), pair.kind);
        let mut sync = FolderSync::new(
            engine.store().clone(),
            engine.client().clone(),
            pair,
            self.system_trash(),
            self.i18n.tr("conflict-word", &[]),
        );
        if kind == PairKind::Mirror {
            sync = sync.with_links(Links {
                email: account.email.clone(),
                shortcut_targets: mount.join(&self.layout.shortcut_targets),
            });
        }
        // A vigia de atividade suspeita da conta confere cada arquivo antes de ele subir.
        let screen = {
            let engine = Arc::downgrade(engine);
            move |name: &str, path: &Path| {
                let now = gdl_common::time::now_ms();
                engine
                    .upgrade()
                    .is_none_or(|e| e.screen_write(name, path, now))
            }
        };
        let sync = Arc::new(sync.with_screen(Arc::new(screen)));
        let stop = shutdown.child_token();
        let running = tokio::spawn({
            let (sync, stop, events) = (sync.clone(), stop.clone(), engine.subscribe());
            let timing = self.config.folder_timing;
            async move { sync.run(events, timing, stop).await }
        });
        // O estado do par (parado, esperando confirmação) chega à interface pelo sinal e, quando
        // muda para um que pede ação, ao usuário pelo aviso; o emblema da raiz muda junto.
        let daemon = Arc::downgrade(self);
        let (mut changes, until) = (sync.state_changes(), stop.clone());
        let mut trashed = sync.trashed_changes();
        let mut trashed_seen = *trashed.borrow();
        let (email, root) = (
            account.email.clone(),
            sync.pair().local.display().to_string(),
        );
        tokio::spawn(async move {
            loop {
                // A comparação mandou itens que saíram do Drive para a lixeira do sistema.
                let mut sent_to_trash = None;
                tokio::select! {
                    () = until.cancelled() => break,
                    changed = changes.changed() => if changed.is_err() { break },
                    changed = trashed.changed() => match changed {
                        Ok(()) => {
                            let total = *trashed.borrow_and_update();
                            sent_to_trash = Some(total - trashed_seen);
                            trashed_seen = total;
                        }
                        Err(_) => break,
                    },
                }
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                if let Some(count) = sent_to_trash {
                    let note = notify::items_trashed(count, &daemon.i18n);
                    let account = email.clone();
                    daemon.notify(&Notification { account, ..note }).await;
                    continue;
                }
                daemon.emit_local_folders_changed().await;
                daemon
                    .emit_states_changed(std::slice::from_ref(&root))
                    .await;
                let state = *changes.borrow_and_update();
                if let Some(note) = notify::for_folder(id, &name, &state, &daemon.i18n) {
                    daemon
                        .notify(&Notification {
                            account: email.clone(),
                            ..note
                        })
                        .await;
                }
            }
        });
        self.forward_folder_states(sync.clone(), stop.clone());
        // Um laço por par: um que ainda estivesse ali não pode seguir rodando sem dono.
        if let Some((_, orphan, _)) = runners.lock().insert(id, (sync, stop, running)) {
            tracing::warn!(par = id, "um laço já rodava neste par; parado");
            orphan.cancel();
        }
    }

    pub async fn local_folders(&self) -> Result<Vec<LocalFolder>, ServiceError> {
        let mut out = vec![];
        for (email, engine, _) in self.folder_accounts().await {
            let pairs = engine.store().pairs().map_err(failed)?;
            for pair in pairs.into_iter().filter(|p| p.kind == PairKind::Folder) {
                let (state, deletes) = state_text(&pair.state);
                out.push(LocalFolder {
                    account: email.clone(),
                    id: pair.id,
                    path: pair.local.display().to_string(),
                    state,
                    deletes,
                });
            }
        }
        Ok(out)
    }

    pub async fn check_local_folder(
        &self,
        account: &str,
        path: &str,
    ) -> Result<FolderCheck, ServiceError> {
        let engine = self.account_engine(account).await?;
        Ok(match self.check(Path::new(path), None).await {
            Ok((local, info)) => FolderCheck {
                refusal: String::new(),
                size: info.size,
                files: info.files,
                exists: self.existing_folder(&engine, &local).await?.is_some(),
            },
            Err(refusal) => FolderCheck {
                refusal: refusal_text(refusal).into(),
                ..Default::default()
            },
        })
    }

    pub async fn add_local_folder(
        self: &Arc<Self>,
        account: &str,
        path: &str,
        merge: bool,
    ) -> Result<i64, ServiceError> {
        self.folders_refused()?;
        let account = self.folder_account(account).await?;
        let engine = &account.engine;
        let local = self.validated(path, None).await?;
        let remote = match self.existing_folder(engine, &local).await? {
            Some(id) if merge => id,
            _ => self.new_computer_folder(engine, &file_name(&local)).await?,
        };
        let id = engine
            .store()
            .add_pair(&local, &ItemId(remote), PairKind::Folder, Direction::TwoWay)
            .map_err(failed)?;
        self.restart_folder(&account, id).await?;
        Ok(id)
    }

    /// Uma pasta nova em "Computadores/<este computador>", chamada `name` (ou `name (2)`…).
    pub(super) async fn new_computer_folder(
        &self,
        engine: &Arc<Engine>,
        name: &str,
    ) -> Result<String, ServiceError> {
        let computer = self.computer_folder(engine).await?;
        let name = free_name(engine, &computer, name)?;
        let made = engine
            .client()
            .create_folder(&computer, &name, None)
            .await
            .map_err(failed)?;
        remember(engine, made)
    }

    pub async fn remove_local_folder(&self, account: &str, id: i64) -> Result<(), ServiceError> {
        let account = self.folder_account(account).await?;
        self.drop_pair(&account, id).await
    }

    /// Para o laço do par e o desfaz (os arquivos ficam nos dois lados).
    pub(super) async fn drop_pair(
        &self,
        account: &FolderAccount,
        id: i64,
    ) -> Result<(), ServiceError> {
        account.runners.stop(id);
        account.engine.store().remove_pair(id).map_err(failed)?;
        self.emit_local_folders_changed().await;
        Ok(())
    }

    pub async fn relocate_local_folder(
        self: &Arc<Self>,
        account: &str,
        id: i64,
        path: &str,
    ) -> Result<(), ServiceError> {
        self.folders_refused()?;
        let account = self.folder_account(account).await?;
        let local = self.validated(path, Some(id)).await?;
        let Some(moving) = account.runners.begin_move(id) else {
            return Err(ServiceError::Refused("moving".into()));
        };
        let old = account.runners.stop(id);
        // A passada em andamento do laço antigo ainda grava o estado sincronizado do par: o lugar
        // novo (que zera esse estado) só entra quando ele termina, e a resposta não espera.
        let daemon = self.clone();
        tokio::spawn(async move {
            let _moving = moving;
            if let Some(old) = old {
                let _ = old.await;
            }
            let moved = async {
                // A política pode ter mudado durante a espera.
                daemon.folders_refused()?;
                let store = account.engine.store();
                store.set_pair_local(id, &local).map_err(failed)?;
                store
                    .set_pair_state(id, PairState::Active)
                    .map_err(failed)?;
                daemon.restart_folder(&account, id).await
            };
            if let Err(e) = moved.await {
                tracing::warn!(erro = %e, "não deu para levar a pasta para o lugar novo");
            }
        });
        Ok(())
    }

    /// A lixeira do usuário, para onde vai o que sai do Drive.
    pub(super) fn system_trash(&self) -> Trash {
        Trash::new(self.config.paths.data_home.join("Trash"))
    }

    /// Sobe (de novo) o laço do par `id` como está no banco e avisa a interface.
    pub(super) async fn restart_folder(
        self: &Arc<Self>,
        account: &FolderAccount,
        id: i64,
    ) -> Result<(), ServiceError> {
        let pair = account.engine.store().pair(id).map_err(failed)?;
        let pair = pair.ok_or_else(|| ServiceError::NotFound(id.to_string()))?;
        self.start_folder(account, pair);
        self.emit_local_folders_changed().await;
        Ok(())
    }

    /// A política do administrador desliga as pastas do computador.
    fn folders_refused(&self) -> Result<(), ServiceError> {
        if self.prefs().disable_mirrored_folders {
            return Err(ServiceError::Refused("folders-disabled".into()));
        }
        Ok(())
    }

    /// `path` serve (senão `Refused` com o motivo), já no caminho canônico.
    async fn validated(&self, path: &str, except: Option<i64>) -> Result<PathBuf, ServiceError> {
        match self.check(Path::new(path), except).await {
            Ok((local, _)) => Ok(local),
            Err(r) => Err(ServiceError::Refused(refusal_text(r).into())),
        }
    }

    pub async fn confirm_folder_deletes(&self, account: &str, id: i64) -> Result<(), ServiceError> {
        let sync = self
            .folder_account(account)
            .await?
            .runners
            .get(id)
            .ok_or_else(|| ServiceError::NotFound(id.to_string()))?;
        sync.confirm_deletes().map_err(failed)
    }

    /// `StatesChanged` com os caminhos que ficaram sincronizados no par: espera o primeiro e
    /// junta os que chegarem em `STATES_SIGNAL_GAP`, como os da montagem.
    fn forward_folder_states(self: &Arc<Self>, sync: Arc<FolderSync>, stop: CancellationToken) {
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = stop.cancelled() => break,
                    () = sync.synced_ready() => {}
                }
                tokio::time::sleep(super::STATES_SIGNAL_GAP).await;
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                let paths: Vec<String> = sync
                    .take_synced()
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
                daemon.emit_states_changed(&paths).await;
            }
        });
    }

    /// A pasta do botão direito que pode virar pasta sincronizada ("Sincronizar esta pasta"):
    /// uma só, que existe, fora das montagens, das outras pastas e dos dados do app, com alguma
    /// conta conectada e a política deixando. Sem medir a pasta (o menu precisa ser rápido): o
    /// configurador confere o resto.
    pub(super) async fn folder_to_sync(&self, paths: &[String]) -> Option<PathBuf> {
        let [path] = paths else {
            return None;
        };
        let path = Path::new(path).canonicalize().ok()?;
        if self.folders_refused().is_err()
            || !path.is_dir()
            || self.accounts.lock().await.is_empty()
        {
            return None;
        }
        let taken = self.taken(None).await;
        guard::overlap(&path, &taken).is_none().then_some(path)
    }

    /// O emblema de cada caminho, como o atributo `user.gdl.state` da montagem: a raiz de uma
    /// pasta mostra o estado dela (`conflict` parada ou esperando confirmação), o que está
    /// dentro `offline` se já sincronizado e `syncing` se não; fora das pastas, vazio.
    pub async fn folder_states(&self, paths: &[String]) -> Vec<String> {
        let pairs = self.all_pairs().await;
        paths
            .iter()
            .map(|p| {
                pair_state(&pairs, Path::new(p))
                    .map_or("", |(_, s)| s)
                    .to_owned()
            })
            .collect()
    }

    /// O emblema de `path` numa pasta sincronizada e o motor da conta dela.
    pub(super) async fn folder_state(&self, path: &Path) -> Option<(Arc<Engine>, &'static str)> {
        let pairs = self.all_pairs().await;
        pair_state(&pairs, path).map(|(engine, state)| (engine.clone(), state))
    }

    pub async fn folder_roots(&self) -> Vec<String> {
        let pairs = self.all_pairs().await;
        pairs
            .iter()
            .map(|info| info.pair.local.display().to_string())
            .collect()
    }

    /// Os pares que são pastas no computador, de todas as contas conectadas, com o motor de cada
    /// um. Os backups de dispositivo ficam de fora: são envios avulsos, e o ponto de montagem
    /// guardado pode ser hoje de outro disco.
    async fn all_pairs(&self) -> Vec<PairInfo> {
        // O banco é lido depois de soltar a lista de contas.
        let accounts: Vec<_> = {
            let accounts = self.accounts.lock().await;
            let copy = |a: &super::Account| (a.engine.clone(), a.folders.clone());
            accounts.iter().map(copy).collect()
        };
        let mut pairs = vec![];
        for (engine, runners) in accounts {
            for pair in engine.store().pairs().unwrap_or_default() {
                if pair.kind != PairKind::Device {
                    let sync = runners.get(pair.id);
                    pairs.push(PairInfo {
                        engine: engine.clone(),
                        pair,
                        sync,
                    });
                }
            }
        }
        pairs
    }

    pub(super) async fn emit_local_folders_changed(&self) {
        if let Some(emitter) = self.emitter() {
            let _ = Service::local_folders_changed(&emitter).await;
        }
    }

    /// As pastas que outra não pode conter nem estar dentro: os dados do app, as montagens e as
    /// pastas de todas as contas (menos `except`, que está mudando de lugar).
    pub(super) async fn taken(&self, except: Option<i64>) -> Vec<PathBuf> {
        let mut taken = vec![self.config.paths.data_dir.clone()];
        for (_, engine, mount) in self.folder_accounts().await {
            taken.push(mount);
            let pairs = engine.store().pairs().unwrap_or_default();
            taken.extend(
                pairs
                    .into_iter()
                    .filter(|p| p.kind != PairKind::Device && Some(p.id) != except)
                    .map(|p| p.local),
            );
        }
        taken
    }

    /// A pasta serve? Confere contra `taken` e devolve também o caminho canônico.
    pub(super) async fn check(
        &self,
        path: &Path,
        except: Option<i64>,
    ) -> Result<(PathBuf, guard::FolderInfo), Refusal> {
        let taken = self.taken(except).await;
        let path = path.to_path_buf();
        tokio::task::spawn_blocking(move || {
            let info = guard::check_folder(&path, &taken, guard::network_fs)?;
            Ok((path.canonicalize().map_err(|_| Refusal::Missing)?, info))
        })
        .await
        .unwrap_or(Err(Refusal::Unreadable))
    }

    /// A pasta de mesmo nome que `local` já em "Computadores/<este computador>" (reinstalação).
    async fn existing_folder(
        &self,
        engine: &Arc<Engine>,
        local: &Path,
    ) -> Result<Option<String>, ServiceError> {
        let Some(computer) = self.find_computer_folder(engine).await? else {
            return Ok(None);
        };
        Ok(live_folder(engine, &computer, &file_name(local))?.map(|i| i.id.0))
    }

    /// A pasta deste computador: a do banco da conta, se ainda viva; senão, procurada no Drive
    /// (uma vez por conta, não a cada checagem).
    async fn find_computer_folder(
        &self,
        engine: &Arc<Engine>,
    ) -> Result<Option<String>, ServiceError> {
        let store = engine.store();
        let known = store.kv_get(COMPUTER_KEY).map_err(failed)?;
        if let Some(id) = known.filter(|id| {
            let item = store.get(&ItemId(id.clone())).ok().flatten();
            item.is_some_and(|i| !i.trashed)
        }) {
            return Ok(Some(id));
        }
        let found = find_support(engine, COMPUTER, &self.config.computer.id).await?;
        if let Some(id) = &found {
            store.kv_set(COMPUTER_KEY, id).map_err(failed)?;
            let computer = store.get(&ItemId(id.clone())).map_err(failed)?;
            if let Some(computers) = computer.and_then(|i| i.parent) {
                note_computers(engine, &computers.0)?;
            }
        }
        Ok(found)
    }

    /// "Computadores/<este computador>" no Drive, criando o que faltar.
    async fn computer_folder(&self, engine: &Arc<Engine>) -> Result<String, ServiceError> {
        if let Some(id) = self.find_computer_folder(engine).await? {
            return Ok(id);
        }
        let client = engine.client();
        let computers = match find_computers(engine).await? {
            Some(id) => id,
            None => {
                let root = client.get("root").await.map_err(failed)?;
                let name = self.i18n.tr("computers-folder", &[]);
                let made = client
                    .create_folder_with_props(&root.id, &name, &[ROLE])
                    .await;
                let id = remember(engine, made.map_err(failed)?)?;
                note_computers(engine, &id)?;
                id
            }
        };
        let computer = &self.config.computer;
        let made = client
            .create_folder_with_props(&computers, &computer.name, &[(COMPUTER, &computer.id)])
            .await
            .map_err(failed)?;
        let id = remember(engine, made)?;
        engine.store().kv_set(COMPUTER_KEY, &id).map_err(failed)?;
        Ok(id)
    }

    /// E-mail, motor e montagem de cada conta conectada.
    async fn folder_accounts(&self) -> Vec<(String, Arc<Engine>, PathBuf)> {
        let accounts = self.accounts.lock().await;
        accounts
            .iter()
            .map(|a| (a.email.clone(), a.engine.clone(), a.mount_point.clone()))
            .collect()
    }

    pub(super) async fn folder_account(
        &self,
        account: &str,
    ) -> Result<FolderAccount, ServiceError> {
        self.with_account(account, |a| FolderAccount {
            email: a.email.clone(),
            engine: a.engine.clone(),
            runners: a.folders.clone(),
            shutdown: a.shutdown.clone(),
            mount: a.mount_point.clone(),
        })
        .await
    }
}

/// A primeira pasta de apoio com a propriedade, gravada no banco da conta.
async fn find_support(
    engine: &Engine,
    key: &str,
    value: &str,
) -> Result<Option<String>, ServiceError> {
    let found = engine.client().find_by_app_property(key, value).await;
    let first = found.map_err(failed)?.into_iter().next();
    first.map(|f| remember(engine, f)).transpose()
}

/// Na partida da conta: os backups (deste e de outros computadores) aparecem na montagem mesmo
/// sem pares. Procura uma vez, enquanto a pasta não é conhecida; sem rede, fica para a próxima.
pub(super) async fn look_for_computers(engine: &Engine) {
    if !matches!(engine.store().computers_folder(), Ok(None)) {
        return;
    }
    if let Err(e) = find_computers(engine).await {
        tracing::warn!(erro = ?e, "falha ao procurar a pasta Computadores");
    }
}

/// A pasta de apoio "Computadores" no Drive, gravada no banco para a montagem mostrar.
async fn find_computers(engine: &Engine) -> Result<Option<String>, ServiceError> {
    let found = find_support(engine, ROLE.0, ROLE.1).await?;
    if let Some(id) = &found {
        note_computers(engine, id)?;
    }
    Ok(found)
}

/// Grava a pasta de apoio, que a montagem mostra na raiz.
fn note_computers(engine: &Engine, id: &str) -> Result<(), ServiceError> {
    let store = engine.store();
    store
        .set_computers_folder(&ItemId(id.into()))
        .map_err(failed)
}

/// A pasta viva `name` dentro de `parent`, pelo banco.
fn live_folder(
    engine: &Engine,
    parent: &str,
    name: &str,
) -> Result<Option<gdl_store::Item>, ServiceError> {
    let found = engine.store().lookup(&ItemId(parent.into()), name);
    Ok(found
        .map_err(failed)?
        .filter(|i| i.kind == gdl_store::ItemKind::Folder && !i.trashed))
}

/// `name`, ou `name (2)`, `name (3)`… se a pasta do computador já tem uma com esse nome.
fn free_name(engine: &Engine, parent: &str, name: &str) -> Result<String, ServiceError> {
    let mut candidate = name.to_owned();
    for n in 2.. {
        if live_folder(engine, parent, &candidate)?.is_none() {
            break;
        }
        candidate = format!("{name} ({n})");
    }
    Ok(candidate)
}

/// Grava no banco da conta o item que o Drive devolveu e devolve o ID.
fn remember(engine: &Engine, r: RemoteFile) -> Result<String, ServiceError> {
    let id = r.id.clone();
    gdl_engine::changes::apply_changes(engine.store(), vec![Change::Upsert(Box::new(r))])
        .map_err(failed)?;
    Ok(id)
}

/// Um par de pasta do computador: o motor da conta, o par e o laço dele, se está rodando.
struct PairInfo {
    engine: Arc<Engine>,
    pair: Pair,
    sync: Option<Arc<FolderSync>>,
}

/// O emblema de `path` (`FolderStates`) e o motor do par que o contém.
fn pair_state<'a>(pairs: &'a [PairInfo], path: &Path) -> Option<(&'a Arc<Engine>, &'static str)> {
    let (info, rel) = pairs.iter().find_map(|info| {
        let rel = path.strip_prefix(&info.pair.local).ok()?;
        Some((info, rel.to_string_lossy()))
    })?;
    let PairInfo { engine, pair, sync } = info;
    let state = match (rel.is_empty(), pair.state) {
        (true, PairState::Active) => "offline",
        (true, _) => "conflict",
        _ if sync.as_ref().is_some_and(|s| s.has_failed(&rel)) => "error",
        _ if engine.store().is_synced(pair.id, &rel).unwrap_or(false) => "offline",
        _ => "syncing",
    };
    Some((engine, state))
}

pub(super) fn refusal_text(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Missing => "missing",
        Refusal::Unreadable => "unreadable",
        Refusal::Nested => "nested",
        Refusal::Contains => "contains",
        Refusal::Network => "network",
    }
}

fn state_text(state: &PairState) -> (String, u64) {
    let (text, deletes) = match state {
        PairState::Active => ("active", 0),
        PairState::Stopped(StopReason::Missing) => ("stopped-missing", 0),
        PairState::Stopped(StopReason::NoPermission) => ("stopped-no-permission", 0),
        PairState::Stopped(StopReason::Moved) => ("stopped-moved", 0),
        PairState::ConfirmDeletes(n) => ("confirm-deletes", *n),
    };
    (text.into(), deletes)
}
