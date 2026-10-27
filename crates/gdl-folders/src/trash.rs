//! Lixeira do sistema no padrão freedesktop: o que some do Drive vai para lá, nunca é apagado.
//! No mesmo disco da pasta pessoal, a lixeira do usuário; noutro disco, a `.Trash-<uid>` da raiz
//! dele (como o gerenciador de arquivos faz, para não copiar entre discos).

use std::io::{self, Write};
use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::{Path, PathBuf};

use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};

/// O que a especificação manda codificar no `Path=` (além de controles): tudo menos `/`.
const PATH_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'[')
    .add(b']')
    .add(b'\\')
    .add(b'^')
    .add(b'|');

#[derive(Debug, Clone)]
pub struct Trash {
    /// A lixeira do usuário (`$XDG_DATA_HOME/Trash`).
    home: PathBuf,
}

impl Trash {
    pub fn new(home: PathBuf) -> Trash {
        Trash { home }
    }

    /// A lixeira do usuário desta sessão.
    pub fn of_user() -> Trash {
        Trash::new(gdl_common::paths::Paths::from_env().data_home.join("Trash"))
    }

    /// Leva `path` (arquivo ou pasta) para a lixeira; devolve onde ele ficou.
    pub fn put(&self, path: &Path) -> io::Result<PathBuf> {
        let path = std::path::absolute(path)?;
        let device = std::fs::symlink_metadata(&path)?.dev();
        let (dir, shown) = if self.home_device()? == device {
            (self.home.clone(), path.clone())
        } else {
            let top = mount_top(&path, device)?;
            let uid = rustix::process::getuid().as_raw();
            let shown = path.strip_prefix(&top).unwrap_or(&path).to_path_buf();
            (top.join(format!(".Trash-{uid}")), shown)
        };
        let (files, info) = (dir.join("files"), dir.join("info"));
        for d in [&dir, &files, &info] {
            std::fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(d)?;
        }
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "caminho sem nome"))?
            .to_string_lossy()
            .into_owned();
        // O `.trashinfo` criado com `create_new` reserva o nome: dois ao mesmo tempo não colidem.
        for n in 1.. {
            let candidate = match n {
                1 => name.clone(),
                n => format!("{name}.{n}"),
            };
            let info_path = info.join(format!("{candidate}.trashinfo"));
            let mut file = match std::fs::File::options()
                .write(true)
                .create_new(true)
                .open(&info_path)
            {
                Ok(file) => file,
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            };
            let date = chrono::Local::now().format("%Y-%m-%dT%H:%M:%S");
            let encoded = utf8_percent_encode(&shown.to_string_lossy(), PATH_SET).to_string();
            write!(file, "[Trash Info]\nPath={encoded}\nDeletionDate={date}\n")?;
            let dest = files.join(&candidate);
            if let Err(e) = std::fs::rename(&path, &dest) {
                let _ = std::fs::remove_file(&info_path);
                return Err(e);
            }
            return Ok(dest);
        }
        unreachable!("o laço só sai por return")
    }

    /// O disco da lixeira do usuário (o da pasta mais próxima dela que existe).
    fn home_device(&self) -> io::Result<u64> {
        let existing = self
            .home
            .ancestors()
            .find(|p| p.exists())
            .unwrap_or(Path::new("/"));
        Ok(std::fs::metadata(existing)?.dev())
    }
}

/// A raiz do disco onde `path` está: a pasta mais alta ainda no mesmo dispositivo.
fn mount_top(path: &Path, device: u64) -> io::Result<PathBuf> {
    let mut top = path.to_path_buf();
    for ancestor in path.ancestors().skip(1) {
        if std::fs::metadata(ancestor)?.dev() != device {
            break;
        }
        top = ancestor.to_path_buf();
    }
    Ok(top)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn files_and_folders_go_to_the_trash_with_their_info() {
        let tmp = tempfile::tempdir().unwrap();
        let trash = Trash::new(tmp.path().join("Trash"));
        let docs = tmp.path().join("Meus docs");
        std::fs::create_dir(&docs).unwrap();
        std::fs::write(docs.join("nota 1.txt"), b"um").unwrap();
        std::fs::write(tmp.path().join("nota 1.txt"), b"dois").unwrap();

        let first = trash.put(&docs.join("nota 1.txt")).unwrap();
        let second = trash.put(&tmp.path().join("nota 1.txt")).unwrap();
        let folder = trash.put(&docs).unwrap();

        assert_eq!(std::fs::read(&first).unwrap(), b"um");
        assert_eq!(
            std::fs::read(&second).unwrap(),
            b"dois",
            "nome repetido não sobrescreve"
        );
        assert_eq!(second.file_name().unwrap(), "nota 1.txt.2");
        assert!(folder.is_dir());
        assert!(!docs.exists());
        let info =
            std::fs::read_to_string(tmp.path().join("Trash/info/nota 1.txt.trashinfo")).unwrap();
        assert!(info.starts_with("[Trash Info]\nPath=/"), "{info}");
        assert!(info.contains("Meus%20docs/nota%201.txt"), "{info}");
        assert!(info.contains("DeletionDate="), "{info}");
    }
}
