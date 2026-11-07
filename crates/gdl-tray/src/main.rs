//! `gdrive-desktop-linux-tray`: o ícone na bandeja. Só conversa com o serviço e com a janela
//! (nunca com o Google).

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use futures_util::StreamExt;
use gdl_common::config::{Preferences, default_web_browser, load_preferences};
use gdl_common::i18n::I18n;
use gdl_common::links::DRIVE_WEB;
use gdl_common::paths::Paths;
use gdl_common::paths::{exe_dir, startup_exe};
use gdl_common::relaunch::{exec_startup_exe, outdated_for};
use gdl_ipc::launcher::Launcher;
use gdl_ipc::{BUS_NAME, ServiceProxy, Status};
use gdl_tray::hotkey::{DEFAULT_TRIGGER, Hotkey};
use gdl_tray::model::{Action, MenuItem, TrayView, is_signed_out};
use ksni::TrayMethods;
use tokio::signal::unix::{SignalKind, signal};
use tokio::sync::{mpsc, watch};
use tokio::time::{Instant, MissedTickBehavior, interval_at};
use zbus::Connection;

type Error = Box<dyn std::error::Error>;

/// Sem serviço, de quanto em quanto tempo tentar de novo.
const RETRY: Duration = Duration::from_secs(5);
/// Troca de quadro da animação de "sincronizando".
const FRAME: Duration = Duration::from_millis(300);

/// A camada fina sobre o `ksni`: mostra o `TrayView` e manda as ações para o laço principal.
struct Tray {
    view: TrayView,
    frame: usize,
    title: String,
    theme_path: String,
    actions: mpsc::UnboundedSender<Action>,
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        gdl_common::brand::PACKAGE.into()
    }

    fn title(&self) -> String {
        self.title.clone()
    }

    fn icon_name(&self) -> String {
        self.view.icon_at(self.frame).into()
    }

    fn icon_theme_path(&self) -> String {
        self.theme_path.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        ksni::ToolTip {
            title: self.title.clone(),
            description: self.view.tooltip.clone(),
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.actions.send(self.view.click);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        self.view
            .items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => ksni::MenuItem::Separator,
                MenuItem::Disabled(label) => ksni::menu::StandardItem {
                    label: label.clone(),
                    enabled: false,
                    ..Default::default()
                }
                .into(),
                MenuItem::Item { label, action } => {
                    let action = *action;
                    ksni::menu::StandardItem {
                        label: label.clone(),
                        activate: Box::new(move |tray: &mut Tray| {
                            let _ = tray.actions.send(action);
                        }),
                        ..Default::default()
                    }
                    .into()
                }
            })
            .collect()
    }
}

/// Onde estão os ícones `gdl-*` (estrutura de tema): os instalados ao lado do programa ou, num
/// build de desenvolvimento, os do repositório.
fn theme_path() -> String {
    let installed = exe_dir().map(|dir| {
        dir.join("../share")
            .join(gdl_common::brand::PACKAGE)
            .join("icons")
    });
    #[cfg(debug_assertions)]
    let installed = installed.into_iter().chain([PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../assets/icons"
    ))]);
    installed
        .into_iter()
        .find(|dir| dir.is_dir())
        .map(|dir| dir.display().to_string())
        .unwrap_or_default()
}

/// As preferências (política, usuário e padrão), lidas dos mesmos arquivos que o serviço lê.
fn preferences() -> Option<Preferences> {
    load_preferences(&Paths::from_env()).ok().map(|(p, _)| p)
}

/// `xdg-open` (ou o navegador definido) sem travar o laço.
async fn open_outside(target: String, browser: Option<PathBuf>) -> Result<(), Error> {
    tokio::task::spawn_blocking(move || gdl_common::browser::open(&target, browser.as_deref()))
        .await??;
    Ok(())
}

struct App {
    i18n: I18n,
    conn: Connection,
    service: ServiceProxy<'static>,
    launcher: Launcher,
    handle: ksni::Handle<Tray>,
    /// O último `Status`; `None` com o serviço fora do ar.
    status: Option<Status>,
    /// O que a bandeja mostra agora.
    view: TrayView,
    onboarding_checked: bool,
    /// O serviço voltou em outra versão: a bandeja sai do laço e se reinicia no programa novo.
    restart: Option<String>,
}

