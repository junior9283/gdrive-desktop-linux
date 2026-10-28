//! Testes de ponta a ponta da janela, pelo WebDriver do `tauri-driver`. Rodam só pelo
//! `scripts/e2e.sh` (que sobe o Xvfb, o barramento, o serviço de teste e o driver).

use std::time::{Duration, Instant};

use gdl_ipc::{ServiceProxy, Status, UiProxy};
use serde_json::{Value, json};

/// A chave do elemento nas respostas do WebDriver.
const ELEMENT: &str = "element-6066-11e4-a52e-4f735466cecf";
/// As teclas Enter e seta para baixo no WebDriver.
const ENTER: &str = "\u{E007}";
const DOWN: &str = "\u{E015}";

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

    /// A janela em PNG, sem animação no meio e com a fonte embutida carregada.
    async fn screenshot(&self) -> Vec<u8> {
        self.eval(
            "return document.fonts.ready.then(() => \
             Promise.all(document.getAnimations().map(a => a.finished)))",
        )
        .await;
        let png = self.request(reqwest::Method::GET, "screenshot", None).await;
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(png.as_str().expect("captura"))
            .unwrap()
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
        let page = self
            .eval("return location.href + ' | ' + document.body.innerText.slice(0, 300)")
            .await;
        panic!("não apareceu: {css}; a janela mostrava {page}");
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

    /// Troca o conteúdo do campo `css` por `text`.
    async fn fill(&self, css: &str, text: &str) {
        let id = self.find(css).await;
        self.post(&format!("element/{id}/clear"), json!({})).await;
        self.post(&format!("element/{id}/value"), json!({ "text": text }))
            .await;
    }

    /// Digita `keys` no elemento `css`, sem apagar o que ele tem (`ENTER`, `DOWN`…).
    async fn keys(&self, css: &str, keys: &str) {
        let id = self.find(css).await;
        self.post(&format!("element/{id}/value"), json!({ "text": keys }))
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

    /// Passa a mostrar a conta `email` (vazio: volta à primeira) e recarrega a tela.
    async fn show_account(&self, email: &str) {
        let script = match email {
            "" => "localStorage.removeItem('account'); return null".to_string(),
            email => {
                format!(
                    "localStorage.setItem('account', '{email}'); location.reload(); return null"
                )
            }
        };
        self.eval(&script).await;
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

/// A barra de tarefas e a barra de título acham o ícone pela classe da janela (o app_id no
/// Wayland), que tem de ser o nome do `.desktop`; com o nome do programa, ficam sem ícone.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn window_class_is_the_desktop_file_id() {
    let session = Session::open("about").await;
    session.text(".about-version").await;

    // A janela aparece no servidor gráfico um pouco depois da página.
    until("uma janela com a classe do id do app", async || {
        let found = std::process::Command::new("xdotool")
            .args(["search", "--class", &format!("^{}$", gdl_common::APP_ID)])
            .output()
            .unwrap();
        (!found.stdout.trim_ascii().is_empty()).then_some(())
    })
    .await;
}

/// O serviço de teste, pelo barramento do `scripts/e2e.sh`.
async fn service() -> ServiceProxy<'static> {
    let conn = zbus::Connection::session().await.unwrap();
    ServiceProxy::new(&conn).await.unwrap()
}

/// Espera o `Status` do serviço satisfazer `done`.
async fn status_until(service: &ServiceProxy<'_>, what: &str, done: impl Fn(&Status) -> bool) {
    until(what, async || {
        done(&service.status("").await.unwrap()).then_some(())
    })
    .await;
}

/// Espera `probe` dar `Some` (até 15 s, de 100 em 100 ms) e devolve o valor.
async fn until<T>(what: &str, mut probe: impl AsyncFnMut() -> Option<T>) -> T {
    for _ in 0..150 {
        if let Some(value) = probe().await {
            return value;
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
async fn account_button_shows_photo_without_click() {
    let session = Session::open("home").await;

    let src = until("a foto no botão da conta", async || {
        let src = session
            .eval("return document.querySelector('.account-button img.avatar')?.src ?? null")
            .await;
        src.as_str().map(str::to_string)
    })
    .await;

    assert!(src.starts_with("data:image/png;base64,"), "{src}");
}

/// A conta que o `e2e-daemon` deixa para o segundo login.
const ANA: &str = "ana@example.com";

/// Entra na segunda conta pelo D-Bus; ela sai quando o guarda é solto.
async fn sign_in_ana(service: &ServiceProxy<'_>) -> SecondAccount {
    let guard = SecondAccount;
    let url = service
        .begin_login("id-de-teste", "segredo", ANA)
        .await
        .unwrap();
    reqwest::get(url).await.unwrap();
    status_until(service, "a segunda conta", |s| s.accounts.len() == 2).await;
    guard
}

/// Grava `name` no Meu Drive da conta `account` (vazio: a primeira), pela montagem.
async fn write_in_my_drive(service: &ServiceProxy<'_>, account: &str, name: &str) {
    let mount = service.status(account).await.unwrap().mount_point;
    let file = std::path::Path::new(&mount).join("Meu Drive").join(name);
    let body = name.as_bytes().to_vec();
    tokio::task::spawn_blocking(move || std::fs::write(file, body))
        .await
        .unwrap()
        .unwrap();
}

/// Desconecta a segunda conta ao sair, mesmo se o teste falhou no meio: os outros testes contam
/// com uma conta só.
struct SecondAccount;

impl Drop for SecondAccount {
    fn drop(&mut self) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                let _ = service().await.logout(ANA, "discard", "").await;
            })
        });
    }
}

