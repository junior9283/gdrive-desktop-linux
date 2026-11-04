//! Gera o `.desktop` do início automático da variante: o do pacote como está ou, no GeraSync
//! (`--cfg gerasync`), passado pela tabela `packaging/gerasync/rename.sed`, a mesma da instalação.

use std::path::PathBuf;
use std::process::Command;

fn main() {
    let repo = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let desktop = repo.join("packaging/desktop/io.github.junior9283.GDriveDesktopLinux.desktop");
    let table = repo.join("packaging/gerasync/rename.sed");
    println!("cargo:rerun-if-changed={}", desktop.display());
    println!("cargo:rerun-if-changed={}", table.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("autostart.desktop");
    let body = if std::env::var_os("CARGO_CFG_GERASYNC").is_some() {
        let sed = Command::new("sed")
            .arg("-f")
            .arg(&table)
            .arg(&desktop)
            .output()
            .expect("o sed do sistema");
        assert!(sed.status.success(), "sed -f {}", table.display());
        sed.stdout
    } else {
        std::fs::read(&desktop).unwrap()
    };
    std::fs::write(out, body).unwrap();
}
