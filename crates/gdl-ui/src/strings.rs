//! As mensagens do `ui.ftl` cruas, para o `i18n.js` formatar no front-end.

use std::collections::HashMap;

use serde::Serialize;

use crate::screens::Screen;

/// O que o comando `strings` entrega à tela.
#[derive(Debug, Clone, Serialize)]
pub struct UiStrings {
    /// O idioma (`pt-BR` ou `en`), para o `Intl` do JavaScript.
    pub locale: &'static str,
    pub messages: HashMap<String, String>,
}

impl UiStrings {
    /// `lang` no formato de `LANG`.
    pub fn new(lang: &str) -> UiStrings {
        let locale = gdl_common::i18n::language(lang);
        UiStrings {
            locale,
            messages: messages(gdl_common::i18n::ui_source(locale)),
        }
    }

    /// O título da janela de `screen` (`window-<tela>`).
    pub fn title(&self, screen: &Screen) -> Option<&str> {
        self.messages
            .get(&format!("window-{}", screen.name))
            .map(String::as_str)
    }
}

/// `chave = valor` de cada mensagem; as linhas recuadas (e o `}` que fecha uma seleção, que o
/// Fluent aceita sem recuo) continuam o valor, com `\n`. Comentários e linhas em branco ficam de
/// fora.
pub fn messages(source: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut current: Option<(String, String)> = None;
    for line in source.lines() {
        if line.trim().is_empty() || line.starts_with('#') {
            continue;
        }
        if line.starts_with(|c: char| c.is_whitespace() || c == '}') {
            if let Some((_, value)) = current.as_mut() {
                if !value.is_empty() {
                    value.push('\n');
                }
                value.push_str(line.trim());
            }
        } else if let Some((key, value)) = line.split_once('=') {
            out.extend(current.take());
            current = Some((key.trim().to_string(), value.trim().to_string()));
        }
    }
    out.extend(current);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_keep_selections_raw() {
        let map = messages(
            "# comentário\nsimple = Olá, { $name }!\n\nfiles = { $count ->\n    [one] 1 arquivo\n   *[other] { $count } arquivos\n}\n",
        );
        assert_eq!(map["simple"], "Olá, { $name }!");
        assert_eq!(
            map["files"],
            "{ $count ->\n[one] 1 arquivo\n*[other] { $count } arquivos\n}"
        );
        assert_eq!(map.len(), 2);
    }

    #[test]
    fn both_languages_have_the_same_keys() {
        let keys = |lang| {
            let mut keys: Vec<String> = UiStrings::new(lang).messages.into_keys().collect();
            keys.sort();
            keys
        };
        assert_eq!(keys("pt_BR"), keys("en_US"));
        assert_eq!(UiStrings::new("pt_BR").locale, "pt-BR");
        assert_eq!(UiStrings::new("pt_BR").messages["window-about"], "Sobre");
    }
}
