//! Integração com a área de trabalho: início automático na sessão (`AutoStartOnLogin`), a
//! montagem nos "Locais" do Dolphin (`ShowMountPointInSidebar`) e fora do indexador do KDE
//! (Baloo).

use std::io;
use std::path::Path;

use gdl_common::APP_ID;
use gdl_common::fs::{remove_file_if_exists, write_atomic};
use url::Url;

/// Nome do arquivo em `~/.config/autostart`.
pub const AUTOSTART_FILE: &str = "io.github.junior9283.GDriveDesktopLinux.desktop";

const AUTOSTART: &str =
    include_str!("../../../packaging/desktop/io.github.junior9283.GDriveDesktopLinux.desktop");

const TRAY_EXEC: &str = "Exec=/usr/bin/gdrive-desktop-linux-tray";

/// Grava ou apaga `<config_home>/autostart/…desktop`, que abre a bandeja de `bin` (e ela, o
/// serviço) na sessão. Só grava se mudou.
pub fn set_autostart(config_home: &Path, bin: &Path, enabled: bool) -> io::Result<()> {
    if !enabled {
        return remove_autostart(config_home);
    }
    let file = config_home.join("autostart").join(AUTOSTART_FILE);
    let body = autostart_body(bin);
    match std::fs::read_to_string(&file) {
        Ok(current) if current == body => Ok(()),
        _ => write_atomic(&file, body.as_bytes()),
    }
}

/// Apaga `<config_home>/autostart/…desktop` (a desinstalação; sem ele, nada a fazer).
pub fn remove_autostart(config_home: &Path) -> io::Result<()> {
    remove_file_if_exists(&config_home.join("autostart").join(AUTOSTART_FILE))
}

/// O `.desktop` com a bandeja de `bin`, entre aspas (o Exec separa nos espaços); uma pasta que
/// nem entre aspas cabe no Exec fica de fora, e a bandeja é procurada no PATH (a mesma lista que
/// o `scripts/install-dev.sh` recusa). O `TryExec` faz a sessão ignorar a entrada quando o app
/// foi desinstalado pelo gerenciador de pacotes (que não alcança a pasta do usuário).
fn autostart_body(bin: &Path) -> String {
    let bin = bin.to_string_lossy();
    let exec = if bin.contains(['"', '`', '$', '\\', '%']) {
        "TryExec=gdrive-desktop-linux-tray\nExec=gdrive-desktop-linux-tray".to_owned()
    } else {
        format!("TryExec={bin}/gdrive-desktop-linux-tray\nExec=\"{bin}/gdrive-desktop-linux-tray\"")
    };
    AUTOSTART.replace(TRAY_EXEC, &exec)
}

/// Põe (no fim) ou tira a montagem dos "Locais" do KDE (`user-places.xbel`), sem mexer no resto
/// do arquivo. Sem o arquivo, não faz nada: quem cria é o próprio KDE.
pub fn set_sidebar_place(
    places: &Path,
    mount: &Path,
    title: &str,
    enabled: bool,
) -> io::Result<()> {
    let body = match std::fs::read_to_string(places) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
        result => result?,
    };
    let href = Url::from_file_path(mount)
        .map_err(|()| io::Error::other("a montagem precisa ser um caminho absoluto"))?;
    let opening = format!("<bookmark href=\"{href}\"");
    let updated = match (body.find(&opening), enabled) {
        (Some(_), true) | (None, false) => return Ok(()),
        (Some(start), false) => {
            let start = body[..start].rfind('\n').map_or(0, |n| n + 1);
            let Some(end) = body[start..].find("</bookmark>") else {
                return Ok(());
            };
            let end = start + end + "</bookmark>".len();
            let end = end + usize::from(body[end..].starts_with('\n'));
            format!("{}{}", &body[..start], &body[end..])
        }
        (None, true) => {
            let Some(close) = body.rfind("</xbel>") else {
                tracing::warn!(arquivo = %places.display(), "Locais do KDE sem </xbel>; não mexo");
                return Ok(());
            };
            let place = format!(
                " {opening}>\n  <title>{}</title>\n  <info>\n   \
                 <metadata owner=\"http://freedesktop.org\">\n    \
                 <bookmark:icon name=\"{APP_ID}\"/>\n   \
                 </metadata>\n  </info>\n </bookmark>\n",
                xml_text(title)
            );
            format!("{}{place}{}", &body[..close], &body[close..])
        }
    };
    write_atomic(places, updated.as_bytes())
}

