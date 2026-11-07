//! Como o app foi instalado, pelos marcadores em `<prefixo>/share/<PACKAGE>`: `package` (`deb`,
//! `rpm` ou `archlinux`), que vem em cada pacote; `installed-files`, a lista que o
//! `install-dev.sh` grava numa instalação para o usuário; e `tarball`, que ele grava quando quem
//! chama é o `install.sh` do `.tar.gz`. A desinstalação e a atualização automática leem daqui.

use std::path::{Path, PathBuf};

use crate::brand::PACKAGE;

#[derive(Debug, PartialEq)]
pub enum Install {
    /// Por um pacote: `deb`, `rpm` ou `archlinux` (o marcador que cada pacote traz).
    Package(String),
    /// Pelo `install.sh`/`install-dev.sh` no prefixo, com a lista do que instalou; `tarball`: veio
    /// do `.tar.gz` do release (não do código-fonte).
    User {
        prefix: PathBuf,
        manifest: PathBuf,
        tarball: bool,
    },
}

/// A pasta do app em `<prefixo>/share`, ao lado de `<prefixo>/bin`.
pub fn own_share(prefix: &Path) -> PathBuf {
    prefix.join("share").join(PACKAGE)
}

/// Como foram instalados os programas da pasta `bin`; `None` fora de uma instalação (rodando
/// do código-fonte, por exemplo).
pub fn detect(bin: &Path) -> Option<Install> {
    let prefix = bin.parent()?;
    let share = own_share(prefix);
    if let Ok(kind) = std::fs::read_to_string(share.join("package")) {
        return Some(Install::Package(kind.trim().to_owned()));
    }
    let manifest = share.join("installed-files");
    manifest.is_file().then(|| Install::User {
        prefix: prefix.to_path_buf(),
        tarball: share.join("tarball").is_file(),
        manifest,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `<raiz>/share/<PACKAGE>` com os marcadores `files` (nome, conteúdo).
    fn share_with(root: &Path, files: &[(&str, &str)]) -> PathBuf {
        let share = own_share(root);
        std::fs::create_dir_all(&share).unwrap();
        for (name, body) in files {
            std::fs::write(share.join(name), body).unwrap();
        }
        share
    }

    #[test]
    fn package_marker_wins_over_the_list() {
        let dir = tempfile::tempdir().unwrap();
        share_with(dir.path(), &[("installed-files", ""), ("package", "rpm\n")]);

        let install = detect(&dir.path().join("bin"));

        assert_eq!(install, Some(Install::Package("rpm".into())));
    }

    #[test]
    fn list_means_a_user_install_and_nothing_means_none() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        assert_eq!(detect(&bin), None);

        let share = share_with(dir.path(), &[("installed-files", "")]);

        assert_eq!(
            detect(&bin),
            Some(Install::User {
                prefix: dir.path().to_path_buf(),
                manifest: share.join("installed-files"),
                tarball: false,
            })
        );
    }

    #[test]
    fn tarball_marker_tells_the_release_install_apart() {
        let dir = tempfile::tempdir().unwrap();
        share_with(dir.path(), &[("installed-files", ""), ("tarball", "")]);

        let install = detect(&dir.path().join("bin"));

        assert!(
            matches!(install, Some(Install::User { tarball: true, .. })),
            "{install:?}"
        );
    }
}
