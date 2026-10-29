//! Serviço de teste: D-Bus ponto a ponto por um par de sockets (sem barramento real), Drive
//! simulado e um serviço de notificações falso do outro lado do mesmo socket.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gdl_common::paths::Paths;
use gdl_drive::client::Backoff;
use gdl_drive::oauth::OAuthEndpoints;
use gdl_drive::token_store::{MemoryStore, TokenStore};
use gdl_engine::engine::Intervals;
use gdl_fake_drive::FakeDrive;
use zbus::connection::Builder;
use zbus::zvariant::OwnedValue;
use zbus::{Connection, Guid};

use crate::daemon::{Api, Config, Daemon};
use crate::dbus::Service;
use crate::devices::UDISKS_PATH;
use crate::fake_udisks;
use crate::metered::NETWORK_MANAGER_PATH;
use crate::notify::NOTIFICATIONS_PATH;
use gdl_ipc::{OBJECT_PATH, ServiceProxy};

/// Notificação recebida pelo serviço falso.
#[derive(Debug, Clone)]
pub(crate) struct Note {
    /// ID que o servidor devolveu.
    pub id: u32,
    /// Aviso anterior que este substitui (0: nenhum).
    pub replaces_id: u32,
    pub summary: String,
    pub body: String,
    pub actions: Vec<String>,
}

/// O aviso cujo resumo começa assim demora `SLOW_DELAY` para o servidor responder.
pub(crate) const SLOW: &str = "lento:";
pub(crate) const SLOW_DELAY: Duration = Duration::from_secs(4);

struct FakeNotifications(Arc<Mutex<Vec<Note>>>);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl FakeNotifications {
    /// Como os servidores de verdade (KDE, GNOME): o corpo aceita marcação.
    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into(), "body-markup".into(), "actions".into()]
    }

    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        _app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        _hints: HashMap<String, OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        // Um servidor lento (ou parado) de propósito, para os testes de envio.
        if summary.starts_with(SLOW) {
            tokio::time::sleep(SLOW_DELAY).await;
        }
        let mut notes = self.0.lock().unwrap();
        // Como no servidor de verdade: substituir mantém o ID.
        let id = match replaces_id {
            0 => notes.len() as u32 + 1,
            id => id,
        };
        notes.push(Note {
            id,
            replaces_id,
            summary,
            body,
            actions,
        });
        id
    }
}

/// NetworkManager falso: só a propriedade `Metered` (NMMetered).
#[derive(Default)]
pub(crate) struct FakeNetworkManager {
    metered: u32,
}

#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl FakeNetworkManager {
    #[zbus(property)]
    fn metered(&self) -> u32 {
        self.metered
    }
}

/// Este computador nos testes (sem perguntar ao barramento do sistema).
pub(crate) const COMPUTER_ID: &str = "0123456789abcdef";
pub(crate) const COMPUTER_NAME: &str = "Computador de teste";

pub(crate) struct Harness {
    pub fake: FakeDrive,
    pub daemon: Arc<Daemon>,
    pub proxy: ServiceProxy<'static>,
    pub tokens: Arc<MemoryStore>,
    pub home: PathBuf,
    notes: Arc<Mutex<Vec<Note>>>,
    client: Connection,
    _server: Connection,
    dir: tempfile::TempDir,
}

impl Harness {
    pub async fn new() -> Harness {
        Harness::with_policy("").await
    }

    /// `policy`: conteúdo do `policies.toml` do administrador.
    pub async fn with_policy(policy: &str) -> Harness {
        let fake = FakeDrive::start().await;
        let dir = tempfile::tempdir().unwrap();
        let home = dir.path().join("home");
        std::fs::create_dir_all(&home).unwrap();
        let tokens = Arc::new(MemoryStore::default());
        let notes = Arc::new(Mutex::new(Vec::new()));
        write_policy(dir.path(), policy);
        let (daemon, proxy, client, server) =
            serve(config(&fake, dir.path(), &tokens), &notes).await;
        Harness {
            fake,
            daemon,
            proxy,
            tokens,
            home,
            notes,
            client,
            _server: server,
            dir,
        }
    }

    /// Para o serviço (pela saída normal, que cancela o que estiver em andamento) e sobe outro
    /// sobre os mesmos dados, que reconecta a conta. `policy` troca a política.
    pub async fn restart(&mut self, policy: Option<&str>) {
        self.restart_without_resuming(policy).await;
        self.daemon.resume_saved_accounts().await.unwrap();
    }

    /// Um serviço novo no lugar deste, que ainda não reconectou as contas guardadas.
    pub async fn restart_without_resuming(&mut self, policy: Option<&str>) {
        self.daemon.shutdown().await;
        if let Some(policy) = policy {
            write_policy(self.dir.path(), policy);
        }
        let (daemon, proxy, client, server) = serve(
            config(&self.fake, self.dir.path(), &self.tokens),
            &self.notes,
        )
        .await;
        (self.daemon, self.proxy) = (daemon, proxy);
        (self.client, self._server) = (client, server);
    }

