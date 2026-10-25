//! `gdrive-desktop-linux-ui`: as janelas do app (Tauri). Abre quando a bandeja ou o atalho pedem
//! uma tela e sai quando a última janela fecha. Tudo o que mostra vem do serviço.

use std::process::ExitCode;
use std::sync::Arc;

use gdl_common::i18n::env_lang;
use gdl_ui::bridge::{Bridge, Emit, RECONNECT, UiError};
use gdl_ui::screens::{self, Screen};
use gdl_ui::single::{Requests, SingleInstance};
use gdl_ui::strings::UiStrings;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, State, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
use zbus::Connection;

/// Mostra a tela `screen`: traz para a frente a janela dela, ou abre uma.
fn show(app: &AppHandle, screen: &Screen) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window(screen.name) {
        window.unminimize()?;
        window.show()?;
        return window.set_focus();
    }
    let strings = app.state::<UiStrings>();
    let title = strings.title(screen).unwrap_or(screen.name);
    let url = WebviewUrl::App(format!("index.html?screen={}", screen.name).into());
    WebviewWindowBuilder::new(app, screen.name, url)
        .title(title)
        .inner_size(screen.width, screen.height)
        .build()?;
    Ok(())
}

fn show_named(app: &AppHandle, name: &str) -> Result<(), String> {
    show(app, screens::find(name)?).map_err(|e| e.to_string())
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
fn open_screen(app: AppHandle, screen: String) -> Result<(), String> {
    show_named(&app, &screen)
}

/// Fica com o nome da janela e devolve a ponte e os pedidos de tela; `None` se outra janela já
/// estava aberta (e recebeu o pedido).
async fn start(screen: &Screen) -> zbus::Result<Option<(Bridge, Requests)>> {
    let conn = Connection::session().await?;
    match SingleInstance::claim(&conn, screen.name).await? {
        SingleInstance::Primary(requests) => Ok(Some((Bridge::new(&conn).await?, requests))),
        SingleInstance::Secondary => Ok(None),
    }
}

fn main() -> ExitCode {
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
    let (bridge, mut requests) = match tauri::async_runtime::block_on(start(screen)) {
        Ok(Some((bridge, requests))) => (Arc::new(bridge), requests),
        Ok(None) => return ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(erro = %e, "sem barramento de sessão");
            return ExitCode::FAILURE;
        }
    };
    let result = tauri::Builder::default()
        .manage(bridge.clone())
        .manage(UiStrings::new(&env_lang()))
        .invoke_handler(tauri::generate_handler![
            call,
            strings,
            clipboard_write,
            close_window,
            open_screen
        ])
        .setup(move |app| {
            let handle = app.handle().clone();
            show(&handle, screen)?;
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
                    if let Err(e) = show_named(&handle, &name) {
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
