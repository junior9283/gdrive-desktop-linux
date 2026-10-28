//! Login OAuth 2.0 com redirecionamento para o loopback e PKCE (S256).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use axum::Router;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use gdl_common::i18n::I18n;
use gdl_common::time::now_ms;
use rand::RngCore;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::net::TcpListener;
use tokio::sync::{oneshot, watch};
use tokio::task::JoinHandle;
use url::Url;

pub use crate::token_store::{ClientCredentials, TokenSet};

/// Acesso completo ao Drive, mais o e-mail da conta (para o `userinfo`).
const SCOPES: &str = "openid email https://www.googleapis.com/auth/drive";
/// O token é renovado quando falta menos que isto para expirar.
const REFRESH_MARGIN_MS: i64 = 60_000;

/// Cliente HTTP do login, com prazos para uma conexão pendurada não travar o app.
fn http_client() -> reqwest::Client {
    crate::http_builder()
        .timeout(Duration::from_secs(30))
        .build()
        .expect("configuração fixa do cliente HTTP")
}

/// Texto aleatório em base64url, para o `state` e o `code_verifier`.
fn random_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

#[derive(Debug, Clone)]
pub struct OAuthEndpoints {
    pub auth: Url,
    pub token: Url,
    pub userinfo: Url,
}

impl OAuthEndpoints {
    /// Endereços públicos do OAuth do Google.
    pub fn google() -> OAuthEndpoints {
        OAuthEndpoints {
            auth: "https://accounts.google.com/o/oauth2/v2/auth"
                .parse()
                .unwrap(),
            token: "https://oauth2.googleapis.com/token".parse().unwrap(),
            userinfo: "https://openidconnect.googleapis.com/v1/userinfo"
                .parse()
                .unwrap(),
        }
    }
}

/// Falha ao falar com o servidor de tokens (na troca do código ou na renovação).
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("o acesso à conta foi revogado; é preciso entrar de novo")]
    Revoked,
    #[error("falha de rede ao falar com o Google: {0}")]
    Network(String),
    #[error("o servidor de login respondeu {0}")]
    Http(u16),
}

impl LoginError {
    /// O motivo que a janela traduz (`LoginFinished`); os outros erros vão como texto.
    pub fn reason(&self) -> Option<&'static str> {
        match self {
            LoginError::BlockedByOrg => Some("blocked-by-organization"),
            LoginError::AccountDisabled => Some("account-disabled"),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for AuthError {
    fn from(e: reqwest::Error) -> Self {
        AuthError::Network(e.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum LoginError {
    #[error("o login não foi concluído a tempo")]
    Timeout,
    #[error("o acesso à conta foi recusado")]
    Denied,
    #[error("a organização não permite usar esta conta neste app")]
    BlockedByOrg,
    #[error("a conta está desativada")]
    AccountDisabled,
    #[error(
        "o Google não enviou o token de renovação; remova o acesso do app na conta e entre de novo"
    )]
    MissingRefreshToken,
    #[error("não foi possível abrir o retorno do login no computador: {0}")]
    Loopback(#[from] std::io::Error),
    #[error(transparent)]
    Auth(#[from] AuthError),
}

impl From<reqwest::Error> for LoginError {
    fn from(e: reqwest::Error) -> Self {
        LoginError::Auth(e.into())
    }
}

/// Resposta do endpoint de token.
#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: i64,
    refresh_token: Option<String>,
}

/// POST no endpoint de token. 400/401 querem dizer código ou renovação inválidos.
async fn post_token(
    http: &reqwest::Client,
    url: &Url,
    form: &[(&str, &str)],
) -> Result<TokenResponse, AuthError> {
    let resp = http.post(url.clone()).form(form).send().await?;
    match resp.status() {
        s if s.is_success() => Ok(resp.json().await?),
        StatusCode::BAD_REQUEST | StatusCode::UNAUTHORIZED => Err(AuthError::Revoked),
        s => Err(AuthError::Http(s.as_u16())),
    }
}

#[derive(Deserialize)]
struct UserInfo {
    email: String,
}

type CallbackResult = Result<String, LoginError>;

/// Estado do servidor de loopback que recebe o retorno do navegador.
struct Callback {
    state: String,
    reply: Mutex<Option<oneshot::Sender<CallbackResult>>>,
    i18n: I18n,
}

/// Login no navegador: abre `auth_url()` e espera o Google redirecionar para o loopback.
pub struct LoginFlow {
    creds: ClientCredentials,
    endpoints: OAuthEndpoints,
    state: String,
    verifier: String,
    redirect_uri: String,
    result: oneshot::Receiver<CallbackResult>,
    _server: AbortOnDrop,
}

/// Derruba o servidor de loopback quando o login termina ou é abandonado.
struct AbortOnDrop(JoinHandle<()>);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

impl LoginFlow {
    /// Sobe o servidor de loopback em `127.0.0.1` numa porta livre e prepara o PKCE.
    pub async fn start(
        creds: ClientCredentials,
        endpoints: OAuthEndpoints,
    ) -> Result<LoginFlow, LoginError> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        let (reply, result) = oneshot::channel();
        let state = random_token();
        let callback = Arc::new(Callback {
            state: state.clone(),
            reply: Mutex::new(Some(reply)),
            i18n: I18n::from_env(),
        });
        let app = Router::new()
            .route("/", get(on_redirect))
            .with_state(callback);
        let server = tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        });
        Ok(LoginFlow {
            creds,
            endpoints,
            state,
            verifier: random_token(),
            redirect_uri: format!("http://127.0.0.1:{port}"),
            result,
            _server: AbortOnDrop(server),
        })
    }

    /// Endereço a abrir no navegador.
    pub fn auth_url(&self) -> Url {
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()));
        let mut url = self.endpoints.auth.clone();
        url.query_pairs_mut()
            .append_pair("client_id", &self.creds.client_id)
            .append_pair("redirect_uri", &self.redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", SCOPES)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("state", &self.state)
            .append_pair("code_challenge", &challenge)
            .append_pair("code_challenge_method", "S256");
        url
    }

