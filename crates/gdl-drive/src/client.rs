//! Cliente da Drive API v3: metadados, download por intervalo, upload multipart ou retomável,
//! lixeira e mudanças. Toda chamada passa pelo recuo exponencial e renova o token após um 401.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use bytes::{Bytes, BytesMut};
use gdl_common::time::rfc3339;
use reqwest::header::{CONTENT_RANGE, CONTENT_TYPE, LOCATION, RANGE};
use reqwest::{Method, RequestBuilder, StatusCode};
use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use url::Url;

pub use crate::model::*;
use crate::oauth::AccessTokenProvider;
use crate::retry::with_backoff;
pub use crate::retry::{Backoff, DriveError};

pub type Result<T> = std::result::Result<T, DriveError>;

/// Abaixo disto o upload vai num pedido só (multipart); a partir disto, em sessão retomável.
pub const RESUMABLE_THRESHOLD: u64 = 8 * 1024 * 1024;
/// Tamanho de cada trecho do upload retomável (múltiplo de 256 KiB, como a API exige).
const CHUNK: usize = 8 * 1024 * 1024;

/// Onde o upload retomável guarda a URL da sessão, para continuar depois de uma queda ou reinício.
pub trait SessionSlot: Send + Sync {
    fn load(&self) -> Option<String>;
    fn store(&self, url: Option<&str>);
}

/// Upload sem sessão guardada (recomeça do zero se o processo cair).
pub struct NoSession;

impl SessionSlot for NoSession {
    fn load(&self) -> Option<String> {
        None
    }

    fn store(&self, _url: Option<&str>) {}
}

impl SessionSlot for std::sync::Mutex<Option<String>> {
    fn load(&self) -> Option<String> {
        self.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn store(&self, url: Option<&str>) {
        *self.lock().unwrap_or_else(|e| e.into_inner()) = url.map(String::from);
    }
}

/// Situação de uma sessão retomável no servidor.
enum SessionState {
    /// Bytes recebidos até agora.
    At(u64),
    Done(Box<RemoteFile>),
}

pub struct DriveClient {
    http: reqwest::Client,
    base: Url,
    upload_base: Url,
    tokens: Arc<dyn AccessTokenProvider>,
    backoff: Backoff,
}

/// Converte a resposta de erro da API no `DriveError` correspondente.
async fn error_from(resp: reqwest::Response) -> DriveError {
    let status = resp.status().as_u16();
    let body: Value = resp.json().await.unwrap_or_default();
    match (
        status,
        body["error"]["errors"][0]["reason"]
            .as_str()
            .unwrap_or_default(),
    ) {
        (401, _) => DriveError::Unauthorized,
        (403, "userRateLimitExceeded" | "rateLimitExceeded") | (429, _) => DriveError::RateLimited,
        (403, "storageQuotaExceeded" | "quotaExceeded") => DriveError::QuotaExceeded,
        (403, "uploadLimitExceeded" | "dailyLimitExceeded") => DriveError::DailyUploadLimit,
        (403, "downloadQuotaExceeded") => DriveError::DownloadQuotaExceeded,
        (403, _) => DriveError::PermissionDenied,
        // 410: sessão de upload expirada.
        (404 | 410, _) => DriveError::NotFound,
        (409, _) => DriveError::AlreadyExists,
        (s, _) => DriveError::Http(s),
    }
}

/// Pede o `id` reservado na criação.
fn with_id(meta: &mut Value, id: Option<&str>) {
    if let Some(id) = id {
        meta["id"] = id.into();
    }
}

/// `Range: bytes=0-N` de uma resposta 308: quantos bytes o servidor já tem.
fn received_bytes(resp: &reqwest::Response) -> u64 {
    resp.headers()
        .get(RANGE)
        .and_then(|v| v.to_str().ok()?.rsplit('-').next()?.parse::<u64>().ok())
        .map_or(0, |last| last + 1)
}

impl DriveClient {
    pub fn new(base: Url, upload_base: Url, tokens: Arc<dyn AccessTokenProvider>) -> DriveClient {
        let http = crate::http_builder()
            .read_timeout(Duration::from_secs(60))
            .build()
            .expect("configuração fixa do cliente HTTP");
        DriveClient {
            http,
            base,
            upload_base,
            tokens,
            backoff: Backoff::default(),
        }
    }

    /// Troca o recuo (nos testes, para não esperar segundos).
    pub fn with_backoff(mut self, backoff: Backoff) -> DriveClient {
        self.backoff = backoff;
        self
    }

    fn api(&self, path: &str) -> Url {
        self.base.join(path).expect("caminho relativo válido")
    }