/// O menu da conta lista as outras contas e deixa adicionar uma: "Adicionar outra conta" abre o
/// primeiro uso em modo "adicionar" (sem o guia do Client ID), a conta nova entra na lista e
/// clicar nela troca a conta mostrada pela janela.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn account_menu_switches_and_adds_accounts() {
    let _cleanup = SecondAccount;
    let session = Session::open("home").await;
    session
        .wait_text(".header-title", "Sincronização concluída")
        .await;
    let home = session.request(reqwest::Method::GET, "window", None).await;
    session.click(".account-button").await;
    session.find("[data-action='add-account']").await;
    let rows_before = session
        .eval("return document.querySelectorAll('.menu .account-row').length")
        .await;

    session.click("[data-action='add-account']").await;
    let add = session.new_window().await;
    session.post("window", json!({ "handle": add })).await;
    let title = session.wait_text(".step-title", "outra conta").await;
    session.click(".continue").await;
    until("o link de login", async || {
        let ready = session
            .eval("return document.querySelector('.generate-link')?.disabled === false")
            .await;
        ready.as_bool().unwrap_or(false).then_some(())
    })
    .await;
    session.click(".generate-link").await;
    let url = until("o link na tela", async || {
        let url = session
            .eval("return document.querySelector('.login-url')?.value || null")
            .await;
        url.as_str().map(str::to_string)
    })
    .await;
    // O teste faz o papel do navegador.
    reqwest::get(url).await.unwrap();
    // A janela do "adicionar" fecha sozinha quando a conta entra.
    until("a janela de adicionar fechar", async || {
        let handles = session
            .request(reqwest::Method::GET, "window/handles", None)
            .await;
        (handles.as_array()?.len() == 1).then_some(())
    })
    .await;
    session.post("window", json!({ "handle": home })).await;

    until("a conta nova na lista", async || {
        session.click(".account-button").await;
        let rows = session
            .eval("return document.querySelectorAll('.menu .account-row').length")
            .await;
        (rows == json!(1)).then_some(())
    })
    .await;
    let row = session.text(".menu .account-row").await;
    session
        .click(&format!("[data-action='switch-{ANA}']"))
        .await;
    // A janela inteira passa a mostrar a conta escolhida: o cartão do menu é dela.
    until("o cartão da conta escolhida", async || {
        session.click(".account-button").await;
        let card = session.text(".menu .account-card").await;
        card.contains(ANA).then_some(())
    })
    .await;
    // De volta à primeira (que passa a ser "outra" na lista).
    let first = service().await.status("").await.unwrap().account;
    session
        .click(&format!("[data-action='switch-{first}']"))
        .await;

    assert!(title.contains("Adicione"), "{title}");
    assert_eq!(rows_before, json!(0), "uma conta só não lista outras");
    assert!(row.contains(ANA), "{row}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn account_menu_stays_inside_window() {
    let session = Session::open("home").await;
    // Janela larga: o botão da conta fica colado na borda direita, como em tela cheia.
    session
        .post("window/rect", json!({ "width": 1900, "height": 1000 }))
        .await;
    session.click(".account-button").await;
    // O cartão alarga quando chegam o nome, a foto e o uso.
    session
        .wait_text(".menu .account-card", "Pessoa de Teste")
        .await;

    // O cartão pode alargar de novo depois da primeira medida: vale a posição depois do
    // reposicionamento, e sem ele o menu fica para fora até o tempo acabar.
    until("o menu dentro da janela", async || {
        let rect = session
            .eval(
                "const r = document.querySelector('.menu').getBoundingClientRect(); \
                 return [r.left, r.right, window.innerWidth]",
            )
            .await;
        let [left, right, width] = serde_json::from_value::<[f64; 3]>(rect).ok()?;
        (left >= 0.0 && right <= width).then_some(())
    })
    .await;
}

