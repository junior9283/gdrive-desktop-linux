//! Serviço para os testes de ponta a ponta da janela (`scripts/e2e.sh`): o serviço de verdade
//! sobre o Drive simulado, já com a conta conectada, alguns arquivos (um deles off-line) e uma
//! política, publicado no barramento de sessão (o do `dbus-run-session` do script). Escreve
//! `pronto` quando dá para usar e sai com SIGTERM.
//!
//! O "navegador" anota em `$GDL_E2E_OPENED` cada endereço ou caminho que o serviço abre. SIGUSR1
//! enche o armazenamento do Drive simulado (todo pedido recebe `storageQuotaExceeded`) e SIGUSR2
//! o libera; feito, o estado (`full` ou `free`) vai para `$GDL_E2E_QUOTA`.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use gdl_common::paths::Paths;
use gdl_daemon::{Api, Config, Daemon, Service};
use gdl_drive::client::Backoff;
use gdl_drive::oauth::OAuthEndpoints;
use gdl_drive::token_store::MemoryStore;
use gdl_engine::engine::Intervals;
use gdl_fake_drive::{FakeDrive, Fault};
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
    let browser = recording_browser(dir.path())?;
    let home = dir.path().join("home");
    std::fs::create_dir_all(&home)?;
    let paths = Paths::for_test(dir.path());
    // Uma preferência da política, para a tela mostrar "Gerenciado pelo administrador".
    std::fs::create_dir_all(&paths.etc_dir)?;
    std::fs::write(
        paths.etc_dir.join("policies.toml"),
        "AutoStartOnLogin = false\n",
    )?;
    let daemon = Daemon::new(Config {
        paths,
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
        browser: Some(browser),
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
    let mount = proxy.status().await?.mount_point;
    let notes = std::path::Path::new(&mount).join("Meu Drive/notas.txt");
    proxy.pin(&notes.display().to_string()).await?;
    // Lido pela montagem, o arquivo fica inteiro no cache: o fixado já está baixado.
    tokio::task::spawn_blocking(move || std::fs::read(notes)).await??;
    // Um envio pela montagem, para a atividade de sincronização ter o que mostrar.
    let file = std::path::Path::new(&mount).join("Meu Drive/enviado-pelo-teste.txt");
    tokio::task::spawn_blocking(move || std::fs::write(file, b"oi")).await??;
    while proxy.status().await?.syncing > 0 {
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    println!("pronto");

    let mut term = signal(SignalKind::terminate())?;
    let mut fill = signal(SignalKind::user_defined1())?;
    let mut free = signal(SignalKind::user_defined2())?;
    loop {
        tokio::select! {
            _ = term.recv() => break,
            _ = tokio::signal::ctrl_c() => break,
            _ = daemon.exit_requested() => break,
            _ = fill.recv() => {
                fake.inject(Fault::QuotaExceeded { times: u32::MAX });
                report_quota("full")?;
            }
            _ = free.recv() => {
                fake.clear_faults();
                report_quota("free")?;
            }
        }
    }
    daemon.shutdown().await;
    Ok(())
}

/// Confirma o estado do armazenamento em `$GDL_E2E_QUOTA` (sem a variável, não faz nada).
fn report_quota(state: &str) -> std::io::Result<()> {
    match std::env::var_os("GDL_E2E_QUOTA") {
        Some(file) => std::fs::write(file, state),
        None => Ok(()),
    }
}

/// Um "navegador" que só anota o que recebeu em `$GDL_E2E_OPENED` (sem a variável, não faz nada).
fn recording_browser(dir: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join("navegador");
    std::fs::write(
        &script,
        "#!/bin/sh\n[ -n \"$GDL_E2E_OPENED\" ] && printf '%s\\n' \"$1\" >> \"$GDL_E2E_OPENED\"\nexit 0\n",
    )?;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))?;
    Ok(script)
}
