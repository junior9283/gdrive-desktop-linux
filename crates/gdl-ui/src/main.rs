//! `gdrive-desktop-linux-ui`: as janelas do app (Tauri). Abre quando a bandeja ou o atalho pedem
//! uma tela e sai quando a última janela fecha. Tudo o que mostra vem do serviço.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use ashpd::desktop::file_chooser::SelectedFiles;
use gdl_common::browser;
use gdl_common::config::default_web_browser;
use gdl_common::i18n::{I18n, env_lang};
use gdl_common::links;
use gdl_common::paths::Paths;
use gdl_ui::bridge::{Bridge, Emit, RECONNECT, UiError};
use gdl_ui::screens::{self, Screen};
use gdl_ui::single::{Requests, SingleInstance};
use gdl_ui::strings::UiStrings;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use zbus::Connection;

/// Mostra a tela `screen`: traz para a frente a janela dela, ou abre uma. `arg` vai para a tela
/// (o texto da pesquisa, por exemplo): na query `?arg=` ou, com a janela já aberta, no evento
/// `screen-arg`.
fn show(app: &AppHandle, screen: &Screen, arg: Option<&str>) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(screen.name) {
        if let Some(arg) = arg {
            window.emit_to(screen.name, "screen-arg", arg)?;
        }
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let title = app.state::<I18n>().tr(&screen.title_key(), &[]);
    let mut query = format!("index.html?screen={}", screen.name);
    if let Some(arg) = arg {
        query.push_str("&arg=");
        query.extend(url::form_urlencoded::byte_serialize(arg.as_bytes()));
    }
    let url = WebviewUrl::App(query.into());
    let mut builder = WebviewWindowBuilder::new(app, screen.name, url)
        .title(title)
        .inner_size(screen.width, screen.height)
        .decorations(!screen.floating)
        .always_on_top(screen.floating)
        .skip_taskbar(screen.floating);
    if screen.floating {
        builder = builder.center();
    }
    builder.build()?;
    Ok(())
}

fn show_named(app: &AppHandle, name: &str, arg: Option<&str>) -> Result<(), String> {
    show(app, screens::find(name)?, arg).map_err(|e| e.to_string())
}

/// Atende um pedido `tela[:argumento]` (da linha de comando ou de `Ui.Open`).
fn show_request(app: &AppHandle, request: &str) -> Result<(), String> {
    let (screen, arg) = screens::request(request)?;
    show_named(app, screen.name, arg)
}

#[tauri::command]
async fn call(
    bridge: State<'_, Arc<Bridge>>,
    method: String,
    args: Value,
) -> Result<Value, UiError> {
    bridge.call(&method, args).await
}

#[tauri::command]
fn strings(strings: State<'_, UiStrings>) -> UiStrings {
    strings.inner().clone()
}

/// As bibliotecas que vão no app, em JSON (geradas no build pelo `build.rs`), para o "Sobre".
#[tauri::command]
fn third_party() -> &'static str {
    include_str!(concat!(env!("OUT_DIR"), "/third-party.json"))
}

#[tauri::command]
fn clipboard_write(app: AppHandle, text: String) -> Result<(), String> {
    // A área de transferência do GTK só pode ser usada na linha principal.
    app.run_on_main_thread(move || {
        gtk::Clipboard::get(&gtk::gdk::SELECTION_CLIPBOARD).set_text(&text);
    })
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn close_window(window: WebviewWindow) -> Result<(), String> {
    window.close().map_err(|e| e.to_string())
}

#[tauri::command]
fn open_screen(app: AppHandle, screen: String, arg: Option<String>) -> Result<(), String> {
    show_named(&app, &screen, arg.as_deref())
}

/// `xdg-open` (ou o navegador definido) sem travar a janela.
async fn open_outside(target: String, browser: Option<PathBuf>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || browser::open(&target, browser.as_deref()))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}

/// Abre no navegador o endereço de `name` (a tabela fixa de `gdl_common::links`), com o
/// navegador de `DefaultWebBrowser` se o administrador definiu um.
#[tauri::command]
async fn open_link(name: String) -> Result<(), String> {
    let url = links::url(&name).ok_or_else(|| format!("link desconhecido: {name}"))?;
    open_outside(url, default_web_browser(&Paths::from_env())).await
}

/// Abre no gerenciador de arquivos a pasta do Drive: a montagem que o serviço informa (a página
/// não escolhe o caminho).
#[tauri::command]
async fn open_mount(bridge: State<'_, Arc<Bridge>>) -> Result<(), String> {
    let mount = bridge.mount_point().await.map_err(|e| e.message)?;
    if mount.is_empty() {
        return Err("sem a pasta do Drive".into());
    }
    open_outside(mount, None).await
}

