//! `gdrive-desktop-linux`: CLI provisória do serviço (até a bandeja chegar) e manipulador dos
//! ponteiros de Documentos Google.

use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use futures_util::StreamExt;
use gdl_common::browser;
use gdl_common::config::load_preferences;
use gdl_common::i18n::I18n;
use gdl_common::paths::Paths;
use gdl_common::time::rfc3339;
use gdl_engine::gdocs::{open_url, pointer_doc_id};
use gdl_ipc::{ServiceError, ServiceProxy};

#[derive(Parser)]
#[command(
    name = "gdrive-desktop-linux",
    version,
    about = "Google Drive para Linux"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Conecta uma conta Google (abre o navegador).
    Login {
        #[arg(long)]
        client_id: Option<String>,
        /// Arquivo com o segredo do cliente (fora do histórico do shell).
        #[arg(long)]
        client_secret_file: Option<PathBuf>,
    },
    /// Mostra a conta, a pasta e o estado da sincronização.
    Status,
    /// Deixa disponível off-line.
    Pin { path: PathBuf },
    /// Deixa só on-line.
    Unpin { path: PathBuf },
    /// Pausa a sincronização.
    Pause,
    /// Retoma a sincronização.
    Resume,
    /// Lista o que foi guardado em Lost and Found.
    LostAndFound,
    /// Desconecta a conta.
    Logout {
        /// Sai quando os envios pendentes terminarem.
        #[arg(long, conflicts_with_all = ["move_to", "discard"])]
        finish: bool,
        /// Copia os arquivos com envio pendente para esta pasta antes de sair.
        #[arg(long = "move", value_name = "PASTA", conflicts_with = "discard")]
        move_to: Option<PathBuf>,
        /// Descarta os envios pendentes.
        #[arg(long)]
        discard: bool,
    },
    /// Abre um ponteiro (`.gdoc`, `.gsheet`…) no navegador.
    OpenPointer { file: PathBuf },
}

type Res<T = ()> = Result<T, ServiceError>;

fn failed(e: impl std::fmt::Display) -> ServiceError {
    ServiceError::Failed(e.to_string())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    let i18n = I18n::from_env();
    let result = match cli.command {
        Command::OpenPointer { file } => open_pointer(&file, &i18n),
        command => service(command, &i18n).await,
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}", describe(e, &i18n));
            ExitCode::FAILURE
        }
    }
}

fn open_pointer(file: &Path, i18n: &I18n) -> Res {
    let not_pointer =
        || failed(i18n.tr("cli-not-pointer", &[("path", &file.display().to_string())]));
    let json = std::fs::read(file).map_err(failed)?;
    // O endereço sai do ID, nunca do campo `url`: um ponteiro forjado não abre outra coisa.
    let id = pointer_doc_id(&json).ok_or_else(not_pointer)?;
    let launcher = load_preferences(&Paths::from_env())
        .ok()
        .and_then(|(prefs, _)| prefs.default_web_browser);
    browser::open(&open_url(&id), launcher.as_deref()).map_err(failed)
}

/// Mensagem do erro: a descrição que o serviço mandou, ou o aviso de que ele não está rodando.
fn describe(e: ServiceError, i18n: &I18n) -> String {
    match e {
        ServiceError::ZBus(zbus::Error::MethodError(name, _, _))
            if name.ends_with("ServiceUnknown") =>
        {
            i18n.tr("cli-no-service", &[("error", name.as_str())])
        }
        ServiceError::ZBus(e) => e.to_string(),
        ServiceError::PendingUploads(message)
        | ServiceError::NotLoggedIn(message)
        | ServiceError::NotFound(message)
        | ServiceError::InvalidArgs(message)
        | ServiceError::Managed(message)
        | ServiceError::Failed(message) => message,
    }
}

fn absolute(path: &Path) -> Res<String> {
    let path = std::path::absolute(path).map_err(failed)?;
    Ok(path.display().to_string())
}

