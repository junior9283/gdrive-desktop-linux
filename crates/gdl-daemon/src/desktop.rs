//! Integração com a área de trabalho: início automático na sessão (`AutoStartOnLogin`) e a
//! montagem nos "Locais" do Dolphin (`ShowMountPointInSidebar`).

use std::io;
use std::path::Path;

use gdl_common::fs::{remove_file_if_exists, write_atomic};
use url::Url;

/// ID do app (arquivo `.desktop` e ícone).
pub const APP_ID: &str = "io.github.junior9283.GDriveDesktopLinux";
/// Nome do arquivo em `~/.config/autostart`.
pub const AUTOSTART_FILE: &str = "io.github.junior9283.GDriveDesktopLinux.desktop";

const AUTOSTART: &str =
    include_str!("../../../packaging/desktop/io.github.junior9283.GDriveDesktopLinux.desktop");

const TRAY_EXEC: &str = "Exec=/usr/bin/gdrive-desktop-linux-tray";

/// Grava ou apaga `<config_home>/autostart/…desktop`, que abre a bandeja de `bin` (e ela, o
/// serviço) na sessão. Só grava se mudou.
pub fn set_autostart(config_home: &Path, bin: &Path, enabled: bool) -> io::Result<()> {
    let file = config_home.join("autostart").join(AUTOSTART_FILE);
    if !enabled {
        return remove_file_if_exists(&file);
    }
    let body = autostart_body(bin);
    match std::fs::read_to_string(&file) {
        Ok(current) if current == body => Ok(()),
        _ => write_atomic(&file, body.as_bytes()),
    }
}

/// O `.desktop` com a bandeja de `bin`, entre aspas (o Exec separa nos espaços); uma pasta que
/// nem entre aspas cabe no Exec fica de fora, e a bandeja é procurada no PATH (a mesma lista que
/// o `scripts/install-dev.sh` recusa).
fn autostart_body(bin: &Path) -> String {
    let bin = bin.to_string_lossy();
    let exec = if bin.contains(['"', '`', '$', '\\', '%']) {
        "Exec=gdrive-desktop-linux-tray".to_owned()
    } else {
        format!("Exec=\"{bin}/gdrive-desktop-linux-tray\"")
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

fn xml_text(text: &str) -> String {
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
}
