//! Testes de ponta a ponta da janela, pelo WebDriver do `tauri-driver`. Rodam só pelo
//! `scripts/e2e.sh` (que sobe o Xvfb, o barramento, o serviço de teste e o driver).

use std::time::{Duration, Instant};

use gdl_ipc::{ServiceProxy, Status, UiProxy};
use serde_json::{Value, json};

/// A chave do elemento nas respostas do WebDriver.
const ELEMENT: &str = "element-6066-11e4-a52e-4f735466cecf";

struct Session {
    http: reqwest::Client,
    base: String,
}

impl Session {
    /// Abre a janela com `--open <screen>`.
    async fn open(screen: &str) -> Session {
        let driver = std::env::var("GDL_E2E_WEBDRIVER").expect("rode pelo scripts/e2e.sh");
        let http = reqwest::Client::new();
        let caps = json!({ "capabilities": { "alwaysMatch": { "tauri:options": {
            "application": env!("CARGO_BIN_EXE_gdrive-desktop-linux-ui"),
            "args": ["--open", screen],
        }}}});
        let reply: Value = http
            .post(format!("{driver}/session"))
            .json(&caps)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        let id = reply["value"]["sessionId"]
            .as_str()
            .unwrap_or_else(|| panic!("sessão não abriu: {reply}"));
        Session {
            http,
            base: format!("{driver}/session/{id}"),
        }
    }

