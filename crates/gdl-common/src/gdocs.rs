//! Tipos de Documento Google e a extensão do arquivo-ponteiro de cada um.

const GOOGLE_APPS: &str = "application/vnd.google-apps.";

/// Tipo do Documento Google (o que vem depois de `application/vnd.google-apps.`) e a extensão
/// do ponteiro local.
const POINTERS: [(&str, &str); 9] = [
    ("document", "gdoc"),
    ("spreadsheet", "gsheet"),
    ("presentation", "gslides"),
    ("form", "gform"),
    ("drawing", "gdraw"),
    ("map", "gmap"),
    ("site", "gsite"),
    ("script", "gscript"),
    ("jam", "gjam"),
];

/// Extensão dos tipos sem extensão própria.
const OTHER_POINTER: &str = "glink";

/// Extensão do ponteiro local de um Documento Google; `None` para pastas, atalhos e arquivos
/// comuns. Tipos sem extensão própria viram `.glink`.
pub fn pointer_extension(mime: &str) -> Option<&'static str> {
    let kind = mime.strip_prefix(GOOGLE_APPS)?;
    if matches!(kind, "folder" | "shortcut") {
        return None;
    }
    Some(
        POINTERS
            .iter()
            .find(|(k, _)| *k == kind)
            .map_or(OTHER_POINTER, |(_, ext)| ext),
    )
}

/// `ext` (sem o ponto) é a extensão de um ponteiro de Documento Google.
pub fn is_pointer_extension(ext: &str) -> bool {
    ext == OTHER_POINTER || POINTERS.iter().any(|(_, e)| *e == ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Valores de `attr="…"` em `text`, na ordem.
    fn attr_values<'a>(text: &'a str, attr: &str) -> Vec<&'a str> {
        text.split(&format!("{attr}=\""))
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap())
            .collect()
    }

    /// Os tipos MIME registrados no sistema, e os que o manipulador `.desktop` abre, cobrem toda
    /// extensão que o motor cria, e nada além dela.
    #[test]
    fn packaged_mime_types_match_pointer_extensions() {
        let xml = include_str!("../../../packaging/mime/gdrive-desktop-linux.xml");
        let desktop = include_str!(
            "../../../packaging/desktop/io.github.junior9283.GDriveDesktopLinux.OpenPointer.desktop"
        );
        let globs: Vec<&str> = attr_values(xml, "glob pattern")
            .into_iter()
            .map(|g| g.trim_start_matches("*."))
            .collect();
        let mut expected: Vec<&str> = POINTERS.iter().map(|(_, ext)| *ext).collect();
        expected.push(OTHER_POINTER);
        let handled = desktop
            .lines()
            .find_map(|l| l.strip_prefix("MimeType="))
            .unwrap();

        assert_eq!(globs, expected);
        assert!(globs.iter().all(|ext| is_pointer_extension(ext)));
        let registered = attr_values(xml, "mime-type type");
        assert_eq!(
            handled.trim_end_matches(';').split(';').collect::<Vec<_>>(),
            registered
        );
    }
}
