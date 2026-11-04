//! Textos traduzidos (pt-BR e inglês) em arquivos Fluent.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

/// O `gerasync.ftl` de `lang`, que no GeraSync troca o nome do app nos textos; vazio na variante
/// de sempre.
#[cfg(gerasync)]
macro_rules! brand_ftl {
    ($lang:literal) => {
        include_str!(concat!("../../../locales/", $lang, "/gerasync.ftl"))
    };
}
#[cfg(not(gerasync))]
macro_rules! brand_ftl {
    ($lang:literal) => {
        ""
    };
}

const PT_BR: &str = include_str!("../../../locales/pt-BR/main.ftl");
const EN: &str = include_str!("../../../locales/en/main.ftl");
const BRAND_PT_BR: &str = brand_ftl!("pt-BR");
const BRAND_EN: &str = brand_ftl!("en");
// O `i18n.js` fica com o último valor de cada chave: o do `gerasync.ftl`, que vem depois.
const UI_PT_BR: &str = concat!(
    include_str!("../../../locales/pt-BR/ui.ftl"),
    "\n",
    brand_ftl!("pt-BR")
);
const UI_EN: &str = concat!(
    include_str!("../../../locales/en/ui.ftl"),
    "\n",
    brand_ftl!("en")
);

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
        let (main, brand) = if id == "pt-BR" {
            (PT_BR, BRAND_PT_BR)
        } else {
            (EN, BRAND_EN)
        };
        let langid: LanguageIdentifier = id.parse().expect("identificador de idioma fixo");
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle.set_use_isolating(false);
        let resource =
            FluentResource::try_new(main.to_string()).expect("arquivo .ftl embutido válido");
        bundle
            .add_resource(resource)
            .expect("chaves .ftl sem duplicatas");
        let brand = FluentResource::try_new(brand.to_string()).expect("gerasync.ftl válido");
        bundle.add_resource_overriding(brand);
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

    const BASE_UI_PT_BR: &str = include_str!("../../../locales/pt-BR/ui.ftl");
    const BASE_UI_EN: &str = include_str!("../../../locales/en/ui.ftl");

    /// `(chave, primeira linha do valor)` de cada mensagem de um `.ftl`, na ordem.
    fn entries(source: &str) -> impl Iterator<Item = (&str, &str)> {
        source
            .lines()
            .filter(|l| !l.starts_with([' ', '#', '}']))
            .filter_map(|l| l.split_once('=').map(|(k, v)| (k.trim(), v)))
    }

    fn keys(source: &str) -> Vec<&str> {
        entries(source).map(|(key, _)| key).collect()
    }

    /// O `gerasync.ftl` só troca textos que existem: uma chave a mais seria um texto que nenhuma
    /// tela pede.
    #[test]
    fn gerasync_overrides_only_known_keys() {
        let langs = [
            (
                include_str!("../../../locales/pt-BR/gerasync.ftl"),
                PT_BR,
                BASE_UI_PT_BR,
            ),
            (
                include_str!("../../../locales/en/gerasync.ftl"),
                EN,
                BASE_UI_EN,
            ),
        ];
        for (brand, main, ui) in langs {
            let known: Vec<&str> = keys(main).into_iter().chain(keys(ui)).collect();
            let brand_keys = keys(brand);
            assert!(!brand_keys.is_empty());
            for key in brand_keys {
                assert!(known.contains(&key), "{key}");
            }
        }
    }

    #[cfg(gerasync)]
    #[test]
    fn gerasync_messages_never_name_the_old_app() {
        for lang in ["pt_BR", "en"] {
            let i18n = I18n::new(lang);
            assert_eq!(i18n.tr("notify-title", &[]), crate::brand::DISPLAY_NAME);
            // A janela fica com o último valor de cada chave (`parseFtl`, no `i18n.js`).
            let ui: std::collections::HashMap<_, _> = entries(ui_source(language(lang))).collect();
            for key in keys(if lang == "en" { EN } else { PT_BR }) {
                assert!(!i18n.tr(key, &[]).contains("gdrive-desktop-linux"), "{key}");
            }
            for (key, value) in ui {
                assert!(!value.contains("gdrive-desktop-linux"), "{lang} {key}");
            }
        }
    }

    #[test]
    fn missing_key_returns_the_key() {
        assert_eq!(I18n::new("en").tr("nao-existe", &[]), "nao-existe");
    }
}