    /// `method` em `path` (relativo à sessão) e o `value` da resposta.
    async fn request(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Value {
        let mut request = self.http.request(method, format!("{}/{path}", self.base));
        if let Some(body) = body {
            request = request.json(&body);
        }
        let reply = request.send().await.unwrap();
        reply.json::<Value>().await.unwrap()["value"].take()
    }

    async fn post(&self, path: &str, body: Value) -> Value {
        self.request(reqwest::Method::POST, path, Some(body)).await
    }

    /// O elemento `css`, esperando ele aparecer (a tela monta depois de carregar).
    async fn find(&self, css: &str) -> String {
        for _ in 0..100 {
            let found = self
                .post("element", json!({ "using": "css selector", "value": css }))
                .await;
            if let Some(id) = found[ELEMENT].as_str() {
                return id.to_string();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("não apareceu: {css}");
    }

    /// O texto renderizado de `css` (`innerText`). O "texto do elemento" do WebDriver dá vazio
    /// para um nome cortado com reticências (`overflow: hidden`), que ele julga escondido.
    async fn text(&self, css: &str) -> String {
        let id = self.find(css).await;
        let text = self
            .post(
                "execute/sync",
                json!({ "script": "return arguments[0].innerText", "args": [{ ELEMENT: id }] }),
            )
            .await;
        text.as_str().unwrap_or_default().to_string()
    }

    /// Espera o texto de `css` conter `wanted`.
    async fn wait_text(&self, css: &str, wanted: &str) -> String {
        let mut last = String::new();
        for _ in 0..150 {
            last = self.text(css).await;
            if last.contains(wanted) {
                return last;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("{css}: esperava {wanted:?}, ficou {last:?}");
    }

    /// Digita `text` no campo `css`.
    async fn fill(&self, css: &str, text: &str) {
        let id = self.find(css).await;
        self.post(&format!("element/{id}/value"), json!({ "text": text }))
            .await;
    }

    async fn click(&self, css: &str) {
        let id = self.find(css).await;
        self.post(&format!("element/{id}/click"), json!({})).await;
    }

    /// A janela que apareceu depois da primeira.
    async fn new_window(&self) -> String {
        let first = self.request(reqwest::Method::GET, "window", None).await;
        for _ in 0..150 {
            let handles = self
                .request(reqwest::Method::GET, "window/handles", None)
                .await;
            let other = handles
                .as_array()
                .into_iter()
                .flatten()
                .find(|h| **h != first);
            if let Some(handle) = other.and_then(Value::as_str) {
                return handle.to_string();
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("a janela nova não apareceu");
    }

    /// Roda `script` na página (uma promessa é esperada).
    async fn eval(&self, script: &str) -> Value {
        self.post("execute/sync", json!({ "script": script, "args": [] }))
            .await
    }
}

/// Fecha a sessão mesmo se o teste falhou no meio, e espera a janela soltar o nome no barramento
/// (senão a próxima sessão daria com ela e sairia como segunda instância).
impl Drop for Session {
    fn drop(&mut self) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let _ = self.http.delete(&self.base).send().await;
                let conn = zbus::Connection::session().await.unwrap();
                let dbus = zbus::fdo::DBusProxy::new(&conn).await.unwrap();
                for _ in 0..100 {
                    let name = gdl_ipc::UI_BUS_NAME.try_into().unwrap();
                    if !dbus.name_has_owner(name).await.unwrap_or(false) {
                        return;
                    }
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
            })
        });
    }
}
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn about_window_opens_and_shows_version() {
    let session = Session::open("about").await;

    let version = session.text(".about-version").await;
    let license = session.text(".about-license").await;
    let unofficial = session.text(".about-unofficial").await;
    // A fonte embutida carregou mesmo com a CSP restrita.
    let inter = session
        .eval(
            "return document.fonts.ready.then(fs => \
             [...fs].some(f => f.family.includes('Inter') && f.status === 'loaded'))",
        )
        .await;

    assert!(
        version.contains(env!("CARGO_PKG_VERSION")),
        "versão: {version}"
    );
    assert!(license.contains("GPL-3.0"), "licença: {license}");
    assert!(
        unofficial.to_lowercase().contains("não oficial")
            || unofficial.to_lowercase().contains("unofficial"),
        "aviso: {unofficial}"
    );
    assert_eq!(inter, Value::Bool(true));
}

/// O serviço de teste, pelo barramento do `scripts/e2e.sh`.
async fn service() -> ServiceProxy<'static> {
    let conn = zbus::Connection::session().await.unwrap();
    ServiceProxy::new(&conn).await.unwrap()
}

/// Espera o `Status` do serviço satisfazer `done`.
async fn status_until(service: &ServiceProxy<'_>, what: &str, done: impl Fn(&Status) -> bool) {
    for _ in 0..150 {
        if done(&service.status().await.unwrap()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("esperando: {what}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn home_shows_synced_and_activity() {
    let session = Session::open("home").await;

    session
        .wait_text(".header-title", "Sincronização concluída")
        .await;
    session.click("[data-view='activity']").await;
    // O arquivo que o serviço de teste gravou pela montagem.
    let row = session
        .wait_text(".activity-table tbody", "enviado-pelo-teste.txt")
        .await;

    assert!(row.contains("Enviado"), "{row}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn pause_button_toggles() {
    let service = service().await;
    let session = Session::open("home").await;
    session.wait_text(".header-pause", "Pausar").await;

    session.click(".header-pause").await;
    session
        .wait_text(".header-title", "Sincronização pausada")
        .await;
    let paused = service.status().await.unwrap().paused;
    session.click(".header-pause").await;
    session.wait_text(".header-pause", "Pausar").await;

    assert!(paused);
    assert!(!service.status().await.unwrap().paused);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn quit_asks_when_syncing() {
    let service = service().await;
    // Pausado, um arquivo novo fica esperando o envio.
    service.pause().await.unwrap();
    let mount = service.status().await.unwrap().mount_point;
    let file = std::path::Path::new(&mount).join("Meu Drive/na-fila.txt");
    tokio::task::spawn_blocking(move || std::fs::write(file, b"esperando"))
        .await
        .unwrap()
        .unwrap();
    status_until(&service, "envio na fila", |s| s.pending_uploads > 0).await;
    let session = Session::open("home").await;

    session.click(".more-button").await;
    session.click("[data-action='quit']").await;
    let question = session.text("dialog[open] .dialog-body").await;
    session.click("dialog[open] .dialog-cancel").await;
    let still_up = service.status().await.is_ok();
    service.resume().await.unwrap();

    assert!(question.contains("1"), "{question}");
    assert!(still_up);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn onboarding_logs_in_with_client_id() {
    let service = service().await;
    service.logout("discard", "").await.unwrap();
    status_until(&service, "sem conta", |s| s.state == "signed-out").await;
    let session = Session::open("onboarding").await;

    // Sem o segredo, Continuar não sai do lugar.
    session.fill("#client-id", "id-de-teste").await;
    session.click(".continue").await;
    let missing = session.text(".step-error").await;
    session.fill("#client-secret", "segredo").await;
    session.click(".continue").await;
    session.click(".generate-link").await;
    let mut url = Value::Null;
    for _ in 0..100 {
        url = session
            .eval("return document.querySelector('.login-url')?.value || null")
            .await;
        if url.is_string() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    // O teste faz o papel do navegador.
    reqwest::get(url.as_str().expect("sem o link de login"))
        .await
        .unwrap();
    let title = session.wait_text(".step-title", "computador").await;
    let account = service.status().await.unwrap().account;

    assert!(missing.contains("Client ID"), "{missing}");
    assert!(title.contains("Seu Drive"), "{title}");
    assert!(!account.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn opens_in_under_1500ms() {
    let session = Session::open("about").await;
    session.text(".about-version").await;
    let conn = zbus::Connection::session().await.unwrap();
    let ui = UiProxy::new(&conn).await.unwrap();

    let start = Instant::now();
    ui.open("home").await.unwrap();
    let handle = session.new_window().await;
    session.post("window", json!({ "handle": handle })).await;
    // O cabeçalho e os cartões do Início: o que o usuário espera ver.
    let title = session.text(".header-title").await;
    let cards = session.text(".view .cards").await;
    let elapsed = start.elapsed();

    assert!(!title.is_empty());
    assert!(!cards.is_empty());
    assert!(elapsed < Duration::from_millis(1500), "{elapsed:?}");
}
