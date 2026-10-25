//! Documentos Google aparecem como pequenos arquivos-ponteiro (`.gdoc`, `.gsheet`…) que abrem o
//! documento no navegador.

pub use gdl_common::gdocs::{is_pointer_extension, pointer_extension};

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

/// `doc_id` de um ponteiro montado por `pointer_json`. Só vale um ID do Drive (letras, números,
/// `-` e `_`), porque ele vai para o endereço aberto no navegador.
pub fn pointer_doc_id(json: &[u8]) -> Option<String> {
    let value: serde_json::Value = serde_json::from_slice(json).ok()?;
    let id = value.get("doc_id")?.as_str()?;
    let valid = !id.is_empty()
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then(|| id.into())
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
    fn doc_id_comes_back_from_the_pointer() {
        let json = pointer_json("abc", "application/vnd.google-apps.document", "a@b.c");
        assert_eq!(pointer_doc_id(json.as_bytes()).as_deref(), Some("abc"));
        assert_eq!(pointer_doc_id(b"texto comum"), None);
        // Um ponteiro forjado não injeta nada no endereço aberto.
        assert_eq!(pointer_doc_id(br#"{"doc_id":"abc&x=file:///etc"}"#), None);
        assert_eq!(pointer_doc_id(br#"{"doc_id":""}"#), None);
        assert!(is_pointer_extension("gsheet") && is_pointer_extension("glink"));
        assert!(!is_pointer_extension("odt"));
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
