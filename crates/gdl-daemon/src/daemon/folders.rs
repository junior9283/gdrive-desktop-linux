//! As pastas do computador ("Computadores"): o serviço guarda os pares no banco de cada conta,
//! roda um laço por par enquanto a conta está conectada e cria as pastas de apoio no Drive.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use gdl_drive::client::RemoteFile;
use gdl_drive::model::Change;
use gdl_engine::engine::Engine;
use gdl_folders::guard::{self, Refusal};
use gdl_folders::sync::FolderSync;
use gdl_folders::trash::Trash;
use gdl_ipc::{FolderCheck, LocalFolder, ServiceError};
use gdl_store::ItemId;
use gdl_store::folders::{Direction, Pair, PairKind, PairState, StopReason};
use tokio_util::sync::CancellationToken;

use super::{Daemon, failed};
use crate::dbus::Service;

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

/// O que o serviço guarda de cada conta: um laço por par.
#[derive(Default)]
pub(super) struct Runners(Mutex<RunnerMap>);

type RunnerMap = HashMap<i64, (Arc<FolderSync>, CancellationToken)>;

impl Runners {
    fn lock(&self) -> std::sync::MutexGuard<'_, RunnerMap> {
        self.0.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn stop(&self, id: i64) {
        if let Some((_, stop)) = self.lock().remove(&id) {
            stop.cancel();
        }
    }

    fn get(&self, id: i64) -> Option<Arc<FolderSync>> {
        self.lock().get(&id).map(|(sync, _)| sync.clone())
    }
}

impl Daemon {
    /// Sobe os pares guardados da conta (na partida dela); param com `shutdown`.
    pub(super) fn start_folders(
        self: &Arc<Self>,
        engine: &Arc<Engine>,
        runners: &Arc<Runners>,
        shutdown: &CancellationToken,
    ) {
        match engine.store().pairs() {
            Ok(pairs) => {
                for pair in pairs.into_iter().filter(|p| p.kind == PairKind::Folder) {
                    self.start_folder(engine, runners, shutdown, pair);
                }
            }
            Err(e) => tracing::warn!(erro = %e, "falha ao ler as pastas do computador"),
        }
    }

