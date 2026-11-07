//! A consulta da última release (`crate::updates`), a oferta de atualização e a atualização em si
//! (`crate::upgrade`). O que já foi oferecido fica em `<state_dir>/atualizacao.json`: a mesma
//! versão volta a ser oferecida só depois de `UpdateCheck::every`, até ser instalada; e a versão
//! que está sendo instalada, para o serviço novo dizer, ao partir, se a troca deu certo.

use std::path::PathBuf;
use std::sync::Arc;

use gdl_common::fs::{remove_dir_if_exists, write_atomic};
use gdl_common::links::release_notes;
use gdl_common::paths::create_private_dir;
use gdl_ipc::update_step;
use serde::{Deserialize, Serialize};

use super::{Daemon, Exit, ServiceError};
use crate::notify;
use crate::updates::{self, installed, text, version};
use crate::upgrade::install::InstallOutcome;
use crate::upgrade::{self, Offer, ReleaseFiles, UpgradeError};

/// Onde a atualização está.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum UpdateStep {
    /// Parada: há (ou não) uma versão oferecida.
    #[default]
    Available,
    Downloading,
    /// O `pkexec` está pedindo a senha (os pacotes do sistema).
    WaitingPassword,
    /// O `install.sh` do `.tar.gz` está rodando, ou o serviço vai trocar de programa.
    Installing,
}

impl UpdateStep {
    /// O nome no D-Bus.
    pub fn as_str(self) -> &'static str {
        match self {
            UpdateStep::Available => update_step::AVAILABLE,
            UpdateStep::Downloading => update_step::DOWNLOADING,
            UpdateStep::WaitingPassword => update_step::WAITING_PASSWORD,
            UpdateStep::Installing => update_step::INSTALLING,
        }
    }
}

/// A versão oferecida (desde a última consulta) e o passo da atualização.
#[derive(Debug, Default)]
pub(crate) struct UpdateState {
    offer: Option<Offer>,
    step: UpdateStep,
}

/// `<state_dir>/atualizacao.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct Saved {
    /// A última versão oferecida (`X.Y.Z`) e quando (ms desde 1970).
    #[serde(default)]
    version: String,
    #[serde(default)]
    offered_ms: i64,
    /// A versão pedida ao instalador, até o serviço novo partir.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    installing: Option<String>,
}

pub(crate) const STATE_FILE: &str = "atualizacao.json";
/// O arquivo das versões até a 1.3: só a versão avisada.
const OLD_STATE_FILE: &str = "versao-avisada";
/// A pasta do download em `<cache_dir>`, apagada depois de cada tentativa.
pub(crate) const DOWNLOAD_DIR: &str = "atualizacao";

impl Daemon {
    /// Consulta depois de `UpdateCheck::first` e a cada `every`, enquanto o serviço existir.
    pub fn watch_releases(self: &Arc<Self>) {
        let (first, every) = (self.config.updates.first, self.config.updates.every);
        let daemon = Arc::downgrade(self);
        tokio::spawn(async move {
            tokio::time::sleep(first).await;
            while let Some(daemon) = daemon.upgrade() {
                daemon.check_release().await;
                // Sem segurar o serviço enquanto dorme: quando ele sai, o laço acaba.
                drop(daemon);
                tokio::time::sleep(every).await;
            }
        });
    }