/// Põe (ou tira) a montagem das pastas que o Baloo não indexa, em `<config_home>/baloofilerc`.
/// Sem o arquivo, só o cria numa sessão do KDE (com `kdeglobals`).
pub fn set_baloo_exclusion(config_home: &Path, mount: &Path, enabled: bool) -> io::Result<()> {
    let file = config_home.join("baloofilerc");
    let body = match std::fs::read_to_string(&file) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            if !config_home.join("kdeglobals").exists() {
                return Ok(());
            }
            String::new()
        }
        result => result?,
    };
    match baloo_exclusion(&body, mount, enabled) {
        Some(updated) => write_atomic(&file, updated.as_bytes()),
        None => Ok(()),
    }
}

/// O `baloofilerc` com a montagem nas pastas que o Baloo não indexa (`enabled`) ou sem ela;
/// `None` se já está assim. As outras exclusões ficam. Indexar a montagem baixaria o Drive
/// inteiro para ler o conteúdo dos arquivos.
pub fn baloo_exclusion(body: &str, mount: &Path, enabled: bool) -> Option<String> {
    // Na lista do KConfig, vírgula e barra invertida vão escapadas.
    let ours = format!("{}/", mount.display())
        .replace('\\', "\\\\")
        .replace(',', "\\,");
    let mut lines: Vec<String> = body.lines().map(String::from).collect();
    let general = lines.iter().position(|l| l.trim() == "[General]");
    let section_end = |start: usize| {
        lines[start + 1..]
            .iter()
            .position(|l| l.starts_with('['))
            .map_or(lines.len(), |n| start + 1 + n)
    };
    let key = general.and_then(|start| {
        (start + 1..section_end(start)).find(|&i| {
            let name = lines[i].split('=').next().unwrap_or_default().trim();
            name == "exclude folders" || name.starts_with("exclude folders[")
        })
    });
    match (key, enabled) {
        (Some(i), _) => {
            let (name, value) = lines[i].split_once('=').unwrap_or((&lines[i], ""));
            let mut folders = split_kconfig_list(value);
            let present = folders.contains(&ours);
            if present == enabled {
                return None;
            }
            if enabled {
                folders.push(ours);
            } else {
                folders.retain(|f| *f != ours);
            }
            if folders.is_empty() {
                lines.remove(i);
            } else {
                lines[i] = format!("{name}={}", folders.join(","));
            }
        }
        (None, false) => return None,
        (None, true) => {
            let line = format!("exclude folders[$e]={ours}");
            match general {
                Some(start) => {
                    // Depois da última linha com conteúdo da seção.
                    let mut at = section_end(start);
                    while at > start + 1 && lines[at - 1].trim().is_empty() {
                        at -= 1;
                    }
                    lines.insert(at, line);
                }
                None => {
                    if !lines.is_empty() {
                        lines.push(String::new());
                    }
                    lines.extend(["[General]".into(), line]);
                }
            }
        }
    }
    Some(lines.join("\n") + "\n")
}

/// Os itens de uma lista do KConfig, separados por vírgulas sem escape (o escape fica).
fn split_kconfig_list(value: &str) -> Vec<String> {
    let mut items = vec![String::new()];
    let mut escaped = false;
    for c in value.chars() {
        match (c, escaped) {
            (',', false) => items.push(String::new()),
            _ => items.last_mut().unwrap().push(c),
        }
        escaped = c == '\\' && !escaped;
    }
    items.retain(|i| !i.is_empty());
    items
}

/// O texto como texto dentro de XML ou da marcação dos avisos.
pub(crate) fn xml_text(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn autostart_file_follows_preference() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("autostart").join(AUTOSTART_FILE);

        let bin = Path::new("/opt/com espaço/bin");
        set_autostart(tmp.path(), bin, true).unwrap();
        let body = std::fs::read_to_string(&file).unwrap();
        assert!(body.starts_with("[Desktop Entry]\n"), "{body}");
        assert!(
            body.contains("\nExec=\"/opt/com espaço/bin/gdrive-desktop-linux-tray\"\n"),
            "{body}"
        );
        // Sem o programa (desinstalado pelo gerenciador), a sessão ignora a entrada.
        assert!(
            body.contains("\nTryExec=/opt/com espaço/bin/gdrive-desktop-linux-tray\n"),
            "{body}"
        );

        set_autostart(tmp.path(), bin, false).unwrap();
        assert!(!file.exists());
        set_autostart(tmp.path(), bin, false).unwrap();
    }

    #[test]
    fn autostart_falls_back_to_the_path_when_the_folder_cannot_be_quoted() {
        let tmp = tempfile::tempdir().unwrap();
        set_autostart(tmp.path(), Path::new("/opt/a\"b"), true).unwrap();
        let body = std::fs::read_to_string(tmp.path().join("autostart").join(AUTOSTART_FILE));
        assert!(
            body.unwrap().contains("\nExec=gdrive-desktop-linux-tray\n"),
            "procura no PATH"
        );
    }

    const PLACES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE xbel>
