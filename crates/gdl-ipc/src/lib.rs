//! Contrato D-Bus do serviço: nomes, tipos, erros e o proxy usado pela CLI, pela bandeja e pela
//! interface. O objeto do serviço (`gdl-daemon`) implementa os mesmos métodos.

use serde::{Deserialize, Serialize};
use zbus::zvariant::Type;

pub mod launcher;
#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub const BUS_NAME: &str = "io.github.junior9283.GDriveDesktopLinux1";
pub const OBJECT_PATH: &str = "/io/github/junior9283/GDriveDesktopLinux1";
/// A janela (`gdrive-desktop-linux-ui`), que fica com este nome enquanto está aberta.
pub const UI_BUS_NAME: &str = "io.github.junior9283.GDriveDesktopLinux1.Ui";
pub const UI_OBJECT_PATH: &str = "/io/github/junior9283/GDriveDesktopLinux1/Ui";
/// A bandeja (`gdrive-desktop-linux-tray`), que fica com este nome enquanto está aberta.
pub const TRAY_BUS_NAME: &str = "io.github.junior9283.GDriveDesktopLinux1.Tray";

/// A janela: a bandeja e o atalho pedem uma tela por aqui.
#[zbus::proxy(
    interface = "io.github.junior9283.GDriveDesktopLinux1.Ui",
    default_service = "io.github.junior9283.GDriveDesktopLinux1.Ui",
    default_path = "/io/github/junior9283/GDriveDesktopLinux1/Ui",
    gen_blocking = false
)]
pub trait Ui {
    /// Mostra a janela na tela `screen` (`home`, `onboarding`, `search`, `offline`, `errors`,
    /// `preferences`, `about`).
    fn open(&self, screen: &str) -> zbus::Result<()>;
}

/// Fica com o nome `name` sem entrar na fila: `true` se ficou, `false` se outro processo já o tem
/// (instância única da janela e da bandeja).
pub async fn claim_name(conn: &zbus::Connection, name: &str) -> zbus::Result<bool> {
    use zbus::fdo::{RequestNameFlags, RequestNameReply};
    let name = zbus::names::WellKnownName::try_from(name)?;
    let reply = conn
        .request_name_with_flags(name, RequestNameFlags::DoNotQueue.into())
        .await;
    match reply {
        Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => Ok(true),
        Ok(RequestNameReply::Exists | RequestNameReply::InQueue) | Err(zbus::Error::NameTaken) => {
            Ok(false)
        }
        Err(e) => Err(e),
    }
}

/// Logo depois de instalar o pacote, o barramento da sessão (o dbus-broker só lê as pastas de
/// ativação ao iniciar) ainda não sabe abrir o serviço: pede que ele releia a configuração.
pub async fn ensure_activatable(conn: &zbus::Connection) -> zbus::Result<()> {
    let bus = zbus::fdo::DBusProxy::new(conn).await?;
    let names = bus.list_activatable_names().await?;
    if !names.iter().any(|name| name.as_str() == BUS_NAME) {
        bus.reload_config().await?;
    }
    Ok(())
}

/// Os avisos de que o nome do serviço ficou sem dono (o serviço saiu ou caiu), vindos do barramento.
pub async fn service_lost(
    conn: &zbus::Connection,
) -> zbus::Result<futures_util::stream::BoxStream<'static, ()>> {
    use futures_util::StreamExt;
    let bus = zbus::fdo::DBusProxy::new(conn).await?;
    let changes = bus
        .receive_name_owner_changed_with_args(&[(0, BUS_NAME)])
        .await?;
    Ok(changes
        .filter(|change| {
            futures_util::future::ready(change.args().is_ok_and(|a| a.new_owner().is_none()))
        })
        .map(|_| ())
        .boxed())
}

/// Ninguém atende do outro lado: o nome não tem dono (ou, ponto a ponto, não há o objeto).
pub fn nobody_there(e: &zbus::Error) -> bool {
    const NOBODY: [&str; 3] = [
        "org.freedesktop.DBus.Error.ServiceUnknown",
        "org.freedesktop.DBus.Error.NameHasNoOwner",
        "org.freedesktop.DBus.Error.UnknownObject",
    ];
    matches!(e, zbus::Error::MethodError(name, ..) if NOBODY.contains(&name.as_str()))
}