    /// Uma consulta: versão maior que a instalada vira a oferta, avisada se a mesma versão não
    /// foi avisada há menos de `every`. Sem rede, sem release ou com resposta estranha, nada
    /// acontece (só o log detalhado registra).
    pub async fn check_release(&self) {
        let release = match updates::latest(&self.config.updates.url).await {
            Ok(Some(release)) => release,
            Ok(None) => return,
            Err(e) => {
                tracing::debug!(erro = %e, "não deu para consultar a última versão");
                return;
            }
        };
        if release.version <= installed() {
            return;
        }
        let mut offer = upgrade::offer(release, &self.config.install_kind);
        // Sem a chave do projeto, nenhuma assinatura confere: só "Ver novidades".
        if self.config.updates.public_key.is_empty() {
            offer.files = None;
        }
        let (latest, can_install) = (offer.version, offer.files.is_some());
        self.set_offer(offer).await;
        let mut saved = self.saved_update();
        let now = gdl_common::time::now_ms();
        let every = i64::try_from(self.config.updates.every.as_millis()).unwrap_or(i64::MAX);
        if version(&saved.version) == Some(latest) && now - saved.offered_ms < every {
            return;
        }
        saved.version = text(latest);
        saved.offered_ms = now;
        // Grava antes de avisar: uma falha no aviso não faz avisar de novo na hora.
        self.save_update(&saved);
        let note = notify::new_version(&saved.version, can_install, &self.i18n);
        self.notify(&note).await;
    }

    /// A oferta atual e o passo da atualização.
    pub fn update_state(&self) -> (Option<Offer>, UpdateStep) {
        let state = self.update.lock().unwrap();
        (state.offer.clone(), state.step)
    }

    /// "Atualizar agora": reserva a atualização e a roda em segundo plano (o andamento sai em
    /// `UpdateChanged` e nos avisos). Sem versão oferecida, `NotFound`; já atualizando, `Refused`.
    pub fn start_update(self: &Arc<Self>) -> Result<(), ServiceError> {
        let offer = self.reserve_update()?;
        let daemon = self.clone();
        tokio::spawn(async move { daemon.run_update(offer).await });
        Ok(())
    }

    /// `start_update` esperando o fim.
    #[cfg(test)]
    pub async fn update_now(self: &Arc<Self>) -> Result<(), ServiceError> {
        let offer = self.reserve_update()?;
        self.run_update(offer).await;
        Ok(())
    }

    /// A versão oferecida; com os arquivos para instalar, já marca `Downloading`, e outra
    /// reserva no meio é recusada.
    fn reserve_update(&self) -> Result<Offer, ServiceError> {
        let mut state = self.update.lock().unwrap();
        let offer = state
            .offer
            .clone()
            .ok_or_else(|| ServiceError::NotFound("versão nova".into()))?;
        if offer.files.is_some() {
            if state.step != UpdateStep::Available {
                return Err(ServiceError::Refused("updating".into()));
            }
            state.step = UpdateStep::Downloading;
        }
        Ok(offer)
    }

    /// Baixa, confere e instala a versão reservada; com a instalação feita, pede para trocar de
    /// programa (`Exit::Upgrade`). Sem os arquivos para instalar sozinho, abre a página da
    /// release.
    async fn run_update(self: &Arc<Self>, offer: Offer) {
        let target = text(offer.version);
        let Some(files) = &offer.files else {
            self.open(release_notes(&target));
            return;
        };
        self.update_changed(UpdateStep::Downloading, &target).await;
        let dir = self.config.paths.cache_dir.join(DOWNLOAD_DIR);
        let result = self.download_and_install(files, &dir, &target).await;
        let removed = tokio::task::spawn_blocking(move || remove_dir_if_exists(&dir)).await;
        if let Ok(Err(e)) = removed {
            tracing::warn!(erro = %e, "não deu para apagar a atualização baixada");
        }
        let note = match result {
            Ok(InstallOutcome::Done) => {
                tracing::info!(versao = %target, "atualização instalada; trocando de programa");
                self.exit.send_replace(Some(Exit::Upgrade));
                return;
            }
            // A oferta volta, no lugar do aviso de progresso.
            Ok(InstallOutcome::Canceled) => notify::new_version(&target, true, &self.i18n),
            Err(e) => {
                tracing::warn!(erro = ?e, versao = %target, "a atualização falhou");
                notify::update_failed(&e, &self.i18n)
            }
        };
        self.set_installing(None);
        self.set_step(UpdateStep::Available).await;
        self.notify(&note).await;
    }