<xbel xmlns:bookmark="http://www.freedesktop.org/standards/desktop-bookmarks">
 <bookmark href="file:///home/ana">
  <title>Pasta pessoal</title>
 </bookmark>
 <bookmark href="trash:/">
  <title>Lixeira</title>
 </bookmark>
</xbel>
"#;

    #[test]
    fn sidebar_bookmark_added_and_removed_keeping_other_places() {
        let tmp = tempfile::tempdir().unwrap();
        let places = tmp.path().join("user-places.xbel");
        let mount = Path::new("/home/ana/Google Drive");
        std::fs::write(&places, PLACES).unwrap();

        set_sidebar_place(&places, mount, "Google Drive", true).unwrap();
        set_sidebar_place(&places, mount, "Google Drive", true).unwrap();

        let body = std::fs::read_to_string(&places).unwrap();
        assert_eq!(
            body.matches("file:///home/ana/Google%20Drive").count(),
            1,
            "{body}"
        );
        assert!(body.contains("<title>Google Drive</title>"), "{body}");
        assert!(body.contains("<title>Lixeira</title>") && body.ends_with("</xbel>\n"));
        let ours = body.find("Google%20Drive").unwrap();
        assert!(ours > body.find("trash:/").unwrap(), "vai no fim da lista");

        set_sidebar_place(&places, mount, "Google Drive", false).unwrap();
        assert_eq!(std::fs::read_to_string(&places).unwrap(), PLACES);

        let missing = tmp.path().join("sem-arquivo.xbel");
        set_sidebar_place(&missing, mount, "Google Drive", true).unwrap();
        assert!(!missing.exists(), "sem o arquivo do KDE, não cria");
    }

    #[test]
    fn baloo_skips_the_mount_keeping_other_exclusions() {
        let mount = Path::new("/home/ana/Google Drive");
        let body = "[Basic Settings]\nIndexing-Enabled=true\n\n[General]\ndbVersion=2\n\
                    exclude folders[$e]=$HOME/build/,/srv/\n";

        let merged = baloo_exclusion(body, mount, true).unwrap();

        assert!(
            merged.contains("exclude folders[$e]=$HOME/build/,/srv/,/home/ana/Google Drive/\n"),
            "{merged}"
        );
        assert!(
            merged.starts_with("[Basic Settings]\nIndexing-Enabled=true\n"),
            "{merged}"
        );
        assert_eq!(baloo_exclusion(&merged, mount, true), None, "já está lá");
        assert_eq!(
            baloo_exclusion(&merged, mount, false).as_deref(),
            Some(body)
        );
    }

    #[test]
    fn baloo_file_is_only_created_in_a_kde_session() {
        let tmp = tempfile::tempdir().unwrap();
        let mount = Path::new("/home/ana/Google Drive");
        let file = tmp.path().join("baloofilerc");

        set_baloo_exclusion(tmp.path(), mount, true).unwrap();
        assert!(!file.exists(), "fora do KDE, nada");

        std::fs::write(tmp.path().join("kdeglobals"), "").unwrap();
        set_baloo_exclusion(tmp.path(), mount, true).unwrap();
        let body = std::fs::read_to_string(&file).unwrap();
        assert!(body.contains("=/home/ana/Google Drive/"), "{body}");
    }

    #[test]
    fn baloo_gets_the_key_and_the_section_when_missing() {
        let mount = Path::new("/home/ana/Drive, pessoal");
        assert_eq!(
            baloo_exclusion("[General]\ndbVersion=2\n", mount, true).unwrap(),
            "[General]\ndbVersion=2\nexclude folders[$e]=/home/ana/Drive\\, pessoal/\n"
        );
        assert_eq!(
            baloo_exclusion("", mount, true).unwrap(),
            "[General]\nexclude folders[$e]=/home/ana/Drive\\, pessoal/\n"
        );
        assert_eq!(baloo_exclusion("", mount, false), None);
    }
}
