//! Drive simulado em memória para os testes: fala HTTP como a Drive API v3 e o OAuth do Google,
//! com falhas injetáveis (limite de taxa, códigos de erro, queda de conexão, sem rede).

mod net;
mod server;
mod sharing;
mod state;

use std::sync::Arc;
use std::sync::atomic::Ordering;

use serde_json::Value;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;
use url::Url;

use state::{FOLDER_MIME, SHORTCUT_MIME, Shared, StatusFault};

/// Token de acesso que o simulado sempre aceita, para testes que não passam pelo login.
pub const TEST_ACCESS_TOKEN: &str = "token-de-teste";

/// Falha a injetar nas próximas requisições.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    /// As próximas `times` chamadas à API recebem 403 `userRateLimitExceeded`.
    RateLimit403 { times: u32 },
    /// As próximas `times` chamadas à API recebem o código `code`.
    Status { code: u16, times: u32 },
    /// As próximas `times` chamadas à API recebem 403 `storageQuotaExceeded` (armazenamento cheio).
    QuotaExceeded { times: u32 },
    /// A conexão cai depois de trafegar mais `n` bytes (uma vez só).
    DropConnectionAfterBytes(u64),
    /// As próximas `times` criações (POST na API) são processadas, mas a resposta se perde (a
    /// conexão cai antes de ela chegar ao cliente).
    LoseCreateResponses { times: u32 },
    /// A conexão para de andar, sem cair, depois de trafegar mais `n` bytes (uma vez só): o
    /// envio fica no meio, como quando o serviço morre durante a transferência.
    StallConnectionAfterBytes(u64),
    /// Toda conexão cai até `clear_faults`.
    Offline,
    /// As miniaturas (`/thumb`) demoram este tempo para responder, até `clear_faults`.
    DelayThumbs(std::time::Duration),
    /// Os envios de conteúdo (`/upload/…`) demoram este tempo para responder, até `clear_faults`.
    DelayUploads(std::time::Duration),
    /// As miniaturas (`/thumb`) respondem o erro `code` da API, até `clear_faults`.
    ThumbStatus(u16),
    /// As miniaturas (`/thumb`) mandam o começo da imagem e a conexão cai no meio do corpo,
    /// até `clear_faults`.
    CutThumbBody,
}

pub struct FakeDrive {
    shared: Arc<Shared>,
    server: JoinHandle<()>,
}

impl Drop for FakeDrive {
    fn drop(&mut self) {
        self.server.abort();
    }
}

