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
use gdl_ipc::launcher::Launcher;
use gdl_ipc::{BUS_NAME, OBJECT_PATH};
use tokio::signal::unix::{SignalKind, signal};

const HELP: &str = "\
Uso: gdrive-desktop-linuxd [opção]

Serviço do usuário do gdrive-desktop-linux: monta a conta e atende a janela, a bandeja e os
gerenciadores de arquivos pelo D-Bus. Normalmente é iniciado pelo systemd ou pelo próprio D-Bus;
se já estiver rodando, outra cópia sai sem fazer nada.

Opções:
  -h, --help     mostra esta ajuda e sai
  -V, --version  mostra a versão e sai
";

fn main() -> ExitCode {
    // Antes de tudo: as opções só respondem, sem tocar no barramento nem na conta.
    match std::env::args().nth(1).as_deref() {
        None => serve(),
        Some("-V" | "--version") => {
            println!("gdrive-desktop-linuxd {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Some("-h" | "--help") => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        Some(other) => {
            eprintln!("gdrive-desktop-linuxd: opção desconhecida: {other}\n\n{HELP}");
            ExitCode::from(2)
        }
    }
}

#[tokio::main]
async fn serve() -> ExitCode {
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
        ui: Launcher::default_program(),
        backoff: Backoff::default(),
        intervals: Intervals::default(),
        computer: gdl_daemon::daemon::Computer::detect().await,
        folder_timing: Default::default(),
    };
    let daemon = Daemon::new(config)?;
    let conn = zbus::connection::Builder::session()?
        .serve_at(OBJECT_PATH, Service::new(daemon.clone()))?
        .build()
        .await?;
    // Antes de montar qualquer coisa: com outra instância rodando, esta sai e deixa tudo como está.
    // O padrão do zbus (`Builder::name`) toma o nome de quem já o tem: a segunda instância
    // derrubava a primeira e montava "~/Google Drive (1)".
    if !gdl_ipc::claim_name(&conn, BUS_NAME).await? {
        tracing::info!("o serviço já está rodando nesta sessão; nada a fazer");
        return Ok(Exit::Quit);
    }
    daemon.attach(conn.clone()).await?;
    // A conexão limitada (NetworkManager) e os dispositivos (UDisks2) vêm do barramento do
    // sistema.
    match zbus::Connection::system().await {
        Ok(system) => {
            daemon.watch_metered(system.clone());
            daemon.watch_devices(system);
        }
        Err(e) => tracing::info!(
            erro = %e,
            "sem barramento do sistema: sem pausa na conexão limitada nem dispositivos"
        ),
    }
    // A reconexão (que pode indexar o Drive inteiro) corre em paralelo à espera do SIGTERM.
    let mut term = signal(SignalKind::terminate())?;
    let resume = tokio::spawn({
        let daemon = daemon.clone();
        async move {
            if let Err(e) = daemon.resume_saved_accounts().await {
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