/// Uma conta na lista de `Status`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct AccountSummary {
    pub email: String,
    pub mount_point: String,
    /// Os mesmos valores de `Status::state`, só desta conta.
    pub state: String,
    /// Envios na fila desta conta (o que a desconexão precisa saber).
    pub pending_uploads: u32,
}

/// Resposta de `Status`; sem conta conectada, tudo vazio. Os campos são os da conta pedida
/// (vazio: a primeira), menos `state`, que com o pedido vazio é o pior entre as contas.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Status {
    pub account: String,
    pub mount_point: String,
    /// `signed-out`, `starting` (reconectando as contas guardadas), `synced`, `syncing`, `paused`,
    /// `offline`, `quota-full`, `error` ou `needs-sign-in`.
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
    /// Todas as contas conectadas, na ordem em que foram adicionadas.
    pub accounts: Vec<AccountSummary>,
    /// Cabe mais uma conta: menos de 4 e sem `DisableAddAccount` (esta só vale a partir da
    /// primeira conta).
    pub can_add_account: bool,
}

/// Um aviso do histórico (`Notifications`), o mais novo primeiro.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NotificationItem {
    pub id: u64,
    /// Família do aviso: `lost-and-found`, `offline`, `disk-low`, `quota-full`, `upload-failed`,
    /// `copied-as-shortcut`, `moved-to-shared-drive`, `folder-move-unsupported`, `mount-moved`,
    /// `resume-syncing` ou `upload-canceled`.
    pub kind: String,
    pub title: String,
    pub body: String,
    pub time_ms: i64,
    pub read: bool,
    /// Ação do botão (`open-lost-found`, `open-canceled`, `resume`); vazio sem botão.
    pub action_key: String,
    pub action_label: String,
    /// A conta que originou o aviso (e-mail); vazio nos avisos que não são de uma conta. A ação
    /// do botão age nela.
    #[serde(default)]
    pub account: String,
}

/// Uma família de aviso que o usuário pode desligar (`NotificationSettings`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NotificationSetting {
    pub kind: String,
    pub label: String,
    pub enabled: bool,
}

/// Uma linha de `Errors`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SyncErrorItem {
    /// Vazio nas cópias do Lost and Found.
    pub item_id: String,
    pub name: String,
    /// Caminho completo na montagem (a cópia, no caso do Lost and Found).
    pub path: String,
    /// `quota`, `upload-limit`, `no-permission`, `too-large`, `cloud-missing`, `local-missing`,
    /// `lost-and-found` ou `unknown`.
    pub category: String,
    /// O erro técnico; vazio no Lost and Found.
    pub message: String,
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

/// Resposta de `OfflineSummary`: o espaço dos arquivos off-line e do cache.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct OfflineSummary {
    pub pinned_bytes: u64,
    /// Conteúdo baixado no disco (off-line e cache comum).
    pub cache_bytes: u64,
    /// Espaço livre no disco do cache.
    pub free_bytes: u64,
    /// Itens fixados por conta própria.
    pub pinned_count: u32,
}

/// Uma linha de `OfflineItems`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct OfflineEntry {
    pub id: String,
    pub name: String,
    /// Caminho completo na montagem.
    pub path: String,
    pub is_folder: bool,
    pub size: u64,
    /// `on` (off-line) ou `mixed` (pasta com parte do conteúdo off-line).
    pub pin: String,
}

/// Um resultado de `Search` ou de `RecentItems`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SearchResult {
    pub id: String,
    pub name: String,
    /// Caminho completo na montagem.
    pub path: String,
    /// Tipo do Google (pastas, Documentos Google e atalhos); vazio nos demais arquivos.
    pub mime: String,
    pub is_folder: bool,
}

/// Um item de `SharedWithMe`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SharedItem {
    pub id: String,
    pub name: String,
    pub mime: String,
    /// Nome de quem compartilhou (o dono).
    pub owner: String,
    pub shared_ms: i64,
}