/// Os programas de teste não vêm de uma instalação: a CLI recusa, e a tela mostra o motivo e
/// devolve os botões (nada é removido).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn uninstall_screen_shows_why_it_could_not_uninstall() {
    let session = Session::open("uninstall").await;
    session.click("[data-action=uninstall]").await;

    session
        .wait_text(
            ".uninstall-progress",
            "Não foi possível saber como o app foi instalado",
        )
        .await;
    let disabled = session
        .eval("return document.querySelector('[data-action=uninstall]').disabled")
        .await;
    assert_eq!(disabled, json!(false));
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
    let paused = service.status("").await.unwrap().paused;
    session.click(".header-pause").await;
    session.wait_text(".header-pause", "Pausar").await;

    assert!(paused);
    assert!(!service.status("").await.unwrap().paused);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn quit_asks_when_syncing() {
    let service = service().await;
    // Pausado, um arquivo novo fica esperando o envio.
    service.pause().await.unwrap();
    let mount = service.status("").await.unwrap().mount_point;
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
    let still_up = service.status("").await.is_ok();
    service.resume().await.unwrap();

    assert!(question.contains("1"), "{question}");
    assert!(still_up);
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn onboarding_logs_in_with_client_id() {
    let service = service().await;
    service.logout("", "discard", "").await.unwrap();
    status_until(&service, "sem conta", |s| s.state == "signed-out").await;
    let session = Session::open("onboarding").await;

    // Primeiro o guia do Client ID: cada parte abre a sua página do Google Cloud.
    let guide = session.wait_text(".step-title", "Client ID").await;
    let parts = session
        .eval("return document.querySelectorAll('.guide-part').length")
        .await;
    let troubles = session
        .eval("return document.querySelectorAll('.troubles dt').length")
        .await;
    // Os valores da tela de consentimento vêm com o campo onde colar; os escopos, sem rótulo.
    let labels = session
        .eval("return [...document.querySelectorAll('.copy-label')].map((l) => l.textContent)")
        .await;
    let guide_text = session
        .eval("return document.querySelector('.guide').textContent")
        .await;
    session.click(".guide-part button.tonal").await;
    let project = opened("console.cloud.google.com/projectcreate").await;
    session.click(".continue").await;

    // Sem o segredo, Continuar não sai do lugar.
    session.fill("#client-id", "id-de-teste").await;
    session.click(".continue").await;
    let missing = session.text(".step-error").await;
    session.fill("#client-secret", "segredo").await;
    session.click(".continue").await;
    // O botão só vale com o link pedido ao serviço; antes disso, o clique se perde.
    until("o link de login", async || {
        let ready = session
            .eval("return document.querySelector('.generate-link')?.disabled === false")
            .await;
        ready.as_bool().unwrap_or(false).then_some(())
    })
    .await;
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
    let account = service.status("").await.unwrap().account;

    assert!(guide.contains("Google Cloud"), "{guide}");
    assert_eq!(parts, 6);
    assert_eq!(troubles, 5, "os problemas comuns viram itens da lista");
    assert_eq!(
        labels,
        json!([
            "Página inicial",
            "Política de privacidade",
            "Termos de serviço",
            "Domínio autorizado"
        ])
    );
    assert!(
        !guide_text.as_str().unwrap().contains("undefined"),
        "{guide_text}"
    );
    assert_eq!(project, "https://console.cloud.google.com/projectcreate");
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

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn about_lists_third_party_licenses() {
    let session = Session::open("about").await;

    session.click(".third-party summary").await;
    let list = session.wait_text(".licenses", "tauri").await;

    assert!(list.contains("OFL-1.1"), "a fonte Inter");
    assert!(list.contains("MIT"), "{list}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn managed_pref_is_locked() {
    let session = Session::open("preferences").await;

    session.click(".settings-gear").await;
    let row = session
        .wait_text(
            ".pref[data-key='AutoStartOnLogin']",
            "Gerenciado pelo administrador",
        )
        .await;
    let disabled = session
        .eval("return document.querySelector(\".pref[data-key='AutoStartOnLogin'] input\").matches(':disabled')")
        .await;
    let free = session
        .eval("return document.querySelector(\".pref[data-key='PauseSyncingOnMeteredConnection'] input\").matches(':disabled')")
        .await;

    assert!(row.contains("Iniciar"), "{row}");
    assert_eq!(disabled, Value::Bool(true));
    assert_eq!(free, Value::Bool(false));
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn saving_restart_key_shows_restart_banner() {
    let service = service().await;
    let session = Session::open("preferences").await;

    session.click(".settings-gear").await;
    session
        .fill(".pref[data-key='ContentCacheMaxKbytes'] input", "2")
        .await;
    let pending = session.text(".pending-changes").await;
    session.click(".save").await;
    let banner = session
        .wait_text(".restart-banner", "Reiniciar agora")
        .await;
    let prefs = service.preferences().await.unwrap();
    service.reset_preferences().await.unwrap();

    let saved = prefs
        .iter()
        .find(|p| p.key == "ContentCacheMaxKbytes")
        .unwrap();
    assert!(pending.contains("pendentes"), "{pending}");
    assert!(banner.contains("reiniciar"), "{banner}");
    assert_eq!(saved.value, (2u64 * 1024 * 1024).to_string());
}

/// Espera o "navegador" do serviço de teste anotar um endereço ou caminho que contenha `wanted`.
async fn opened(wanted: &str) -> String {
    let file = std::env::var("GDL_E2E_OPENED").expect("rode pelo scripts/e2e.sh");
    until(&format!("o serviço abrir {wanted:?}"), async || {
        let opened = std::fs::read_to_string(&file).unwrap_or_default();
        opened
            .lines()
            .find(|l| l.contains(wanted))
            .map(str::to_string)
    })
    .await
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn search_finds_by_name_and_opens() {
    // Um arquivo lido pela montagem fica entre os recentes (o teste do primeiro uso, que roda
    // antes, refaz a conta e zera o banco local).
    let mount = service().await.status("").await.unwrap().mount_point;
    let notes = std::path::Path::new(&mount).join("Meu Drive/notas.txt");
    tokio::task::spawn_blocking(move || std::fs::read(notes))
        .await
        .unwrap()
        .unwrap();
    let session = Session::open("search").await;
    // Campo vazio: as sugestões recentes.
    let recent = session.wait_text(".results", "notas.txt").await;

    session.fill(".search-field", "relat").await;
    let results = session.wait_text(".results", "relatório.pdf").await;
    session.keys(".search-field", DOWN).await;
    session.keys(".search-field", ENTER).await;
    let path = opened("relatório.pdf").await;

    assert!(results.contains("Documentos"), "o caminho: {results}");
    assert!(recent.contains("Recentes"), "{recent}");
    assert!(
        path.ends_with("Google Drive/Meu Drive/Documentos/relatório.pdf"),
        "{path}"
    );
}

/// Enter logo depois de digitar, antes da pesquisa voltar: abre o resultado do texto novo, não o
/// primeiro da lista que ainda estava na tela.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn search_enter_right_after_typing_opens_the_new_result() {
    let session = Session::open("search").await;
    session.wait_text(".results", "Recentes").await;

    session.fill(".search-field", "inventario").await;
    session.keys(".search-field", ENTER).await;

    assert!(
        opened("inventario.ods")
            .await
            .ends_with("Meu Drive/inventario.ods")
    );
}

/// O armazenamento do Drive simulado cheio enquanto existir (sinais do `e2e-daemon`). Ao sair,
/// libera o espaço e refaz os envios barrados, para um teste que falhou no meio não deixar a
/// cota cheia para os outros.
struct QuotaFull;

impl QuotaFull {
    async fn start() -> QuotaFull {
        QuotaFull::set("USR1", "full").await;
        QuotaFull
    }

    /// Manda `signal` ao serviço de teste e espera ele confirmar `state` em `$GDL_E2E_QUOTA`.
    async fn set(signal: &str, state: &str) {
        let var = |name| std::env::var(name).expect("rode pelo scripts/e2e.sh");
        let (pid, file) = (var("GDL_E2E_DAEMON_PID"), var("GDL_E2E_QUOTA"));
        let status = std::process::Command::new("kill")
            .args([&format!("-{signal}"), &pid])
            .status()
            .unwrap();
        assert!(status.success(), "kill -{signal} {pid}");
        until(&format!("armazenamento {state}"), async || {
            (std::fs::read_to_string(&file).ok()? == state).then_some(())
        })
        .await;
    }
}

impl Drop for QuotaFull {
    fn drop(&mut self) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async {
                QuotaFull::set("USR2", "free").await;
                let service = service().await;
                for error in service.errors("").await.unwrap_or_default() {
                    if error.category == "quota" {
                        let _ = service.retry("", &error.item_id).await;
                    }
                }
            })
        });
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn errors_list_groups_and_retries() {
    let service = service().await;
    let _quota = QuotaFull::start().await;
    let mount = service.status("").await.unwrap().mount_point;
    let file = std::path::Path::new(&mount).join("Meu Drive/sem-espaco.txt");
    tokio::task::spawn_blocking(move || std::fs::write(file, b"cheio"))
        .await
        .unwrap()
        .unwrap();
    status_until(&service, "erro de cota", |s| s.errors >= 1).await;
    let session = Session::open("errors").await;

    let group = session
        .wait_text(".error-group[data-category='quota']", "sem-espaco.txt")
        .await;
    // Só libera o espaço: quem refaz o envio é o "Tentar de novo" da tela.
    QuotaFull::set("USR2", "free").await;
    session
        .click(".error-group[data-category='quota'] .row-menu")
        .await;
    session.click("[data-action='retry']").await;
    status_until(&service, "envio refeito", |s| {
        s.errors == 0 && s.pending_uploads == 0
    })
    .await;
    let empty = session.wait_text(".errors", "Nenhum erro").await;

    assert!(group.contains("Armazenamento"), "{group}");
    assert!(!empty.contains("sem-espaco.txt"), "{empty}");
}

/// "Redefinir os dados locais" age na conta mostrada pela janela: com um envio parado na primeira
/// (que recusaria) e a segunda aberta, a segunda é redefinida.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn reset_local_data_uses_the_shown_account() {
    let service = service().await;
    let _cleanup = sign_in_ana(&service).await;
    let _quota = QuotaFull::start().await;
    write_in_my_drive(&service, "", "parado.txt").await;
    status_until(&service, "o envio parado na primeira", |s| {
        s.pending_uploads >= 1
    })
    .await;
    let session = Session::open("home").await;
    session.show_account(ANA).await;
    session.wait_text(".header-title", "").await;

    session.click(".more-button").await;
    session.click("[data-action='reset-local']").await;
    let question = session.wait_text("dialog[open] .dialog-body", ANA).await;
    session.click("dialog[open] .actions .filled").await;
    let toast = session.wait_text(".toast", "").await;
    session.show_account("").await;

    assert!(question.contains(ANA), "{question}");
    assert!(toast.contains("redefinidos"), "{toast}");
}

/// "Terminar de sincronizar e depois desconectar" não diz "Conta desconectada" enquanto o envio
/// está parado: diz que está terminando, e só avisa a desconexão quando a conta sai.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn disconnect_after_sync_reports_when_done() {
    let service = service().await;
    let _cleanup = sign_in_ana(&service).await;
    let quota = QuotaFull::start().await;
    write_in_my_drive(&service, ANA, "parado.txt").await;
    until("o envio parado na segunda", async || {
        (service.status(ANA).await.ok()?.pending_uploads >= 1).then_some(())
    })
    .await;
    let session = Session::open("home").await;
    session.show_account(ANA).await;
    session.wait_text(".header-title", "").await;

    session.click(".more-button").await;
    session.click("[data-action='disconnect']").await;
    session.find("dialog[open] input[value='finish']").await;
    session.click("dialog[open] .actions .filled").await;
    let finishing = session.wait_text("body", "Terminando").await;
    let still_there = service.status("").await.unwrap().accounts.len();
    drop(quota);
    for error in service.errors(ANA).await.unwrap_or_default() {
        let _ = service.retry(ANA, &error.item_id).await;
    }
    let done = session.wait_text("body", "Conta desconectada").await;
    session.show_account("").await;

    assert!(!finishing.contains("Conta desconectada"), "cedo demais");
    assert_eq!(still_there, 2, "a conta fica até o envio terminar");
    assert!(done.contains("desconectada"), "{done}");
}

