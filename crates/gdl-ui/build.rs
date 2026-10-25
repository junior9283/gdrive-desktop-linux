//! O build da janela: o do Tauri e a lista dos softwares de terceiros do "Sobre"
//! (`OUT_DIR/third-party.json`), tirada do `cargo metadata` do workspace.

use std::path::PathBuf;
use std::process::Command;

#[path = "src/third_party.rs"]
mod third_party;

fn main() {
    tauri_build::build();
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    println!(
        "cargo:rerun-if-changed={}",
        workspace.join("Cargo.lock").display()
    );
    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR")).join("third-party.json");
    // Sem a lista, o build de desenvolvimento só avisa; o de release (o que vai para os
    // usuários) para, porque o Sobre sairia sem os créditos que as licenças exigem.
    let list = match metadata(&workspace) {
        Ok(m) => third_party::licenses(&m),
        Err(e) if std::env::var("PROFILE").as_deref() == Ok("release") => {
            panic!("sem a lista de terceiros do Sobre: {e}")
        }
        Err(e) => {
            println!("cargo:warning=sem a lista de terceiros do Sobre: {e}");
            serde_json::Value::Array(Vec::new())
        }
    };
    std::fs::write(out, list.to_string()).expect("gravar third-party.json");
}

/// O `cargo metadata` do workspace, só com o que vale para a plataforma do build.
fn metadata(workspace: &std::path::Path) -> Result<serde_json::Value, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let target = std::env::var("TARGET").map_err(|e| e.to_string())?;
    let output = Command::new(cargo)
        .args([
            "metadata",
            "--format-version",
            "1",
            "--locked",
            "--offline",
            "--filter-platform",
            &target,
        ])
        .arg("--manifest-path")
        .arg(workspace.join("Cargo.toml"))
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    serde_json::from_slice(&output.stdout).map_err(|e| e.to_string())
}