impl FakeDrive {
    /// Sobe o servidor em `127.0.0.1` numa porta livre, com um Meu Drive vazio.
    pub async fn start() -> FakeDrive {
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("porta livre no loopback");
        let origin: Url = format!("http://{}/", listener.local_addr().expect("endereço local"))
            .parse()
            .unwrap();
        let shared = Arc::new(Shared::new(origin));
        let app = server::router(shared.clone());
        let listener = net::FaultyListener::new(listener, shared.clone());
        let server = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("servidor do Drive simulado");
        });
        FakeDrive { shared, server }
    }

    fn url(&self, path: &str) -> Url {
        self.shared.origin.join(path).unwrap()
    }

    /// `…/drive/v3/`
    pub fn base_url(&self) -> Url {
        self.url("drive/v3/")
    }

    /// `…/upload/drive/v3/`
    pub fn upload_url(&self) -> Url {
        self.url("upload/drive/v3/")
    }

    pub fn token_url(&self) -> Url {
        self.url("token")
    }

    /// Tela de consentimento simulada: aprova na hora e redireciona para o `redirect_uri`.
    pub fn auth_url(&self) -> Url {
        self.url("o/oauth2/auth")
    }

    pub fn userinfo_url(&self) -> Url {
        self.url("oauth2/v3/userinfo")
    }

    pub fn root_id(&self) -> String {
        self.shared.state().root_id.clone()
    }

    pub fn email(&self) -> String {
        self.shared.state().email.clone()
    }

    pub fn add_folder(&self, parent: &str, name: &str) -> String {
        self.shared
            .state()
            .add(parent, name, FOLDER_MIME, vec![], None)
    }

    pub fn add_file(&self, parent: &str, name: &str, bytes: Vec<u8>) -> String {
        self.shared
            .state()
            .add(parent, name, "application/octet-stream", bytes, None)
    }

    /// Como `add_file`, mas com o tipo `mime` (as imagens ganham `thumbnailLink`).
    pub fn add_file_with_mime(
        &self,
        parent: &str,
        name: &str,
        mime: &str,
        bytes: Vec<u8>,
    ) -> String {
        self.shared.state().add(parent, name, mime, bytes, None)
    }

    /// A rota de miniaturas passa a responder 302 para `location`.
    pub fn redirect_thumbs_to(&self, location: &str) {
        self.shared.state().thumb_redirect = Some(location.into());
    }

    pub fn add_gdoc(&self, parent: &str, name: &str, mime: &str) -> String {
        self.shared.state().add(parent, name, mime, vec![], None)
    }

    pub fn add_shortcut(&self, parent: &str, name: &str, target: &str) -> String {
        self.shared
            .state()
            .add(parent, name, SHORTCUT_MIME, vec![], Some(target.into()))
    }

    /// Arquivo de `owner` compartilhado com o usuário ("Compartilhados comigo").
    pub fn add_shared_file(&self, name: &str, owner: &str, bytes: Vec<u8>) -> String {
        self.shared
            .state()
            .add_shared(name, "application/octet-stream", owner, bytes)
    }

    /// Pasta de `owner` compartilhada com o usuário; o que for posto nela (`add_file`) também é
    /// compartilhado.
    pub fn add_shared_folder(&self, name: &str, owner: &str) -> String {
        self.shared
            .state()
            .add_shared(name, FOLDER_MIME, owner, vec![])
    }

    /// Simula uma edição feita por outra pessoa: novo conteúdo, nova revisão e uma mudança.
    pub fn modify_remote(&self, id: &str, bytes: Vec<u8>) {
        self.shared.state().set_content(id, bytes, None);
    }

    /// Como `modify_remote`, mas só depois de servir `pages` páginas de `files.list`: simula uma
    /// edição feita enquanto o app ainda está listando o Drive.
    pub fn modify_remote_after_list_pages(&self, id: &str, bytes: Vec<u8>, pages: u32) {
        self.shared.state().edit_during_listing = Some((pages, id.into(), bytes));
    }

    /// O dono tira a permissão de edição da conta do app.
    pub fn make_read_only(&self, id: &str) {
        let mut state = self.shared.state();
        state.file_mut(id).can_edit = false;
        state.record_change(id);
    }

    /// Simula outra pessoa movendo o item para outra pasta.
    pub fn move_remote(&self, id: &str, new_parent: &str) {
        let mut state = self.shared.state();
        state.file_mut(id).parents = vec![new_parent.into()];
        state.record_change(id);
    }

    pub fn trash_remote(&self, id: &str) {
        let mut state = self.shared.state();
        state.file_mut(id).trashed = true;
        state.record_change(id);
    }

    /// Apaga de vez, como esvaziar a lixeira pela web.
    pub fn delete_remote(&self, id: &str) {
        let mut state = self.shared.state();
        state.files.remove(id);
        state.record_change(id);
    }

    /// As permissões do item, no formato da API.
    pub fn permissions(&self, id: &str) -> Vec<Value> {
        self.shared.state().files[id].permissions.clone()
    }

    /// Avisos por e-mail de compartilhamento enviados até agora: (e-mail, mensagem).
    pub fn share_emails(&self) -> Vec<(String, Option<String>)> {
        self.shared.state().share_emails.clone()
    }

    pub fn content(&self, id: &str) -> Vec<u8> {
        let state = self.shared.state();
        state.files[id].content.to_vec()
    }

    /// Metadados como a API devolve.
    pub fn meta(&self, id: &str) -> Value {
        self.shared
            .state()
            .file_json(id)
            .unwrap_or_else(|| panic!("arquivo {id} não existe no Drive simulado"))
    }

    pub fn inject(&self, fault: Fault) {
        match fault {
            Fault::RateLimit403 { times } => {
                self.shared.state().fault = Some(StatusFault {
                    code: 403,
                    reason: "userRateLimitExceeded",
                    times,
                });
            }
            Fault::QuotaExceeded { times } => {
                self.shared.state().fault = Some(StatusFault {
                    code: 403,
                    reason: "storageQuotaExceeded",
                    times,
                });
            }
            Fault::Status { code, times } => {
                let reason = match code {
                    403 => "insufficientFilePermissions",
                    404 => "notFound",
                    429 => "rateLimitExceeded",
                    500.. => "backendError",
                    _ => "error",
                };
                self.shared.state().fault = Some(StatusFault {
                    code,
                    reason,
                    times,
                });
            }
            Fault::DropConnectionAfterBytes(n) => self.limit_bytes(n, false),
            Fault::StallConnectionAfterBytes(n) => self.limit_bytes(n, true),
            Fault::LoseCreateResponses { times } => self.shared.state().lose_responses = times,
            Fault::Offline => self.shared.offline.store(true, Ordering::SeqCst),
            Fault::DelayThumbs(delay) => self.shared.state().thumb_delay = Some(delay),
            Fault::DelayUploads(delay) => self.shared.state().upload_delay = Some(delay),
            Fault::ThumbStatus(code) => self.shared.state().thumb_status = Some(code),
            Fault::CutThumbBody => self.shared.state().thumb_cut = true,
        }
    }

    pub fn clear_faults(&self) {
        {
            let mut state = self.shared.state();
            state.fault = None;
            state.lose_responses = 0;
            state.thumb_delay = None;
            state.upload_delay = None;
            state.thumb_status = None;
            state.thumb_cut = false;
        }
        self.shared.drop_budget.store(-1, Ordering::SeqCst);
        self.shared.stall.store(false, Ordering::SeqCst);
        self.shared.cut_next_write.store(false, Ordering::SeqCst);
        self.shared.offline.store(false, Ordering::SeqCst);
    }

    /// Depois de `n` bytes, a conexão cai (ou, com `stall`, empaca).
    fn limit_bytes(&self, n: u64, stall: bool) {
        self.shared.limit_reached.store(false, Ordering::SeqCst);
        self.shared.stall.store(stall, Ordering::SeqCst);
        self.shared.drop_budget.store(n as i64, Ordering::SeqCst);
    }

    /// Se a conexão já caiu (ou empacou) pelo limite de `DropConnectionAfterBytes` ou
    /// `StallConnectionAfterBytes`.
    pub fn byte_limit_reached(&self) -> bool {
        self.shared.limit_reached.load(Ordering::SeqCst)
    }

    /// O maior número de envios de conteúdo que já correram ao mesmo tempo.
    pub fn max_concurrent_uploads(&self) -> u32 {
        self.shared.state().max_uploads_in_flight
    }

    /// Bytes de conteúdo entregues por `alt=media`.
    pub fn downloaded_bytes(&self) -> u64 {
        self.shared.state().downloaded
    }

    /// Quantas requisições começaram com `path_prefix` (caminho e consulta, como `/upload/…?uploadType=multipart`).
    pub fn requests(&self, path_prefix: &str) -> u64 {
        self.shared
            .state()
            .request_log
            .iter()
            .filter(|r| r.starts_with(path_prefix))
            .count() as u64
    }

    /// Quantas requisições usaram o método HTTP `method` (`"DELETE"`, `"PATCH"`…).
    pub fn requests_by_method(&self, method: &str) -> u64 {
        let state = self.shared.state();
        state.method_log.iter().filter(|m| *m == method).count() as u64
    }

    /// IDs dos itens fora da lixeira com o nome `name`.
    pub fn find_live(&self, name: &str) -> Vec<String> {
        let state = self.shared.state();
        state
            .files
            .values()
            .filter(|f| f.name == name && !f.trashed)
            .map(|f| f.id.clone())
            .collect()
    }

    /// Emite um código de autorização ligado ao `code_challenge` (S256), como faria a tela de consentimento.
    pub fn issue_code(&self, code_challenge: &str) -> String {
        self.shared.state().issue_code(code_challenge)
    }

    /// Validade, em segundos, dos próximos tokens de acesso emitidos.
    pub fn set_token_lifetime(&self, secs: i64) {
        self.shared.state().token_lifetime_s = secs;
    }
}
