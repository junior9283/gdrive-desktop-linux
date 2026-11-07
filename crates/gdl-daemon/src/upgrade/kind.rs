//! O formato da instalação, para escolher o pacote do release e o comando que o instala
//! (`gdl_common::install` lê os marcadores). Instalação do código-fonte ou pasta movida:
//! `Source`, que não se atualiza sozinha.

use std::path::{Path, PathBuf};

use gdl_common::install::{Install, detect};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InstallKind {
    Deb,
    Rpm,
    Arch,
    /// O `.tar.gz`, instalado em `prefix` sem root.
    Tar {
        prefix: PathBuf,
    },
    Source,
}

/// O formato da instalação dos programas da pasta `bin` (a de `paths::startup_exe`).
pub fn install_kind(bin: &Path) -> InstallKind {
    match detect(bin) {
        Some(Install::Package(kind)) => match kind.as_str() {
            "deb" => InstallKind::Deb,
            "rpm" => InstallKind::Rpm,
            "archlinux" => InstallKind::Arch,
            _ => InstallKind::Source,
        },
        Some(Install::User {
            prefix,
            tarball: true,
            ..
        }) => InstallKind::Tar { prefix },
        _ => InstallKind::Source,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gdl_common::install::own_share;

    /// A pasta `bin` de uma instalação com os marcadores `files` (nome, conteúdo).
    fn bin_with(root: &Path, files: &[(&str, &str)]) -> PathBuf {
        let share = own_share(root);
        std::fs::create_dir_all(&share).unwrap();
        for (name, body) in files {
            std::fs::write(share.join(name), body).unwrap();
        }
        root.join("bin")
    }

    #[test]
    fn packages_map_to_their_format() {
        for (word, want) in [
            ("deb\n", InstallKind::Deb),
            ("rpm", InstallKind::Rpm),
            ("archlinux", InstallKind::Arch),
            ("xyz", InstallKind::Source),
        ] {
            let dir = tempfile::tempdir().unwrap();
            let bin = bin_with(dir.path(), &[("package", word)]);
            assert_eq!(install_kind(&bin), want, "{word}");
        }
    }

    #[test]
    fn tarball_is_tar_and_other_user_installs_are_source() {
        let dir = tempfile::tempdir().unwrap();
        let bin = bin_with(dir.path(), &[("installed-files", ""), ("tarball", "")]);
        assert_eq!(
            install_kind(&bin),
            InstallKind::Tar {
                prefix: dir.path().to_path_buf()
            }
        );

        let dir = tempfile::tempdir().unwrap();
        assert_eq!(install_kind(&dir.path().join("bin")), InstallKind::Source);
        let bin = bin_with(dir.path(), &[("installed-files", "")]);
        assert_eq!(install_kind(&bin), InstallKind::Source);
    }
}