/// Mostrando outra conta, a lista de erros aponta os erros da primeira numa faixa, e "Trocar de
/// conta" passa a mostrá-los.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn errors_screen_points_to_the_other_account() {
    let service = service().await;
    let _cleanup = sign_in_ana(&service).await;
    let _quota = QuotaFull::start().await;
    let first = service.status("").await.unwrap();
    write_in_my_drive(&service, "", "sem-espaco.txt").await;
    status_until(&service, "erro de cota na primeira", |s| {
        s.accounts
            .iter()
            .any(|a| a.email == first.account && a.errors >= 1)
    })
    .await;
    let session = Session::open("errors").await;
    session.show_account(ANA).await;

    let banner = session.wait_text(".other-account", &first.account).await;
    session.click(".other-account .switch-account").await;
    let group = session
        .wait_text(".error-group[data-category='quota']", "sem-espaco.txt")
        .await;
    let banners = session
        .eval("return document.querySelectorAll('.other-account').length")
        .await;
    session.show_account("").await;

    assert!(banner.contains("Trocar de conta"), "{banner}");
    assert!(group.contains("Armazenamento"), "{group}");
    assert_eq!(banners, json!(0), "a conta mostrada não aponta para si");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn offline_screen_removes_offline_access() {
    let service = service().await;
    let session = Session::open("offline").await;

    let usage = session.wait_text(".usage", "Off-line").await;
    session.wait_text(".offline-items", "notas.txt").await;
    session
        .click(".offline-items [data-name='notas.txt'] .remove-offline")
        .await;
    let after = session.wait_text(".offline", "Nenhum arquivo").await;

    assert!(usage.contains("Livre"), "{usage}");
    assert_eq!(service.offline_summary("").await.unwrap().pinned_count, 0);
    assert!(!after.contains("notas.txt"), "{after}");
}

/// O id do item `name` do Drive de teste (pela pesquisa do serviço).
async fn item_id(name: &str) -> String {
    let found = service().await.search("", name).await.unwrap();
    found
        .into_iter()
        .find(|r| r.name == name)
        .unwrap_or_else(|| panic!("{name} não está no Drive de teste"))
        .id
}

