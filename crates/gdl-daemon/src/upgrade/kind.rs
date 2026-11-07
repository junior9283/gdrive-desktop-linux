//! O formato da instalação, para escolher o pacote do release e o comando que o instala
//! (`gdl_common::install` lê os marcadores). Instalação do código-fonte ou pasta movida:
//! `Source`, que não se atualiza sozinha.

use std::path::{Path, PathBuf};

use gdl_common::brand::PACKAGE;
use gdl_common::install::{Install, detect};

use crate::updates::{Version, text};

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

/// O fim do nome do `.tar.gz` do release, depois de `<nome>-<versão>` (a pasta que ele cria).
pub const TAR_SUFFIX: &str = "-x86_64.tar.gz";

impl InstallKind {
    /// Instala pelo gerenciador de pacotes do sistema, com a senha do `pkexec`.
    pub fn needs_root(&self) -> bool {
        matches!(
            self,
            InstallKind::Deb | InstallKind::Rpm | InstallKind::Arch
        )
    }

    /// O nome do pacote desta variante neste formato, no release `version` (os nomes que o
    /// `scripts/package.sh` gera); `None` quando não há pacote para esta instalação (os releases
    /// só trazem x86_64).
    pub fn package_name(&self, version: Version) -> Option<String> {
        if !cfg!(target_arch = "x86_64") {
            return None;
        }
        let (p, v) = (PACKAGE, text(version));
        Some(match self {
            InstallKind::Deb => format!("{p}_{v}_amd64.deb"),
            InstallKind::Rpm => format!("{p}-{v}-1.x86_64.rpm"),
            InstallKind::Arch => format!("{p}-{v}-1-x86_64.pkg.tar.zst"),
            InstallKind::Tar { .. } => format!("{p}-{v}{TAR_SUFFIX}"),
            InstallKind::Source => return None,
        })
    }
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

    /// Os nomes do `scripts/package.sh` (os do release 1.3.2, com outra versão).
    #[test]
    fn package_names_match_package_sh() {
        let (v, p) = ((1, 4, 0), PACKAGE);
        let tar = InstallKind::Tar {
            prefix: "/home/x/.local".into(),
        };
        let name = |kind: &InstallKind| kind.package_name(v);
        assert_eq!(
            name(&InstallKind::Deb),
            Some(format!("{p}_1.4.0_amd64.deb"))
        );
        assert_eq!(
            name(&InstallKind::Rpm),
            Some(format!("{p}-1.4.0-1.x86_64.rpm"))
        );
        assert_eq!(
            name(&InstallKind::Arch),
            Some(format!("{p}-1.4.0-1-x86_64.pkg.tar.zst"))
        );
        assert_eq!(name(&tar), Some(format!("{p}-1.4.0-x86_64.tar.gz")));
        assert_eq!(name(&InstallKind::Source), None);
    }

    #[test]
    fn only_system_packages_need_root() {
        assert!(InstallKind::Deb.needs_root());
        assert!(InstallKind::Rpm.needs_root());
        assert!(InstallKind::Arch.needs_root());
        let tar = InstallKind::Tar {
            prefix: "/x".into(),
        };
        assert!(!tar.needs_root());
        assert!(!InstallKind::Source.needs_root());
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
