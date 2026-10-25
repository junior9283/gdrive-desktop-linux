//! Gravação de arquivos de estado e preferências.

use std::io::Write;
use std::path::Path;

/// Grava `bytes` num arquivo ao lado e renomeia por cima de `path`: quem lê nunca vê o arquivo
/// pela metade. Cria a pasta se faltar.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.persist(path).map_err(|e| e.error)?;
    Ok(())
}

/// Apaga o arquivo; já não existir não é erro.
pub fn remove_file_if_exists(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_atomic_creates_the_folder_and_replaces_the_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a/b/estado.json");
        write_atomic(&path, b"um").unwrap();
        write_atomic(&path, b"dois").unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"dois");
        assert_eq!(
            std::fs::read_dir(path.parent().unwrap()).unwrap().count(),
            1
        );
    }
}
