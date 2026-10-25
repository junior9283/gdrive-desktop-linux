//! Os textos das telas: o `ui.ftl` do idioma do sistema, cru, para o `i18n.js` ler e formatar.

use serde::Serialize;

/// O que o comando `strings` entrega à tela.
#[derive(Debug, Clone, Serialize)]
pub struct UiStrings {
    /// O idioma (`pt-BR` ou `en`), para o `Intl` do JavaScript.
    pub locale: &'static str,
    /// O arquivo `ui.ftl` desse idioma.
    pub source: &'static str,
}

impl UiStrings {
    /// `lang` no formato de `LANG`.
    pub fn new(lang: &str) -> UiStrings {
        let locale = gdl_common::i18n::language(lang);
        UiStrings {
            locale,
            source: gdl_common::i18n::ui_source(locale),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_follow_the_system_language() {
        let pt = UiStrings::new("pt_BR.UTF-8");
        assert_eq!(pt.locale, "pt-BR");
        assert!(pt.source.contains("about-version = Versão"));
        assert_eq!(UiStrings::new("de_DE").locale, "en");
    }
}
