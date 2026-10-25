//! Interface D-Bus do serviço, usada pela CLI, pela bandeja e pelo Dolphin.

use std::sync::Arc;

use gdl_drive::token_store::ClientCredentials;
use gdl_ipc::{AccountInfo, ActivityItem, ServiceError, Status};
use zbus::object_server::SignalEmitter;

use crate::daemon::Daemon;

/// Objeto publicado em `OBJECT_PATH`.
pub struct Service {
    daemon: Arc<Daemon>,
}

impl Service {
    pub fn new(daemon: Arc<Daemon>) -> Service {
        Service { daemon }
    }
}

/// Os mesmos métodos e sinais do proxy em `gdl_ipc::Service`: mudou aqui, mude lá (os testes do
/// serviço chamam tudo pelo proxy, e uma divergência falha neles).
#[zbus::interface(name = "io.github.junior9283.GDriveDesktopLinux1.Service")]
impl Service {
    /// Abre o login no navegador e devolve o endereço; o resultado vem em `LoginFinished`.
    async fn begin_login(
        &self,
        client_id: &str,
        client_secret: &str,
    ) -> Result<String, ServiceError> {
        let creds = ClientCredentials {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
        };
        self.daemon.begin_login(creds).await
    }

    async fn status(&self) -> Result<Status, ServiceError> {
        self.daemon.status().await
    }

    async fn account_info(&self) -> Result<AccountInfo, ServiceError> {
        self.daemon.account_info().await
    }

    async fn activity(&self) -> Result<Vec<ActivityItem>, ServiceError> {
        self.daemon.activity().await
    }

    async fn pin(&self, path: &str) -> Result<(), ServiceError> {
        self.daemon.set_pinned(path, true).await
    }

    async fn unpin(&self, path: &str) -> Result<(), ServiceError> {
        self.daemon.set_pinned(path, false).await
    }

    async fn pause(&self) -> Result<(), ServiceError> {
        self.daemon.set_paused(true).await
    }

    async fn resume(&self) -> Result<(), ServiceError> {
        self.daemon.set_paused(false).await
    }

    async fn list_lost_and_found(&self) -> Result<Vec<String>, ServiceError> {
        self.daemon.lost_and_found().await
    }

    /// `mode`: vazio, `finish`, `move` (para a pasta `dir`) ou `discard`.
    async fn logout(&self, mode: &str, dir: &str) -> Result<(), ServiceError> {
        self.daemon.logout(mode, dir).await
    }

    #[zbus(signal)]
    pub async fn login_finished(
        emitter: &SignalEmitter<'_>,
        ok: bool,
        email: &str,
        error: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn status_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn activity_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