    /// Espera o retorno do navegador (no app, 300 s), troca o código por tokens e busca o e-mail.
    pub async fn wait(self, timeout: Duration) -> Result<TokenSet, LoginError> {
        let code = match tokio::time::timeout(timeout, self.result).await {
            Ok(Ok(result)) => result?,
            Ok(Err(_)) | Err(_) => return Err(LoginError::Timeout),
        };
        let http = http_client();
        let tokens = post_token(
            &http,
            &self.endpoints.token,
            &[
                ("grant_type", "authorization_code"),
                ("code", &code),
                ("code_verifier", &self.verifier),
                ("client_id", &self.creds.client_id),
                ("client_secret", &self.creds.client_secret),
                ("redirect_uri", &self.redirect_uri),
            ],
        )
        .await?;
        let response = http
            .get(self.endpoints.userinfo.clone())
            .bearer_auth(&tokens.access_token)
            .send()
            .await?;
        // Com o código aceito, só uma conta desativada é recusada ao abrir os próprios dados.
        if response.status() == StatusCode::FORBIDDEN {
            return Err(LoginError::AccountDisabled);
        }
        let user: UserInfo = response.error_for_status()?.json().await?;
        Ok(TokenSet {
            refresh_token: tokens
                .refresh_token
                .ok_or(LoginError::MissingRefreshToken)?,
            access_token: tokens.access_token,
            expires_at_ms: now_ms() + tokens.expires_in * 1000,
            email: user.email,
        })
    }
}

/// O que o retorno do navegador significa: `?code=…` ou `?error=…` (o `state` já conferido).
fn parse_callback(params: &HashMap<String, String>) -> CallbackResult {
    match (params.get("code"), params.get("error").map(String::as_str)) {
        (Some(code), None) => Ok(code.clone()),
        (_, Some("admin_policy_enforced" | "org_internal")) => Err(LoginError::BlockedByOrg),
        _ => Err(LoginError::Denied),
    }
}

/// Página própria mostrada no navegador ao fim do login.
fn result_page(i18n: &I18n, ok: bool) -> Html<String> {
    let message = i18n.tr(if ok { "login-success" } else { "login-failure" }, &[]);
    Html(format!(
        "<!doctype html><html><head><meta charset=\"utf-8\"><title>{}</title>\
         <style>body{{font-family:sans-serif;display:grid;place-items:center;height:90vh;color:#202124}}</style>\
         </head><body><p>{message}</p></body></html>",
        i18n.tr("login-page-title", &[])
    ))
}