impl App {
    /// Relê o estado e atualiza o ícone, se algo visível mudou. Na primeira vez sem conta, abre
    /// o primeiro uso (salvo `DisableOnboardingDialog`).
    async fn refresh(&mut self) {
        // O registro detalhado pode mudar pela janela: é lido junto, a cada vez.
        let (status, verbose, update) = tokio::join!(
            self.service.status(""),
            self.service.verbose_logging(),
            self.service.update_info()
        );
        // Ao achar o serviço (na partida e quando ele volta, numa atualização), confere a versão.
        let came_back = self.status.is_none() && status.is_ok();
        self.status = status.ok();
        if came_back
            && let Ok(version) = self.service.service_version().await
            && outdated_for(&version)
        {
            tracing::info!(versao = %version, "o serviço foi atualizado; reiniciando a bandeja");
            self.restart = Some(version);
            return;
        }
        let verbose = verbose.unwrap_or(false);
        let signed_out = self.status.as_ref().is_some_and(is_signed_out);
        let view = match &self.status {
            Some(status) => TrayView::from_status(status, verbose, &self.i18n)
                .with_update(&update.unwrap_or_default(), &self.i18n),
            None => TrayView::service_down(&self.i18n),
        };
        if view != self.view {
            self.view = view.clone();
            self.handle.update(|tray| tray.view = view).await;
        }
        if signed_out && !self.onboarding_checked {
            self.onboarding_checked = true;
            if !preferences().is_some_and(|p| p.disable_onboarding_dialog) {
                self.launcher.open("onboarding").await;
            }
        }
    }

    /// Faz o que o menu pediu; `false` quando a bandeja deve sair.
    async fn run(&mut self, action: Action) -> bool {
        let done: Result<(), Error> = match action {
            Action::Open(screen) => {
                self.launcher.open(screen).await;
                Ok(())
            }
            Action::OpenFolder => match &self.status {
                Some(status) => open_outside(status.mount_point.clone(), None).await,
                None => Ok(()),
            },
            Action::OpenWeb => {
                let browser = default_web_browser(&Paths::from_env());
                open_outside(DRIVE_WEB.into(), browser).await
            }
            Action::Pause => self.service.pause().await.map_err(Error::from),
            Action::Resume => self.service.resume().await.map_err(Error::from),
            Action::GenerateDiagnostics => self
                .service
                .generate_diagnostics()
                .await
                .map(drop)
                .map_err(Error::from),
            Action::SetVerbose(on) => {
                let done = self.service.set_verbose_logging(on).await;
                // O rótulo do item segue o serviço.
                self.refresh().await;
                done.map_err(Error::from)
            }
            Action::StartService => self.start_service().await,
            Action::StartUpdate => self.service.start_update().await.map_err(Error::from),
            // Com o serviço no ar, a saída vem com `Quitting`; sem ele, sai já.
            Action::Quit => return self.service.quit().await.is_ok(),
        };
        if let Err(e) = done {
            tracing::warn!(erro = %e, ?action, "ação do menu falhou");
        }
        true
    }

    /// Inicia o serviço pela ativação do D-Bus.
    async fn start_service(&mut self) -> Result<(), Error> {
        let dbus = zbus::fdo::DBusProxy::new(&self.conn).await?;
        dbus.start_service_by_name(BUS_NAME.try_into()?, 0).await?;
        self.refresh().await;
        Ok(())
    }
}

/// O atalho que `SearchHotKey` pede (o padrão, sem a preferência).
fn search_trigger() -> String {
    preferences()
        .and_then(|p| p.search_hot_key)
        .filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_TRIGGER.into())
}

/// Mantém o atalho da pesquisa registrado no portal com o gatilho de `trigger`, refazendo o
/// registro quando ele muda. Sem portal, só registra no log: a bandeja segue sem atalho.
async fn keep_search_hotkey(
    conn: Connection,
    description: String,
    mut trigger: watch::Receiver<String>,
    actions: mpsc::UnboundedSender<Action>,
) {
    loop {
        let wanted = trigger.borrow_and_update().clone();
        let actions = actions.clone();
        let on_press = move || {
            let _ = actions.send(Action::Open("search"));
        };
        let hotkey = match Hotkey::register(&conn, &wanted, &description, on_press).await {
            Ok(hotkey) => Some(hotkey),
            Err(e) => {
                tracing::info!(erro = %e, atalho = %wanted, "sem atalho global da pesquisa");
                None
            }
        };
        let changed = trigger.changed().await;
        if let Some(hotkey) = hotkey {
            hotkey.unregister().await;
        }
        if changed.is_err() {
            return;
        }
    }
}

/// Um relógio que, se ficou desligado no `select!`, não entrega os toques perdidos de uma vez.
fn ticker(every: Duration) -> tokio::time::Interval {
    let mut ticker = interval_at(Instant::now() + every, every);
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
    ticker
}

