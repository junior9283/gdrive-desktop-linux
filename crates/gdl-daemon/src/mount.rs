//! Onde a conta é montada: `DefaultMountPoint` ou `~/Google Drive`; se a pasta já tiver
//! arquivos, `~/Google Drive (1)`, `(2)`…

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Quantas pastas numeradas tentar antes de desistir.
const MAX_TRIES: u32 = 100;

/// Pasta da montagem: a preferida ou, se ela já tiver arquivos, a primeira numerada livre.
pub fn choose(preferred: &Path) -> io::Result<PathBuf> {
    let name = preferred
        .file_name()
        .ok_or_else(|| io::Error::other("pasta da montagem sem nome"))?
        .to_string_lossy()
        .into_owned();
    for n in 0..MAX_TRIES {
        let candidate = match n {
            0 => preferred.to_path_buf(),
            _ => preferred.with_file_name(format!("{name} ({n})")),
        };
        unmount_if_orphan(&candidate);
        match std::fs::read_dir(&candidate) {
            Ok(mut entries) => {
                if entries.next().is_none() {
                    return Ok(candidate);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                std::fs::create_dir_all(&candidate)?;
                return Ok(candidate);
            }
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("nenhuma pasta livre para a montagem"))
}

/// Uma montagem FUSE de uma execução que caiu fica no lugar dando "endpoint não conectado".
fn unmount_if_orphan(path: &Path) {
    let orphan = std::fs::metadata(path)
        .is_err_and(|e| e.raw_os_error() == Some(rustix::io::Errno::NOTCONN.raw_os_error()));
    if orphan {
        let unmounted = Command::new("fusermount3")
            .args(["-u", "-z"])
            .arg(path)
            .stdin(Stdio::null())
            .status();
        if !unmounted.is_ok_and(|s| s.success()) {
            tracing::warn!(pasta = %path.display(), "não deu para desmontar a montagem órfã");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_folder_is_created_and_empty_folder_is_reused() {
        let home = tempfile::tempdir().unwrap();
        let preferred = home.path().join("Google Drive");

        assert_eq!(choose(&preferred).unwrap(), preferred);
        assert!(preferred.is_dir());
        assert_eq!(choose(&preferred).unwrap(), preferred);
    }

    #[test]
    fn folders_with_files_are_skipped() {
        let home = tempfile::tempdir().unwrap();
        let preferred = home.path().join("Google Drive");
        for dir in ["Google Drive", "Google Drive (1)"] {
            std::fs::create_dir_all(home.path().join(dir)).unwrap();
            std::fs::write(home.path().join(dir).join("x"), b"").unwrap();
        }

        assert_eq!(
            choose(&preferred).unwrap(),
            home.path().join("Google Drive (2)")
        );
    }
}