async fn on_redirect(
    State(cb): State<Arc<Callback>>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    // Retorno sem o `state` deste login (forjado por outro programa ou página): recusa sem encerrar o
    // fluxo, para não deixar ninguém derrubar o login da pessoa.
    if params.get("state") != Some(&cb.state) {
        return (StatusCode::BAD_REQUEST, result_page(&cb.i18n, false)).into_response();
    }
    let result = parse_callback(&params);
    let ok = result.is_ok();
    // Só o primeiro retorno conta; um segundo acesso à mesma URL mostra a falha.
    let delivered = cb
        .reply
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take()
        .is_some_and(|tx| tx.send(result).is_ok());
    let status = if ok && delivered {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    (status, result_page(&cb.i18n, status == StatusCode::OK)).into_response()
}

/// Quem entrega o token de acesso válido para as chamadas à API.
#[async_trait]
pub trait AccessTokenProvider: Send + Sync {
    /// Token válido, renovado antes de expirar.
    async fn access_token(&self) -> Result<String, AuthError>;
    /// Renova depois de um 401 com o token `stale`. Se outra chamada já renovou, devolve o novo
    /// token sem ir de novo ao servidor.
    async fn refresh_now(&self, stale: &str) -> Result<String, AuthError>;
    /// `true` enquanto o Google recusa o login da conta (só entrando de novo). O padrão é um
    /// provedor que nunca é recusado.
    fn signed_out(&self) -> watch::Receiver<bool> {
        watch::channel(false).1
    }
}

/// Credenciais e tokens de uma conta.
struct Session {
    creds: ClientCredentials,
    tokens: TokenSet,
}

/// Tokens de uma conta, renovados 60 s antes de expirar. Quando o Google recusa a renovação, a
/// sessão se perde (`signed_out` vira `true`) e toda chamada dá `AuthError::Revoked` na hora, sem
/// ir à rede, até `replace` trazer um login novo.
pub struct AccessTokens {
    http: reqwest::Client,
    token_url: Url,
    session: tokio::sync::Mutex<Option<Session>>,
    signed_out: watch::Sender<bool>,
}

impl AccessTokens {
    pub fn new(creds: ClientCredentials, tokens: TokenSet, token_url: Url) -> AccessTokens {
        AccessTokens {
            http: http_client(),
            token_url,
            session: tokio::sync::Mutex::new(Some(Session { creds, tokens })),
            signed_out: watch::Sender::new(false),
        }
    }

    /// Conta cujo login se perdeu (falta no chaveiro): já nasce recusada, até `replace`.
    pub fn revoked(token_url: Url) -> AccessTokens {
        AccessTokens {
            http: http_client(),
            token_url,
            session: tokio::sync::Mutex::new(None),
            signed_out: watch::Sender::new(true),
        }
    }

    /// Troca a sessão por um login novo da mesma conta.
    pub async fn replace(&self, creds: ClientCredentials, tokens: TokenSet) {
        let mut session = self.session.lock().await;
        *session = Some(Session { creds, tokens });
        self.signed_out.send_replace(false);
    }

    /// Os tokens atuais, para guardar no chaveiro; `None` sem sessão.
    pub async fn current(&self) -> Option<TokenSet> {
        self.session.lock().await.as_ref().map(|s| s.tokens.clone())
    }

    /// Renova com o lock seguro, para chamadas paralelas não renovarem cada uma. Se o Google
    /// recusa, a sessão some.
    async fn refresh(&self, guard: &mut Option<Session>) -> Result<String, AuthError> {
        let session = guard.as_mut().ok_or(AuthError::Revoked)?;
        let result = post_token(
            &self.http,
            &self.token_url,
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", &session.tokens.refresh_token),
                ("client_id", &session.creds.client_id),
                ("client_secret", &session.creds.client_secret),
            ],
        )
        .await;
        let fresh = match result {
            Ok(fresh) => fresh,
            Err(AuthError::Revoked) => {
                *guard = None;
                self.signed_out.send_replace(true);
                return Err(AuthError::Revoked);
            }
            Err(e) => return Err(e),
        };
        let tokens = &mut session.tokens;
        tokens.access_token = fresh.access_token;
        tokens.expires_at_ms = now_ms() + fresh.expires_in * 1000;
        if let Some(refresh) = fresh.refresh_token {
            tokens.refresh_token = refresh;
        }
        Ok(tokens.access_token.clone())
    }
}