    fn file_api(&self, id: &str) -> Url {
        self.api(&format!("files/{id}"))
    }

    fn upload_api(&self, path: &str) -> Url {
        self.upload_base
            .join(path)
            .expect("caminho relativo válido")
    }

    /// Uma tentativa autenticada; num 401, renova o token uma vez e repete.
    async fn send_once(&self, build: &impl Fn() -> RequestBuilder) -> Result<reqwest::Response> {
        let token = self.tokens.access_token().await?;
        let mut resp = build().bearer_auth(&token).send().await?;
        if resp.status() == StatusCode::UNAUTHORIZED {
            let token = self.tokens.refresh_now(&token).await?;
            resp = build().bearer_auth(&token).send().await?;
        }
        if resp.status().is_success() {
            Ok(resp)
        } else {
            Err(error_from(resp).await)
        }
    }

    /// Chamada completa com recuo: pedido e leitura da resposta (`finish`) a cada tentativa.
    async fn call<T, Fut>(
        &self,
        build: impl Fn() -> RequestBuilder,
        finish: impl Fn(reqwest::Response) -> Fut,
    ) -> Result<T>
    where
        Fut: Future<Output = Result<T>>,
    {
        let (build, finish) = (&build, &finish);
        with_backoff(&self.backoff, || async move {
            finish(self.send_once(build).await?).await
        })
        .await
    }

    async fn json<T: DeserializeOwned>(&self, build: impl Fn() -> RequestBuilder) -> Result<T> {
        self.call(build, |resp| async move { Ok(resp.json().await?) })
            .await
    }

    async fn file(&self, build: impl Fn() -> RequestBuilder) -> Result<RemoteFile> {
        self.json(build).await
    }

    pub async fn about(&self) -> Result<About> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            user: User,
            storage_quota: Quota,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct User {
            email_address: String,
        }
        #[derive(Deserialize)]
        struct Quota {
            limit: Option<String>,
            usage: Option<String>,
        }
        let raw: Raw = self
            .json(|| {
                self.http
                    .get(self.api("about"))
                    .query(&[("fields", "user(emailAddress),storageQuota(limit,usage)")])
            })
            .await?;
        Ok(About {
            email: raw.user.email_address,
            root_id: self.get("root").await?.id,
            quota_total: parse_u64(raw.storage_quota.limit.as_deref()),
            quota_used: parse_u64(raw.storage_quota.usage.as_deref()).unwrap_or(0),
        })
    }

