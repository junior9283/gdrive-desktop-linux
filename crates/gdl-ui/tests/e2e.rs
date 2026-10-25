//! Testes de ponta a ponta da janela, pelo WebDriver do `tauri-driver`. Rodam só pelo
//! `scripts/e2e.sh` (que sobe o Xvfb, o barramento, o serviço de teste e o driver).

use std::time::Duration;

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

    /// O texto do elemento `css`, esperando ele aparecer (a tela monta depois de carregar).
    async fn text(&self, css: &str) -> String {
        for _ in 0..100 {
            let found = self
                .post("element", json!({ "using": "css selector", "value": css }))
                .await;
            if let Some(id) = found[ELEMENT].as_str() {
                let text = self
                    .request(reqwest::Method::GET, &format!("element/{id}/text"), None)
                    .await;
                return text.as_str().unwrap_or_default().to_string();
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!("não apareceu: {css}");
    }

    /// Roda `script` na página (uma promessa é esperada).
    async fn eval(&self, script: &str) -> Value {
        self.post("execute/sync", json!({ "script": script, "args": [] }))
            .await
    }

    async fn close(self) {
        let _ = self.http.delete(&self.base).send().await;
    }
}

#[tokio::test]
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
    session.close().await;

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
