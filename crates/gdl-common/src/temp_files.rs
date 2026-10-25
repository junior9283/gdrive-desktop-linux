//! Arquivos temporários e de trava de editores e navegadores: existem só no computador e nunca
//! sobem para o Drive.

/// Verdadeiro para travas e temporários de editores (`.~lock.*#`, `*.swp`, `*.swx`,
/// `*.kate-swp`, `~$*`, `.goutputstream-*`) e downloads incompletos (`*.part`, `*.crdownload`).
pub fn is_local_only(name: &str) -> bool {
    (name.starts_with(".~lock.") && name.ends_with('#'))
        || name.starts_with("~$")
        || name.starts_with(".goutputstream-")
        || [".swp", ".swx", ".kate-swp", ".part", ".crdownload"]
            .iter()
            .any(|ext| name.len() > ext.len() && name.ends_with(ext))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editor_temporaries_are_local_only() {
        for name in [
            ".~lock.relatorio.odt#",
            "foo.swp",
            "foo.swx",
            "notas.txt.kate-swp",
            "~$planilha.xlsx",
            ".goutputstream-ABC123",
            "video.mp4.part",
            "instalador.crdownload",
        ] {
            assert!(is_local_only(name), "{name}");
        }
        for name in [
            "relatorio.odt",
            "swp",
            "notas.txt",
            ".bashrc",
            "~arquivo",
            "lock.odt#",
            ".~lock.odt",
        ] {
            assert!(!is_local_only(name), "{name}");
        }
    }
}
