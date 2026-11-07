//! Tirar os programas: pelo gerenciador de pacotes, se vieram de um pacote, ou pela lista que o
//! `install.sh` gravou, se foram instalados para o usuário.

use std::ffi::OsStr;
use std::io;
use std::path::{Component, Path, PathBuf};

use gdl_common::fs::remove_file_if_exists;

use gdl_common::brand::PACKAGE;
use gdl_common::install::own_share;
pub use gdl_common::install::{Install, detect};

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

/// O arquivo que o `install-dev.sh` grava em `environment.d` para o Qt achar os plugins do
/// Dolphin: a única coisa instalada fora do prefixo.
fn qt_env_file() -> PathBuf {
    Path::new("environment.d").join(format!("50-{PACKAGE}.conf"))
}

/// Apaga os arquivos da lista e a própria lista, e as pastas que ficarem vazias dentro da pasta
/// do app. Só o que está dentro de `prefix` (e o arquivo do Qt em `environment.d`): uma lista
/// adulterada não apaga outra coisa.
pub fn remove_listed(prefix: &Path, manifest: &Path) -> io::Result<()> {
    let own = own_share(prefix);
    for line in std::fs::read_to_string(manifest)?.lines() {
        let file = Path::new(line);
        let inside = file.is_absolute()
            && (file.starts_with(prefix) || file.ends_with(qt_env_file()))
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
        let qt_env = dir.path().join(".config").join(qt_env_file());
        let other_conf = dir.path().join(".config/environment.d/90-outro.conf");
        for file in [&qt_env, &other_conf] {
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, b"").unwrap();
        }
        let list = [
            &program,
            &icon,
            &outside,
            &escape,
            &qt_env,
            &other_conf,
            &prefix.join("sumiu"),
        ]
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
        assert!(!qt_env.exists(), "deixou o arquivo do Qt em environment.d");
        assert!(other_conf.exists(), "apagou outro arquivo de environment.d");
        assert!(
            prefix.join("bin").is_dir(),
            "apagou uma pasta que não é do app"
        );
    }
}