/// Escolhe `value` no `<select>` de `css`, como o usuário.
async fn choose(session: &Session, css: &str, value: &str) {
    session
        .eval(&format!(
            "const s = document.querySelector({css:?}); s.value = {value:?}; \
             s.dispatchEvent(new Event('change'));"
        ))
        .await;
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn share_screen_adds_people_and_opens_the_link() {
    let id = item_id("relatório.pdf").await;
    let session = Session::open(&format!("share:{id}")).await;
    session.wait_text(".people", "Proprietário").await;

    session.fill(".emails", "ana@example.com").await;
    session.click(".send").await;
    session.wait_text(".people", "ana@example.com").await;
    let typed = session
        .eval("return document.querySelector('.emails').value")
        .await;
    choose(&session, ".general-kind", "anyone").await;
    let general = session.wait_text(".general", "internet").await;

    assert_eq!(typed, "", "o campo fica livre depois de enviar");
    assert!(general.contains("Qualquer pessoa"), "{general}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn versions_screen_lists_and_keeps_a_version() {
    let id = item_id("notas.txt").await;
    let session = Session::open(&format!("versions:{id}")).await;
    session.wait_text(".version-list", "Atual").await;

    session.click(".version-list li:last-child .row-menu").await;
    session.click(".menu [data-action='keep']").await;
    let list = session
        .wait_text(".version-list", "Mantida para sempre")
        .await;

    assert_eq!(list.matches("Atual").count(), 1, "{list}");
}

/// O que `SharedDrives` diz da conta principal: `(nome, ligado)` de cada drive.
async fn drives(service: &ServiceProxy<'_>) -> Vec<(String, bool)> {
    let list = service.shared_drives("").await.unwrap();
    list.into_iter().map(|d| (d.name, d.enabled)).collect()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn shared_drives_screen_saves_and_creates() {
    let service = service().await;
    let team = service.shared_drives("").await.unwrap()[0].id.clone();
    let session = Session::open("shared-drives").await;
    session.wait_text(".drive-list", "Equipe").await;
    let idle = session
        .eval("return document.querySelector('.save').disabled")
        .await;
    assert_eq!(idle, json!(true), "sem mudança, não há o que salvar");

    // Desliga o drive e salva: o serviço passa a ter o drive desligado.
    session.click(&format!("[data-drive='{team}'] input")).await;
    session.click(".save").await;
    session.wait_text(".toast", "Alterações salvas").await;
    let after_save = drives(&service).await;

    // Cria um drive pelo diálogo: ele entra na lista e no serviço, ligado.
    session.click(".new-drive").await;
    session.fill(".dialog .drive-name", "Nova equipe").await;
    session.click(".dialog .filled").await;
    session.wait_text(".drive-list", "Nova equipe").await;
    let after_create = drives(&service).await;

    // Deixa o Drive de teste como estava para os outros testes.
    service.set_shared_drives("", &[&team]).await.unwrap();
    assert_eq!(after_save, [("Equipe".to_string(), false)]);
    assert_eq!(
        after_create,
        [
            ("Equipe".to_string(), false),
            ("Nova equipe".to_string(), true)
        ]
    );
}

/// "Gerenciar drives compartilhados" da bandeja abre sem conta: com duas, a tela começa na conta
/// mostrada na janela e deixa escolher a outra.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn tray_shared_drives_lets_pick_the_account() {
    let service = service().await;
    let _cleanup = sign_in_ana(&service).await;
    let first = service.status("").await.unwrap().account;
    let session = Session::open("shared-drives").await;
    session.show_account(ANA).await;
    session.wait_text(".drive-list", "Equipe").await;
    let picked = "return document.querySelector('.drive-account')?.value ?? null";
    let shown = session.eval(picked).await;

    session
        .eval(&format!(
            "const s = document.querySelector('.drive-account'); s.value = '{first}'; \
             s.dispatchEvent(new Event('change')); return null"
        ))
        .await;
    session.wait_text(".drive-list", "Equipe").await;
    let other = session.eval(picked).await;
    session.show_account("").await;
    assert_eq!(shown, json!(ANA), "começa na conta da janela");
    assert_eq!(other, json!(first));
}

/// O passo 5 do primeiro uso: as pastas do `user-dirs.dirs` (o `e2e.sh` deixa só Documentos, com
/// um arquivo), marcadas e sincronizadas ao concluir.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn onboarding_suggests_folders() {
    let service = service().await;
    let documents = std::env::var("GDL_E2E_DOCUMENTS").expect("rode pelo scripts/e2e.sh");
    let session = Session::open("onboarding").await;
    for _ in 0..2 {
        session.click(".continue").await;
    }

    let row = session.wait_text(".suggested-folders", "1 arquivo").await;
    session
        .click(&format!(
            ".suggested-folders [data-path='{documents}'] input"
        ))
        .await;
    session.click(".continue").await;
    let added = until("a pasta sugerida sincronizando", async || {
        let folders = service.local_folders().await.unwrap();
        (!folders.is_empty()).then_some(folders)
    })
    .await;
    // O passo seguinte, o Resumo, lista a pasta escolhida.
    let summary = session.wait_text(".summary", &documents).await;
    for folder in &added {
        service.remove_local_folder("", folder.id).await.unwrap();
    }

    assert!(row.contains("Documentos"), "{row}");
    assert!(summary.contains(&documents), "{summary}");
    assert_eq!(
        added.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        [documents.as_str()]
    );
}

/// A aba "Meu computador" sugere as pastas do usuário que ainda não sincronizam (as mesmas do
/// primeiro uso); "Adicionar" abre o configurador de sempre.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn computer_tab_suggests_folders() {
    let service = service().await;
    let documents = std::env::var("GDL_E2E_DOCUMENTS").expect("rode pelo scripts/e2e.sh");
    let session = Session::open("preferences:computer").await;

    let suggested = session.wait_text(".suggested-folders", "Documentos").await;
    session
        .click(&format!(
            ".suggested-folders [data-path='{documents}'] .add-suggested"
        ))
        .await;
    session
        .wait_text(".dialog .folder-summary", "1 arquivo")
        .await;
    session.click(".dialog .filled").await;
    let added = until("a pasta sugerida sincronizando", async || {
        let folders = service.local_folders().await.unwrap();
        (!folders.is_empty()).then_some(folders)
    })
    .await;
    let gone = session
        .eval("return document.querySelector('.suggested-folders').textContent")
        .await;
    for folder in &added {
        service.remove_local_folder("", folder.id).await.unwrap();
    }

    assert!(suggested.contains("Adicionar"), "{suggested}");
    assert_eq!(
        added.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        [documents.as_str()]
    );
    assert!(!gone.as_str().unwrap().contains("Documentos"), "{gone}");
}

