//! Os endereços que a bandeja e a janela abrem no navegador, numa tabela fixa: a janela pede pelo
//! nome e nunca manda um endereço qualquer.

const PROJECT: &str = "https://github.com/junior9283/gdrive-desktop-linux";

/// O Meu Drive na web.
pub const DRIVE_WEB: &str = "https://drive.google.com/drive/my-drive";

/// A lixeira do Drive na web.
pub const DRIVE_TRASH: &str = "https://drive.google.com/drive/trash";

/// O endereço de `name` (`drive-web`, `storage`, `gemini`, `project`, `privacy`, `terms`,
/// `faq`, `help`, `client-id-guide` ou as páginas do Google Cloud do guia do primeiro uso,
/// `gcp-*`).
pub fn url(name: &str) -> Option<String> {
    let doc = |file: &str| format!("{PROJECT}/blob/main/docs/{file}");
    // O Google Cloud Console, no projeto que estiver escolhido lá.
    let cloud = |page: &str| format!("https://console.cloud.google.com/{page}");
    Some(match name {
        "drive-web" => DRIVE_WEB.into(),
        "storage" => "https://one.google.com/storage".into(),
        "gemini" => "https://gemini.google.com/".into(),
        "project" => PROJECT.into(),
        "privacy" => doc("politica-de-privacidade.md"),
        "terms" => doc("termos-de-uso.md"),
        "faq" => doc("perguntas-frequentes.md"),
        "help" => doc("ajuda.md"),
        "client-id-guide" => doc("guia-client-id.md"),
        "gcp-new-project" => cloud("projectcreate"),
        "gcp-drive-api" => cloud("apis/library/drive.googleapis.com"),
        "gcp-branding" => cloud("auth/branding"),
        "gcp-scopes" => cloud("auth/scopes"),
        "gcp-audience" => cloud("auth/audience"),
        "gcp-new-client" => cloud("auth/clients/create"),
        _ => return None,
    })
}

/// O tamanho máximo do endereço de uma *issue* nova: acima disso o GitHub recusa.
pub const MAX_ISSUE_URL: usize = 6000;

/// O endereço que abre uma *issue* nova no projeto com `title` e `body` preenchidos, e se o fim
/// de `body` foi cortado para caber em `MAX_ISSUE_URL`.
pub fn new_issue(title: &str, body: &str) -> (String, bool) {
    let build = |body: &str| {
        let mut url = url::Url::parse(&format!("{PROJECT}/issues/new")).expect("endereço fixo");
        url.query_pairs_mut()
            .append_pair("title", title)
            .append_pair("body", body);
        url.to_string()
    };
    // Cada caractere vira ao menos um byte no endereço: mais que o limite nunca cabe.
    let total = body.chars().count();
    let mut keep = total.min(MAX_ISSUE_URL);
    loop {
        let body: String = body.chars().take(keep).collect();
        let url = build(&body);
        if url.len() <= MAX_ISSUE_URL || keep == 0 {
            return (url, keep < total);
        }
        keep = (keep * MAX_ISSUE_URL / url.len()).min(keep - 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_issue_fits_the_limit() {
        let (short, cut) = new_issue("Falha", "linha 1\nlinha 2");
        assert_eq!(
            short,
            format!("{PROJECT}/issues/new?title=Falha&body=linha+1%0Alinha+2")
        );
        assert!(!cut);
        let (long, cut) = new_issue("Falha", &"ç@ ".repeat(5000));
        assert!(cut);
        assert!(long.len() <= MAX_ISSUE_URL, "{}", long.len());
        assert!(long.len() > MAX_ISSUE_URL - 50, "corta só o que passa");
    }

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
        for name in ["privacy", "terms", "faq", "help", "client-id-guide"] {
            let url = url(name).unwrap();
            let file = url.rsplit('/').next().unwrap();
            assert!(docs.join(file).is_file(), "{file}");
        }
    }
    /// As páginas do guia do Client ID (o teste da janela confere que o guia só usa estas).
    #[test]
    fn cloud_pages_open_the_cloud_console() {
        let pages = [
            "new-project",
            "drive-api",
            "branding",
            "scopes",
            "audience",
            "new-client",
        ];
        for page in pages {
            let url = url(&format!("gcp-{page}")).unwrap();
            assert!(
                url.starts_with("https://console.cloud.google.com/"),
                "{page}: {url}"
            );
        }
    }
}
