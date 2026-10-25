//! Tipos de Documento Google e a extensão do arquivo-ponteiro de cada um.

const GOOGLE_APPS: &str = "application/vnd.google-apps.";

/// Extensão do ponteiro local de um Documento Google; `None` para pastas, atalhos e arquivos
/// comuns. Tipos sem extensão própria viram `.glink`.
pub fn pointer_extension(mime: &str) -> Option<&'static str> {
    Some(match mime.strip_prefix(GOOGLE_APPS)? {
        "folder" | "shortcut" => return None,
        "document" => "gdoc",
        "spreadsheet" => "gsheet",
        "presentation" => "gslides",
        "form" => "gform",
        "drawing" => "gdraw",
        "map" => "gmap",
        "site" => "gsite",
        "script" => "gscript",
        "jam" => "gjam",
        _ => "glink",
    })
}