/// Abre no gerenciador de arquivos a pasta do computador `id` da conta (o caminho vem do serviço,
/// não da página).
#[tauri::command]
async fn open_local_folder(
    bridge: State<'_, Arc<Bridge>>,
    account: String,
    id: i64,
) -> Result<(), String> {
    let path = bridge.local_folder_path(&account, id).await.map_err(|e| e.message)?;
    open_outside(path, None).await
}

/// Pede uma pasta ao usuário pelo portal (o seletor do próprio ambiente); `None` se ele cancelou.
#[tauri::command]
async fn pick_folder(
    bridge: State<'_, Arc<Bridge>>,
    title: String,
) -> Result<Option<String>, String> {
    let request = SelectedFiles::open_file()
        .title(title.as_str())
        .directory(true)
        .modal(true)
        .connection(Some(bridge.connection().clone()))
        .send()
        .await
        .map_err(|e| e.to_string())?;
    let files = match request.response() {
        Ok(files) => files,
        Err(ashpd::Error::Response(ashpd::desktop::ResponseError::Cancelled)) => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let path = files
        .uris()
        .first()
        .and_then(|uri| url::Url::parse(uri.as_str()).ok()?.to_file_path().ok());
    Ok(path.map(|p| p.display().to_string()))
}

/// Desinstala pela CLI ao lado da janela (que pede a senha de administrador se o app veio de um
/// pacote); o erro é a mensagem que ela escreveu.
#[tauri::command]
async fn uninstall(purge: bool) -> Result<(), String> {
    let cli = gdl_common::paths::exe_dir()
        .ok_or("sem a pasta dos programas")?
        .join("gdrive-desktop-linux");
    let mut command = std::process::Command::new(cli);
    command.args(["uninstall", "--yes"]);
    if purge {
        command.arg("--purge");
    }
    let out = tauri::async_runtime::spawn_blocking(move || command.output())
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        return Ok(());
    }
    Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
}

/// Fica com o nome da janela; `None` se outra janela já estava aberta (e recebeu `request`).
async fn start(request: &str) -> zbus::Result<Option<(Bridge, Requests)>> {
    let conn = Connection::session().await?;
    match SingleInstance::claim(&conn, request).await? {
        SingleInstance::Primary(requests) => Ok(Some((Bridge::new(&conn).await?, requests))),
        SingleInstance::Secondary => Ok(None),
    }
}

fn main() -> ExitCode {
    // Antes do GTK: vira a classe das janelas (o app_id no Wayland), pela qual a barra de tarefas
    // e a barra de título acham o `.desktop` e o ícone.
    gtk::glib::set_prgname(Some(gdl_common::APP_ID));
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    let screen = match screens::from_args(std::env::args().skip(1)) {
        Ok(screen) => screen,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::from(2);
        }
    };
    let forward = screen.as_deref().unwrap_or(screens::HOME);
    let (bridge, mut requests) = match tauri::async_runtime::block_on(start(forward)) {
        Ok(Some((bridge, requests))) => (Arc::new(bridge), requests),
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(erro = %e, "sem barramento de sessão");
            return ExitCode::FAILURE;
        }
    };
    let lang = env_lang();
    let result = tauri::Builder::default()
        .manage(bridge.clone())
        // Os títulos das janelas (Rust) e os textos das telas (JavaScript).
        .manage(I18n::new(&lang))
        .manage(UiStrings::new(&lang))
        .invoke_handler(tauri::generate_handler![
            call,
            strings,
            third_party,
            clipboard_write,
            close_window,
            open_screen,
            open_link,
            open_mount,
            open_local_folder,
            pick_folder,
            uninstall
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            if let Some(request) = &screen {
                show_request(&handle, request)?;
            }
            let emit: Emit = Arc::new({
                let handle = handle.clone();
                move |name, payload| {
                    if let Err(e) = handle.emit(name, payload) {
                        tracing::warn!(erro = %e, evento = name, "evento não entregue");
                    }
                }
            });
            tauri::async_runtime::spawn({
                let (bridge, emit) = (bridge.clone(), emit.clone());
                async move {
                    if let Err(e) = bridge.forward_signals(emit).await {
                        tracing::warn!(erro = %e, "sem os sinais do serviço");
                    }
                }
            });
            tauri::async_runtime::spawn(bridge.watch(RECONNECT, emit));
            tauri::async_runtime::spawn(async move {
                while let Some(name) = requests.recv().await {
                    if let Err(e) = show_request(&handle, &name) {
                        tracing::warn!(erro = %e, "pedido de tela recusado");
                    }
                }
            });
            Ok(())
        })
        .run(tauri::generate_context!());
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(erro = %e, "a janela parou");
            ExitCode::FAILURE
        }
    }
}
