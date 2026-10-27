//! Backup de dispositivos removíveis: o serviço observa o UDisks2, pergunta (ou faz sozinho, se
//! lembrado) e envia o dispositivo uma vez para "Computadores/<este computador>/<rótulo>".

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use futures_util::StreamExt;
use gdl_folders::sync::FolderSync;
use gdl_ipc::{DeviceItem, ServiceError};
use gdl_store::ItemId;
use gdl_store::folders::{Direction, Pair, PairKind, PairState};
use zbus::Connection;
use zbus::fdo::ObjectManagerProxy;

use super::{Daemon, failed};
use crate::devices::{
    Choice, Choices, DRIVE, Device, FILESYSTEM, UDISKS_PATH, UDISKS_SERVICE, mounted_removables,
};
use crate::notify;

/// Depois de um dispositivo entrar, o sistema o monta um pouco depois: olha de novo.
const MOUNT_RECHECKS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(10),
];

/// Os dispositivos conectados, as escolhas guardadas e os backups em andamento.
pub(super) struct Devices(Mutex<DeviceState>);

struct DeviceState {
    present: HashMap<String, Device>,
    choices: Choices,
    busy: HashSet<String>,
}

impl Devices {
    pub(super) fn load(path: std::path::PathBuf) -> Devices {
        Devices(Mutex::new(DeviceState {
            present: HashMap::new(),
            choices: Choices::load(path),
            busy: HashSet::new(),
        }))
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, DeviceState> {
        crate::lock(&self.0)
    }
}

/// A interface é de unidade ou de sistema de arquivos (as que mudam a lista de dispositivos).
fn relevant<'a>(mut interfaces: impl Iterator<Item = &'a str>) -> bool {
    interfaces.any(|i| [DRIVE, FILESYSTEM].contains(&i))
}

/// Onde o banco da conta guarda o par de cada dispositivo (o mesmo backup a cada conexão).
fn pair_key(id: &str) -> String {
    format!("devices.{id}")
}

