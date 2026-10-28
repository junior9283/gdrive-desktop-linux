//! "Mover tudo para…" do aviso de Lost and Found: leva as cópias guardadas da conta para uma pasta
//! que o usuário escolheu, sem sobrescrever nada no destino.

use std::io::{self, ErrorKind};
use std::path::{Path, PathBuf};

use gdl_common::names::{create_numbered, fit, split_ext};
use gdl_engine::kept::KeptFolder;
use gdl_ipc::ServiceError;

use super::mcp::copy_contents;
use super::{Daemon, failed};

impl Daemon {
    /// Move o Lost and Found da conta para a pasta `dest` (caminho absoluto de uma pasta fora das
    /// montagens e do próprio Lost and Found) e devolve quantas entradas moveu.
    pub async fn move_lost_and_found(
        &self,
        account: &str,
        dest: &str,
    ) -> Result<u32, ServiceError> {
        let engine = self.account_engine(account).await?;
        let kept = engine
            .kept()
            .get(KeptFolder::LostAndFound)
            .dir()
            .to_path_buf();
        let mounts = self.mount_points().await;
        let refuse = {
            let dest = dest.to_owned();
            move || ServiceError::InvalidArgs(dest)
        };
        let dest = PathBuf::from(dest);
        // Tudo que toca o disco fica fora da thread assíncrona.
        tokio::task::spawn_blocking(move || {
            let outside = |d: &PathBuf| d.is_dir() && !mounts.iter().any(|m| d.starts_with(m));
            let dest = dest
                .is_absolute()
                .then(|| dest.canonicalize().ok())
                .flatten()
                .filter(outside)
                .ok_or_else(refuse.clone())?;
            let Ok(kept_real) = kept.canonicalize() else {
                return Ok(0);
            };
            if dest.starts_with(&kept_real) {
                return Err(refuse());
            }
            move_all(&kept, &dest).map_err(failed)
        })
        .await
        .map_err(failed)?
    }
}

/// Move cada entrada de `from` para `dest`; um nome que já existe em `dest` ganha um número
/// (`nome (1).ext`, `pasta (1)`). Devolve quantas entradas moveu.
pub(super) fn move_all(from: &Path, dest: &Path) -> io::Result<u32> {
    let mut moved = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let dir = entry.file_type()?.is_dir();
        // O nome livre é reservado criando o lugar vazio, que a troca substitui.
        let target = if dir {
            free_dir(dest, &name)?
        } else {
            let (stem, ext) = split_ext(&name);
            create_numbered(dest, stem, ext)?.0
        };
        if let Err(e) = relocate(&entry.path(), &target, dir) {
            let _ = remove(&target, dir);
            return Err(e);
        }
        moved += 1;
    }
    Ok(moved)
}

/// Cria em `dest` a pasta `name`, ou `name (1)`, `name (2)`… se já existir.
fn free_dir(dest: &Path, name: &str) -> io::Result<PathBuf> {
    for n in 0.. {
        let path = match n {
            0 => dest.join(name),
            n => dest.join(fit(name, &format!(" ({n})"))),
        };
        match std::fs::create_dir(&path) {
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            done => return done.map(|()| path),
        }
    }
    unreachable!()
}

/// Troca o lugar reservado `to` por `from`; entre discos, copia e apaga a origem.
fn relocate(from: &Path, to: &Path, dir: bool) -> io::Result<()> {
    match std::fs::rename(from, to) {
        Err(e) if e.kind() == ErrorKind::CrossesDevices => {
            if dir {
                copy_contents(from, to)?;
            } else {
                std::fs::copy(from, to)?;
            }
            remove(from, dir)
        }
        done => done,
    }
}

fn remove(path: &Path, dir: bool) -> io::Result<()> {
    if dir {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn move_lost_found_moves_everything() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, dest) = (tmp.path().join("lf"), tmp.path().join("destino"));
        std::fs::create_dir_all(from.join("Recuperados 2026-10-28/sub")).unwrap();
        std::fs::create_dir(&dest).unwrap();
        std::fs::write(from.join("a (local 2026-10-28 101010).txt"), "a").unwrap();
        std::fs::write(from.join("Recuperados 2026-10-28/sub/b.txt"), "b").unwrap();

        let moved = move_all(&from, &dest).unwrap();

        assert_eq!(moved, 2);
        assert_eq!(read(&dest.join("a (local 2026-10-28 101010).txt")), "a");
        assert_eq!(read(&dest.join("Recuperados 2026-10-28/sub/b.txt")), "b");
        assert_eq!(std::fs::read_dir(&from).unwrap().count(), 0);
    }

    #[test]
    fn move_lost_found_numbers_clashes() {
        let tmp = tempfile::tempdir().unwrap();
        let (from, dest) = (tmp.path().join("lf"), tmp.path().join("destino"));
        std::fs::create_dir_all(from.join("pasta")).unwrap();
        std::fs::create_dir_all(dest.join("pasta")).unwrap();
        std::fs::write(from.join("nota.txt"), "nova").unwrap();
        std::fs::write(from.join("pasta/x"), "x").unwrap();
        std::fs::write(dest.join("nota.txt"), "antiga").unwrap();

        assert_eq!(move_all(&from, &dest).unwrap(), 2);

        assert_eq!(read(&dest.join("nota.txt")), "antiga", "nada é sobrescrito");
        assert_eq!(read(&dest.join("nota (1).txt")), "nova");
        assert_eq!(read(&dest.join("pasta (1)/x")), "x");
        assert!(
            std::fs::read_dir(dest.join("pasta"))
                .unwrap()
                .next()
                .is_none()
        );
    }
}