    /// Login pelo D-Bus, com o teste fazendo o papel do navegador. Devolve `LoginFinished`.
    pub async fn login(&self) -> (bool, String, String) {
        self.login_expecting("").await
    }

    /// Login que espera voltar como `expected` (vazio, conta nova).
    pub async fn login_expecting(&self, expected: &str) -> (bool, String, String) {
        self.login_with("id-de-teste", "segredo", expected).await
    }

    /// Login com o Client ID e o segredo dados (vazios, o serviço usa os guardados).
    pub async fn login_with(
        &self,
        client_id: &str,
        secret: &str,
        expected: &str,
    ) -> (bool, String, String) {
        use futures_util::StreamExt;

        let mut finished = self.proxy.receive_login_finished().await.unwrap();
        let url = self
            .proxy
            .begin_login(client_id, secret, expected)
            .await
            .unwrap();
        reqwest::get(url).await.unwrap();
        let signal = tokio::time::timeout(Duration::from_secs(10), finished.next())
            .await
            .expect("LoginFinished")
            .unwrap();
        let args = signal.args().unwrap();
        (args.ok, args.email.to_string(), args.error.to_string())
    }

    /// Login pelo D-Bus na conta `email` do Drive simulado (criada antes com `add_account`).
    pub async fn login_as(&self, email: &str) -> (bool, String, String) {
        self.fake.login_as(email);
        self.login().await
    }

    /// A conta `email` entra de novo (o navegador volta com ela): devolve `LoginFinished`.
    pub async fn sign_in_again_as(&self, email: &str, expected: &str) -> (bool, String, String) {
        self.fake.login_as(email);
        self.login_expecting(expected).await
    }

    /// Entra na conta `email` e confere que deu certo.
    pub async fn logged_in_as(&self, email: &str) {
        let (ok, _, error) = self.login_as(email).await;
        assert!(ok, "login de {email}: {error}");
    }

    /// A conta principal e `email` conectadas, nessa ordem.
    pub async fn two_accounts(&self, email: &str) {
        self.fake.add_account(email);
        self.logged_in().await;
        self.logged_in_as(email).await;
    }

    /// A montagem de uma conta que não é a primeira: `Google Drive (<e-mail>)`.
    pub fn mount_of(&self, email: &str) -> PathBuf {
        self.home.join(format!("Google Drive ({email})"))
    }

    /// Os e-mails guardados em `accounts`, na ordem do arquivo.
    pub fn saved_accounts(&self) -> Vec<String> {
        std::fs::read_to_string(self.paths().data_dir.join("accounts"))
            .unwrap_or_default()
            .lines()
            .map(|line| line.split('\t').next().unwrap_or_default().to_string())
            .collect()
    }

    pub async fn logged_in(&self) {
        let (ok, _, error) = self.login().await;
        assert!(ok, "login: {error}");
    }

    /// `Meu Drive` dentro da montagem padrão.
    pub fn drive(&self) -> PathBuf {
        self.home.join("Google Drive").join("Meu Drive")
    }

    /// O que o serviço mandou abrir no navegador ou no gerenciador de arquivos, em ordem.
    pub fn opened(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("opened.txt"))
            .unwrap_or_default()
            .lines()
            .map(String::from)
            .collect()
    }

    /// As pastas do serviço deste teste.
    pub fn paths(&self) -> Paths {
        Paths::for_test(self.dir.path())
    }

    /// Troca a `Metered` do NetworkManager falso e avisa a mudança.
    pub async fn set_metered(&self, metered: u32) {
        let iface = self
            .client
            .object_server()
            .interface::<_, FakeNetworkManager>(NETWORK_MANAGER_PATH)
            .await
            .unwrap();
        iface.get_mut().await.metered = metered;
        iface
            .get()
            .await
            .metered_changed(iface.signal_emitter())
            .await
            .unwrap();
    }

    /// Conecta um pendrive montado em `mount` (o UDisks2 falso avisa a entrada).
    pub async fn plug(&self, uuid: &str, label: &str, mount: &Path) {
        fake_udisks::plug(self.client.object_server(), uuid, label, mount)
            .await
            .unwrap();
    }

    /// Desconecta o pendrive `uuid`.
    pub async fn unplug(&self, uuid: &str) {
        fake_udisks::unplug(self.client.object_server(), uuid)
            .await
            .unwrap();
    }

    pub fn notes(&self) -> Vec<Note> {
        self.notes.lock().unwrap().clone()
    }
}

// O serviço e a conexão se referenciam (como no `main`, quem sai chama `shutdown`).
impl Drop for Harness {
    fn drop(&mut self) {
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(self.daemon.shutdown())
        });
    }
}

/// Espera sem bloquear o runtime.
pub(crate) async fn wait_until(what: &str, done: impl FnMut() -> bool) {
    wait_until_within(Duration::from_secs(10), what, done).await;
}

