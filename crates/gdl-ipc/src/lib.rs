//! Contrato D-Bus do serviço: nomes, tipos, erros e o proxy usado pela CLI, pela bandeja e pela
//! interface. O objeto do serviço (`gdl-daemon`) implementa os mesmos métodos.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub const BUS_NAME: &str = "io.github.junior9283.GDriveDesktopLinux1";
pub const OBJECT_PATH: &str = "/io/github/junior9283/GDriveDesktopLinux1";

/// Resposta de `Status`; sem conta conectada, tudo vazio.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Status {
    pub account: String,
    pub mount_point: String,
    /// `signed-out`, `synced`, `syncing`, `paused`, `offline`, `quota-full` ou `error`.
    pub state: String,
    /// Itens subindo ou descendo agora (envios na fila e fixados por baixar).
    pub syncing: u32,
    pub pending_uploads: u32,
    pub paused: bool,
    pub offline: bool,
    /// Milissegundos desde 1970; 0 se nunca sincronizou.
    pub last_sync_ms: i64,
    pub errors: u32,
    /// Tempo estimado para terminar os envios; -1 sem estimativa.
    pub remaining_secs: i64,
}

/// Uma linha de `Activity`: o último estado de um item que subiu, desceu ou foi para a lixeira.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ActivityItem {
    /// Ordem do registro (maior = mais recente).
    pub seq: u64,
    pub item_id: String,
    pub name: String,
    /// Caminho completo na montagem.
    pub path: String,
    pub size: i64,
    /// `upload-queued`, `uploading`, `uploaded`, `upload-canceling`, `upload-canceled`,
    /// `upload-failed`, `downloading`, `downloaded`, `trashing`, `trashed`, `restoring` ou
    /// `restored`.
    pub state: String,
    /// Andamento do envio; -1 quando não se sabe.
    pub percent: i32,
    pub time_ms: i64,
}

/// Resposta de `AccountInfo`: a conta conectada, para o menu da conta.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AccountInfo {
    pub email: String,
    pub name: String,
    /// A foto da conta em PNG (ou no formato que o Google mandar); vazia se não houver.
    pub photo_png: Vec<u8>,
    pub quota_used: i64,
    /// -1: armazenamento ilimitado.
    pub quota_total: i64,
}

#[derive(Debug, zbus::DBusError)]
#[zbus(prefix = "io.github.junior9283.GDriveDesktopLinux1.Error")]
pub enum ServiceError {
    #[zbus(error)]
    ZBus(zbus::Error),
    /// Há envios pendentes e `Logout` veio sem dizer o que fazer com eles.
    PendingUploads(String),
    NotLoggedIn(String),
    NotFound(String),
    InvalidArgs(String),
    Failed(String),
}

impl ServiceError {
    /// O nome do erro sem o prefixo (`PendingUploads`), para quem escolhe a mensagem; `ZBus` para
    /// erros do próprio D-Bus (serviço fora do ar, por exemplo). Escrito à mão porque o
    /// `name()` do zbus dá, para `ZBus`, o nome do erro de transporte, não `ZBus`.
    pub fn short_name(&self) -> &'static str {
        match self {
            ServiceError::ZBus(_) => "ZBus",
            ServiceError::PendingUploads(_) => "PendingUploads",
            ServiceError::NotLoggedIn(_) => "NotLoggedIn",
            ServiceError::NotFound(_) => "NotFound",
            ServiceError::InvalidArgs(_) => "InvalidArgs",
            ServiceError::Failed(_) => "Failed",
        }
    }
}

/// Os mesmos métodos e sinais do objeto do serviço (`gdl_daemon::dbus::Service`): mudou aqui, mude
/// lá.
#[zbus::proxy(
    interface = "io.github.junior9283.GDriveDesktopLinux1.Service",
    default_service = "io.github.junior9283.GDriveDesktopLinux1",
    default_path = "/io/github/junior9283/GDriveDesktopLinux1",
    gen_blocking = false
)]
pub trait Service {
    /// Abre o login no navegador e devolve o endereço; o resultado vem em `LoginFinished`.
    fn begin_login(&self, client_id: &str, client_secret: &str) -> Result<String, ServiceError>;
    fn status(&self) -> Result<Status, ServiceError>;
    fn account_info(&self) -> Result<AccountInfo, ServiceError>;
    /// A atividade de sincronização, a mais recente primeiro (até 500 itens).
    fn activity(&self) -> Result<Vec<ActivityItem>, ServiceError>;
    fn pin(&self, path: &str) -> Result<(), ServiceError>;
    fn unpin(&self, path: &str) -> Result<(), ServiceError>;
    fn pause(&self) -> Result<(), ServiceError>;
    fn resume(&self) -> Result<(), ServiceError>;
    fn list_lost_and_found(&self) -> Result<Vec<String>, ServiceError>;
    /// `mode`: vazio, `finish`, `move` (para a pasta `dir`) ou `discard`.
    fn logout(&self, mode: &str, dir: &str) -> Result<(), ServiceError>;

    #[zbus(signal)]
    fn login_finished(&self, ok: bool, email: &str, error: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn status_changed(&self) -> zbus::Result<()>;
    /// A atividade mudou (no máximo um a cada 250 ms).
    #[zbus(signal)]
    fn activity_changed(&self) -> zbus::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_name_is_the_last_part_of_the_dbus_error_name() {
        let e = ServiceError::PendingUploads("3 arquivos".into());
        assert_eq!(e.short_name(), "PendingUploads");
        let e = ServiceError::ZBus(zbus::Error::Failure("x".into()));
        assert_eq!(e.short_name(), "ZBus");
    }
}
