//! Serviço para os testes de ponta a ponta da janela (`scripts/e2e.sh`): o serviço de verdade
//! sobre o Drive simulado, já com a conta conectada e alguns arquivos, publicado no barramento de
//! sessão (o do `dbus-run-session` do script). Escreve `pronto` quando dá para usar e sai com
//! SIGTERM.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use gdl_common::paths::Paths;
use gdl_daemon::{Api, Config, Daemon, Service};
use gdl_drive::client::Backoff;
use gdl_drive::oauth::OAuthEndpoints;
use gdl_drive::token_store::MemoryStore;
use gdl_engine::engine::Intervals;
use gdl_fake_drive::FakeDrive;
use gdl_ipc::{BUS_NAME, OBJECT_PATH, ServiceProxy};
use tokio::signal::unix::{SignalKind, signal};

type Error = Box<dyn std::error::Error>;

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let docs = fake.add_folder(&root, "Documentos");
    fake.add_file(&docs, "relatório.pdf", vec![7; 300 * 1024]);
    fake.add_file(&root, "notas.txt", b"lembrar de testar".to_vec());
    fake.add_gdoc(&root, "Planilha", "application/vnd.google-apps.spreadsheet");

    let dir = tempfile::tempdir()?;
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home)?;
    let daemon = Daemon::new(Config {
        paths: Paths::for_test(dir.path()),
        lang: gdl_common::i18n::env_lang(),
        home,
        oauth: OAuthEndpoints {
            auth: fake.auth_url(),
            token: fake.token_url(),
            userinfo: fake.userinfo_url(),
        },
        api: Api {
            base: fake.base_url(),
            upload: fake.upload_url(),
        },
        tokens: Arc::new(MemoryStore::default()),
        // Nada abre de verdade: o login é feito aqui mesmo.
        browser: Some("/bin/true".into()),
        backoff: Backoff::default(),
        intervals: Intervals::default(),
    })?;
    let conn = zbus::connection::Builder::session()?
        .serve_at(OBJECT_PATH, Service::new(daemon.clone()))?
        .name(BUS_NAME)?
        .build()
        .await?;
    daemon.attach(conn.clone()).await?;

    // O login pelo próprio D-Bus, com este programa no papel do navegador.
    let proxy = ServiceProxy::new(&zbus::Connection::session().await?).await?;
    let mut finished = proxy.receive_login_finished().await?;
    let url = proxy.begin_login("id-de-teste", "segredo").await?;
    reqwest::get(url).await?;
    let signal_ok = tokio::time::timeout(Duration::from_secs(30), finished.next())
        .await?
        .ok_or("sem LoginFinished")?
        .args()?
        .ok;
    if !signal_ok {
        return Err("o login falhou".into());
    }
    // Um envio pela montagem, para a atividade de sincronização ter o que mostrar.
    let mount = proxy.status().await?.mount_point;
    let file = std::path::Path::new(&mount).join("Meu Drive/enviado-pelo-teste.txt");
    tokio::task::spawn_blocking(move || std::fs::write(file, b"oi")).await??;
    while proxy.status().await?.pending_uploads > 0 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("pronto");

    let mut term = signal(SignalKind::terminate())?;
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
        _ = daemon.exit_requested() => {}
    }
    daemon.shutdown().await;
    Ok(())
}