/// Uma pasta do computador nova em `/tmp`, com `a.txt`.
fn computer_folder(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-e2e-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), b"um").unwrap();
    dir
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn drive_tab_switches_to_mirror_and_back() {
    let service = service().await;
    let dir = std::env::temp_dir().join(format!("Espelho-e2e-{}", std::process::id()));
    let path = dir.display().to_string();
    let session = Session::open("preferences:drive").await;

    session
        .click("input[name='drive-mode'][value='mirror']")
        .await;
    session.wait_text(".dialog .folder-summary", "livres").await;
    session
        .eval(&format!(
            "const f = document.querySelector('.mirror-path'); f.value = {path:?}; \
             f.dispatchEvent(new Event('change', {{ bubbles: true }}));"
        ))
        .await;
    session.wait_text(".dialog .folder-summary", "livres").await;
    session.click(".dialog .filled").await;
    session.wait_text(".toast", "sendo espelhado").await;
    let shown = session.wait_text(".drive-folder", &path).await;
    let mirrored = service.mirror("").await.unwrap();

    // A volta espera os envios do Espelho (o Drive de teste traz arquivos para baixar).
    let mut back = false;
    for _ in 0..50 {
        session
            .click("input[name='drive-mode'][value='stream']")
            .await;
        session.click(".dialog .mirror-remove").await;
        session.click(".dialog .filled").await;
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        if service.mirror("").await.unwrap().is_empty() {
            back = true;
            break;
        }
    }
    let removed = !dir.exists();
    let _ = std::fs::remove_dir_all(&dir);

    assert_eq!(mirrored, path);
    assert!(shown.contains(&path), "{shown}");
    assert!(back, "de volta ao Stream");
    assert!(removed, "a pasta antiga vai para a lixeira");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn connected_folders_view_lists_the_pairs() {
    let service = service().await;
    let dir = computer_folder("Projetos");
    let path = dir.display().to_string();
    let id = service.add_local_folder("", &path, false).await.unwrap();
    let home = Session::open("home").await;

    let card = home.wait_text(".cards", "1 pasta sincronizando").await;
    home.click("[data-view='folders']").await;
    let row = home
        .wait_text(&format!(".local-folder[data-path='{path}']"), "Projetos")
        .await;
    service.remove_local_folder("", id).await.unwrap();
    home.wait_text(".view", "Nenhuma pasta").await;
    std::fs::remove_dir_all(&dir).unwrap();

    assert!(card.contains("Pastas conectadas"), "{card}");
    assert!(row.contains(&path), "{row}");
}

/// O seletor do sistema não roda no Xvfb: o configurador abre pelo argumento `folder:<caminho>`
/// (o mesmo do Dolphin).
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn computer_tab_adds_and_stops_a_folder() {
    let service = service().await;
    let dir = computer_folder("Documentos");
    let path = dir.display().to_string();
    let session = Session::open(&format!("preferences:folder:{path}")).await;

    session
        .wait_text(".dialog .folder-summary", "1 arquivo")
        .await;
    session.click(".dialog .filled").await;
    session.wait_text(".toast", "está sendo sincronizada").await;
    let name = dir.file_name().unwrap().to_str().unwrap().to_string();
    session.wait_text(".local-folders", &name).await;
    let added = service.local_folders().await.unwrap();

    session
        .click(&format!(".local-folder[data-path='{path}'] .row-menu"))
        .await;
    session.click("[data-action='stop']").await;
    session.click(".dialog .filled").await;
    session.wait_text(".local-folders", "Nenhuma pasta").await;
    let after = service.local_folders().await.unwrap();
    let kept = dir.join("a.txt").exists();
    std::fs::remove_dir_all(&dir).unwrap();

    assert_eq!(
        added.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        [path.as_str()]
    );
    assert!(after.is_empty(), "{after:?}");
    assert!(kept, "parar não apaga os arquivos");
}

/// O pendrive do serviço de teste na aba "Meu computador": ignorar, remover a escolha e fazer
/// backup lembrando.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn computer_tab_backs_up_a_device() {
    let service = service().await;
    let session = Session::open("preferences:computer").await;
    let row = ".device[data-id='E2E-0001']";
    session.wait_text(".devices", "PENDRIVE").await;

    session.click(&format!("{row} .device-ignore")).await;
    session.wait_text(row, "Ignorado").await;
    session
        .click(&format!("{row}[data-choice='ignored'] .device-forget"))
        .await;
    // A linha redesenhada, sem a escolha.
    session.find(&format!("{row}[data-choice='']")).await;

    session.click(&format!("{row} .device-backup")).await;
    session.click(".dialog .device-remember").await;
    session.click(".dialog .filled").await;
    session.wait_text(".toast", "Fazendo backup").await;
    session.find(&format!("{row}[data-choice='backup']")).await;
    let found = until("a foto no Drive", async || {
        let found = service.search("", "foto.jpg").await.unwrap();
        found
            .into_iter()
            .map(|r| r.path)
            .find(|p| p.contains("PENDRIVE"))
    })
    .await;
    service.forget_device("E2E-0001").await.unwrap();

    assert!(found.contains("Computadores"), "{found}");
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn feedback_screen_opens_a_prefilled_issue() {
    let session = Session::open("feedback").await;
    session.wait_text("h1", "Enviar feedback").await;
    let blocked = session
        .eval("return document.querySelector('.send').disabled")
        .await;

    session.fill(".feedback-text", "A pasta não abre").await;
    session.click(".feedback-kinds input[value=idea]").await;
    session.click(".send").await;
    let url = opened("/issues/new?").await;

    assert_eq!(blocked, json!(true), "sem descrição não envia");
    assert!(
        url.starts_with("https://github.com/junior9283/gdrive-desktop-linux/issues/new?"),
        "{url}"
    );
    assert!(
        url.contains("title=Sugest%C3%A3o%3A+A+pasta+n%C3%A3o+abre&"),
        "{url}"
    );
    assert!(url.contains("body=A+pasta+n%C3%A3o+abre%0A"), "{url}");
    assert!(
        url.contains(env!("CARGO_PKG_VERSION")),
        "a versão vai junto: {url}"
    );
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn shortcut_screen_picks_a_folder() {
    let service = service().await;
    let id = item_id("Planilha.gsheet").await;
    let session = Session::open(&format!("shortcut:{id}")).await;
    let places = session.wait_text(".folder-list", "Equipe").await;
    assert!(places.contains("Meu Drive"), "{places}");
    let blocked = session
        .eval("return document.querySelector('.add').disabled")
        .await;
    assert_eq!(
        blocked,
        json!(true),
        "na lista de locais não há pasta escolhida"
    );

    // Entra no Drive compartilhado (sem subpastas) e cria o atalho nele.
    session.click(".folder-list li:last-child .folder").await;
    session.wait_text(".crumbs", "Equipe").await;
    session.click(".add").await;

    let path = until("o atalho no Drive compartilhado", async || {
        let found = service.search("", "Planilha").await.unwrap();
        found
            .into_iter()
            .map(|r| r.path)
            .find(|p| p.contains("Drives compartilhados/Equipe/"))
    })
    .await;
    // O atalho leva o nome do item, sem a extensão do documento Google (é um link).
    assert!(path.ends_with("Equipe/Planilha"), "{path}");
}

/// A bandeja do `scripts/e2e.sh`, encerrada no fim do teste.
struct TrayProcess(std::process::Child);

impl Drop for TrayProcess {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn tray_publishes_menu() {
    let bin = std::env::var("GDL_E2E_BIN").expect("rode pelo scripts/e2e.sh");
    let _tray = TrayProcess(
        std::process::Command::new(format!("{bin}/gdrive-desktop-linux-tray"))
            .stderr(std::process::Stdio::null())
            .spawn()
            .unwrap(),
    );
    let conn = zbus::Connection::session().await.unwrap();
    let dbus = zbus::fdo::DBusProxy::new(&conn).await.unwrap();

    let item = until("a bandeja publicar o StatusNotifierItem", async || {
        let names = dbus.list_names().await.unwrap();
        names
            .into_iter()
            .find(|n| n.starts_with("org.kde.StatusNotifierItem-"))
    })
    .await;
    // O menu da conta conectada chega quando a bandeja lê o estado do serviço.
    let layout = until("o menu da conta conectada", async || {
        let reply = conn
            .call_method(
                Some(item.as_str()),
                "/MenuBar",
                Some("com.canonical.dbusmenu"),
                "GetLayout",
                &(0i32, -1i32, Vec::<String>::new()),
            )
            .await
            .unwrap();
        let body = reply.body();
        let structure: zbus::zvariant::Structure = body.deserialize().unwrap();
        let layout = zbus::zvariant::Value::from(structure).to_string();
        layout.contains("Arquivos off-line").then_some(layout)
    })
    .await;

    assert!(layout.contains("\"Pesquisar\""), "{layout}");
    assert!(layout.contains("Sair"), "{layout}");
}

/// As imagens do README (`docs/imagens`): `GDL_SCREENSHOTS=docs/imagens scripts/e2e.sh
/// screenshots`. Sem a variável, não faz nada. O primeiro uso vai por último: ele sai da conta.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn screenshots_for_the_readme() {
    let Some(dir) = std::env::var_os("GDL_SCREENSHOTS").map(std::path::PathBuf::from) else {
        return;
    };
    std::fs::create_dir_all(&dir).unwrap();
    let save = async |session: &Session, name: &str| {
        std::fs::write(dir.join(format!("{name}.png")), session.screenshot().await).unwrap();
    };

    let home = Session::open("home").await;
    home.wait_text(".header-title", "Sincronização concluída")
        .await;
    until("a foto da conta", async || {
        let loaded = home
            .eval("return !!document.querySelector('.account-button img.avatar')?.complete")
            .await;
        (loaded == Value::Bool(true)).then_some(())
    })
    .await;
    save(&home, "janela-principal").await;
    home.click(".account-button").await;
    home.wait_text(".menu .account-card", "Pessoa de Teste")
        .await;
    home.find(".menu .account-card img.avatar").await;
    save(&home, "menu-da-conta").await;
    home.click(".header-title").await;
    home.click("[data-view='activity']").await;
    home.wait_text(".activity-table tbody", "enviado-pelo-teste.txt")
        .await;
    save(&home, "atividade").await;
    home.click("[data-view='drives']").await;
    home.wait_text(".drives", "Equipe").await;
    save(&home, "visao-drives").await;
    drop(home);

    let (summary, _) = open_onboarding_summary().await;
    save(&summary, "resumo").await;
    drop(summary);

    let gear = Session::open("preferences").await;
    gear.click(".settings-gear").await;
    gear.wait_text(".tab-content", "Iniciar").await;
    save(&gear, "configuracoes").await;
    drop(gear);

    let search = Session::open("search").await;
    search.fill(".search-field", "r").await;
    search.wait_text(".results", "relatório.pdf").await;
    save(&search, "pesquisa").await;
    drop(search);

    for (screen, css, wanted, name) in [
        (
            "offline",
            ".offline-items",
            "notas.txt",
            "arquivos-off-line",
        ),
        ("preferences", "p.path", "Google Drive", "preferencias"),
        ("about", ".about-version", "0.", "sobre"),
        ("uninstall", ".uninstall", "", "desinstalar"),
        ("feedback", ".feedback", "", "feedback"),
        (
            "shared-drives",
            ".drive-list",
            "Equipe",
            "drives-compartilhados",
        ),
        ("home:tour", ".tour", "O estado da sincronização", "tour"),
    ] {
        let session = Session::open(screen).await;
        session.wait_text(css, wanted).await;
        save(&session, name).await;
    }
    let (report, notes) = (item_id("relatório.pdf").await, item_id("notas.txt").await);
    let sheet = item_id("Planilha.gsheet").await;
    for (screen, css, wanted, name) in [
        (
            format!("share:{report}"),
            ".people",
            "Proprietário",
            "compartilhar",
        ),
        (
            format!("versions:{notes}"),
            ".version-list",
            "Atual",
            "versoes",
        ),
        (
            format!("shortcut:{sheet}"),
            ".folder-list",
            "Equipe",
            "adicionar-atalho",
        ),
        (
            "preferences:computer".to_string(),
            ".devices",
            "PENDRIVE",
            "meu-computador",
        ),
    ] {
        let session = Session::open(&screen).await;
        session.wait_text(css, wanted).await;
        save(&session, name).await;
    }
    let mirror = Session::open("preferences:drive").await;
    mirror
        .click("input[name='drive-mode'][value='mirror']")
        .await;
    mirror.wait_text(".dialog .folder-summary", "livres").await;
    save(&mirror, "espelhar").await;
    mirror.click(".dialog .dialog-cancel").await;
    drop(mirror);

    let service = service().await;
    service.logout("", "discard", "").await.unwrap();
    status_until(&service, "sem conta", |s| s.state == "signed-out").await;
    let onboarding = Session::open("onboarding").await;
    onboarding.wait_text(".step-title", "Client ID").await;
    save(&onboarding, "primeiro-uso").await;
}

// Da bandeja, "Redefinir as preferências" abre a janela com a confirmação; nada muda sem ela.
// "Mais opções" leva a "Gerenciar drives compartilhados".
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn home_arg_reset_preferences_asks_first() {
    let session = Session::open("home:reset-preferences").await;
    let question = session.text("dialog[open] .dialog-body").await;
    session.click("dialog[open] .dialog-cancel").await;
    assert!(!question.is_empty());

    session.click(".more-button").await;
    session.find("[data-action='shared-drives']").await;
}

// Início tem o cartão "Drives compartilhados", que leva à visão com os drives da conta.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn shared_drives_view_lists_the_fake_drives() {
    let session = Session::open("home").await;
    session.wait_text(".cards", "Drives compartilhados").await;
    session.click("[data-view='drives']").await;
    let list = session.wait_text(".drives", "Equipe").await;
    session.find(".drives .open-folder").await;
    session.find(".manage-shared-drives").await;
    assert!(list.contains("Equipe"), "{list}");
}

// "Compartilhar" no menu de uma linha enviada abre a janela Compartilhar daquele item.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn activity_share_opens_the_share_window() {
    let session = Session::open("home").await;
    session.click("[data-view='activity']").await;
    session
        .wait_text(".activity-table tbody", "enviado-pelo-teste.txt")
        .await;
    session
        .click(".activity-table tbody tr:has(td[title$='enviado-pelo-teste.txt']) .row-menu")
        .await;
    session.click("[data-action='share']").await;
    let share = session.new_window().await;
    session.post("window", json!({ "handle": share })).await;
    let title = session.wait_text(".share", "Compartilhar").await;
    assert!(title.contains("enviado-pelo-teste.txt"), "{title}");
}

