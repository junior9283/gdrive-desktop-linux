//! O menu do botão direito no COSMIC Files. Ele não aceita extensões nem submenus: só ações fixas
//! (`context_actions` da configuração), que aparecem em todo arquivo e rodam um comando. As nossas
//! chamam `gdrive-desktop-linux menu-action`, que faz a ação pelo serviço e avisa quando o item não
//! é do Meu Drive. As ações que o usuário criou ficam como estão.

use std::io;
use std::path::Path;

use gdl_common::fs::write_atomic;
use serde::{Deserialize, Serialize};

use crate::menu::Action;

/// A configuração do COSMIC Files, em `~/.config` (só existe se ele já rodou).
const APP_CONFIG: &str = "cosmic/com.system76.CosmicFiles";
const ACTIONS_FILE: &str = "v1/context_actions";
/// O programa que as ações chamam.
const CLI: &str = "gdrive-desktop-linux";

/// As ações soltas no menu (todas em todo arquivo; poucas, para não poluir).
const ACTIONS: [(Action, Selection); 5] = [
    (Action::OpenWeb, Selection::Any),
    (Action::CopyLink, Selection::Any),
    (Action::Offline, Selection::Any),
    (Action::OnlineOnly, Selection::Any),
    (Action::NewDocument, Selection::Folders),
];

/// Uma ação, no formato do COSMIC Files (`ContextActionPreset`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct Preset {
    name: String,
    confirm: bool,
    selection: Selection,
    steps: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Serialize, Deserialize)]
enum Selection {
    #[default]
    #[serde(alias = "any")]
    Any,
    #[serde(alias = "files")]
    Files,
    #[serde(alias = "folders")]
    Folders,
}

/// Põe as nossas ações no COSMIC Files (no lugar das que houver), com o rótulo de cada uma por
/// `label` e o comando de `bin`.
pub fn add_actions(
    config_home: &Path,
    bin: &Path,
    label: impl Fn(Action) -> String,
) -> io::Result<()> {
    update(config_home, |presets| {
        presets.extend(ACTIONS.map(|(action, selection)| Preset {
            name: format!("Google Drive: {}", label(action)),
            confirm: false,
            selection,
            steps: vec![format!("{} {} %F", command(bin), action.id())],
        }));
    })
}

/// Tira as nossas ações do COSMIC Files (a desinstalação).
pub fn remove_actions(config_home: &Path) -> io::Result<()> {
    update(config_home, |_| {})
}

/// O começo do comando das nossas ações (o que as identifica).
fn command(bin: &Path) -> String {
    format!("\"{}\" menu-action", bin.join(CLI).display())
}

/// Tira as nossas ações e deixa `add` pôr as novas. Sem o COSMIC Files, ou com o arquivo
/// ilegível, não mexe em nada; só grava se mudou.
fn update(config_home: &Path, add: impl FnOnce(&mut Vec<Preset>)) -> io::Result<()> {
    let app = config_home.join(APP_CONFIG);
    if !app.is_dir() {
        return Ok(());
    }
    let file = app.join(ACTIONS_FILE);
    let current = match std::fs::read_to_string(&file) {
        Ok(body) => body,
        Err(e) if e.kind() == io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e),
    };
    let mut presets: Vec<Preset> = match current.trim() {
        "" => Vec::new(),
        body => match ron::from_str(body) {
            Ok(presets) => presets,
            Err(e) => {
                tracing::warn!(erro = %e, arquivo = %file.display(), "ações do COSMIC ilegíveis; não mexo");
                return Ok(());
            }
        },
    };
    let before = presets.clone();
    // De qualquer pasta: uma instalação em outro prefixo também é nossa.
    let ours = format!("{CLI}\" menu-action ");
    presets.retain(|p| !p.steps.iter().any(|s| s.contains(&ours)));
    add(&mut presets);
    if presets == before {
        return Ok(());
    }
    let body = ron::ser::to_string_pretty(&presets, ron::ser::PrettyConfig::default())
        .map_err(io::Error::other)?;
    write_atomic(&file, body.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINE: &str = r#"[
    (
        name: "Abrir no terminal",
        selection: folders,
        steps: ["kitty --directory %f"],
    ),
]"#;

    fn label(action: Action) -> String {
        action.label_key().to_uppercase()
    }

    fn presets(file: &Path) -> Vec<Preset> {
        ron::from_str(&std::fs::read_to_string(file).unwrap()).unwrap()
    }

    #[test]
    fn adds_ours_after_the_users_and_removes_them_again() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join(APP_CONFIG).join(ACTIONS_FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, MINE).unwrap();
        let bin = Path::new("/opt/com espaço/bin");

        add_actions(tmp.path(), bin, label).unwrap();
        add_actions(tmp.path(), bin, label).unwrap();

        let all = presets(&file);
        assert_eq!(all.len(), 1 + ACTIONS.len(), "{all:?}");
        assert_eq!(
            (all[0].name.as_str(), all[0].selection),
            ("Abrir no terminal", Selection::Folders)
        );
        let link = &all[2];
        assert_eq!(link.name, "Google Drive: MENU-COPY-LINK");
        assert_eq!(
            link.steps,
            ["\"/opt/com espaço/bin/gdrive-desktop-linux\" menu-action copy-link %F"]
        );
        assert_eq!(all.last().unwrap().selection, Selection::Folders);

        add_actions(tmp.path(), Path::new("/usr/bin"), label).unwrap();
        assert_eq!(
            presets(&file).len(),
            1 + ACTIONS.len(),
            "trocar a pasta não duplica"
        );

        remove_actions(tmp.path()).unwrap();
        let left = presets(&file);
        assert_eq!(left.len(), 1, "{left:?}");
        assert_eq!(left[0].steps, ["kitty --directory %f"]);
    }

    #[test]
    fn without_cosmic_files_or_with_a_broken_file_nothing_changes() {
        let tmp = tempfile::tempdir().unwrap();
        add_actions(tmp.path(), Path::new("/usr/bin"), label).unwrap();
        assert!(!tmp.path().join("cosmic").exists());

        let file = tmp.path().join(APP_CONFIG).join(ACTIONS_FILE);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(&file, "[ (name: ").unwrap();
        add_actions(tmp.path(), Path::new("/usr/bin"), label).unwrap();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "[ (name: ");
    }

    #[test]
    fn creates_the_file_when_cosmic_files_has_none() {
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(APP_CONFIG)).unwrap();
        add_actions(tmp.path(), Path::new("/usr/bin"), label).unwrap();
        let file = tmp.path().join(APP_CONFIG).join(ACTIONS_FILE);
        assert_eq!(presets(&file).len(), ACTIONS.len());
    }
}
