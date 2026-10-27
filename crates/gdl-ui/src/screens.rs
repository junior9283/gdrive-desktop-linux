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

pub const SCREENS: [Screen; 11] = [
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
    screen("uninstall", 520.0, 480.0),
    // As duas abrem com o id do item (`share:<id>`).
    screen("share", 520.0, 640.0),
    screen("versions", 560.0, 480.0),
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

/// Um pedido de tela, `tela` ou `tela:argumento` (o item da janela "Compartilhar", por exemplo).
/// Os nomes das telas não têm `:`.
pub fn request(text: &str) -> Result<(&'static Screen, Option<&str>), String> {
    let (name, arg) = text
        .split_once(':')
        .map_or((text, None), |(name, arg)| (name, Some(arg)));
    Ok((find(name)?, arg))
}

/// O pedido de tela da linha de comando (`--open <tela>[:argumento]`), já conferido; sem pedido,
/// a principal. Com `--dbus` (a ativação pelo D-Bus, que chega com o pedido de tela logo depois),
/// nenhum.
pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<Option<String>, String> {
    let mut args = args.into_iter();
    let mut screen = Some(HOME.to_owned());
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--open" => {
                let text = args.next().ok_or("--open sem tela")?;
                request(&text)?;
                screen = Some(text);
            }
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
        assert_eq!(from_args(args(&[])).unwrap().unwrap(), "home");
        assert_eq!(
            from_args(args(&["--open", "about"])).unwrap().unwrap(),
            "about"
        );
        assert!(from_args(args(&["--open", "x"])).is_err());
        assert!(from_args(args(&["--open"])).is_err());
        assert!(from_args(args(&["--outro"])).is_err());
    }

    #[test]
    fn a_request_can_carry_an_argument() {
        let (screen, arg) = request("share:1AbC:x").unwrap();
        assert_eq!((screen.name, arg), ("share", Some("1AbC:x")));
        assert_eq!(request("about").unwrap(), (find("about").unwrap(), None));
        assert!(request("x:1").is_err());
        assert_eq!(
            from_args(args(&["--open", "versions:9"])),
            Ok(Some("versions:9".into()))
        );
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
