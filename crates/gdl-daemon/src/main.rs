//! `gdrive-desktop-linuxd`: o serviço do usuário (iniciado pelo systemd ou pelo D-Bus).

use std::process::ExitCode;
use std::sync::Arc;

use gdl_common::i18n;
use gdl_common::paths::Paths;
use gdl_daemon::daemon::Exit;
use gdl_daemon::{Api, Config, Daemon, Service};
use gdl_drive::client::Backoff;
use gdl_drive::oauth::OAuthEndpoints;
use gdl_drive::token_store::SecretServiceStore;
use gdl_engine::engine::Intervals;
use gdl_ipc::{BUS_NAME, OBJECT_PATH};
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    // `Restart` sobe o serviço de novo aqui mesmo, sem depender do systemd.
    loop {
        match run().await {
            Ok(Exit::Restart) => tracing::info!("reiniciando o serviço"),
            Ok(Exit::Quit) => return ExitCode::SUCCESS,
            Err(e) => {
                tracing::error!(erro = %e, "o serviço parou");
                return ExitCode::FAILURE;
            }
        }
    }
}

async fn run() -> Result<Exit, Box<dyn std::error::Error>> {
    let home = std::env::var_os("HOME").ok_or("HOME não definido")?;
    let config = Config {
        paths: Paths::from_env(),
        lang: i18n::env_lang(),
        home: home.into(),
        oauth: OAuthEndpoints::google(),
        api: Api::google(),
        tokens: Arc::new(SecretServiceStore::new().await?),
        browser: None,
        backoff: Backoff::default(),
        intervals: Intervals::default(),
    };
    let daemon = Daemon::new(config)?;
    let conn = zbus::connection::Builder::session()?
        .serve_at(OBJECT_PATH, Service::new(daemon.clone()))?
        .name(BUS_NAME)?
        .build()
        .await?;
    daemon.attach(conn.clone()).await?;
    // A conexão limitada vem do NetworkManager, no barramento do sistema.
    match zbus::Connection::system().await {
        Ok(system) => daemon.watch_metered(system),
        Err(e) => {
            tracing::info!(erro = %e, "sem barramento do sistema: a conexão limitada não pausa")
        }
    }
    // A reconexão (que pode indexar o Drive inteiro) corre em paralelo à espera do SIGTERM.
    let mut term = signal(SignalKind::terminate())?;
    let resume = tokio::spawn({
        let daemon = daemon.clone();
        async move {
            if let Err(e) = daemon.resume_saved_account().await {
                tracing::warn!(erro = %e, "não deu para reconectar a conta");
            }
        }
    });
    // O systemd para o serviço com SIGTERM: desmonta antes de sair.
    let exit = tokio::select! {
        _ = term.recv() => Exit::Quit,
        _ = tokio::signal::ctrl_c() => Exit::Quit,
        exit = daemon.exit_requested() => exit,
    };
    resume.abort();
    daemon.shutdown().await;
    // Solta o nome no barramento antes de o próximo serviço (no `Restart`) pedi-lo.
    conn.object_server()
        .remove::<Service, _>(OBJECT_PATH)
        .await?;
    conn.release_name(BUS_NAME).await?;
    Ok(exit)
}
