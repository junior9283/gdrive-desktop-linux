//! Textos traduzidos (pt-BR e inglês) em arquivos Fluent.

use fluent_bundle::concurrent::FluentBundle;
use fluent_bundle::{FluentArgs, FluentResource};
use unic_langid::LanguageIdentifier;

const PT_BR: &str = include_str!("../../../locales/pt-BR/main.ftl");
const EN: &str = include_str!("../../../locales/en/main.ftl");

/// Textos do app num idioma: pt-BR quando o idioma do sistema começa com `pt`, senão inglês.
pub struct I18n {
    bundle: FluentBundle<FluentResource>,
}

impl I18n {
    /// `lang` no formato das variáveis `LANG`/`LC_MESSAGES`, como `pt_BR.UTF-8`.
    pub fn new(lang: &str) -> I18n {
        let language = lang.split(['_', '-', '.', '@']).next().unwrap_or_default();
        let (id, source) = if language == "pt" {
            ("pt-BR", PT_BR)
        } else {
            ("en", EN)
        };
        let langid: LanguageIdentifier = id.parse().expect("identificador de idioma fixo");
        let mut bundle = FluentBundle::new_concurrent(vec![langid]);
        bundle.set_use_isolating(false);
        let resource =
            FluentResource::try_new(source.to_string()).expect("arquivo .ftl embutido válido");
        bundle
            .add_resource(resource)
            .expect("chaves .ftl sem duplicatas");
        I18n { bundle }
    }

    /// Texto traduzido de `key`; se a chave não existir, devolve a própria chave.
    pub fn tr(&self, key: &str, args: &[(&str, &str)]) -> String {
        let Some(pattern) = self.bundle.get_message(key).and_then(|m| m.value()) else {
            return key.to_string();
        };
        let fargs: FluentArgs = args.iter().copied().collect();
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
    fn missing_key_returns_the_key() {
        assert_eq!(I18n::new("en").tr("nao-existe", &[]), "nao-existe");
    }
}
