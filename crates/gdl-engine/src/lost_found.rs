//! `Lost and Found` local: cópias de edições que não puderam subir, para nada se perder.

use std::fs::File;
use std::path::{Path, PathBuf};

use gdl_common::names::{create_unique, split_ext};
use gdl_common::time::{local_stamp, now_ms};

use crate::events::LostReason;

/// Pasta `<data_dir>/lost_and_found/` da conta.
#[derive(Clone)]
pub struct LostAndFound {
    dir: PathBuf,
}

impl LostAndFound {
    pub fn new(dir: PathBuf) -> LostAndFound {
        LostAndFound { dir }
    }

    /// Copia `src` como `<nome> (local AAAA-MM-DD HHMMSS).<ext>`, sem sobrescrever outra cópia.
    pub fn preserve(
        &self,
        src: &Path,
        original_name: &str,
        reason: LostReason,
    ) -> std::io::Result<PathBuf> {
        std::fs::create_dir_all(&self.dir)?;
        let (stem, ext) = split_ext(original_name);
        let stamp = local_stamp(now_ms());
        let (path, mut dst) = create_unique(&self.dir, stem, |n| match n {
            0 => format!(" (local {stamp}){ext}"),
            n => format!(" (local {stamp} {}){ext}", n + 1),
        })?;
        std::io::copy(&mut File::open(src)?, &mut dst)?;
        dst.sync_all()?;
        tracing::info!(caminho = %path.display(), motivo = ?reason, "edição local guardada no Lost and Found");
        Ok(path)
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Cópias guardadas, da mais antiga à mais nova.
    pub fn list(&self) -> Vec<PathBuf> {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return vec![];
        };
        let mut files: Vec<_> = entries
            .filter_map(|e| {
                let e = e.ok()?;
                Some((e.metadata().ok()?.modified().ok()?, e.path()))
            })
            .collect();
        files.sort();
        files.into_iter().map(|(_, path)| path).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::LostReason;

    #[test]
    fn preserved_copy_keeps_content_extension_and_gets_a_local_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("conteudo");
        std::fs::write(&src, b"minha versao").unwrap();
        let lost = LostAndFound::new(tmp.path().join("lost_and_found"));

        let first = lost
            .preserve(&src, "relatorio.odt", LostReason::RemoteChanged)
            .unwrap();
        let second = lost
            .preserve(&src, "relatorio.odt", LostReason::RemoteChanged)
            .unwrap();

        let name = first.file_name().unwrap().to_str().unwrap();
        assert!(name.starts_with("relatorio (local "), "{name}");
        assert!(name.ends_with(").odt"), "{name}");
        assert_eq!(std::fs::read(&first).unwrap(), b"minha versao");
        assert_ne!(
            first, second,
            "duas cópias no mesmo segundo não se sobrescrevem"
        );
        assert_eq!(lost.list(), [first, second]);
    }

    #[test]
    fn empty_lost_and_found_lists_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            LostAndFound::new(tmp.path().join("nao_existe"))
                .list()
                .is_empty()
        );
    }
}
