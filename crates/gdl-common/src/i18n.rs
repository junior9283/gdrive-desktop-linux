//! Textos traduzidos (pt-BR e inglês) em arquivos Fluent.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

const PT_BR: &str = include_str!("../../../locales/pt-BR/main.ftl");
const EN: &str = include_str!("../../../locales/en/main.ftl");
const UI_PT_BR: &str = include_str!("../../../locales/pt-BR/ui.ftl");
const UI_EN: &str = include_str!("../../../locales/en/ui.ftl");

/// Idioma do sistema, pelas variáveis `LC_ALL`, `LC_MESSAGES` e `LANG`, nessa ordem.
pub fn env_lang() -> String {
    ["LC_ALL", "LC_MESSAGES", "LANG"]
        .iter()
        .find_map(|var| std::env::var(var).ok().filter(|v| !v.is_empty()))
        .unwrap_or_default()
}

/// O idioma dos textos para `lang` (formato de `LANG`, como `pt_BR.UTF-8`): `pt-BR` quando ele
/// começa com `pt`, senão `en`.
pub fn language(lang: &str) -> &'static str {
    match lang.split(['_', '-', '.', '@']).next() {
        Some("pt") => "pt-BR",
        _ => "en",
    }
}

/// O arquivo `ui.ftl` (textos da janela) do idioma `language`.
pub fn ui_source(language: &str) -> &'static str {
    if language == "pt-BR" { UI_PT_BR } else { UI_EN }
}

/// Textos do app (`main.ftl`) no idioma de `language`; os da janela (`ui.ftl`) vão crus para
/// o front-end (`ui_source`).
pub struct I18n {
    bundle: FluentBundle<FluentResource>,
}

impl I18n {
    /// `lang` no formato das variáveis `LANG`/`LC_MESSAGES`, como `pt_BR.UTF-8`.
    pub fn new(lang: &str) -> I18n {
        let id = language(lang);
        let main = if id == "pt-BR" { PT_BR } else { EN };
        let langid: LanguageIdentifier = id.parse().expect("identificador de idioma fixo");
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle.set_use_isolating(false);
        let resource =
            FluentResource::try_new(main.to_string()).expect("arquivo .ftl embutido válido");
        bundle
            .add_resource(resource)
            .expect("chaves .ftl sem duplicatas");
        I18n { bundle }
    }

    /// Idioma do sistema (`env_lang`).
    pub fn from_env() -> I18n {
        I18n::new(&env_lang())
    }

    /// Texto traduzido de `key`; se a chave não existir, devolve a própria chave.
    pub fn tr(&self, key: &str, args: &[(&str, &str)]) -> String {
        self.format(key, args.iter().copied().collect())
    }

    /// `tr` com `$count` como número, para os plurais (`[1]`, `*[other]`) escolherem a forma.
    pub fn tr_count(&self, key: &str, count: u64) -> String {
        let mut args = FluentArgs::new();
        args.set("count", count);
        self.format(key, args)
    }

    fn format(&self, key: &str, fargs: FluentArgs) -> String {
        let Some(pattern) = self.bundle.get_message(key).and_then(|m| m.value()) else {
            return key.to_string();
        };
        let mut errors = vec![];
        self.bundle
            .format_pattern(pattern, Some(&fargs), &mut errors)
            .into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn virtual_folder_names_follow_language() {
        assert_eq!(
            I18n::new("pt_BR.UTF-8").tr("folder-my-drive", &[]),
            "Meu Drive"
        );
        assert_eq!(I18n::new("de_DE").tr("folder-my-drive", &[]), "My Drive");
        assert_eq!(I18n::new("en_US").tr("folder-trash", &[]), ".Drive Trash");
        assert_eq!(I18n::new("pt").tr("folder-trash", &[]), ".Lixeira do Drive");
    }

    #[test]
    fn arguments_are_interpolated_without_isolation_marks() {
        let t = I18n::new("pt_BR").tr("mount-moved", &[("path", "/x")]);
        assert_eq!(t, "A pasta do Drive foi aberta em /x");
    }

    #[test]
    fn count_chooses_the_plural_form() {
        let i18n = I18n::new("pt_BR");
        assert_eq!(i18n.tr_count("tray-syncing", 1), "Sincronizando 1 arquivo");
        assert_eq!(i18n.tr_count("tray-syncing", 3), "Sincronizando 3 arquivos");
        assert_eq!(i18n.tr_count("tray-paused", 0), "Pausado");
    }

    #[test]
    fn language_is_pt_br_or_english() {
        assert_eq!(language("pt_PT.UTF-8"), "pt-BR");
        assert!(ui_source("pt-BR").contains("about-name ="));
        assert_eq!(I18n::new("pt_BR").tr("window-about", &[]), "Sobre");
        assert_eq!(language(""), "en");
    }

    #[test]
    fn missing_key_returns_the_key() {
        assert_eq!(I18n::new("en").tr("nao-existe", &[]), "nao-existe");
    }
}