    async fn download_and_install(
        &self,
        files: &ReleaseFiles,
        dir: &std::path::Path,
        target: &str,
    ) -> Result<InstallOutcome, UpgradeError> {
        let key = &self.config.updates.public_key;
        let package = upgrade::verify::download(files, dir, key).await?;
        let kind = &self.config.install_kind;
        let step = if kind.needs_root() {
            UpdateStep::WaitingPassword
        } else {
            UpdateStep::Installing
        };
        self.update_changed(step, target).await;
        // Gravado antes: se a instalação trocar o programa no meio, o serviço novo sabe.
        self.set_installing(Some(target.into()));
        self.config
            .installer
            .install(kind, &package.path, &package.sha256)
            .await
    }

    /// Ao partir: se a partida anterior pediu uma versão ao instalador, avisa se ela pegou.
    pub async fn announce_upgrade(&self) {
        let mut saved = self.saved_update();
        let Some(target) = saved.installing.take() else {
            return;
        };
        self.save_update(&saved);
        let note = if version(&target) == Some(installed()) {
            notify::updated(&target, &self.i18n)
        } else {
            tracing::warn!(versao = %target, "a versão instalada não é a pedida");
            notify::update_not_applied(&self.i18n)
        };
        self.notify(&note).await;
    }

    /// "Ver novidades": a página da última versão oferecida.
    pub(crate) fn open_release(&self) -> Result<(), ServiceError> {
        let saved = self.saved_update();
        let offered = version(&saved.version)
            .ok_or_else(|| ServiceError::NotFound("versão avisada".into()))?;
        self.open(release_notes(&text(offered)));
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn requested_exit(&self) -> Option<Exit> {
        *self.exit.borrow()
    }

    async fn set_offer(&self, offer: Offer) {
        let changed = {
            let mut state = self.update.lock().unwrap();
            let changed = state.offer.as_ref() != Some(&offer);
            state.offer = Some(offer);
            changed
        };
        if changed {
            self.emit_update_changed().await;
        }
    }

    async fn set_step(&self, step: UpdateStep) {
        self.update.lock().unwrap().step = step;
        self.emit_update_changed().await;
    }

    /// Um passo novo: avisa quem mostra e troca o aviso pelo do passo.
    async fn update_changed(&self, step: UpdateStep, target: &str) {
        self.set_step(step).await;
        self.notify(&notify::update_progress(step, target, &self.i18n))
            .await;
    }

    fn state_file(&self) -> PathBuf {
        self.config.paths.state_dir.join(STATE_FILE)
    }

    /// O estado gravado; sem ele, a versão avisada pelas versões antigas (sem a hora).
    fn saved_update(&self) -> Saved {
        if let Ok(body) = std::fs::read(self.state_file()) {
            return serde_json::from_slice(&body).unwrap_or_default();
        }
        let old = self.config.paths.state_dir.join(OLD_STATE_FILE);
        let version = std::fs::read_to_string(old).unwrap_or_default();
        Saved {
            version: version.trim().into(),
            ..Saved::default()
        }
    }

    fn save_update(&self, saved: &Saved) {
        let state_dir = &self.config.paths.state_dir;
        let body = serde_json::to_vec(saved).expect("JSON de campos simples");
        let written = create_private_dir(state_dir)
            .and_then(|()| write_atomic(&self.state_file(), &body))
            .and_then(|()| gdl_common::fs::remove_file_if_exists(&state_dir.join(OLD_STATE_FILE)));
        if let Err(e) = written {
            tracing::warn!(erro = %e, "não deu para guardar o estado da atualização");
        }
    }

    fn set_installing(&self, target: Option<String>) {
        let mut saved = self.saved_update();
        if saved.installing != target {
            saved.installing = target;
            self.save_update(&saved);
        }
    }
}
