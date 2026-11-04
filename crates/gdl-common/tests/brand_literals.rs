//! A identidade do app vem só de `gdl_common::brand`: um nome escrito à mão em outro lugar
//! vazaria para o GeraSync.

use std::path::{Path, PathBuf};

fn crates() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// Os `.rs` de `src/` de todos os crates, menos o próprio `brand.rs` e os arquivos de teste.
fn sources() -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending: Vec<PathBuf> = std::fs::read_dir(crates())
        .unwrap()
        .map(|e| e.unwrap().path().join("src"))
        .collect();
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for path in entries.map(|e| e.unwrap().path()) {
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "tests" {
                    pending.push(path);
                }
            } else if name.ends_with(".rs") && name != "brand.rs" && !name.contains("test") {
                found.push(path);
            }
        }
    }
    found
}

/// Os arquivos em que "Google Drive" é o nome do app (a pasta, a barra lateral, o menu), não o
/// serviço do Google.
const APP_NAME_FILES: &[&str] = &[
    "mount.rs",
    "desktop.rs",
    "menu.rs",
    "menu_action.rs",
    "daemon.rs",
    "cosmic.rs",
];

/// O filtro do registro pelo nome dos crates dos programas (`gdrive_desktop_linuxd`…), que é o
/// mesmo nas duas variantes: não é a identidade do app.
const CRATE_TARGET: &str = "with_target(\"gdrive_desktop_linux\"";

#[test]
fn no_identity_literals_outside_brand() {
    let mut offenders = Vec::new();
    for path in sources() {
        let text = std::fs::read_to_string(&path).unwrap();
        let name = path.file_name().unwrap().to_string_lossy();
        // O bloco de testes no fim do arquivo pode citar os valores de sempre.
        let code = text.split("#[cfg(test)]\nmod tests").next().unwrap();
        // Dentro de `cfg_attr(not(gerasync), ...)`, os literais da variante de sempre são o
        // esperado (o atributo do zbus não aceita constante).
        let mut default_attr = false;
        for (number, line) in code.lines().enumerate() {
            if line.contains("not(gerasync)") {
                default_attr = !line.contains(")]");
                continue;
            }
            if default_attr {
                default_attr = !line.trim_start().starts_with(")]");
                continue;
            }
            if line.trim_start().starts_with("//") {
                continue;
            }
            let code = line
                // O repositório (páginas e API do GitHub) é o mesmo nas duas variantes.
                .replace("junior9283/gdrive-desktop-linux", "")
                .replace(CRATE_TARGET, "");
            let old_name = [
                "GDriveDesktopLinux",
                "gdrive-desktop-linux",
                "gdrive_desktop_linux",
            ]
            .iter()
            .any(|old| code.contains(old))
                || (APP_NAME_FILES.contains(&name.as_ref()) && line.contains("\"Google Drive"));
            if old_name {
                offenders.push(format!(
                    "{}:{}: {}",
                    path.display(),
                    number + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(offenders.is_empty(), "{}", offenders.join("\n"));
}