    /// Todos os itens fora da lixeira, entregues página a página (1000 por página). Para na
    /// primeira página que `page` recusar.
    pub async fn list_all<E: From<DriveError>>(
        &self,
        mut page: impl FnMut(Vec<RemoteFile>) -> std::result::Result<(), E>,
    ) -> std::result::Result<(), E> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            #[serde(default)]
            files: Vec<RemoteFile>,
            next_page_token: Option<String>,
        }
        let fields = format!("nextPageToken,files({FILE_FIELDS})");
        let mut token: Option<String> = None;
        loop {
            let p: Page = self
                .json(|| {
                    let req = self.http.get(self.api("files"));
                    let req = req.query(&[
                        ("q", "trashed = false"),
                        ("pageSize", "1000"),
                        ("fields", &fields),
                    ]);
                    match &token {
                        Some(t) => req.query(&[("pageToken", t)]),
                        None => req,
                    }
                })
                .await?;
            page(p.files)?;
            match p.next_page_token {
                Some(t) => token = Some(t),
                None => return Ok(()),
            }
        }
    }

    pub async fn get(&self, id: &str) -> Result<RemoteFile> {
        let url = self.file_api(id);
        self.file(|| self.http.get(url.clone()).query(&[("fields", FILE_FIELDS)]))
            .await
    }

    /// Bytes `[start, end_excl)` do conteúdo.
    pub async fn download_range(&self, id: &str, start: u64, end_excl: u64) -> Result<Bytes> {
        if start >= end_excl {
            return Ok(Bytes::new());
        }
        let url = self.file_api(id);
        let range = format!("bytes={start}-{}", end_excl - 1);
        self.call(
            || {
                self.http
                    .get(url.clone())
                    .query(&[("alt", "media")])
                    .header(RANGE, &range)
            },
            |resp| async move {
                // O Drive sempre atende intervalos; um 200 seria o arquivo inteiro na memória.
                if resp.status() != StatusCode::PARTIAL_CONTENT {
                    return Err(DriveError::Protocol(format!(
                        "download por intervalo respondeu {}",
                        resp.status()
                    )));
                }
                Ok(resp.bytes().await?)
            },
        )
        .await
    }

    /// Cria um item só de metadados (pasta ou atalho).
    async fn create(&self, meta: &Value) -> Result<RemoteFile> {
        self.file(|| {
            self.http
                .post(self.api("files"))
                .query(&[("fields", FILE_FIELDS)])
                .json(meta)
        })
        .await
    }

    /// IDs reservados para criar itens: com um deles, repetir a criação não duplica o item.
    pub async fn generate_ids(&self, count: u32) -> Result<Vec<String>> {
        #[derive(Deserialize)]
        struct Raw {
            ids: Vec<String>,
        }
        let count = count.to_string();
        let raw: Raw = self
            .json(|| {
                self.http
                    .get(self.api("files/generateIds"))
                    .query(&[("count", count.as_str()), ("space", "drive")])
            })
            .await?;
        Ok(raw.ids)
    }

    /// `id`: reservado com `generate_ids`, para a nova tentativa não duplicar a pasta.
    pub async fn create_folder(
        &self,
        parent: &str,
        name: &str,
        id: Option<&str>,
    ) -> Result<RemoteFile> {
        let mut meta = json!({ "name": name, "mimeType": FOLDER_MIME, "parents": [parent] });
        with_id(&mut meta, id);
        let result = self.create(&meta).await;
        self.created(id, result).await
    }

    /// Uma criação com ID reservado que dá 409 já tinha dado certo antes (a resposta é que se
    /// perdeu): o item é o que está no Drive com esse ID.
    async fn created(&self, id: Option<&str>, result: Result<RemoteFile>) -> Result<RemoteFile> {
        match (result, id) {
            (Err(DriveError::AlreadyExists), Some(id)) => self.get(id).await,
            (result, _) => result,
        }
    }

    /// Sem ID reservado: o `files.generateIds` não serve para atalhos (só para arquivos comuns e
    /// pastas).
    pub async fn create_shortcut(
        &self,
        parent: &str,
        name: &str,
        target_id: &str,
    ) -> Result<RemoteFile> {
        self.create(&json!({
            "name": name,
            "mimeType": SHORTCUT_MIME,
            "parents": [parent],
            "shortcutDetails": { "targetId": target_id },
        }))
        .await
    }

    async fn patch_meta(
        &self,
        id: &str,
        meta: &Value,
        query: &[(&str, &str)],
    ) -> Result<RemoteFile> {
        let url = self.file_api(id);
        self.file(|| {
            self.http
                .patch(url.clone())
                .query(&[("fields", FILE_FIELDS)])
                .query(query)
                .json(meta)
        })
        .await
    }

    pub async fn rename_move(
        &self,
        id: &str,
        new_name: Option<&str>,
        add_parent: Option<&str>,
        remove_parent: Option<&str>,
    ) -> Result<RemoteFile> {
        let meta = new_name.map_or_else(|| json!({}), |n| json!({ "name": n }));
        let query: Vec<(&str, &str)> =
            [("addParents", add_parent), ("removeParents", remove_parent)]
                .into_iter()
                .filter_map(|(k, v)| Some((k, v?)))
                .collect();
        self.patch_meta(id, &meta, &query).await
    }

    /// Troca a data de modificação (`touch`, cópia que preserva datas).
    pub async fn set_modified_time(&self, id: &str, modified_ms: i64) -> Result<RemoteFile> {
        self.patch_meta(id, &json!({ "modifiedTime": rfc3339(modified_ms) }), &[])
            .await
    }

    /// Manda para a lixeira do Drive (ou tira de lá). Nunca apaga de vez.
    pub async fn set_trashed(&self, id: &str, trashed: bool) -> Result<RemoteFile> {
        self.patch_meta(id, &json!({ "trashed": trashed }), &[])
            .await
    }

    pub async fn start_page_token(&self) -> Result<String> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Raw {
            start_page_token: String,
        }
        let raw: Raw = self
            .json(|| self.http.get(self.api("changes/startPageToken")))
            .await?;
        Ok(raw.start_page_token)
    }

    /// Mudanças desde `token`, juntando todas as páginas, e o token para a próxima consulta.
    pub async fn changes(&self, token: &str) -> Result<(Vec<Change>, String)> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Page {
            #[serde(default)]
            changes: Vec<RawChange>,
            next_page_token: Option<String>,
            new_start_page_token: Option<String>,
        }
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct RawChange {
            file_id: String,
            #[serde(default)]
            removed: bool,
            file: Option<RemoteFile>,
        }
        let fields =
            format!("nextPageToken,newStartPageToken,changes(fileId,removed,file({FILE_FIELDS}))");
        let mut all = vec![];
        let mut page_token = token.to_string();
        loop {
            let page: Page = self
                .json(|| {
                    self.http.get(self.api("changes")).query(&[
                        ("pageToken", page_token.as_str()),
                        ("pageSize", "1000"),
                        ("includeRemoved", "true"),
                        ("fields", &fields),
                    ])
                })
                .await?;
            all.extend(page.changes.into_iter().map(|c| match c.file {
                Some(file) if !c.removed => Change::Upsert(Box::new(file)),
                _ => Change::Removed(c.file_id),
            }));
            match (page.next_page_token, page.new_start_page_token) {
                (Some(next), _) => page_token = next,
                (None, Some(new_start)) => return Ok((all, new_start)),
                (None, None) => {
                    return Err(DriveError::Protocol(
                        "changes.list sem token seguinte".into(),
                    ));
                }
            }
        }
    }

    pub async fn upload_new(
        &self,
        parent: &str,
        name: &str,
        src: &Path,
        modified_ms: i64,
        session: &dyn SessionSlot,
        id: Option<&str>,
    ) -> Result<RemoteFile> {
        let mut meta =
            json!({ "name": name, "parents": [parent], "modifiedTime": rfc3339(modified_ms) });
        with_id(&mut meta, id);
        let result = self
            .upload(Method::POST, self.upload_api("files"), meta, src, session)
            .await;
        self.created(id, result).await
    }

    /// Troca o conteúdo de um arquivo existente; o ID continua o mesmo.
    pub async fn upload_update(
        &self,
        id: &str,
        src: &Path,
        modified_ms: i64,
        session: &dyn SessionSlot,
    ) -> Result<RemoteFile> {
        let meta = json!({ "modifiedTime": rfc3339(modified_ms) });
        self.upload(
            Method::PATCH,
            self.upload_api(&format!("files/{id}")),
            meta,
            src,
            session,
        )
        .await
    }

    async fn upload(
        &self,
        method: Method,
        url: Url,
        meta: Value,
        src: &Path,
        session: &dyn SessionSlot,
    ) -> Result<RemoteFile> {
        let size = tokio::fs::metadata(src).await?.len();
        if size < RESUMABLE_THRESHOLD {
            self.upload_multipart(method, url, &meta, src, size).await
        } else {
            self.upload_resumable(method, url, &meta, src, size, session)
                .await
        }
    }

    async fn upload_multipart(
        &self,
        method: Method,
        url: Url,
        meta: &Value,
        src: &Path,
        size: u64,
    ) -> Result<RemoteFile> {
        let boundary = format!("gdl{:016x}", rand::random::<u64>());
        let head = format!(
            "--{boundary}\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n{meta}\r\n--{boundary}\r\nContent-Type: application/octet-stream\r\n\r\n"
        );
        let tail = format!("\r\n--{boundary}--\r\n");
        // O conteúdo é lido direto no corpo: uma cópia só.
        let mut body = Vec::with_capacity(head.len() + size as usize + tail.len());
        body.extend_from_slice(head.as_bytes());
        tokio::fs::File::open(src)
            .await?
            .read_to_end(&mut body)
            .await?;
        body.extend_from_slice(tail.as_bytes());
        let body = Bytes::from(body);
        let content_type = format!("multipart/related; boundary={boundary}");
        self.file(|| {
            self.http
                .request(method.clone(), url.clone())
                .query(&[("uploadType", "multipart"), ("fields", FILE_FIELDS)])
                .header(CONTENT_TYPE, &content_type)
                .body(body.clone())
        })
        .await
    }

    /// Upload retomável: abre a sessão (guardando a URL no `session` antes de mandar bytes), envia
    /// em trechos e, se a conexão cair, consulta quanto chegou e continua dali.
    async fn upload_resumable(
        &self,
        method: Method,
        url: Url,
        meta: &Value,
        src: &Path,
        size: u64,
        session: &dyn SessionSlot,
    ) -> Result<RemoteFile> {
        // Cópia em memória da sessão desta chamada (`session` pode não guardar nada).
        let saved = &std::sync::Mutex::new(session.load());
        let (method, url) = (&method, &url);
        let file = with_backoff(&self.backoff, || async move {
            let mut current = saved.load();
            let mut offset = 0;
            if let Some(u) = current.as_deref() {
                match self.session_state(u, size).await {
                    Ok(SessionState::Done(file)) => return Ok(*file),
                    Ok(SessionState::At(at)) => offset = at,
                    // Sessão expirada: abre outra.
                    Err(DriveError::NotFound) => current = None,
                    Err(e) => return Err(e),
                }
            }
            let session_url = match current {
                Some(u) => u,
                None => {
                    let u = self.open_session(method, url, meta, size).await?;
                    session.store(Some(&u));
                    saved.store(Some(&u));
                    u
                }
            };
            self.send_from(&session_url, src, offset, size).await
        })
        .await?;
        session.store(None);
        Ok(file)
    }

    async fn open_session(
        &self,
        method: &Method,
        url: &Url,
        meta: &Value,
        size: u64,
    ) -> Result<String> {
        let build = || {
            self.http
                .request(method.clone(), url.clone())
                .query(&[("uploadType", "resumable"), ("fields", FILE_FIELDS)])
                .header("X-Upload-Content-Length", size)
                .json(meta)
        };
        let resp = self.send_once(&build).await?;
        resp.headers()
            .get(LOCATION)
            .and_then(|v| v.to_str().ok())
            .map(String::from)
            .ok_or_else(|| DriveError::Protocol("sessão retomável sem Location".into()))
    }

    /// Pergunta ao servidor quantos bytes da sessão já chegaram (a URL da sessão é a credencial).
    async fn session_state(&self, session_url: &str, size: u64) -> Result<SessionState> {
        let resp = self
            .http
            .put(session_url)
            .header(CONTENT_RANGE, format!("bytes */{size}"))
            .send()
            .await?;
        match resp.status() {
            StatusCode::PERMANENT_REDIRECT => Ok(SessionState::At(received_bytes(&resp))),
            s if s.is_success() => Ok(SessionState::Done(Box::new(resp.json().await?))),
            _ => Err(error_from(resp).await),
        }
    }

    /// Envia o arquivo a partir de `offset`, um trecho por pedido.
    async fn send_from(
        &self,
        session_url: &str,
        src: &Path,
        mut offset: u64,
        size: u64,
    ) -> Result<RemoteFile> {
        let mut file = tokio::fs::File::open(src).await?;
        // Um buffer só para todos os trechos: `split` entrega o trecho e a memória volta no
        // `resize` seguinte, quando o pedido anterior já a soltou.
        let mut buf = BytesMut::new();
        let mut pos = None;
        loop {
            if pos != Some(offset) {
                file.seek(std::io::SeekFrom::Start(offset)).await?;
            }
            let len = CHUNK.min((size - offset) as usize);
            buf.resize(len, 0);
            file.read_exact(&mut buf).await?;
            pos = Some(offset + len as u64);
            let chunk = buf.split().freeze();
            let last = offset + len as u64 - 1;
            let resp = self
                .http
                .put(session_url)
                .header(CONTENT_RANGE, format!("bytes {offset}-{last}/{size}"))
                .body(chunk)
                .send()
                .await?;
            match resp.status() {
                StatusCode::PERMANENT_REDIRECT => offset = received_bytes(&resp),
                s if s.is_success() => return Ok(resp.json().await?),
                _ => return Err(error_from(resp).await),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::time::Duration;

    use gdl_fake_drive::{FakeDrive, Fault, TEST_ACCESS_TOKEN};

    use super::*;
    use crate::oauth::{AccessTokenProvider, AuthError};

    const MIB: usize = 1024 * 1024;

    /// Token fixo; depois de um 401, troca para o token válido do simulado.
    struct FixedToken(Mutex<String>);

    #[async_trait::async_trait]
    impl AccessTokenProvider for FixedToken {
        async fn access_token(&self) -> std::result::Result<String, AuthError> {
            Ok(self.0.lock().unwrap().clone())
        }

        async fn refresh_now(&self, _stale: &str) -> std::result::Result<String, AuthError> {
            *self.0.lock().unwrap() = TEST_ACCESS_TOKEN.into();
            Ok(TEST_ACCESS_TOKEN.into())
        }
    }

    fn client_with_token(fake: &FakeDrive, token: &str) -> DriveClient {
        DriveClient::new(
            fake.base_url(),
            fake.upload_url(),
            Arc::new(FixedToken(Mutex::new(token.into()))),
        )
        .with_backoff(Backoff {
            base: Duration::from_millis(1),
            max: Duration::from_millis(4),
            tries: 8,
        })
    }

    fn client(fake: &FakeDrive) -> DriveClient {
        client_with_token(fake, TEST_ACCESS_TOKEN)
    }

    fn temp_file(dir: &tempfile::TempDir, name: &str, len: usize) -> (std::path::PathBuf, Vec<u8>) {
        let data: Vec<u8> = (0..len).map(|n| (n % 251) as u8).collect();
        let path = dir.path().join(name);
        std::fs::write(&path, &data).unwrap();
        (path, data)
    }

    /// Token vencido e sem rede para renovar.
    struct NoNetworkToRefresh;

    #[async_trait::async_trait]
    impl AccessTokenProvider for NoNetworkToRefresh {
        async fn access_token(&self) -> std::result::Result<String, AuthError> {
            Err(AuthError::Network("sem rota".into()))
        }

        async fn refresh_now(&self, _stale: &str) -> std::result::Result<String, AuthError> {
            Err(AuthError::Network("sem rota".into()))
        }
    }

    /// Bug: depois de 1 h sem rede, a renovação do token falhava por rede e o erro chegava como
    /// falha de acesso; a fila avisava "falha no envio" em vez de esperar a rede.
    #[tokio::test]
    async fn token_refresh_without_network_is_offline() {
        let fake = FakeDrive::start().await;
        let c = DriveClient::new(
            fake.base_url(),
            fake.upload_url(),
            Arc::new(NoNetworkToRefresh),
        );
        let err = c.get(&fake.root_id()).await.unwrap_err();
        assert!(matches!(err, DriveError::Offline), "{err:?}");
    }

    /// O serviço roda cada chamada numa tarefa própria: as futures precisam ser `Send + 'static`.
    #[test]
    fn every_call_can_run_in_a_spawned_task() {
        fn spawnable<F: std::future::Future + Send + 'static>(_: F) {}
        let url: Url = "http://127.0.0.1:1/".parse().unwrap();
        let c = Arc::new(DriveClient::new(
            url.clone(),
            url,
            Arc::new(FixedToken(Mutex::new(String::new()))),
        ));
        macro_rules! spawnable_call {
            ($($call:tt)*) => {{
                let c = c.clone();
                spawnable(async move { c.$($call)*.await })
            }};
        }
        spawnable_call!(about());
        spawnable_call!(list_all(|_| Ok::<_, DriveError>(())));
        spawnable_call!(download_range("x", 0, 1));
        spawnable_call!(changes("1"));
        spawnable_call!(set_trashed("x", true));
        spawnable_call!(create_folder("p", "n", None));
        spawnable_call!(upload_new(
            "p",
            "n",
            Path::new("/nada"),
            0,
            &NoSession,
            None
        ));
        spawnable_call!(generate_ids(1));
    }

    #[tokio::test]
    async fn about_reports_email_root_and_quota() {
        let fake = FakeDrive::start().await;
        fake.add_file(&fake.root_id(), "a", vec![0; 10]);
        let about = client(&fake).about().await.unwrap();
        assert_eq!(about.email, fake.email());
        assert_eq!(about.root_id, fake.root_id());
        assert_eq!(about.quota_used, 10);
        assert!(about.quota_total.is_some());
    }

    #[tokio::test]
    async fn lists_all_items_across_pages() {
        let fake = FakeDrive::start().await;
        let root = fake.root_id();
        for n in 0..2500 {
            fake.add_file(&root, &format!("f{n}"), vec![]);
        }
        let gone = fake.add_file(&root, "lixo", vec![]);
        fake.trash_remote(&gone);
        let mut pages = vec![];
        client(&fake)
            .list_all(|page| {
                pages.push(page.len());
                Ok::<_, DriveError>(())
            })
            .await
            .unwrap();
        assert_eq!(pages, [1000, 1000, 500]);
    }

    #[tokio::test]
    async fn get_maps_files_folders_docs_and_shortcuts() {
        let fake = FakeDrive::start().await;
        let root = fake.root_id();
        let file = fake.add_file(&root, "a.txt", b"oi".to_vec());
        let doc = fake.add_gdoc(&root, "Doc", "application/vnd.google-apps.document");
        let link = fake.add_shortcut(&root, "atalho", &doc);
        let c = client(&fake);

        let f = c.get(&file).await.unwrap();
        assert_eq!(
            (f.name.as_str(), f.size, f.parents.clone()),
            ("a.txt", Some(2), vec![root.clone()])
        );
        assert_eq!(f.md5.as_deref(), fake.meta(&file)["md5Checksum"].as_str());
        assert!(
            f.head_revision.is_some()
                && f.modified_ms > 0
                && f.created_ms > 0
                && f.can_edit
                && !f.trashed
        );
        assert!(f.web_view_link.is_some());

        let d = c.get(&doc).await.unwrap();
        assert_eq!(
            (d.mime_type.as_str(), d.size, d.md5),
            ("application/vnd.google-apps.document", None, None)
        );

        let s = c.get(&link).await.unwrap();
        assert_eq!(
            s.shortcut_target,
            Some((doc, "application/vnd.google-apps.document".into()))
        );
    }

    #[tokio::test]
    async fn download_range_sends_range_header() {
        let fake = FakeDrive::start().await;
        let data: Vec<u8> = (0..1000u32).map(|n| n as u8).collect();
        let id = fake.add_file(&fake.root_id(), "a.bin", data.clone());
        let got = client(&fake).download_range(&id, 100, 200).await.unwrap();
        assert_eq!(got.as_ref(), &data[100..200]);
        assert_eq!(fake.downloaded_bytes(), 100);
    }

    #[tokio::test]
    async fn small_file_uses_multipart_large_uses_resumable() {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let (small, small_data) = temp_file(&dir, "pequeno", MIB);
        let (large, large_data) = temp_file(&dir, "grande", 9 * MIB);
        let c = client(&fake);
        let root = fake.root_id();
        let a = c
            .upload_new(
                &root,
                "pequeno.bin",
                &small,
                1_700_000_000_000,
                &NoSession,
                None,
            )
            .await
            .unwrap();
        let b = c
            .upload_new(
                &root,
                "grande.bin",
                &large,
                1_700_000_000_000,
                &NoSession,
                None,
            )
            .await
            .unwrap();
        assert_eq!(
            fake.requests("/upload/drive/v3/files?uploadType=multipart"),
            1
        );
        assert_eq!(
            fake.requests("/upload/drive/v3/files?uploadType=resumable"),
            1
        );
        assert_eq!(fake.content(&a.id), small_data);
        assert_eq!(fake.content(&b.id), large_data);
        assert_eq!(
            (a.name.as_str(), a.modified_ms, a.parents.clone()),
            ("pequeno.bin", 1_700_000_000_000, vec![root])
        );
    }

    /// Bug: com a resposta da criação perdida, a nova tentativa criava o item de novo.
    #[tokio::test]
    async fn create_with_reserved_id_is_not_duplicated_when_the_response_is_lost() {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let (small, data) = temp_file(&dir, "pequeno", 1000);
        let c = client(&fake);
        let root = fake.root_id();
        let ids = c.generate_ids(2).await.unwrap();

        fake.inject(Fault::LoseCreateResponses { times: 1 });
        let folder = c
            .create_folder(&root, "Pasta", Some(&ids[0]))
            .await
            .unwrap();
        fake.inject(Fault::LoseCreateResponses { times: 1 });
        let file = c
            .upload_new(&root, "a.bin", &small, 0, &NoSession, Some(&ids[1]))
            .await
            .unwrap();

        assert_eq!(folder.id, ids[0]);
        assert_eq!(fake.find_live("Pasta"), vec![ids[0].clone()]);
        assert_eq!(file.id, ids[1]);
        assert_eq!(fake.find_live("a.bin"), vec![ids[1].clone()]);
        assert_eq!(fake.content(&file.id), data);
        assert!(matches!(
            c.create_folder(&root, "Outra", Some("inventado")).await,
            Err(DriveError::Http(400))
        ));
    }

    #[tokio::test]
    async fn upload_update_replaces_content_and_keeps_id() {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let id = fake.add_file(&fake.root_id(), "a.txt", b"v1".to_vec());
        let old_rev = fake.meta(&id)["headRevisionId"].clone();
        let (path, data) = temp_file(&dir, "a.txt", 10);
        let f = client(&fake)
            .upload_update(&id, &path, 1_600_000_000_000, &NoSession)
            .await
            .unwrap();
        assert_eq!(f.id, id);
        assert_eq!(fake.content(&id), data);
        assert_ne!(fake.meta(&id)["headRevisionId"], old_rev);
        assert_eq!(f.modified_ms, 1_600_000_000_000);
    }

    #[tokio::test]
    async fn resumable_upload_resumes_after_drop() {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let (path, data) = temp_file(&dir, "grande", 9 * MIB);
        let session = Mutex::new(None::<String>);
        fake.inject(Fault::DropConnectionAfterBytes(3 * MIB as u64));
        let f = client(&fake)
            .upload_new(&fake.root_id(), "grande.bin", &path, 0, &session, None)
            .await
            .unwrap();
        assert_eq!(fake.content(&f.id), data);
        assert_eq!(
            fake.requests("/upload/drive/v3/files?uploadType=resumable"),
            1,
            "a mesma sessão é retomada"
        );
        assert_eq!(
            *session.lock().unwrap(),
            None,
            "sessão concluída é esquecida"
        );
    }

    #[tokio::test]
    async fn saved_session_is_resumed_without_starting_over() {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let (path, data) = temp_file(&dir, "grande", 9 * MIB);
        let session = Mutex::new(None::<String>);
        fake.inject(Fault::Offline);
        let c = client(&fake);
        assert!(matches!(
            c.upload_new(&fake.root_id(), "g.bin", &path, 0, &session, None)
                .await,
            Err(DriveError::Offline)
        ));
        fake.clear_faults();
        // Primeiro envio falhou antes de abrir a sessão: nada guardado.
        assert_eq!(*session.lock().unwrap(), None);

        fake.inject(Fault::DropConnectionAfterBytes(2 * MIB as u64));
        let no_retry = client(&fake).with_backoff(Backoff {
            base: Duration::from_millis(1),
            max: Duration::from_millis(1),
            tries: 1,
        });
        assert!(
            no_retry
                .upload_new(&fake.root_id(), "g.bin", &path, 0, &session, None)
                .await
                .is_err()
        );
        let saved = session.lock().unwrap().clone();
        assert!(
            saved.is_some(),
            "a URL da sessão foi guardada antes de enviar os bytes"
        );

        let f = c
            .upload_new(&fake.root_id(), "g.bin", &path, 0, &session, None)
            .await
            .unwrap();
        assert_eq!(fake.content(&f.id), data);
        assert_eq!(
            fake.requests("/upload/drive/v3/files?uploadType=resumable"),
            1
        );
    }

    #[tokio::test]
    async fn metadata_operations_round_trip() {
        let fake = FakeDrive::start().await;
        let root = fake.root_id();
        let c = client(&fake);
        let folder = c.create_folder(&root, "Pasta", None).await.unwrap();
        assert_eq!(folder.mime_type, FOLDER_MIME);
        let file = fake.add_file(&root, "a.txt", vec![]);
        let moved = c
            .rename_move(&file, Some("b.txt"), Some(&folder.id), Some(&root))
            .await
            .unwrap();
        assert_eq!(
            (moved.name.as_str(), moved.parents.clone()),
            ("b.txt", vec![folder.id.clone()])
        );
        assert!(c.set_trashed(&file, true).await.unwrap().trashed);
        assert!(!c.set_trashed(&file, false).await.unwrap().trashed);
        let link = c
            .create_shortcut(&root, "atalho", &folder.id)
            .await
            .unwrap();
        assert_eq!(link.shortcut_target, Some((folder.id, FOLDER_MIME.into())));
    }

    #[tokio::test]
    async fn changes_report_upserts_and_removals_with_new_token() {
        let fake = FakeDrive::start().await;
        let root = fake.root_id();
        let edited = fake.add_file(&root, "a", b"1".to_vec());
        let c = client(&fake);
        let token = c.start_page_token().await.unwrap();
        fake.modify_remote(&edited, b"2".to_vec());
        let (changes, next) = c.changes(&token).await.unwrap();
        assert!(matches!(&changes[..], [Change::Upsert(f)] if f.id == edited && f.size == Some(1)));
        let (none, again) = c.changes(&next).await.unwrap();
        assert!(none.is_empty());
        assert_eq!(again, next);
    }

    #[tokio::test]
    async fn rate_limit_is_retried() {
        let fake = FakeDrive::start().await;
        fake.inject(Fault::RateLimit403 { times: 3 });
        assert_eq!(client(&fake).about().await.unwrap().email, fake.email());
    }

    #[tokio::test]
    async fn server_errors_are_retried_until_tries_run_out() {
        let fake = FakeDrive::start().await;
        fake.inject(Fault::Status {
            code: 503,
            times: 100,
        });
        assert!(matches!(
            client(&fake).about().await,
            Err(DriveError::Http(503))
        ));
        assert_eq!(fake.requests("/drive/v3/about"), 8);
    }

    #[tokio::test]
    async fn permission_denied_is_not_retried() {
        let fake = FakeDrive::start().await;
        let id = fake.add_file(&fake.root_id(), "a", vec![]);
        fake.inject(Fault::Status {
            code: 403,
            times: 5,
        });
        assert!(matches!(
            client(&fake).get(&id).await,
            Err(DriveError::PermissionDenied)
        ));
        assert_eq!(fake.requests(&format!("/drive/v3/files/{id}")), 1);
    }

    #[tokio::test]
    async fn not_found_is_not_retried() {
        let fake = FakeDrive::start().await;
        assert!(matches!(
            client(&fake).get("naoexiste").await,
            Err(DriveError::NotFound)
        ));
        assert_eq!(fake.requests("/drive/v3/files/naoexiste"), 1);
    }

    #[tokio::test]
    async fn unauthorized_refreshes_token_once_then_succeeds() {
        let fake = FakeDrive::start().await;
        let about = client_with_token(&fake, "vencido").about().await.unwrap();
        assert_eq!(about.email, fake.email());
    }

    #[tokio::test]
    async fn offline_maps_to_offline_error() {
        let fake = FakeDrive::start().await;
        fake.inject(Fault::Offline);
        assert!(matches!(
            client(&fake).about().await,
            Err(DriveError::Offline)
        ));
    }
}