// Preferências como no original: duas abas e as configurações atrás da engrenagem, com "Voltar".
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn preferences_has_two_tabs_and_a_gear() {
    let session = Session::open("preferences:drive").await;
    session
        .find("[data-tab='drive'][aria-selected='true']")
        .await;
    let tabs = session
        .eval("return [...document.querySelectorAll('[role=tab]')].map(t => t.dataset.tab)")
        .await;
    session.click(".settings-gear").await;
    let settings = session.wait_text(".tab-content", "Iniciar").await;
    session.click(".settings-back").await;
    session
        .find("[data-tab='drive'][aria-selected='true']")
        .await;

    assert_eq!(tabs, json!(["computer", "drive"]));
    assert!(settings.contains("Configurações"), "{settings}");
}

// O que muda atrás da engrenagem continua pendente ao voltar às abas, e o Salvar grava junto.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn gear_changes_are_saved_with_the_tabs() {
    let service = service().await;
    let session = Session::open("preferences:drive").await;
    session.click(".settings-gear").await;
    session
        .click(".pref[data-key='PauseSyncingOnMeteredConnection'] input")
        .await;
    session.click(".settings-back").await;
    let pending = session.text(".pending-changes").await;
    session.click(".save").await;
    session.wait_text(".toast", "salv").await;
    let prefs = service.preferences().await.unwrap();
    service.reset_preferences().await.unwrap();

    let metered = prefs
        .iter()
        .find(|p| p.key == "PauseSyncingOnMeteredConnection")
        .unwrap();
    assert!(pending.contains("pendentes"), "{pending}");
    assert_eq!(metered.value, "true");
}