    fn start_folder(
        self: &Arc<Self>,
        engine: &Arc<Engine>,
        runners: &Arc<Runners>,
        shutdown: &CancellationToken,
        pair: Pair,
    ) {
        let id = pair.id;
        let sync = Arc::new(FolderSync::new(
            engine.store().clone(),
            engine.client().clone(),
            pair,
            Trash::new(self.config.paths.data_home.join("Trash")),
            self.i18n.tr("conflict-word", &[]),
        ));
        let stop = shutdown.child_token();
        tokio::spawn({
            let (sync, stop, events) = (sync.clone(), stop.clone(), engine.subscribe());
            let timing = self.config.folder_timing;
            async move { sync.run(events, timing, stop).await }
        });
        // O estado do par (parado, esperando confirmação) chega à interface pelo sinal.
        let daemon = Arc::downgrade(self);
        let (mut changes, until) = (sync.state_changes(), stop.clone());
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    () = until.cancelled() => break,
                    changed = changes.changed() => if changed.is_err() { break },
                }
                let Some(daemon) = daemon.upgrade() else {
                    break;
                };
                daemon.emit_local_folders_changed().await;
            }
        });
        runners.lock().insert(id, (sync, stop));
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
        if self.prefs().disable_mirrored_folders {
            return Err(ServiceError::Refused("folders-disabled".into()));
        }
        let (engine, runners, shutdown) = self.folder_account(account).await?;
        let local = self.validated(path, None).await?;
        let remote = match self.existing_folder(&engine, &local).await? {
            Some(id) if merge => id,
            _ => {
                let computer = self.computer_folder(&engine).await?;
                let name = free_name(&engine, &computer, &file_name(&local))?;
                let made = engine
                    .client()
                    .create_folder(&computer, &name, None)
                    .await
                    .map_err(failed)?;
                remember(&engine, made)?
            }
        };
        let id = engine
            .store()
            .add_pair(&local, &ItemId(remote), PairKind::Folder, Direction::TwoWay)
            .map_err(failed)?;
        self.restart_folder(&engine, &runners, &shutdown, id)
            .await?;
        Ok(id)
    }

    pub async fn remove_local_folder(&self, account: &str, id: i64) -> Result<(), ServiceError> {
        let (engine, runners, _) = self.folder_account(account).await?;
        runners.stop(id);
        engine.store().remove_pair(id).map_err(failed)?;
        self.emit_local_folders_changed().await;
        Ok(())
    }

    pub async fn relocate_local_folder(
        self: &Arc<Self>,
        account: &str,
        id: i64,
        path: &str,
    ) -> Result<(), ServiceError> {
        let (engine, runners, shutdown) = self.folder_account(account).await?;
        let local = self.validated(path, Some(id)).await?;
        let store = engine.store();
        runners.stop(id);
        store.set_pair_local(id, &local).map_err(failed)?;
        store
            .set_pair_state(id, PairState::Active)
            .map_err(failed)?;
        self.restart_folder(&engine, &runners, &shutdown, id).await
    }

    /// Sobe (de novo) o laço do par `id` como está no banco e avisa a interface.
    async fn restart_folder(
        self: &Arc<Self>,
        engine: &Arc<Engine>,
        runners: &Arc<Runners>,
        shutdown: &CancellationToken,
        id: i64,
    ) -> Result<(), ServiceError> {
        let pair = engine.store().pair(id).map_err(failed)?;
        let pair = pair.ok_or_else(|| ServiceError::NotFound(id.to_string()))?;
        self.start_folder(engine, runners, shutdown, pair);
        self.emit_local_folders_changed().await;
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
        let (_, runners, _) = self.folder_account(account).await?;
        let sync = runners
            .get(id)
            .ok_or_else(|| ServiceError::NotFound(id.to_string()))?;
        sync.confirm_deletes().map_err(failed)
    }

    async fn emit_local_folders_changed(&self) {
        if let Some(emitter) = self.emitter() {
            let _ = Service::local_folders_changed(&emitter).await;
        }
    }

    /// A pasta serve? Confere contra as pastas de todas as contas (menos `except`, que está
    /// mudando de lugar), as montagens e os dados do app.
    /// Devolve também o caminho canônico.
    async fn check(
        &self,
        path: &Path,
        except: Option<i64>,
    ) -> Result<(PathBuf, guard::FolderInfo), Refusal> {
        let mut taken = vec![self.config.paths.data_dir.clone()];
        for (_, engine, mount) in self.folder_accounts().await {
            taken.push(mount);
            let pairs = engine.store().pairs().unwrap_or_default();
            taken.extend(
                pairs
                    .into_iter()
                    .filter(|p| Some(p.id) != except)
                    .map(|p| p.local),
            );
        }
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
        }
        Ok(found)
    }

    /// "Computadores/<este computador>" no Drive, criando o que faltar.
    async fn computer_folder(&self, engine: &Arc<Engine>) -> Result<String, ServiceError> {
        if let Some(id) = self.find_computer_folder(engine).await? {
            return Ok(id);
        }
        let client = engine.client();
        let computers = match find_support(engine, ROLE.0, ROLE.1).await? {
            Some(id) => id,
            None => {
                let root = client.get("root").await.map_err(failed)?;
                let name = self.i18n.tr("computers-folder", &[]);
                let made = client
                    .create_folder_with_props(&root.id, &name, &[ROLE])
                    .await;
                remember(engine, made.map_err(failed)?)?
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

    async fn folder_account(
        &self,
        account: &str,
    ) -> Result<(Arc<Engine>, Arc<Runners>, CancellationToken), ServiceError> {
        self.with_account(account, |a| {
            (a.engine.clone(), a.folders.clone(), a.shutdown.clone())
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

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn refusal_text(refusal: Refusal) -> &'static str {
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
