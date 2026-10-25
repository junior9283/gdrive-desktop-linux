//! `gdrive-desktop-linuxd`: o serviço do usuário (iniciado pelo systemd ou pelo D-Bus).

use std::process::ExitCode;
use std::sync::Arc;

use gdl_common::i18n;
use gdl_common::paths::Paths;
use gdl_daemon::{Api, BUS_NAME, Config, Daemon, OBJECT_PATH, Service};
use gdl_drive::client::Backoff;
use gdl_drive::oauth::OAuthEndpoints;
use gdl_drive::token_store::SecretServiceStore;
use gdl_engine::engine::Intervals;
use tokio::signal::unix::{SignalKind, signal};

#[tokio::main]
async fn main() -> ExitCode {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(erro = %e, "o serviço parou");
            ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), Box<dyn std::error::Error>> {
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
    daemon.attach(conn).await?;
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
    tokio::select! {
        _ = term.recv() => {}
        _ = tokio::signal::ctrl_c() => {}
    }
    resume.abort();
    daemon.shutdown().await;
    Ok(())
}