/// Abre o primeiro uso já no passo Resumo (com a conta de teste) e devolve o texto dele.
async fn open_onboarding_summary() -> (Session, String) {
    let account = service().await.status("").await.unwrap().account;
    let session = Session::open("onboarding").await;
    session.wait_text(".step-title", "").await;
    for _ in 0..3 {
        session.click(".continue").await;
    }
    let summary = session.wait_text(".summary", &account).await;
    (session, summary)
}

// O passo Resumo do primeiro uso mostra a conta e a pasta do Drive escolhidas.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn onboarding_summary_lists_the_choices() {
    let (session, summary) = open_onboarding_summary().await;
    session.click(".back").await;
    session.find(".suggested-folders").await;

    assert!(summary.contains("Streaming"), "{summary}");
    assert!(summary.contains("Google Drive"), "{summary}");
}

// "Fazer o tour" percorre a janela principal com um balão por parte e termina em Concluir.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "precisa do scripts/e2e.sh"]
async fn tour_walks_the_home_screen() {
    let session = Session::open("home:tour").await;
    let first = session
        .wait_text(".tour", "O estado da sincronização")
        .await;
    let outlined = session
        .eval(
            "return document.querySelector('.tour-target')?.classList.contains('header') ?? false",
        )
        .await;
    let mut seen = 1;
    while session.text(".tour .tour-next").await != "Concluir" {
        session.click(".tour .tour-next").await;
        seen += 1;
    }
    session.click(".tour .tour-next").await;
    let gone = session
        .eval("return document.querySelector('.tour') === null")
        .await;

    assert!(first.contains("pausar"), "{first}");
    assert_eq!(outlined, json!(true));
    assert_eq!(seen, 5);
    assert_eq!(gone, json!(true));
}