/// Uma pessoa com acesso a um item (`Sharing`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SharePerson {
    /// O id da permissão, para mudar o papel ou tirar o acesso.
    pub id: String,
    pub email: String,
    pub name: String,
    /// `owner`, `writer`, `commenter` ou `reader`.
    pub role: String,
}

/// Quem tem acesso a um item, para a janela "Compartilhar".
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Sharing {
    pub name: String,
    pub link: String,
    /// A conta pode mudar o acesso (só editores podem).
    pub can_share: bool,
    /// Acesso geral: `restricted`, `domain` ou `anyone`, com o papel (vazio em `restricted`).
    pub general: String,
    pub general_role: String,
    /// O domínio da conta, para "todos do domínio"; vazio em contas pessoais.
    pub domain: String,
    pub people: Vec<SharePerson>,
}

/// Uma versão do conteúdo de um arquivo (`Versions`), da mais nova para a mais antiga.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct VersionItem {
    pub id: String,
    pub modified_ms: i64,
    /// Tamanho em bytes; -1 quando o Drive não diz.
    pub size: i64,
    pub keep_forever: bool,
    pub author: String,
    /// A versão atual (a primeira da lista).
    pub current: bool,
}

/// As versões de um arquivo, para a janela "Gerenciar versões".
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Versions {
    pub name: String,
    pub can_edit: bool,
    pub versions: Vec<VersionItem>,
}

/// Um Drive compartilhado da conta (`SharedDrives`), para a tela de gerenciar.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SharedDriveItem {
    pub id: String,
    pub name: String,
    /// Ligado: indexado e visível na montagem.
    pub enabled: bool,
}

/// Uma pasta do computador sincronizada com o Drive (`LocalFolders`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct LocalFolder {
    /// O e-mail da conta.
    pub account: String,
    /// Identifica a pasta dentro da conta.
    pub id: i64,
    pub path: String,
    /// `active`; `stopped-missing`, `stopped-no-permission`, `stopped-moved`; ou
    /// `confirm-deletes` (as exclusões em `deletes` esperam `ConfirmFolderDeletes`).
    pub state: String,
    pub deletes: u64,
}

/// O que `CheckLocalFolder` acha de uma pasta antes de adicioná-la.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct FolderCheck {
    /// Vazio se a pasta serve; senão `missing`, `unreadable`, `nested`, `contains` ou `network`.
    pub refusal: String,
    pub size: u64,
    pub files: u64,
    /// O Drive já tem, neste computador, uma pasta com o mesmo nome (reinstalação): a interface
    /// pergunta se junta as duas.
    pub exists: bool,
}

/// Uma pasta de destino de "Adicionar atalho ao Drive" (`Folders`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct FolderItem {
    pub id: String,
    pub name: String,
    /// Tem subpastas (a tela mostra a seta para entrar).
    pub has_children: bool,
}

/// Um item do menu "Google Drive" do botão direito (`Menu`), na ordem de exibição.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct MenuEntry {
    /// A ação para `MenuAction`; `-` é um separador.
    pub id: String,
    pub label: String,
    /// `on`, `off` ou `mixed` nas escolhas de acesso off-line; vazio nos outros.
    pub state: String,
}

/// Uma linha de `Preferences`.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Pref {
    pub key: String,
    /// O valor efetivo em JSON (`null` sem valor nem padrão).
    pub value: String,
    /// Definida pela política do administrador: a janela mostra, mas não deixa mudar.
    pub managed: bool,
    /// Só vale depois de reiniciar o serviço.
    pub restart: bool,
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
    /// A preferência (nomeada) é definida pela política do administrador.
    Managed(String),
    /// O serviço recusou o pedido; a mensagem é um motivo estável (`too-many-accounts`,
    /// `add-account-disabled`), para a interface escolher o texto.
    Refused(String),
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
            ServiceError::Managed(_) => "Managed",
            ServiceError::Refused(_) => "Refused",
            ServiceError::Failed(_) => "Failed",
        }
    }

    /// A descrição que o serviço mandou (para `ZBus`, a do erro do D-Bus).
    pub fn into_message(self) -> String {
        match self {
            ServiceError::ZBus(e) => e.to_string(),
            ServiceError::PendingUploads(m)
            | ServiceError::NotLoggedIn(m)
            | ServiceError::NotFound(m)
            | ServiceError::InvalidArgs(m)
            | ServiceError::Managed(m)
            | ServiceError::Refused(m)
            | ServiceError::Failed(m) => m,
        }
    }
}

