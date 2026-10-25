//! Cópias locais guardadas para nada se perder: o `Lost and Found` (edições que não puderam
//! subir) e os `Uploads cancelados` (envios que o usuário cancelou).

use std::fs::File;
use std::path::{Path, PathBuf};

use gdl_common::names::{create_unique, split_ext};
use gdl_common::paths::create_private_dir;
use gdl_common::time::{local_stamp, now_ms};

/// Pasta de cópias guardadas da conta.
#[derive(Clone)]
pub struct KeptCopies {
    dir: PathBuf,
}

impl KeptCopies {
    pub fn new(dir: PathBuf) -> KeptCopies {
        KeptCopies { dir }
    }

    /// Copia `src` como `<nome> (local AAAA-MM-DD HHMMSS).<ext>`, sem sobrescrever outra cópia.
    pub fn preserve(&self, src: &Path, original_name: &str) -> std::io::Result<PathBuf> {
        create_private_dir(&self.dir)?;
        let (stem, ext) = split_ext(original_name);
        let stamp = local_stamp(now_ms());
        let (path, mut dst) = create_unique(&self.dir, stem, |n| match n {
            0 => format!(" (local {stamp}){ext}"),
            n => format!(" (local {stamp} {}){ext}", n + 1),
        })?;
        std::io::copy(&mut File::open(src)?, &mut dst)?;
        dst.sync_all()?;
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

/// As pastas de cópias guardadas de uma conta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeptFolder {
    LostAndFound,
    CanceledUploads,
}

/// `KeptCopies` de cada `KeptFolder`, dentro da pasta de dados da conta.
#[derive(Clone)]
pub struct Kept {
    lost_and_found: KeptCopies,
    canceled_uploads: KeptCopies,
}

impl Kept {
    pub fn in_dir(data_dir: &Path) -> Kept {
        Kept {
            lost_and_found: KeptCopies::new(data_dir.join("lost_and_found")),
            canceled_uploads: KeptCopies::new(data_dir.join("canceled_uploads")),
        }
    }

    pub fn get(&self, folder: KeptFolder) -> &KeptCopies {
        match folder {
            KeptFolder::LostAndFound => &self.lost_and_found,
            KeptFolder::CanceledUploads => &self.canceled_uploads,
        }
    }

    fn all(&self) -> [&KeptCopies; 2] {
        [&self.lost_and_found, &self.canceled_uploads]
    }

    /// As pastas.
    pub fn dirs(&self) -> Vec<PathBuf> {
        self.all().map(|k| k.dir().to_path_buf()).into()
    }

    /// Todas as cópias guardadas, de todas as pastas.
    pub fn list(&self) -> Vec<PathBuf> {
        self.all().iter().flat_map(|k| k.list()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserved_copy_keeps_content_extension_and_gets_a_local_stamp() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("conteudo");
        std::fs::write(&src, b"minha versao").unwrap();
        let lost = KeptCopies::new(tmp.path().join("lost_and_found"));

        let first = lost.preserve(&src, "relatorio.odt").unwrap();
        let second = lost.preserve(&src, "relatorio.odt").unwrap();

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
    fn empty_kept_folder_lists_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(
            KeptCopies::new(tmp.path().join("nao_existe"))
                .list()
                .is_empty()
        );
    }

    #[test]
    fn each_kept_folder_has_its_own_dir() {
        let kept = Kept::in_dir(Path::new("/dados"));
        assert_eq!(
            kept.get(KeptFolder::LostAndFound).dir(),
            Path::new("/dados/lost_and_found")
        );
        assert_eq!(
            kept.get(KeptFolder::CanceledUploads).dir(),
            Path::new("/dados/canceled_uploads")
        );
    }
}
