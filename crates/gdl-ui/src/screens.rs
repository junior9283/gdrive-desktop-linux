//! As telas da janela: cada uma abre numa janela própria, reaproveitada se já estiver aberta.

#[derive(Debug, PartialEq)]
pub struct Screen {
    /// O nome em `--open`, em `Ui.Open` e na query `?screen=` (e o rótulo da janela).
    pub name: &'static str,
    pub width: f64,
    pub height: f64,
}

const fn screen(name: &'static str, width: f64, height: f64) -> Screen {
    Screen {
        name,
        width,
        height,
    }
}

pub const SCREENS: [Screen; 8] = [
    screen("home", 900.0, 640.0),
    screen("onboarding", 560.0, 640.0),
    screen("preferences", 760.0, 640.0),
    screen("search", 640.0, 460.0),
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

pub fn find(name: &str) -> Result<&'static Screen, String> {
    SCREENS
        .iter()
        .find(|s| s.name == name)
        .ok_or_else(|| format!("tela desconhecida: {name}"))
}

/// A tela pedida na linha de comando (`--open <tela>`); sem pedido, a principal.
pub fn from_args(args: impl IntoIterator<Item = String>) -> Result<&'static Screen, String> {
    let mut args = args.into_iter();
    let mut name = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--open" => name = Some(args.next().ok_or("--open sem tela")?),
            other => return Err(format!("argumento desconhecido: {other}")),
        }
    }
    find(name.as_deref().unwrap_or("home"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn open_argument_picks_the_screen() {
        assert_eq!(from_args(args(&[])).unwrap().name, "home");
        assert_eq!(from_args(args(&["--open", "about"])).unwrap().name, "about");
        assert!(from_args(args(&["--open", "x"])).is_err());
        assert!(from_args(args(&["--open"])).is_err());
        assert!(from_args(args(&["--outro"])).is_err());
    }

    #[test]
    fn every_screen_has_a_title() {
        let i18n = gdl_common::i18n::I18n::new("pt_BR");
        for screen in &SCREENS {
            assert_ne!(i18n.tr(&screen.title_key(), &[]), screen.title_key());
        }
    }
}