/// Os mesmos métodos e sinais do objeto do serviço (`gdl_daemon::dbus::Service`): mudou aqui, mude
/// lá. `account` é o e-mail da conta; vazio, a primeira.
#[zbus::proxy(
    interface = "io.github.junior9283.GDriveDesktopLinux1.Service",
    default_service = "io.github.junior9283.GDriveDesktopLinux1",
    default_path = "/io/github/junior9283/GDriveDesktopLinux1",
    gen_blocking = false
)]
pub trait Service {
    /// Abre o login no navegador e devolve o endereço; o resultado vem em `LoginFinished`.
    /// `expected`: o e-mail da conta que precisa entrar de novo; vazio, uma conta nova. Voltar
    /// com outro e-mail dá `LoginFinished(false, e-mail, "other-account")`.
    fn begin_login(
        &self,
        client_id: &str,
        client_secret: &str,
        expected: &str,
    ) -> Result<String, ServiceError>;
    fn status(&self, account: &str) -> Result<Status, ServiceError>;
    fn account_info(&self, account: &str) -> Result<AccountInfo, ServiceError>;
    /// A atividade de sincronização, a mais recente primeiro (até 500 itens).
    fn activity(&self, account: &str) -> Result<Vec<ActivityItem>, ServiceError>;
    fn errors(&self, account: &str) -> Result<Vec<SyncErrorItem>, ServiceError>;
    fn notifications(&self) -> Result<Vec<NotificationItem>, ServiceError>;
    fn mark_notifications_read(&self) -> Result<(), ServiceError>;
    fn dismiss_notification(&self, id: u64) -> Result<(), ServiceError>;
    fn invoke_notification_action(&self, id: u64, key: &str) -> Result<(), ServiceError>;
    fn notification_settings(&self) -> Result<Vec<NotificationSetting>, ServiceError>;
    fn set_notification_enabled(&self, kind: &str, enabled: bool) -> Result<(), ServiceError>;
    fn retry(&self, account: &str, item_id: &str) -> Result<(), ServiceError>;
    /// `canceled`, `too-late` ou `in-use` (o arquivo está aberto para escrita).
    fn cancel_upload(&self, account: &str, item_id: &str) -> Result<String, ServiceError>;
    fn pin(&self, path: &str) -> Result<(), ServiceError>;
    fn unpin(&self, path: &str) -> Result<(), ServiceError>;
    fn pause(&self) -> Result<(), ServiceError>;
    fn resume(&self) -> Result<(), ServiceError>;
    fn list_lost_and_found(&self, account: &str) -> Result<Vec<String>, ServiceError>;
    fn offline_summary(&self, account: &str) -> Result<OfflineSummary, ServiceError>;
    /// Os arquivos off-line: com `folder_id` vazio, os fixados mais altos; numa pasta, o que
    /// está off-line dentro dela.
    fn offline_items(
        &self,
        account: &str,
        folder_id: &str,
    ) -> Result<Vec<OfflineEntry>, ServiceError>;
    fn unpin_item(&self, account: &str, item_id: &str) -> Result<(), ServiceError>;
    /// Libera o cache (menos off-line e envios pendentes) e devolve os bytes liberados.
    fn clear_cache(&self, account: &str) -> Result<u64, ServiceError>;
    fn clear_offline_files(&self, account: &str) -> Result<(), ServiceError>;
    /// Nome (sem diferenciar maiúsculas e acentos) e, com internet, conteúdo; até 50.
    fn search(&self, account: &str, query: &str) -> Result<Vec<SearchResult>, ServiceError>;
    /// Os 10 arquivos abertos por último.
    fn recent_items(&self, account: &str) -> Result<Vec<SearchResult>, ServiceError>;
    /// `target`: `file` (abre o arquivo), `folder` (abre a pasta que o contém) ou `web`.
    fn open_item(&self, account: &str, item_id: &str, target: &str) -> Result<(), ServiceError>;
    fn web_link(&self, account: &str, item_id: &str) -> Result<String, ServiceError>;
    fn shared_with_me(&self, account: &str) -> Result<Vec<SharedItem>, ServiceError>;
    /// Cria um atalho para o item na pasta `folder_id` (vazio = a raiz do Meu Drive).
    fn add_shortcut(
        &self,
        account: &str,
        item_id: &str,
        folder_id: &str,
    ) -> Result<(), ServiceError>;
    /// As pastas onde um atalho pode ir: com `parent_id` vazio, o Meu Drive e os Drives
    /// compartilhados que aceitam itens novos; senão, as subpastas dele.
    fn folders(&self, account: &str, parent_id: &str) -> Result<Vec<FolderItem>, ServiceError>;
    /// Quem tem acesso ao item.
    fn sharing(&self, account: &str, item_id: &str) -> Result<Sharing, ServiceError>;
    /// Dá acesso às pessoas (`emails`) com o papel; `notify` manda o aviso com a mensagem.
    fn share_add(
        &self,
        account: &str,
        item_id: &str,
        emails: &[&str],
        role: &str,
        notify: bool,
        message: &str,
    ) -> Result<(), ServiceError>;
    /// Muda o papel de uma permissão; `role` vazio tira o acesso.
    fn share_set_role(
        &self,
        account: &str,
        item_id: &str,
        permission_id: &str,
        role: &str,
    ) -> Result<(), ServiceError>;
    /// Acesso geral: `restricted`, `domain` ou `anyone`, com o papel.
    fn share_set_general(
        &self,
        account: &str,
        item_id: &str,
        kind: &str,
        role: &str,
    ) -> Result<(), ServiceError>;
    /// Os Drives compartilhados da conta, por nome, com o estado de ligado de cada um.
    fn shared_drives(&self, account: &str) -> Result<Vec<SharedDriveItem>, ServiceError>;
    /// As pastas do computador sincronizadas, de todas as contas.
    fn local_folders(&self) -> Result<Vec<LocalFolder>, ServiceError>;
    fn check_local_folder(&self, account: &str, path: &str) -> Result<FolderCheck, ServiceError>;
    /// Liga a pasta a "Computadores/<este computador>/<nome>" no Drive; `merge` junta com a pasta
    /// de mesmo nome que já esteja lá. Recusa com `Refused` (`folders-disabled` ou o motivo de
    /// `CheckLocalFolder`). Devolve o ID da pasta.
    fn add_local_folder(&self, account: &str, path: &str, merge: bool)
    -> Result<i64, ServiceError>;
    /// Para de sincronizar (os arquivos ficam no computador e no Drive).
    fn remove_local_folder(&self, account: &str, id: i64) -> Result<(), ServiceError>;
    /// A pasta mudou de lugar no computador: o par continua do novo caminho.
    fn relocate_local_folder(&self, account: &str, id: i64, path: &str)
    -> Result<(), ServiceError>;
    /// Aplica as exclusões em massa que a pasta segurou.
    fn confirm_folder_deletes(&self, account: &str, id: i64) -> Result<(), ServiceError>;
    /// Liga exatamente os Drives de `enabled` e desliga os outros. Com envio pendente num que seria
    /// desligado, recusa com `PendingUploads` e nada muda.
    fn set_shared_drives(&self, account: &str, enabled: &[&str]) -> Result<(), ServiceError>;
    /// Cria um Drive compartilhado (nome repetido ganha " (2)"…) e o devolve, já ligado.
    fn create_shared_drive(
        &self,
        account: &str,
        name: &str,
    ) -> Result<SharedDriveItem, ServiceError>;
    /// As versões do arquivo, da mais nova para a mais antiga.
    fn versions(&self, account: &str, item_id: &str) -> Result<Versions, ServiceError>;
    /// Baixa a versão para a pasta de downloads e devolve o caminho do arquivo.
    fn version_download(
        &self,
        account: &str,
        item_id: &str,
        version_id: &str,
    ) -> Result<String, ServiceError>;
    /// Volta o arquivo para a versão (enviada como versão nova).
    fn version_restore(
        &self,
        account: &str,
        item_id: &str,
        version_id: &str,
    ) -> Result<(), ServiceError>;
    /// Marca ou desmarca "manter para sempre".
    fn version_keep(
        &self,
        account: &str,
        item_id: &str,
        version_id: &str,
        keep: bool,
    ) -> Result<(), ServiceError>;
    /// O menu do botão direito para os caminhos; vazio fora do Meu Drive ou sem conta. Uma pasta
    /// do computador que pode ser sincronizada tem só `sync-folder`.
    fn menu(&self, paths: &[&str]) -> Result<Vec<MenuEntry>, ServiceError>;
    /// O emblema de cada caminho dentro de uma pasta sincronizada (`offline`, `syncing` ou
    /// `conflict`), vazio fora delas.
    fn folder_states(&self, paths: &[&str]) -> Result<Vec<String>, ServiceError>;
    /// As raízes das pastas sincronizadas: só abaixo delas `FolderStates` tem o que dizer.
    fn folder_roots(&self) -> Result<Vec<String>, ServiceError>;
    /// Faz a ação do menu nos caminhos; devolve o texto a copiar (o link), ou vazio.
    fn menu_action(&self, action: &str, paths: &[&str]) -> Result<String, ServiceError>;
    /// `mode`: vazio, `finish`, `move` (para a pasta `dir`) ou `discard`.
    fn logout(&self, account: &str, mode: &str, dir: &str) -> Result<(), ServiceError>;
    /// Sai de todas as contas, com o mesmo `mode` e `dir` de `Logout`.
    fn logout_all(&self, mode: &str, dir: &str) -> Result<(), ServiceError>;
    /// Antes de os programas saírem: desmonta, tira o início automático e o lugar nos Locais e,
    /// com `purge` (só sem conta conectada), apaga os dados locais; depois encerra o serviço.
    fn uninstall(&self, purge: bool) -> Result<(), ServiceError>;
    fn preferences(&self) -> Result<Vec<Pref>, ServiceError>;
    /// Grava as preferências (`chave`, valor em JSON) todas ou nenhuma; `true` se alguma só
    /// vale depois de reiniciar.
    fn set_preferences(&self, values: &[(&str, &str)]) -> Result<bool, ServiceError>;
    /// Volta ao padrão tudo o que o usuário mudou (a política continua valendo).
    fn reset_preferences(&self) -> Result<(), ServiceError>;
    /// Reinicia o serviço (para as preferências que pedem isso).
    fn restart(&self) -> Result<(), ServiceError>;
    /// Encerra o serviço; a bandeja sai junto ao ver `Quitting`.
    fn quit(&self) -> Result<(), ServiceError>;