#[async_trait]
impl AccessTokenProvider for AccessTokens {
    async fn access_token(&self) -> Result<String, AuthError> {
        let mut guard = self.session.lock().await;
        let session = guard.as_ref().ok_or(AuthError::Revoked)?;
        if session.tokens.expires_at_ms - REFRESH_MARGIN_MS > now_ms() {
            return Ok(session.tokens.access_token.clone());
        }
        self.refresh(&mut guard).await
    }

    async fn refresh_now(&self, stale: &str) -> Result<String, AuthError> {
        let mut guard = self.session.lock().await;
        let session = guard.as_ref().ok_or(AuthError::Revoked)?;
        if session.tokens.access_token != stale {
            return Ok(session.tokens.access_token.clone());
        }
        self.refresh(&mut guard).await
    }

    fn signed_out(&self) -> watch::Receiver<bool> {
        self.signed_out.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use gdl_common::paths::Paths;
    use gdl_fake_drive::{FakeDrive, Fault};

    use super::*;
    use crate::token_store::{ClientCredentials, MemoryStore, TokenStore};

    fn creds() -> ClientCredentials {
        ClientCredentials {
            client_id: "id-de-teste".into(),
            client_secret: "segredo-de-teste".into(),
        }
    }

    fn endpoints(fake: &FakeDrive) -> OAuthEndpoints {
        OAuthEndpoints {
            auth: fake.auth_url(),
            token: fake.token_url(),
            userinfo: fake.userinfo_url(),
        }
    }

    fn param(url: &Url, key: &str) -> Option<String> {
        url.query_pairs()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.into_owned())
    }

    /// Faz o papel do navegador: abre a URL de autorização e segue o redirecionamento ao loopback.
    async fn approve_in_browser(flow: &LoginFlow) -> reqwest::Response {
        reqwest::get(flow.auth_url()).await.unwrap()
    }

    async fn login(fake: &FakeDrive) -> TokenSet {
        let flow = LoginFlow::start(creds(), endpoints(fake)).await.unwrap();
        assert!(approve_in_browser(&flow).await.status().is_success());
        flow.wait(Duration::from_secs(5)).await.unwrap()
    }

    #[tokio::test]
    async fn auth_url_asks_for_offline_drive_access_with_pkce() {
        let fake = FakeDrive::start().await;
        let flow = LoginFlow::start(creds(), endpoints(&fake)).await.unwrap();
        let url = flow.auth_url();
        assert_eq!(param(&url, "access_type").as_deref(), Some("offline"));
        assert_eq!(param(&url, "prompt").as_deref(), Some("consent"));
        assert_eq!(param(&url, "response_type").as_deref(), Some("code"));
        assert_eq!(
            param(&url, "code_challenge_method").as_deref(),
            Some("S256")
        );
        assert_eq!(param(&url, "client_id").as_deref(), Some("id-de-teste"));
        assert!(
            param(&url, "scope")
                .unwrap()
                .split(' ')
                .any(|s| s == "https://www.googleapis.com/auth/drive")
        );
        assert!(
            param(&url, "redirect_uri")
                .unwrap()
                .starts_with("http://127.0.0.1:")
        );
        assert!(param(&url, "code_challenge").is_some_and(|c| c.len() == 43));
    }

