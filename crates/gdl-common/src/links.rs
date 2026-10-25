//! Os endereços que a bandeja e a janela abrem no navegador, numa tabela fixa: a janela pede pelo
//! nome e nunca manda um endereço qualquer.

const PROJECT: &str = "https://github.com/junior9283/gdrive-desktop-linux";

/// O Meu Drive na web.
pub const DRIVE_WEB: &str = "https://drive.google.com/drive/my-drive";

/// O endereço de `name` (`drive-web`, `storage`, `gemini`, `project`, `privacy`, `terms`,
/// `faq` ou `help`).
pub fn url(name: &str) -> Option<String> {
    let doc = |file: &str| format!("{PROJECT}/blob/main/docs/{file}");
    Some(match name {
        "drive-web" => DRIVE_WEB.into(),
        "storage" => "https://one.google.com/storage".into(),
        "gemini" => "https://gemini.google.com/".into(),
        "project" => PROJECT.into(),
        "privacy" => doc("politica-de-privacidade.md"),
        "terms" => doc("termos-de-uso.md"),
        "faq" => doc("perguntas-frequentes.md"),
        "help" => doc("ajuda.md"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_known_names_have_an_address() {
        assert_eq!(url("drive-web").as_deref(), Some(DRIVE_WEB));
        assert!(url("terms").unwrap().ends_with("/docs/termos-de-uso.md"));
        assert_eq!(url("https://example.com"), None);
    }

    /// Os documentos para onde os links apontam existem no repositório.
    #[test]
    fn project_documents_exist() {
        let docs = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs"));
        for name in ["privacy", "terms", "faq", "help"] {
            let url = url(name).unwrap();
            let file = url.rsplit('/').next().unwrap();
            assert!(docs.join(file).is_file(), "{file}");
        }
    }
}