async fn run() -> Result<(), Error> {
    let i18n = I18n::from_env();
    let conn = Connection::session().await?;
    // Uma bandeja só: a segunda (o "Google Drive" do menu com o app aberto) mostra a principal.
    if !gdl_ipc::claim_name(&conn, gdl_ipc::TRAY_BUS_NAME).await? {
        Launcher::new(conn, Launcher::default_program())
            .open("home")
            .await;
        return Ok(());
    }
    let (actions, mut requested) = mpsc::unbounded_channel();
    let (trigger, triggers) = watch::channel(search_trigger());
    let hotkey = tokio::spawn(keep_search_hotkey(
        conn.clone(),
        i18n.tr("tray-hotkey-search", &[]),
        triggers,
        actions.clone(),
    ));
    let view = TrayView::service_down(&i18n);
    let handle = Tray {
        view: view.clone(),
        frame: 0,
        title: i18n.tr("notify-title", &[]),
        theme_path: theme_path(),
        actions,
    }
    // Na partida da sessão, a bandeja pode subir antes do Plasma: espera o painel aparecer em
    // vez de desistir.
    .assume_sni_available(true)
    .spawn()
    .await?;
    if let Err(e) = gdl_ipc::ensure_activatable(&conn).await {
        tracing::warn!(erro = %e, "o barramento não releu a configuração");
    }
    let service = ServiceProxy::new(&conn).await?;
    let mut changes = service.receive_status_changed().await?;
    let mut update_changes = service.receive_update_changed().await?;
    let mut quitting = service.receive_quitting().await?;
    let mut prefs_changed = service.receive_preferences_changed().await?;
    // O serviço caiu: sem isto o ícone seguiria "em dia" até a próxima mudança de estado.
    let mut lost = gdl_ipc::service_lost(&conn).await?;
    let mut app = App {
        launcher: Launcher::new(conn.clone(), Launcher::default_program()),
        i18n,
        conn,
        service,
        handle,
        status: None,
        view,
        onboarding_checked: false,
        restart: None,
    };
    app.refresh().await;
    let (mut retry, mut frames) = (ticker(RETRY), ticker(FRAME));
    let mut terminate = signal(SignalKind::terminate())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    while app.restart.is_none() {
        tokio::select! {
            _ = retry.tick(), if app.status.is_none() => app.refresh().await,
            Some(_) = changes.next() => app.refresh().await,
            Some(_) = update_changes.next() => app.refresh().await,
            // Esquece o estado antes: a releitura pode subir o serviço novo (ativação do D-Bus),
            // e a volta dele confere a versão.
            Some(_) = lost.next() => {
                app.status = None;
                app.refresh().await;
            }
            Some(_) = quitting.next() => break,
            Some(_) = prefs_changed.next() => {
                let wanted = search_trigger();
                trigger.send_if_modified(|current| {
                    let changed = *current != wanted;
                    if changed {
                        *current = wanted;
                    }
                    changed
                });
            }
            _ = frames.tick(), if app.view.animated() => {
                app.handle.update(|t| t.frame = t.frame.wrapping_add(1)).await;
            }
            Some(action) = requested.recv() => {
                if !app.run(action).await {
                    break;
                }
            }
            _ = terminate.recv() => break,
            _ = interrupt.recv() => break,
        }
    }
    // Sem gatilho, a tarefa solta o atalho no portal e termina (com prazo: o registro pode
    // estar esperando o usuário responder ao portal).
    drop(trigger);
    let _ = tokio::time::timeout(Duration::from_secs(2), hotkey).await;
    app.handle.shutdown().await;
    if let Some(version) = app.restart {
        let error = exec_startup_exe(Some(&version));
        tracing::error!(erro = %error, "não deu para reiniciar a bandeja");
    }
    Ok(())
}

#[tokio::main]
async fn main() -> ExitCode {
    // Antes de qualquer troca do pacote: depois dela, `/proc/self/exe` aponta o arquivo apagado.
    startup_exe();
    gdl_common::crash::install(gdl_common::brand::TRAY_PROGRAM);
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
    match run().await {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            tracing::error!(erro = %e, "a bandeja parou");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use gdl_ipc::testing::PrivateBus;
    use gdl_tray::model::{STILL_ICONS, SYNC_ICONS};

    use super::*;

    #[tokio::test]
    async fn only_the_first_tray_stays() {
        let bus = PrivateBus::start();
        let (first, second) = (bus.connect().await, bus.connect().await);

        let stays = gdl_ipc::claim_name(&first, gdl_ipc::TRAY_BUS_NAME)
            .await
            .unwrap();
        let again = gdl_ipc::claim_name(&second, gdl_ipc::TRAY_BUS_NAME)
            .await
            .unwrap();

        assert!(stays);
        assert!(!again);
    }

    /// O repositório tem um ícone para cada nome que o modelo usa.
    #[test]
    fn every_icon_name_has_a_file() {
        let dir = PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../assets/icons/hicolor/scalable/status"
        ));
        for name in STILL_ICONS.iter().chain(&SYNC_ICONS) {
            assert!(dir.join(format!("{name}.svg")).is_file(), "{name}");
        }
    }

    #[test]
    fn found_icons_in_development() {
        assert!(theme_path().ends_with("assets/icons"), "{}", theme_path());
    }
}
