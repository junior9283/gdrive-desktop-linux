//! O que a bandeja mostra a partir do `Status` do serviço: ícone, dica e menu. Puro, para testar
//! sem área de trabalho.

use gdl_common::i18n::I18n;
use gdl_ipc::Status;

/// Quadros da animação de "sincronizando".
pub const SYNC_ICONS: [&str; 4] = [
    "gdl-syncing-1",
    "gdl-syncing-2",
    "gdl-syncing-3",
    "gdl-syncing-4",
];

/// Os ícones parados.
pub const STILL_ICONS: [&str; 5] = [
    "gdl-synced",
    "gdl-error",
    "gdl-paused",
    "gdl-offline",
    "gdl-signed-out",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Still(&'static str),
    /// Animado com `SYNC_ICONS`.
    Syncing,
}

/// O que um item do menu (ou o clique no ícone) pede.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Abre uma tela da janela (`Launcher::open`).
    Open(&'static str),
    OpenFolder,
    OpenWeb,
    Pause,
    Resume,
    /// Grava o pacote de diagnóstico; o serviço abre a pasta onde ele ficou.
    GenerateDiagnostics,
    /// Liga ou desliga o registro detalhado.
    SetVerbose(bool),
    StartService,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuItem {
    Item { label: String, action: Action },
    Separator,
}

fn item(i18n: &I18n, key: &str, action: Action) -> MenuItem {
    MenuItem::Item {
        label: i18n.tr(key, &[]),
        action,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayView {
    pub icon: Icon,
    pub tooltip: String,
    pub items: Vec<MenuItem>,
    /// O clique esquerdo.
    pub click: Action,
}

/// Sem conta conectada.
pub fn is_signed_out(status: &Status) -> bool {
    status.state == "signed-out"
}

impl TrayView {
    /// Com mais de uma conta conectada (`Status::accounts`), a dica diz quantas são. `verbose` é
    /// o registro detalhado do serviço (o item do menu o inverte).
    pub fn from_status(status: &Status, verbose: bool, i18n: &I18n) -> TrayView {
        let pending = u64::from(status.syncing);
        let errors = u64::from(status.errors);
        let still = Icon::Still;
        let (icon, tooltip) = match status.state.as_str() {
            "signed-out" => return signed_out(i18n, "tray-signed-out", false),
            "syncing" => (Icon::Syncing, i18n.tr_count("tray-syncing", pending)),
            "starting" => (Icon::Syncing, i18n.tr("tray-starting", &[])),
            "paused" => (still("gdl-paused"), i18n.tr_count("tray-paused", pending)),
            "offline" => (still("gdl-offline"), i18n.tr_count("tray-offline", pending)),
            "quota-full" => (still("gdl-error"), i18n.tr("tray-quota-full", &[])),
            "needs-sign-in" => (still("gdl-error"), i18n.tr("tray-needs-sign-in", &[])),
            "error" => (still("gdl-error"), i18n.tr_count("tray-error", errors)),
            _ if status.accounts.len() > 1 => (
                still("gdl-synced"),
                i18n.tr_count("tray-accounts", status.accounts.len() as u64),
            ),
            _ => (still("gdl-synced"), i18n.tr("tray-synced", &[])),
        };
        let pause = if status.paused {
            item(i18n, "tray-resume", Action::Resume)
        } else {
            item(i18n, "tray-pause", Action::Pause)
        };
        // Com algo para subir, a janela pergunta antes de sair (a bandeja não tem diálogo).
        let quit = if status.syncing > 0 || status.pending_uploads > 0 {
            Action::Open("home:quit")
        } else {
            Action::Quit
        };
        let verbose_item = if verbose {
            item(i18n, "tray-verbose-off", Action::SetVerbose(false))
        } else {
            item(i18n, "tray-verbose-on", Action::SetVerbose(true))
        };
        TrayView {
            icon,
            tooltip,
            items: vec![
                item(i18n, "tray-open-folder", Action::OpenFolder),
                item(i18n, "tray-open-web", Action::OpenWeb),
                item(i18n, "tray-search", Action::Open("search")),
                pause,
                MenuItem::Separator,
                item(i18n, "tray-offline-files", Action::Open("offline")),
                MenuItem::Item {
                    label: i18n.tr_count("tray-errors", errors),
                    action: Action::Open("errors"),
                },
                item(i18n, "tray-preferences", Action::Open("preferences")),
                item(i18n, "tray-about", Action::Open("about")),
                item(i18n, "tray-shared-drives", Action::Open("shared-drives")),
                item(i18n, "tray-feedback", Action::Open("feedback")),
                verbose_item,
                item(i18n, "tray-diagnostics", Action::GenerateDiagnostics),
                MenuItem::Separator,
                // A bandeja não tem diálogo: abre a janela, que pede a confirmação.
                item(i18n, "tray-reset", Action::Open("home:reset-preferences")),
                item(
                    i18n,
                    "tray-disconnect-all",
                    Action::Open("home:disconnect-all"),
                ),
                MenuItem::Separator,
                item(i18n, "tray-quit", quit),
            ],
            click: Action::Open("home"),
        }
    }

    /// O serviço não respondeu: como sem login, mais "Iniciar o serviço".
    pub fn service_down(i18n: &I18n) -> TrayView {
        signed_out(i18n, "tray-no-service", true)
    }

    pub fn animated(&self) -> bool {
        self.icon == Icon::Syncing
    }

    /// O nome do ícone no quadro `frame` da animação.
    pub fn icon_at(&self, frame: usize) -> &'static str {
        match self.icon {
            Icon::Still(name) => name,
            Icon::Syncing => SYNC_ICONS[frame % SYNC_ICONS.len()],
        }
    }
}

fn signed_out(i18n: &I18n, tooltip: &str, service_down: bool) -> TrayView {
    let first = if service_down {
        item(i18n, "tray-start-service", Action::StartService)
    } else {
        item(i18n, "tray-sign-in", Action::Open("onboarding"))
    };
    TrayView {
        icon: Icon::Still("gdl-signed-out"),
        tooltip: i18n.tr(tooltip, &[]),
        items: vec![
            first,
            item(i18n, "tray-about", Action::Open("about")),
            MenuItem::Separator,
            item(i18n, "tray-quit", Action::Quit),
        ],
        click: Action::Open("onboarding"),
    }
}

#[cfg(test)]
mod tests {
    use gdl_ipc::AccountSummary;

    use super::*;

    fn status(state: &str, syncing: u32, errors: u32) -> Status {
        Status {
            state: state.into(),
            syncing,
            errors,
            paused: state == "paused",
            offline: state == "offline",
            ..Status::default()
        }
    }

    fn labels(view: &TrayView) -> Vec<&str> {
        view.items
            .iter()
            .map(|i| match i {
                MenuItem::Item { label, .. } => label.as_str(),
                MenuItem::Separator => "---",
            })
            .collect()
    }

    #[test]
    fn tooltip_and_icon_for_each_state() {
        let pt = I18n::new("pt_BR");
        let en = I18n::new("en_US");
        let table = [
            (
                status("synced", 0, 0),
                "gdl-synced",
                "Tudo sincronizado",
                "Everything is up to date",
            ),
            (
                status("syncing", 3, 0),
                "gdl-syncing-1",
                "Sincronizando 3 arquivos",
                "Syncing 3 files",
            ),
            (
                status("syncing", 1, 0),
                "gdl-syncing-1",
                "Sincronizando 1 arquivo",
                "Syncing 1 file",
            ),
            (
                status("offline", 2, 0),
                "gdl-offline",
                "Sem internet (2 para sincronizar)",
                "No internet connection (2 to sync)",
            ),
            (status("paused", 0, 0), "gdl-paused", "Pausado", "Paused"),
            (
                status("paused", 5, 0),
                "gdl-paused",
                "Pausado (5 para sincronizar)",
                "Paused (5 to sync)",
            ),
            (
                status("quota-full", 0, 0),
                "gdl-error",
                "Armazenamento cheio",
                "Storage is full",
            ),
            (
                status("needs-sign-in", 0, 0),
                "gdl-error",
                "Entre de novo na conta",
                "Sign in to the account again",
            ),
            (
                status("error", 0, 2),
                "gdl-error",
                "2 arquivos não foram sincronizados",
                "2 files could not be synced",
            ),
            (
                status("signed-out", 0, 0),
                "gdl-signed-out",
                "Você não fez login",
                "You are not signed in",
            ),
        ];
        for (status, icon, pt_tip, en_tip) in table {
            let view = TrayView::from_status(&status, false, &pt);
            assert_eq!(
                (view.icon_at(0), view.tooltip.as_str()),
                (icon, pt_tip),
                "{status:?}"
            );
            let view = TrayView::from_status(&status, false, &en);
            assert_eq!(view.tooltip, en_tip, "{status:?}");
        }
        let mut two = status("synced", 0, 0);
        two.accounts = vec![AccountSummary::default(); 2];
        assert_eq!(
            TrayView::from_status(&two, false, &pt).tooltip,
            "Conectado com 2 contas"
        );
        assert_eq!(
            TrayView::from_status(&two, false, &en).tooltip,
            "Connected with 2 accounts"
        );
        two.state = "paused".into();
        assert_eq!(TrayView::from_status(&two, false, &pt).tooltip, "Pausado");
    }

    #[test]
    fn syncing_icon_is_animated() {
        let i18n = I18n::new("en");
        let view = TrayView::from_status(&status("syncing", 1, 0), false, &i18n);
        let frames: Vec<&str> = (0..5).map(|f| view.icon_at(f)).collect();
        assert_eq!(
            frames,
            [
                "gdl-syncing-1",
                "gdl-syncing-2",
                "gdl-syncing-3",
                "gdl-syncing-4",
                "gdl-syncing-1"
            ]
        );
        let still = TrayView::from_status(&status("synced", 0, 0), false, &i18n);
        assert_eq!(still.icon_at(3), "gdl-synced");
    }

    #[test]
    fn menu_without_login_has_sign_in_only() {
        let i18n = I18n::new("pt_BR");
        let view = TrayView::from_status(&status("signed-out", 0, 0), false, &i18n);
        assert_eq!(labels(&view), ["Fazer login", "Sobre", "---", "Sair"]);
        assert_eq!(view.click, Action::Open("onboarding"));

        let down = TrayView::service_down(&i18n);
        assert_eq!(labels(&down)[0], "Iniciar o serviço");
        assert_eq!(down.icon_at(0), "gdl-signed-out");
    }

    #[test]
    fn starting_service_does_not_ask_to_sign_in() {
        let i18n = I18n::new("pt_BR");
        let view = TrayView::from_status(&status("starting", 0, 0), false, &i18n);
        assert_eq!(view.click, Action::Open("home"));
        assert!(view.animated());
        assert_eq!(view.tooltip, "Conectando à sua conta…");
        assert!(!is_signed_out(&status("starting", 0, 0)));
    }

    #[test]
    fn signed_in_menu_follows_the_spec() {
        let i18n = I18n::new("pt_BR");
        let view = TrayView::from_status(&status("synced", 0, 0), false, &i18n);
        assert_eq!(
            labels(&view),
            [
                "Abrir a pasta do Google Drive",
                "Abrir o Google Drive no navegador",
                "Pesquisar",
                "Pausar a sincronização",
                "---",
                "Arquivos off-line",
                "Lista de erros",
                "Preferências",
                "Sobre",
                "Gerenciar drives compartilhados",
                "Enviar feedback",
                "Ativar o registro detalhado",
                "Gerar informações de diagnóstico",
                "---",
                "Redefinir as preferências",
                "Desconectar todas as contas",
                "---",
                "Sair",
            ]
        );
        assert!(view.items.contains(&MenuItem::Item {
            label: "Enviar feedback".into(),
            action: Action::Open("feedback"),
        }));
        assert!(view.items.contains(&MenuItem::Item {
            label: "Gerar informações de diagnóstico".into(),
            action: Action::GenerateDiagnostics,
        }));
        assert_eq!(view.click, Action::Open("home"));
        let paused = TrayView::from_status(&status("paused", 0, 0), false, &i18n);
        assert!(labels(&paused).contains(&"Retomar a sincronização"));
    }

    fn action_of(view: &TrayView, wanted: &str) -> Option<Action> {
        view.items.iter().find_map(|i| match i {
            MenuItem::Item { label, action } if label == wanted => Some(*action),
            _ => None,
        })
    }

    #[test]
    fn menu_has_every_more_options_item() {
        let i18n = I18n::new("en");
        let view = TrayView::from_status(&status("synced", 0, 0), false, &i18n);
        assert_eq!(
            action_of(&view, "Manage shared drives"),
            Some(Action::Open("shared-drives"))
        );
        assert_eq!(
            action_of(&view, "Turn on detailed logging"),
            Some(Action::SetVerbose(true))
        );
        // Sem conta, nada que dependa dela.
        let out = TrayView::from_status(&status("signed-out", 0, 0), false, &i18n);
        assert_eq!(action_of(&out, "Disconnect all accounts"), None);
        assert_eq!(action_of(&out, "Manage shared drives"), None);
    }

    #[test]
    fn verbose_label_follows_the_service() {
        let i18n = I18n::new("pt_BR");
        let view = TrayView::from_status(&status("synced", 0, 0), true, &i18n);
        assert_eq!(
            action_of(&view, "Desativar o registro detalhado"),
            Some(Action::SetVerbose(false))
        );
        assert_eq!(action_of(&view, "Ativar o registro detalhado"), None);
    }

    #[test]
    fn tray_confirming_items_open_the_window() {
        // A bandeja não tem diálogo: o que pede confirmação abre a janela com ele.
        let i18n = I18n::new("pt_BR");
        let view = TrayView::from_status(&status("synced", 0, 0), false, &i18n);
        assert_eq!(
            action_of(&view, "Redefinir as preferências"),
            Some(Action::Open("home:reset-preferences"))
        );
        assert_eq!(
            action_of(&view, "Desconectar todas as contas"),
            Some(Action::Open("home:disconnect-all"))
        );
    }

    #[test]
    fn quit_goes_through_the_window() {
        // Com algo para subir, sair pede confirmação, e quem pergunta é a janela.
        let i18n = I18n::new("pt_BR");
        let busy = TrayView::from_status(&status("syncing", 2, 0), false, &i18n);
        assert_eq!(action_of(&busy, "Sair"), Some(Action::Open("home:quit")));
        let waiting = Status {
            pending_uploads: 1,
            ..status("paused", 0, 0)
        };
        let paused = TrayView::from_status(&waiting, false, &i18n);
        assert_eq!(action_of(&paused, "Sair"), Some(Action::Open("home:quit")));
        // Sem nada pendente, sai direto.
        let idle = TrayView::from_status(&status("synced", 0, 0), false, &i18n);
        assert_eq!(action_of(&idle, "Sair"), Some(Action::Quit));
    }

    #[test]
    fn error_item_shows_count() {
        let i18n = I18n::new("pt_BR");
        let view = TrayView::from_status(&status("error", 0, 3), false, &i18n);
        let errors = view.items.iter().find_map(|i| match i {
            MenuItem::Item {
                label,
                action: Action::Open("errors"),
            } => Some(label.as_str()),
            _ => None,
        });
        assert_eq!(errors, Some("Lista de erros (3)"));
    }
}
