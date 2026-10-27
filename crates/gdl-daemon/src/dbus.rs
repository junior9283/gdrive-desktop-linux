//! Interface D-Bus do serviço, usada pela CLI, pela bandeja e pelo Dolphin.

use std::sync::Arc;

use gdl_drive::token_store::ClientCredentials;
use gdl_ipc::{
    AccountInfo, ActivityItem, MenuEntry, NotificationItem, NotificationSetting, OfflineEntry,
    OfflineSummary, Pref, SearchResult, ServiceError, SharedItem, Sharing, Status, SyncErrorItem,
    Versions,
};
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
/// serviço chamam tudo pelo proxy, e uma divergência falha neles). `account` é o e-mail da
/// conta; vazio, a primeira.
#[zbus::interface(name = "io.github.junior9283.GDriveDesktopLinux1.Service")]
impl Service {
    /// Abre o login no navegador e devolve o endereço; o resultado vem em `LoginFinished`.
    /// `expected`: o e-mail da conta que precisa entrar de novo; vazio, uma conta nova.
    async fn begin_login(
        &self,
        client_id: &str,
        client_secret: &str,
        expected: &str,
    ) -> Result<String, ServiceError> {
        let creds = ClientCredentials {
            client_id: client_id.into(),
            client_secret: client_secret.into(),
        };
        self.daemon.begin_login(creds, expected).await
    }

    async fn status(&self, account: &str) -> Result<Status, ServiceError> {
        self.daemon.status(account).await
    }

    async fn account_info(&self, account: &str) -> Result<AccountInfo, ServiceError> {
        self.daemon.account_info(account).await
    }

    async fn activity(&self, account: &str) -> Result<Vec<ActivityItem>, ServiceError> {
        self.daemon.activity(account).await
    }

    async fn errors(&self, account: &str) -> Result<Vec<SyncErrorItem>, ServiceError> {
        self.daemon.errors(account).await
    }

    async fn retry(&self, account: &str, item_id: &str) -> Result<(), ServiceError> {
        self.daemon.retry(account, item_id).await
    }

    async fn notifications(&self) -> Vec<NotificationItem> {
        self.daemon.notifications()
    }

    async fn mark_notifications_read(&self) {
        self.daemon.mark_notifications_read().await
    }

    async fn dismiss_notification(&self, id: u64) -> Result<(), ServiceError> {
        self.daemon.dismiss_notification(id).await
    }

    async fn invoke_notification_action(&self, id: u64, key: &str) -> Result<(), ServiceError> {
        self.daemon.invoke_notification_action(id, key).await
    }

    async fn notification_settings(&self) -> Vec<NotificationSetting> {
        self.daemon.notification_settings()
    }

    async fn set_notification_enabled(
        &self,
        kind: &str,
        enabled: bool,
    ) -> Result<(), ServiceError> {
        self.daemon.set_notification_enabled(kind, enabled).await
    }

