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

struct FakeNotifications(Arc<Mutex<Vec<Note>>>);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl FakeNotifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
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

pub(crate) struct Harness {
    pub fake: FakeDrive,
    pub daemon: Arc<Daemon>,
    pub proxy: ServiceProxy<'static>,
    pub tokens: Arc<MemoryStore>,
    pub home: PathBuf,
    notes: Arc<Mutex<Vec<Note>>>,
    _client: Connection,
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
            _client: client,
            _server: server,
            dir,
        }
    }

    /// Para o serviço (pela saída normal, que cancela o que estiver em andamento) e sobe outro
    /// sobre os mesmos dados, que reconecta a conta. `policy` troca a política.
    pub async fn restart(&mut self, policy: Option<&str>) {
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
        (self._client, self._server) = (client, server);
        self.daemon.resume_saved_account().await.unwrap();
    }

    /// Login pelo D-Bus, com o teste fazendo o papel do navegador. Devolve `LoginFinished`.
    pub async fn login(&self) -> (bool, String, String) {
        use futures_util::StreamExt;

        let mut finished = self.proxy.receive_login_finished().await.unwrap();
        let url = self
            .proxy
            .begin_login("id-de-teste", "segredo")
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

    pub async fn logged_in(&self) {
        let (ok, _, error) = self.login().await;
        assert!(ok, "login: {error}");
    }

    /// `Meu Drive` dentro da montagem padrão.
    pub fn drive(&self) -> PathBuf {
        self.home.join("Google Drive").join("Meu Drive")
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
pub(crate) async fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "esperando: {what}"
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

/// Espera o `Status` satisfazer `done` (com o limite de `wait_until`).
pub(crate) async fn status_until(h: &Harness, what: &str, done: impl Fn(&gdl_ipc::Status) -> bool) {
    let start = std::time::Instant::now();
    loop {
        let status = h.proxy.status().await.unwrap();
        if done(&status) {
            return;
        }
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "esperando: {what} ({status:?})"
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

/// Configuração do serviço sobre o Drive simulado e a pasta `root` do teste.
fn config(fake: &FakeDrive, root: &Path, tokens: &Arc<MemoryStore>) -> Config {
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
        // Nada abre na tela: o "navegador" é o `true`.
        browser: Some("true".into()),
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
        .build();
    let (server, client) = tokio::join!(server, client);
    let (server, client) = (server.unwrap(), client.unwrap());
    daemon.attach(server.clone()).await.unwrap();
    let proxy = ServiceProxy::new(&client).await.unwrap();
    (daemon, proxy, client, server)
}