async fn service(command: Command, i18n: &I18n) -> Res {
    let conn = zbus::Connection::session().await?;
    let proxy = ServiceProxy::new(&conn).await?;
    match command {
        Command::Login {
            client_id,
            client_secret_file,
        } => login(&proxy, client_id, client_secret_file, i18n).await,
        Command::Status => status(&proxy, i18n).await,
        Command::Pin { path } => proxy.pin(&absolute(&path)?).await,
        Command::Unpin { path } => proxy.unpin(&absolute(&path)?).await,
        Command::Pause => proxy.pause().await,
        Command::Resume => proxy.resume().await,
        Command::LostAndFound => {
            for file in proxy.list_lost_and_found().await? {
                println!("{file}");
            }
            Ok(())
        }
        Command::Logout {
            finish,
            move_to,
            discard,
        } => {
            let (mode, dir) = match (finish, move_to, discard) {
                (true, _, _) => ("finish", String::new()),
                (_, Some(dir), _) => ("move", absolute(&dir)?),
                (_, _, true) => ("discard", String::new()),
                _ => ("", String::new()),
            };
            proxy.logout(mode, &dir).await
        }
        Command::OpenPointer { .. } => unreachable!("tratado sem o serviço"),
    }
}

async fn login(
    proxy: &ServiceProxy<'_>,
    client_id: Option<String>,
    secret_file: Option<PathBuf>,
    i18n: &I18n,
) -> Res {
    println!("{}", i18n.tr("cli-client-guide", &[]));
    let client_id = match client_id {
        Some(id) => id,
        None => prompt(&i18n.tr("cli-client-id", &[]), false)?,
    };
    let secret = match secret_file {
        Some(file) => std::fs::read_to_string(file).map_err(failed)?,
        None => prompt(&i18n.tr("cli-client-secret", &[]), true)?,
    };
    let mut finished = proxy.receive_login_finished().await?;
    let url = proxy.begin_login(client_id.trim(), secret.trim()).await?;
    println!("{}\n{url}", i18n.tr("cli-open-url", &[]));
    let signal = finished
        .next()
        .await
        .ok_or_else(|| failed(i18n.tr("cli-service-gone", &[])))?;
    let args = signal.args()?;
    if args.ok {
        println!("{}", i18n.tr("cli-login-ok", &[("email", args.email)]));
        Ok(())
    } else {
        Err(failed(
            i18n.tr("cli-login-failed", &[("error", args.error)]),
        ))
    }
}

/// Lê uma linha do terminal; com `hidden`, sem mostrar o que é digitado.
fn prompt(label: &str, hidden: bool) -> Res<String> {
    print!("{label} ");
    std::io::stdout().flush().map_err(failed)?;
    let stdin = std::io::stdin();
    let restore = hidden.then(|| hide_echo(&stdin)).flatten();
    let mut line = String::new();
    let read = stdin.lock().read_line(&mut line);
    if let Some(original) = restore {
        let _ =
            rustix::termios::tcsetattr(&stdin, rustix::termios::OptionalActions::Now, &original);
        println!();
    }
    read.map_err(failed)?;
    Ok(line.trim().into())
}

/// Desliga o eco do terminal e devolve a configuração anterior (`None` fora de um terminal).
fn hide_echo(stdin: &std::io::Stdin) -> Option<rustix::termios::Termios> {
    use rustix::termios::{LocalModes, OptionalActions, tcgetattr, tcsetattr};

    let original = tcgetattr(stdin).ok()?;
    let mut quiet = original.clone();
    quiet.local_modes.remove(LocalModes::ECHO);
    tcsetattr(stdin, OptionalActions::Now, &quiet).ok()?;
    Some(original)
}

async fn status(proxy: &ServiceProxy<'_>, i18n: &I18n) -> Res {
    let status = proxy.status().await?;
    if status.account.is_empty() {
        println!("{}", i18n.tr("not-logged-in", &[]));
        return Ok(());
    }
    let line = |key: &str, args: &[(&str, &str)]| println!("{}", i18n.tr(key, args));
    line("cli-status-account", &[("account", &status.account)]);
    line("cli-status-mount", &[("path", &status.mount_point)]);
    let pending = status.pending_uploads.to_string();
    line("cli-status-pending", &[("count", &pending)]);
    if status.paused {
        line("cli-status-paused", &[]);
    }
    if status.offline {
        line("cli-status-offline", &[]);
    }
    if status.last_sync_ms > 0 {
        line(
            "cli-status-last-sync",
            &[("when", &rfc3339(status.last_sync_ms))],
        );
    }
    if status.errors > 0 {
        line(
            "cli-status-errors",
            &[("count", &status.errors.to_string())],
        );
    }
    Ok(())
}