/// `wait_until` com outro limite.
pub(crate) async fn wait_until_within(limit: Duration, what: &str, mut done: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !done() {
        assert!(start.elapsed() < limit, "esperando: {what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Espera o `Status` satisfazer `done` (com o limite de `wait_until`).
pub(crate) async fn status_until(h: &Harness, what: &str, done: impl Fn(&gdl_ipc::Status) -> bool) {
    poll_until(what, || h.proxy.status(""), done).await;
}

/// Pergunta ao serviço com `probe` até a resposta satisfazer `done` (o limite de `wait_until`).
pub(crate) async fn poll_until<
    T: std::fmt::Debug,
    F: Future<Output = Result<T, gdl_ipc::ServiceError>>,
>(
    what: &str,
    probe: impl Fn() -> F,
    done: impl Fn(&T) -> bool,
) {
    let start = std::time::Instant::now();
    loop {
        let got = probe().await.unwrap();
        if done(&got) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "esperando: {what} ({got:?})"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Operação de arquivo bloqueante na montagem, fora das threads do runtime.
pub(crate) async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    tokio::task::spawn_blocking(f).await.unwrap()
}

fn write_policy(root: &Path, policy: &str) {
    let paths = Paths::for_test(root);
    std::fs::create_dir_all(&paths.etc_dir).unwrap();
    std::fs::write(paths.etc_dir.join("policies.toml"), policy).unwrap();
}

/// Script que faz o papel do navegador, do gerenciador de arquivos e da janela: anota os argumentos em
/// `opened.txt` (ver `Harness::opened`).
fn launcher(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = root.join("abrir.sh");
    let log = root.join("opened.txt");
    std::fs::write(
        &script,
        format!("#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n", log.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

/// Configuração do serviço sobre o Drive simulado e a pasta `root` do teste.
fn config(fake: &FakeDrive, root: &Path, tokens: &Arc<MemoryStore>) -> Config {
    let launcher = launcher(root);
    Config {
        paths: Paths::for_test(root),
        lang: "pt_BR.UTF-8".into(),
        home: root.join("home"),
        oauth: OAuthEndpoints {
            auth: fake.auth_url(),
            token: fake.token_url(),
            userinfo: fake.userinfo_url(),
        },
        api: Api {
            base: fake.base_url(),
            upload: fake.upload_url(),
        },
        tokens: tokens.clone() as Arc<dyn TokenStore>,
        // Nada abre na tela: o "navegador" só anota o que pediram para abrir.
        browser: Some(launcher.clone()),
        // A janela também: o que se pede a ela fica anotado (`--open tela`).
        ui: launcher,
        backoff: Backoff {
            base: Duration::from_millis(1),
            max: Duration::from_millis(2),
            tries: 2,
        },
        intervals: Intervals {
            changes: Duration::from_millis(50),
            uploads_idle: Duration::from_millis(20),
            maintenance: Duration::from_millis(50),
            eviction: Duration::from_millis(200),
            ..Intervals::default()
        },
        computer: crate::daemon::Computer {
            id: COMPUTER_ID.into(),
            name: COMPUTER_NAME.into(),
        },
        folder_timing: gdl_folders::watch::Timing {
            quiet: Duration::from_millis(100),
            ..Default::default()
        },
        unmount_limit: Duration::from_secs(2),
        // Os testes consultam com `check_release`; o laço não roda.
        updates: crate::updates::UpdateCheck {
            url: fake.releases_url(),
            first: Duration::from_secs(3600),
            every: Duration::from_secs(3600),
        },
    }
}

/// Publica o serviço num lado de um par de sockets; o outro lado é o cliente, que também serve
/// as notificações falsas.
async fn serve(
    config: Config,
    notes: &Arc<Mutex<Vec<Note>>>,
) -> (Arc<Daemon>, ServiceProxy<'static>, Connection, Connection) {
    let daemon = Daemon::new(config).unwrap();
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let server = Builder::unix_stream(a)
        .server(Guid::generate())
        .unwrap()
        .p2p()
        .serve_at(OBJECT_PATH, Service::new(daemon.clone()))
        .unwrap()
        .build();
    let client = Builder::unix_stream(b)
        .p2p()
        .serve_at(NOTIFICATIONS_PATH, FakeNotifications(notes.clone()))
        .unwrap()
        .serve_at(NETWORK_MANAGER_PATH, FakeNetworkManager::default())
        .unwrap()
        .serve_at(UDISKS_PATH, zbus::fdo::ObjectManager)
        .unwrap()
        .build();
    let (server, client) = tokio::join!(server, client);
    let (server, client) = (server.unwrap(), client.unwrap());
    daemon.attach(server.clone()).await.unwrap();
    daemon.watch_metered(server.clone());
    daemon.watch_devices(server.clone());
    let proxy = ServiceProxy::new(&client).await.unwrap();
    (daemon, proxy, client, server)
}

/// Nome e texto de cada arquivo de um `.tar.gz`.
pub fn read_package(path: &Path) -> Vec<(String, String)> {
    use std::io::Read;
    let gz = flate2::read::GzDecoder::new(std::fs::File::open(path).unwrap());
    let mut tar = tar::Archive::new(gz);
    tar.entries()
        .unwrap()
        .map(|e| {
            let mut e = e.unwrap();
            let name = e.path().unwrap().display().to_string();
            let mut text = String::new();
            e.read_to_string(&mut text).unwrap();
            (name, text)
        })
        .collect()
}
