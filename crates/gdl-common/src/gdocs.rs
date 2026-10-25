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
