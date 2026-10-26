//! Tirar os programas: pelo gerenciador de pacotes, se vieram de um pacote, ou pela lista que o
//! `install.sh` gravou, se foram instalados para o usuário.

use std::ffi::OsStr;
use std::io;
use std::path::{Component, Path, PathBuf};

use gdl_common::fs::remove_file_if_exists;

/// O nome do pacote nos três formatos.
const PACKAGE: &str = "gdrive-desktop-linux";

/// Como o app foi instalado.
#[derive(Debug, PartialEq)]
pub enum Install {
    /// Por um pacote: `deb`, `rpm` ou `archlinux` (o marcador que cada pacote traz).
    Package(String),
    /// Pelo `install.sh`/`install-dev.sh` no prefixo, com a lista do que instalou.
    User { prefix: PathBuf, manifest: PathBuf },
}

/// A pasta do app em `<prefixo>/share`, ao lado de `<prefixo>/bin`.
fn own_share(prefix: &Path) -> PathBuf {
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
        manifest,
    })
}

/// O comando (com o `pkexec`, que pede a senha de administrador) que remove o pacote. A
/// ferramenta de baixo nível de cada formato serve a todas as distribuições dele (o `rpm` ao
/// Fedora e ao openSUSE): remover não precisa resolver dependências.
pub fn package_removal(kind: &str) -> Option<Vec<&'static str>> {
    let tool: &[&str] = match kind {
        "deb" => &["dpkg", "--remove"],
        "rpm" => &["rpm", "--erase"],
        "archlinux" => &["pacman", "--remove", "--noconfirm"],
        _ => return None,
    };
    Some([&["pkexec"], tool, &[PACKAGE]].concat())
}

/// Apaga os arquivos da lista e a própria lista, e as pastas que ficarem vazias dentro da pasta
/// do app. Só o que está dentro de `prefix`: uma lista adulterada não apaga outra coisa.
pub fn remove_listed(prefix: &Path, manifest: &Path) -> io::Result<()> {
    let own = own_share(prefix);
    for line in std::fs::read_to_string(manifest)?.lines() {
        let file = Path::new(line);
        let inside = file.is_absolute()
            && file.starts_with(prefix)
            && !file.components().any(|c| c == Component::ParentDir);
        if !inside {
            continue;
        }
        remove_file_if_exists(file)?;
        let mut dir = file.parent();
        while let Some(d) = dir.filter(|d| d.starts_with(&own) && *d != own) {
            if std::fs::remove_dir(d).is_err() {
                break;
            }
            dir = d.parent();
        }
    }
    remove_file_if_exists(manifest)?;
    // Sem os dados do app, a pasta dele fica vazia.
    let _ = std::fs::remove_dir(&own);
    Ok(())
}

/// Avisa o menu, os tipos, os ícones e o systemd de que os arquivos saíram (melhor esforço; a
/// mesma lista do fim do `scripts/install-dev.sh`).
pub fn refresh(prefix: &Path) {
    let share = prefix.join("share");
    let run = |program: &str, args: &[&OsStr]| {
        let _ = std::process::Command::new(program)
            .args(args)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
    };
    let (mime, menu, icons) = (
        share.join("mime"),
        share.join("applications"),
        share.join("icons/hicolor"),
    );
    run("update-mime-database", &[mime.as_os_str()]);
    run("update-desktop-database", &[menu.as_os_str()]);
    run(
        "gtk-update-icon-cache",
        &["-q".as_ref(), "-t".as_ref(), icons.as_os_str()],
    );
    run("systemctl", &["--user".as_ref(), "daemon-reload".as_ref()]);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_marker_wins_over_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let share = own_share(dir.path());
        std::fs::create_dir_all(&share).unwrap();
        std::fs::write(share.join("installed-files"), "").unwrap();
        std::fs::write(share.join("package"), "rpm\n").unwrap();

        let install = detect(&dir.path().join("bin"));

        assert_eq!(install, Some(Install::Package("rpm".into())));
    }

    #[test]
    fn list_means_a_user_install_and_nothing_means_none() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("bin");
        assert_eq!(detect(&bin), None);

        let share = own_share(dir.path());
        std::fs::create_dir_all(&share).unwrap();
        std::fs::write(share.join("installed-files"), "").unwrap();

        assert_eq!(
            detect(&bin),
            Some(Install::User {
                prefix: dir.path().to_path_buf(),
                manifest: share.join("installed-files"),
            })
        );
    }

    #[test]
    fn each_format_removes_its_package_with_pkexec() {
        assert_eq!(
            package_removal("deb").unwrap(),
            ["pkexec", "dpkg", "--remove", PACKAGE]
        );
        assert_eq!(
            package_removal("rpm").unwrap(),
            ["pkexec", "rpm", "--erase", PACKAGE]
        );
        assert_eq!(
            package_removal("archlinux").unwrap(),
            ["pkexec", "pacman", "--remove", "--noconfirm", PACKAGE]
        );
        assert_eq!(package_removal("flatpak"), None);
    }

    #[test]
    fn removes_only_listed_files_inside_the_prefix() {
        let dir = tempfile::tempdir().unwrap();
        let (prefix, outside) = (dir.path().join("pfx"), dir.path().join("fora.txt"));
        let program = prefix.join("bin/gdrive-desktop-linux");
        let icon = own_share(&prefix).join("icons/hicolor/scalable/status/gdl-synced.svg");
        let data = own_share(&prefix).join("conta/metadata.db");
        let unlisted = prefix.join("bin/outro-programa");
        for file in [&program, &icon, &data, &unlisted, &outside] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"").unwrap();
        }
        let manifest = own_share(&prefix).join("installed-files");
        let escape = prefix.join("bin/../../fora.txt");
        let list = [&program, &icon, &outside, &escape, &prefix.join("sumiu")]
            .map(|p| p.display().to_string())
            .join("\n");
        std::fs::write(&manifest, list).unwrap();

        remove_listed(&prefix, &manifest).unwrap();

        assert!(!program.exists() && !icon.exists() && !manifest.exists());
        assert!(
            !own_share(&prefix).join("icons").exists(),
            "pastas vazias do app ficaram"
        );
        assert!(data.exists(), "apagou dados que não estavam na lista");
        assert!(unlisted.exists() && outside.exists());
        assert!(
            prefix.join("bin").is_dir(),
            "apagou uma pasta que não é do app"
        );
    }
}