    async fn cancel_upload(&self, account: &str, item_id: &str) -> Result<String, ServiceError> {
        self.daemon.cancel_upload(account, item_id).await
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

    async fn list_lost_and_found(&self, account: &str) -> Result<Vec<String>, ServiceError> {
        self.daemon.lost_and_found(account).await
    }

    async fn offline_summary(&self, account: &str) -> Result<OfflineSummary, ServiceError> {
        self.daemon.offline_summary(account).await
    }

    /// Os arquivos off-line: com `folder_id` vazio, os fixados mais altos; numa pasta, o que
    /// está off-line dentro dela.
    async fn offline_items(
        &self,
        account: &str,
        folder_id: &str,
    ) -> Result<Vec<OfflineEntry>, ServiceError> {
        self.daemon.offline_items(account, folder_id).await
    }

    async fn unpin_item(&self, account: &str, item_id: &str) -> Result<(), ServiceError> {
        self.daemon.unpin_item(account, item_id).await
    }

    /// Libera o cache (menos off-line e envios pendentes) e devolve os bytes liberados.
    async fn clear_cache(&self, account: &str) -> Result<u64, ServiceError> {
        self.daemon.clear_cache(account).await
    }

    async fn clear_offline_files(&self, account: &str) -> Result<(), ServiceError> {
        self.daemon.clear_offline_files(account).await
    }

    /// Nome (sem diferenciar maiúsculas e acentos) e, com internet, conteúdo; até 50.
    async fn search(&self, account: &str, query: &str) -> Result<Vec<SearchResult>, ServiceError> {
        self.daemon.search(account, query).await
    }

    /// Os 10 arquivos abertos por último.
    async fn recent_items(&self, account: &str) -> Result<Vec<SearchResult>, ServiceError> {
        self.daemon.recent_items(account).await
    }

    /// `target`: `file` (abre o arquivo), `folder` (abre a pasta que o contém) ou `web`.
    async fn open_item(
        &self,
        account: &str,
        item_id: &str,
        target: &str,
    ) -> Result<(), ServiceError> {
        self.daemon.open_item(account, item_id, target).await
    }

    async fn web_link(&self, account: &str, item_id: &str) -> Result<String, ServiceError> {
        self.daemon.web_link(account, item_id).await
    }

    async fn shared_with_me(&self, account: &str) -> Result<Vec<SharedItem>, ServiceError> {
        self.daemon.shared_with_me(account).await
    }

    /// Cria no Meu Drive um atalho para o item compartilhado.
    async fn add_shortcut(&self, account: &str, item_id: &str) -> Result<(), ServiceError> {
        self.daemon.add_shortcut(account, item_id).await
    }

    async fn sharing(&self, account: &str, item_id: &str) -> Result<Sharing, ServiceError> {
        self.daemon.sharing(account, item_id).await
    }

    async fn share_add(
        &self,
        account: &str,
        item_id: &str,
        emails: Vec<String>,
        role: &str,
        notify: bool,
        message: &str,
    ) -> Result<(), ServiceError> {
        self.daemon
            .share_add(account, item_id, &emails, role, notify, message)
            .await
    }

    /// `role` vazio tira o acesso.
    async fn share_set_role(
        &self,
        account: &str,
        item_id: &str,
        permission_id: &str,
        role: &str,
    ) -> Result<(), ServiceError> {
        self.daemon
            .share_set_role(account, item_id, permission_id, role)
            .await
    }

    async fn share_set_general(
        &self,
        account: &str,
        item_id: &str,
        kind: &str,
        role: &str,
    ) -> Result<(), ServiceError> {
        self.daemon
            .share_set_general(account, item_id, kind, role)
            .await
    }

    async fn versions(&self, account: &str, item_id: &str) -> Result<Versions, ServiceError> {
        self.daemon.versions(account, item_id).await
    }

    async fn version_download(
        &self,
        account: &str,
        item_id: &str,
        version_id: &str,
    ) -> Result<String, ServiceError> {
        self.daemon
            .version_download(account, item_id, version_id)
            .await
    }

    async fn version_restore(
        &self,
        account: &str,
        item_id: &str,
        version_id: &str,
    ) -> Result<(), ServiceError> {
        self.daemon
            .version_restore(account, item_id, version_id)
            .await
    }

    async fn version_keep(
        &self,
        account: &str,
        item_id: &str,
        version_id: &str,
        keep: bool,
    ) -> Result<(), ServiceError> {
        self.daemon
            .version_keep(account, item_id, version_id, keep)
            .await
    }

    /// O menu "Google Drive" do botão direito para os caminhos; vazio fora do Meu Drive.
    async fn menu(&self, paths: Vec<String>) -> Vec<MenuEntry> {
        self.daemon.menu(&paths).await
    }

    /// Faz a ação do menu; devolve o texto a copiar (o link em `copy-link`), ou vazio.
    async fn menu_action(&self, action: &str, paths: Vec<String>) -> Result<String, ServiceError> {
        self.daemon.menu_action(action, &paths).await
    }

    /// `mode`: vazio, `finish`, `move` (para a pasta `dir`) ou `discard`.
    async fn logout(&self, account: &str, mode: &str, dir: &str) -> Result<(), ServiceError> {
        self.daemon.logout(account, mode, dir).await
    }

    /// Sai de todas as contas, como `logout`.
    async fn logout_all(&self, mode: &str, dir: &str) -> Result<(), ServiceError> {
        self.daemon.logout_all(mode, dir).await
    }

    /// Prepara a desinstalação e encerra o serviço; `purge` apaga os dados locais (sem conta).
    async fn uninstall(&self, purge: bool) -> Result<(), ServiceError> {
        self.daemon.uninstall(purge).await
    }

    fn preferences(&self) -> Result<Vec<Pref>, ServiceError> {
        self.daemon.preferences()
    }

    /// Grava as preferências (`chave`, valor em JSON) todas ou nenhuma; `true` se alguma só
    /// vale depois de reiniciar.
    async fn set_preferences(&self, values: Vec<(String, String)>) -> Result<bool, ServiceError> {
        self.daemon.set_preferences(values).await
    }

    /// Volta ao padrão tudo o que o usuário mudou (a política continua valendo).
    async fn reset_preferences(&self) -> Result<(), ServiceError> {
        self.daemon.reset_preferences().await
    }

    /// Reinicia o serviço (para as preferências que pedem isso).
    fn restart(&self) {
        self.daemon.restart();
    }

    /// Encerra o serviço; a bandeja sai junto ao ver `Quitting`.
    async fn quit(&self) {
        self.daemon.quit().await;
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

    #[zbus(signal)]
    pub async fn notifications_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn states_changed(emitter: &SignalEmitter<'_>, paths: &[String]) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn preferences_changed(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    #[zbus(signal)]
    pub async fn quitting(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;
}
