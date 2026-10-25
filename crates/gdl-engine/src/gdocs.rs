//! Documentos Google aparecem como pequenos arquivos-ponteiro (`.gdoc`, `.gsheet`…) que abrem o
//! documento no navegador.

pub use gdl_common::gdocs::pointer_extension;

/// Endereço que abre o documento no navegador.
pub fn open_url(id: &str) -> String {
    format!("https://docs.google.com/open?id={id}")
}

/// Conteúdo do ponteiro: JSON de uma linha com o endereço do documento e a conta. Montado à mão
/// (só os valores passam pelo serde) para manter a ordem documentada das chaves, que o `json!`
/// trocaria pela alfabética.
pub fn pointer_json(id: &str, mime: &str, email: &str) -> String {
    let quoted = |text: &str| serde_json::Value::from(text).to_string();
    format!(
        r#"{{"url":{},"doc_id":{},"email":{},"mime":{},"app":"gdrive-desktop-linux"}}"#,
        quoted(&open_url(id)),
        quoted(id),
        quoted(email),
        quoted(mime),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_google_type_has_its_extension() {
        for (kind, ext) in [
            ("document", "gdoc"),
            ("spreadsheet", "gsheet"),
            ("presentation", "gslides"),
            ("form", "gform"),
            ("drawing", "gdraw"),
            ("map", "gmap"),
            ("site", "gsite"),
            ("script", "gscript"),
            ("jam", "gjam"),
            ("fusiontable", "glink"),
        ] {
            let mime = format!("application/vnd.google-apps.{kind}");
            assert_eq!(pointer_extension(&mime), Some(ext), "{mime}");
        }
        for mime in [
            "application/vnd.google-apps.folder",
            "application/vnd.google-apps.shortcut",
            "application/pdf",
        ] {
            assert_eq!(pointer_extension(mime), None, "{mime}");
        }
    }

    #[test]
    fn pointer_is_the_documented_json() {
        let json = pointer_json("abc", "application/vnd.google-apps.document", "a@b.c");
        assert_eq!(
            json,
            r#"{"url":"https://docs.google.com/open?id=abc","doc_id":"abc","email":"a@b.c","mime":"application/vnd.google-apps.document","app":"gdrive-desktop-linux"}"#
        );
        let quoted = pointer_json("x", "m", "a\"b");
        assert!(serde_json::from_str::<serde_json::Value>(&quoted).is_ok());
    }
}