    #[tokio::test]
    async fn full_login_round_trip_returns_tokens() {
        let fake = FakeDrive::start().await;
        let flow = LoginFlow::start(creds(), endpoints(&fake)).await.unwrap();
        let page = approve_in_browser(&flow).await;
        assert_eq!(page.status(), reqwest::StatusCode::OK);
        assert!(
            page.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let tokens = flow.wait(Duration::from_secs(5)).await.unwrap();
        assert_eq!(tokens.email, fake.email());
        assert!(!tokens.access_token.is_empty() && !tokens.refresh_token.is_empty());
        assert!(tokens.expires_at_ms > gdl_common::time::now_ms());
    }

    #[tokio::test]
    async fn wrong_state_is_rejected_without_ending_the_login() {
        let fake = FakeDrive::start().await;
        let flow = LoginFlow::start(creds(), endpoints(&fake)).await.unwrap();
        let redirect: Url = param(&flow.auth_url(), "redirect_uri")
            .unwrap()
            .parse()
            .unwrap();
        for forged in ["?code=abc&state=outro", "?error=access_denied", "?code=abc"] {
            let resp = reqwest::get(format!("{redirect}{forged}")).await.unwrap();
            assert_eq!(resp.status(), reqwest::StatusCode::BAD_REQUEST, "{forged}");
        }
        // Retornos forjados não derrubam o login: o retorno legítimo ainda funciona.
        assert!(approve_in_browser(&flow).await.status().is_success());
        assert_eq!(
            flow.wait(Duration::from_secs(5)).await.unwrap().email,
            fake.email()
        );
    }

    #[tokio::test]
    async fn user_denial_is_reported() {
        let fake = FakeDrive::start().await;
        let flow = LoginFlow::start(creds(), endpoints(&fake)).await.unwrap();
        let url = flow.auth_url();
        let redirect = param(&url, "redirect_uri").unwrap();
        let state = param(&url, "state").unwrap();
        reqwest::get(format!("{redirect}?error=access_denied&state={state}"))
            .await
            .unwrap();
        assert!(matches!(
            flow.wait(Duration::from_secs(5)).await,
            Err(LoginError::Denied)
        ));
    }

    #[tokio::test]
    async fn admin_block_is_its_own_error() {
        for error in ["admin_policy_enforced", "org_internal"] {
            let fake = FakeDrive::start().await;
            let flow = LoginFlow::start(creds(), endpoints(&fake)).await.unwrap();
            let url = flow.auth_url();
            let redirect = param(&url, "redirect_uri").unwrap();
            let state = param(&url, "state").unwrap();
            reqwest::get(format!("{redirect}?error={error}&state={state}"))
                .await
                .unwrap();
            let e = flow.wait(Duration::from_secs(5)).await.unwrap_err();
            assert!(matches!(e, LoginError::BlockedByOrg), "{error}: {e:?}");
            assert_eq!(e.reason(), Some("blocked-by-organization"));
        }
    }

    #[tokio::test]
    async fn disabled_account_is_its_own_error() {
        let fake = FakeDrive::start().await;
        let flow = LoginFlow::start(creds(), endpoints(&fake)).await.unwrap();
        // O Google aceita o código, mas a conta desativada não abre os dados dela.
        fake.inject(Fault::AccountDisabled { times: 1 });
        assert!(approve_in_browser(&flow).await.status().is_success());

        let e = flow.wait(Duration::from_secs(5)).await.unwrap_err();

        assert!(matches!(e, LoginError::AccountDisabled), "{e:?}");
        assert_eq!(e.reason(), Some("account-disabled"));
        assert_eq!(LoginError::Denied.reason(), None);
    }

    #[tokio::test]
    async fn login_times_out() {
        let fake = FakeDrive::start().await;
        let flow = LoginFlow::start(creds(), endpoints(&fake)).await.unwrap();
        assert!(matches!(
            flow.wait(Duration::from_millis(200)).await,
            Err(LoginError::Timeout)
        ));
    }

    #[tokio::test]
    async fn access_token_is_refreshed_before_expiry() {
        let fake = FakeDrive::start().await;
        fake.set_token_lifetime(30);
        let tokens = login(&fake).await;
        fake.set_token_lifetime(3600);
        let provider = AccessTokens::new(creds(), tokens.clone(), fake.token_url());
        let fresh = provider.access_token().await.unwrap();
        assert_ne!(
            fresh, tokens.access_token,
            "faltando 30 s, o token já devia ser renovado"
        );
        assert_eq!(provider.access_token().await.unwrap(), fresh);
        assert_eq!(fake.requests("/token"), 2);
        assert_eq!(
            provider.current().await.unwrap().refresh_token,
            tokens.refresh_token
        );
    }

    #[tokio::test]
    async fn revoked_provider_fails_without_network_until_replaced() {
        let fake = FakeDrive::start().await;
        let provider = AccessTokens::revoked(fake.token_url());
        assert!(*provider.signed_out().borrow());
        assert!(matches!(
            provider.access_token().await,
            Err(AuthError::Revoked)
        ));
        assert!(matches!(
            provider.refresh_now("velho").await,
            Err(AuthError::Revoked)
        ));
        assert_eq!(fake.requests("/token"), 0);

        let tokens = login(&fake).await;
        provider.replace(creds(), tokens.clone()).await;

        assert!(!*provider.signed_out().borrow());
        assert_eq!(provider.access_token().await.unwrap(), tokens.access_token);
    }

    #[tokio::test]
    async fn refused_refresh_makes_the_login_stay_revoked() {
        let fake = FakeDrive::start().await;
        fake.set_token_lifetime(30);
        let tokens = login(&fake).await;
        let provider = AccessTokens::new(creds(), tokens, fake.token_url());
        let mut signed_out = provider.signed_out();
        assert!(!*signed_out.borrow_and_update());
        fake.revoke(&fake.email());
        let asked = fake.requests("/token");

        let first = provider.access_token().await;

        assert!(matches!(first, Err(AuthError::Revoked)), "{first:?}");
        assert_eq!(fake.requests("/token"), asked + 1);
        assert!(signed_out.has_changed().unwrap() && *signed_out.borrow_and_update());
        // Daí em diante falha na hora, sem pedir nada ao endpoint de token.
        assert!(matches!(
            provider.access_token().await,
            Err(AuthError::Revoked)
        ));
        assert!(matches!(
            provider.refresh_now("qualquer").await,
            Err(AuthError::Revoked)
        ));
        assert_eq!(fake.requests("/token"), asked + 1);
    }

    #[tokio::test]
    async fn refresh_now_replaces_a_valid_token() {
        let fake = FakeDrive::start().await;
        let tokens = login(&fake).await;
        let provider = AccessTokens::new(creds(), tokens.clone(), fake.token_url());
        assert_eq!(provider.access_token().await.unwrap(), tokens.access_token);
        assert_ne!(
            provider.refresh_now(&tokens.access_token).await.unwrap(),
            tokens.access_token
        );
    }

    #[tokio::test]
    async fn refresh_now_does_not_repeat_a_refresh_already_done() {
        let fake = FakeDrive::start().await;
        let tokens = login(&fake).await;
        let provider = AccessTokens::new(creds(), tokens.clone(), fake.token_url());
        let first = provider.refresh_now(&tokens.access_token).await.unwrap();
        let second = provider.refresh_now(&tokens.access_token).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(fake.requests("/token"), 2, "um login e uma renovação");
    }

    #[tokio::test]
    async fn revoked_refresh_token_is_reported() {
        let fake = FakeDrive::start().await;
        let mut tokens = login(&fake).await;
        tokens.refresh_token = "revogado".into();
        tokens.expires_at_ms = 0;
        let provider = AccessTokens::new(creds(), tokens, fake.token_url());
        assert!(matches!(
            provider.access_token().await,
            Err(AuthError::Revoked)
        ));
    }

    #[tokio::test]
    async fn tokens_are_never_written_to_disk() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        for dir in [&paths.config_dir, &paths.data_dir, &paths.state_dir] {
            std::fs::create_dir_all(dir).unwrap();
        }
        let fake = FakeDrive::start().await;
        let tokens = login(&fake).await;
        let store = MemoryStore::default();
        store.save(&tokens.email, &creds(), &tokens).await.unwrap();
        assert_eq!(
            store.load(&tokens.email).await.unwrap(),
            Some((creds(), tokens.clone()))
        );

        let mut pending = vec![tmp.path().to_path_buf()];
        while let Some(dir) = pending.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    pending.push(path);
                } else {
                    let body = std::fs::read(&path).unwrap();
                    assert!(
                        !String::from_utf8_lossy(&body).contains(&tokens.refresh_token),
                        "{path:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn debug_output_hides_secrets() {
        let tokens = TokenSet {
            access_token: "acesso-secreto".into(),
            refresh_token: "renovacao-secreta".into(),
            expires_at_ms: 1,
            email: "a@b.c".into(),
        };
        let shown = format!("{tokens:?} {:?}", creds());
        assert!(
            !shown.contains("secret") && !shown.contains("segredo"),
            "{shown}"
        );
        assert!(shown.contains("a@b.c"));
    }
}