    #[zbus(signal)]
    fn login_finished(&self, ok: bool, email: &str, error: &str) -> zbus::Result<()>;
    #[zbus(signal)]
    fn status_changed(&self) -> zbus::Result<()>;
    /// A atividade mudou (no máximo um a cada 250 ms).
    #[zbus(signal)]
    fn activity_changed(&self) -> zbus::Result<()>;
    /// O histórico de notificações mudou (aviso novo, lido ou dispensado).
    #[zbus(signal)]
    fn notifications_changed(&self) -> zbus::Result<()>;
    /// O estado (na nuvem, off-line, sincronizando, conflito) destes caminhos da montagem mudou:
    /// os gerenciadores de arquivos redesenham os emblemas deles.
    #[zbus(signal)]
    fn states_changed(&self, paths: Vec<String>) -> zbus::Result<()>;
    /// Alguma preferência mudou (por `SetPreferences` ou `ResetPreferences`).
    #[zbus(signal)]
    fn preferences_changed(&self) -> zbus::Result<()>;
    /// A lista de pastas do computador ou o estado de uma delas mudou.
    #[zbus(signal)]
    fn local_folders_changed(&self) -> zbus::Result<()>;
    /// O serviço vai sair (`Quit`).
    #[zbus(signal)]
    fn quitting(&self) -> zbus::Result<()>;
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
