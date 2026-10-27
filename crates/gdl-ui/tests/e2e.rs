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

    session.click("[data-tab='settings']").await;
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

    session.click("[data-tab='settings']").await;
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
    drop(home);

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
    ] {
        let session = Session::open(screen).await;
        session.wait_text(css, wanted).await;
        save(&session, name).await;
    }
    let (report, notes) = (item_id("relatório.pdf").await, item_id("notas.txt").await);
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
    ] {
        let session = Session::open(&screen).await;
        session.wait_text(css, wanted).await;
        save(&session, name).await;
    }

    let service = service().await;
    service.logout("", "discard", "").await.unwrap();
    status_until(&service, "sem conta", |s| s.state == "signed-out").await;
    let onboarding = Session::open("onboarding").await;
    onboarding.wait_text(".step-title", "Client ID").await;
    save(&onboarding, "primeiro-uso").await;
}