impl Daemon {
    /// Observa o UDisks2 de `conn` (o barramento do sistema). Os que já estavam conectados na
    /// partida não perguntam: só os que entram depois.
    pub fn watch_devices(self: &Arc<Self>, conn: Connection) {
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            let watch = async {
                let proxy = ObjectManagerProxy::builder(&conn)
                    .destination(UDISKS_SERVICE)?
                    .path(UDISKS_PATH)?
                    .build()
                    .await?;
                // Só sinais de unidades e sistemas de arquivos (não de tarefas, loops…).
                let added = proxy
                    .receive_interfaces_added()
                    .await?
                    .filter_map(|s| async move {
                        let args = s.args().ok()?;
                        relevant(args.interfaces_and_properties.keys().map(|i| i.as_str()))
                            .then_some(true)
                    });
                let removed =
                    proxy
                        .receive_interfaces_removed()
                        .await?
                        .filter_map(|s| async move {
                            let args = s.args().ok()?;
                            relevant(args.interfaces.iter().map(|i| i.as_str())).then_some(false)
                        });
                let mut signals = std::pin::pin!(futures_util::stream::select(added, removed));
                let initial = proxy.get_managed_objects().await?;
                if let Some(daemon) = daemon.upgrade() {
                    daemon
                        .devices_found(mounted_removables(&initial), true)
                        .await;
                }
                while let Some(added) = signals.next().await {
                    let Some(daemon) = daemon.upgrade() else {
                        break;
                    };
                    daemon.rescan_devices(&proxy).await;
                    if added {
                        let (daemon, proxy) = (Arc::downgrade(&daemon), proxy.clone());
                        tokio::spawn(async move {
                            for wait in MOUNT_RECHECKS {
                                tokio::time::sleep(wait).await;
                                let Some(daemon) = daemon.upgrade() else {
                                    break;
                                };
                                daemon.rescan_devices(&proxy).await;
                            }
                        });
                    }
                }
                zbus::Result::Ok(())
            };
            if let Err(e) = watch.await {
                tracing::info!(erro = %e, "sem UDisks2: os dispositivos não são observados");
            }
        });
    }

    async fn rescan_devices(self: &Arc<Self>, proxy: &ObjectManagerProxy<'_>) {
        match proxy.get_managed_objects().await {
            Ok(objects) => {
                self.devices_found(mounted_removables(&objects), false)
                    .await
            }
            Err(e) => tracing::warn!(erro = %e, "falha ao listar os dispositivos"),
        }
    }

    /// A lista nova de conectados: os que entraram fazem backup (lembrados) ou perguntam.
    async fn devices_found(self: &Arc<Self>, found: Vec<Device>, initial: bool) {
        let entered: Vec<(Device, Option<Choice>)> = {
            let mut state = self.devices.lock();
            let present: HashMap<String, Device> =
                found.into_iter().map(|d| (d.id.clone(), d)).collect();
            if present == state.present {
                return;
            }
            let entered = present
                .values()
                .filter(|d| state.present.get(&d.id) != Some(*d))
                .map(|d| (d.clone(), state.choices.choice(&d.id)))
                .collect();
            state.present = present;
            entered
        };
        self.emit_local_folders_changed().await;
        let prefs = self.prefs();
        if initial || prefs.disable_external_media_sync {
            return;
        }
        for (device, choice) in entered {
            match choice {
                // Criar a pasta no Drive não segura os sinais nem os outros dispositivos.
                Some(Choice::Backup) => {
                    let daemon = self.clone();
                    tokio::spawn(async move {
                        if let Err(e) = daemon.start_backup(device).await {
                            tracing::warn!(erro = %e, "backup do dispositivo não começou");
                        }
                    });
                }
                None if prefs.prompt_to_backup_devices => {
                    self.notify(&notify::device_ask(&device.id, &device.label, &self.i18n))
                        .await;
                }
                _ => {}
            }
        }
    }

    pub fn devices(&self) -> Vec<DeviceItem> {
        let state = self.devices.lock();
        // Os conectados, e os desconectados com escolha guardada (sem caminho).
        let present = state
            .present
            .values()
            .map(|d| (&d.id, &d.label, d.mount.display().to_string()));
        let absent = state
            .choices
            .labels()
            .filter(|(id, _)| !state.present.contains_key(*id));
        let mut items: Vec<DeviceItem> = present
            .chain(absent.map(|(id, label)| (id, label, String::new())))
            .map(|(id, label, path)| DeviceItem {
                id: id.clone(),
                label: label.clone(),
                path,
                choice: state.choices.choice(id).map_or("", Choice::as_str).into(),
                busy: state.busy.contains(id),
            })
            .collect();
        items.sort_by(|a, b| (&a.label, &a.id).cmp(&(&b.label, &b.id)));
        items
    }

    pub async fn backup_device(
        self: &Arc<Self>,
        id: &str,
        remember: bool,
    ) -> Result<(), ServiceError> {
        self.devices_refused()?;
        let device = self.present_device(id)?;
        if remember {
            self.choose_device(&device.id, Choice::Backup, &device.label)?;
        }
        self.start_backup(device).await
    }

    pub async fn ignore_device(&self, id: &str) -> Result<(), ServiceError> {
        let device = self.present_device(id)?;
        self.choose_device(&device.id, Choice::Ignored, &device.label)?;
        self.emit_local_folders_changed().await;
        Ok(())
    }

    pub async fn forget_device(&self, id: &str) -> Result<(), ServiceError> {
        let forgot = self.devices.lock().choices.forget(id).map_err(failed)?;
        if !forgot {
            return Err(ServiceError::NotFound(id.into()));
        }
        self.emit_local_folders_changed().await;
        Ok(())
    }

    fn present_device(&self, id: &str) -> Result<Device, ServiceError> {
        let state = self.devices.lock();
        let device = state.present.get(id).cloned();
        device.ok_or_else(|| ServiceError::NotFound(id.into()))
    }

    fn choose_device(&self, id: &str, choice: Choice, label: &str) -> Result<(), ServiceError> {
        let mut state = self.devices.lock();
        state.choices.set(id, choice, label).map_err(failed)
    }

    /// A política do administrador desliga o backup de dispositivos.
    fn devices_refused(&self) -> Result<(), ServiceError> {
        if self.prefs().disable_external_media_sync {
            return Err(ServiceError::Refused("devices-disabled".into()));
        }
        Ok(())
    }

    /// Uma comparação só de envio, em segundo plano; no fim, o aviso de concluído ou de erro.
    async fn start_backup(self: &Arc<Self>, device: Device) -> Result<(), ServiceError> {
        if !self.devices.lock().busy.insert(device.id.clone()) {
            return Ok(());
        }
        let found = self.device_pair(&device).await;
        let (engine, pair) = match found {
            Ok(found) => found,
            Err(e) => {
                self.devices.lock().busy.remove(&device.id);
                return Err(e);
            }
        };
        self.emit_local_folders_changed().await;
        let sync = FolderSync::new(
            engine.store().clone(),
            engine.client().clone(),
            pair,
            self.system_trash(),
            self.i18n.tr("conflict-word", &[]),
        );
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            let error = match sync.reconcile().await {
                Ok(report) => report
                    .failed
                    .into_iter()
                    .next()
                    .map(|(path, e)| format!("{path}: {e}")),
                Err(e) => Some(e.to_string()),
            };
            let stopped = *sync.state_changes().borrow() != PairState::Active;
            let Some(daemon) = daemon.upgrade() else {
                return;
            };
            let error =
                error.or_else(|| stopped.then(|| daemon.i18n.tr("notify-device-unreadable", &[])));
            daemon.devices.lock().busy.remove(&device.id);
            let note =
                notify::device_done(&device.id, &device.label, error.as_deref(), &daemon.i18n);
            daemon.notify(&note).await;
            daemon.emit_local_folders_changed().await;
        });
        Ok(())
    }

    /// O par do dispositivo na primeira conta: o do backup anterior (no novo ponto de montagem)
    /// ou um novo, com a pasta criada no Drive.
    async fn device_pair(
        &self,
        device: &Device,
    ) -> Result<(Arc<gdl_engine::engine::Engine>, Pair), ServiceError> {
        let engine = self.account_engine("").await?;
        let store = engine.store();
        let key = pair_key(&device.id);
        let known = store.kv_get(&key).map_err(failed)?;
        let known: Option<i64> = known.and_then(|id| id.parse().ok());
        self.release_mount(store, &device.mount, known)?;
        let pair = match known.and_then(|id| store.pair(id).ok().flatten()) {
            Some(mut pair) => {
                if pair.local != device.mount {
                    store
                        .set_pair_local(pair.id, &device.mount)
                        .map_err(failed)?;
                    pair.local = device.mount.clone();
                }
                if pair.state != PairState::Active {
                    store
                        .set_pair_state(pair.id, PairState::Active)
                        .map_err(failed)?;
                    pair.state = PairState::Active;
                }
                pair
            }
            None => {
                let remote = self.new_computer_folder(&engine, &device.label).await?;
                let id = store
                    .add_pair(
                        &device.mount,
                        &ItemId(remote),
                        PairKind::Device,
                        Direction::UploadOnly,
                    )
                    .map_err(failed)?;
                store.kv_set(&key, &id.to_string()).map_err(failed)?;
                let pair = store.pair(id).map_err(failed)?;
                pair.ok_or_else(|| ServiceError::NotFound(id.to_string()))?
            }
        };
        Ok((engine, pair))
    }

    /// O backup de outro dispositivo que montava em `mount` (outro pendrive com o mesmo rótulo)
    /// solta o caminho, que no banco só pode ser de um par: guarda um lugar que não existe, até
    /// aquele dispositivo voltar.
    fn release_mount(
        &self,
        store: &gdl_store::Store,
        mount: &std::path::Path,
        keep: Option<i64>,
    ) -> Result<(), ServiceError> {
        let pairs = store.pairs().map_err(failed)?.into_iter();
        let mut others =
            pairs.filter(|p| p.kind == PairKind::Device && p.local == mount && Some(p.id) != keep);
        if let Some(other) = others.next() {
            let away = self
                .config
                .paths
                .data_dir
                .join("unplugged")
                .join(other.id.to_string());
            store.set_pair_local(other.id, &away).map_err(failed)?;
        }
        Ok(())
    }
}
