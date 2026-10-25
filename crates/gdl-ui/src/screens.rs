//! As telas da janela: cada uma abre numa janela própria, reaproveitada se já estiver aberta.

#[derive(Debug, PartialEq)]
pub struct Screen {
    /// O nome em `--open`, em `Ui.Open` e na query `?screen=` (e o rótulo da janela).
    pub name: &'static str,
    pub width: f64,
    pub height: f64,
    /// Janela flutuante: sem decoração, centralizada, por cima das outras e fora da barra de
    /// tarefas (a pesquisa rápida).
    pub floating: bool,
}

const fn screen(name: &'static str, width: f64, height: f64) -> Screen {
    Screen {
        name,
        width,
        height,
        floating: false,
    }
}

pub const SCREENS: [Screen; 8] = [
    screen(HOME, 900.0, 640.0),
    screen("onboarding", 560.0, 640.0),
    screen("preferences", 760.0, 640.0),
    Screen {
        floating: true,
        ..screen("search", 640.0, 460.0)
    },
    screen("errors", 560.0, 600.0),
    screen("offline", 640.0, 560.0),
    screen("about", 480.0, 420.0),
    screen("notification-settings", 480.0, 520.0),
];

impl Screen {
    /// A chave do título da janela no `main.ftl`.
    pub fn title_key(&self) -> String {
        format!("window-{}", self.name)
    }
}

/// A janela principal (a que abre sem pedido e quando a bandeja já está aberta).
pub const HOME: &str = "home";

pub fn find(name: &str) -> Result<&'static Screen, String> {
    SCREENS
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("tela desconhecida: {name}"))
}

/// A tela pedida na linha de comando (`--open <tela>`); sem pedido, a principal. Com `--dbus`
/// (a ativação pelo D-Bus, que chega com o pedido de tela logo depois), nenhuma.
pub fn from_args(
    args: impl IntoIterator<Item = String>,
) -> Result<Option<&'static Screen>, String> {
    let mut args = args.into_iter();
    let mut screen = Some(find(HOME)?);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--open" => screen = Some(find(&args.next().ok_or("--open sem tela")?)?),
            "--dbus" => screen = None,
            other => return Err(format!("argumento desconhecido: {other}")),
        }
    }
    Ok(screen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn open_argument_picks_the_screen() {
        assert_eq!(from_args(args(&[])).unwrap().unwrap().name, "home");
        assert_eq!(
            from_args(args(&["--open", "about"])).unwrap().unwrap().name,
            "about"
        );
        assert!(from_args(args(&["--open", "x"])).is_err());
        assert!(from_args(args(&["--open"])).is_err());
        assert!(from_args(args(&["--outro"])).is_err());
    }

    #[test]
    fn dbus_activation_opens_nothing_by_itself() {
        assert_eq!(from_args(args(&["--dbus"])), Ok(None));
    }

    #[test]
    fn only_search_floats() {
        let floating: Vec<_> = SCREENS
            .iter()
            .filter(|s| s.floating)
            .map(|s| s.name)
            .collect();
        assert_eq!(floating, ["search"]);
    }

    #[test]
    fn every_screen_has_a_title() {
        let i18n = gdl_common::i18n::I18n::new("pt_BR");
        for screen in &SCREENS {
            assert_ne!(i18n.tr(&screen.title_key(), &[]), screen.title_key());
        }
    }
}
