//! O que muda no GeraSync (`--cfg gerasync`) na compilação do serviço:
//!
//! - o `.desktop` do início automático: o do pacote como está ou, no GeraSync, passado pela
//!   tabela `packaging/gerasync/rename.sed`, a mesma da instalação;
//! - o Client ID oficial, de `GERASYNC_CLIENT_ID` e `GERASYNC_CLIENT_SECRET`. Sem eles a
//!   compilação falha, a não ser com `GERASYNC_ALLOW_FAKE_CREDS=1` (CI e testes), que embute
//!   credenciais de mentira. Os valores nunca são impressos.

use std::path::PathBuf;
use std::process::Command;

const FAKE_ID: &str = "gerasync-fake.apps.googleusercontent.com";
const FAKE_SECRET: &str = "fake-secret";

fn main() {
    let gerasync = std::env::var_os("CARGO_CFG_GERASYNC").is_some();
    autostart(gerasync);
    if gerasync {
        official_credentials();
    }
}

fn autostart(gerasync: bool) {
    let repo = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../..");
    let desktop = repo.join("packaging/desktop/io.github.junior9283.GDriveDesktopLinux.desktop");
    let table = repo.join("packaging/gerasync/rename.sed");
    println!("cargo:rerun-if-changed={}", desktop.display());
    println!("cargo:rerun-if-changed={}", table.display());
    let out = PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("autostart.desktop");
    let body = if gerasync {
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

fn official_credentials() {
    for var in [
        "GERASYNC_CLIENT_ID",
        "GERASYNC_CLIENT_SECRET",
        "GERASYNC_ALLOW_FAKE_CREDS",
    ] {
        println!("cargo:rerun-if-env-changed={var}");
    }
    let given = |var: &str| {
        std::env::var(var)
            .ok()
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let (id, secret) = match (given("GERASYNC_CLIENT_ID"), given("GERASYNC_CLIENT_SECRET")) {
        (Some(id), Some(secret)) => (id, secret),
        _ if std::env::var("GERASYNC_ALLOW_FAKE_CREDS").as_deref() == Ok("1") => {
            (FAKE_ID.to_owned(), FAKE_SECRET.to_owned())
        }
        _ => panic!("GeraSync: defina GERASYNC_CLIENT_ID e GERASYNC_CLIENT_SECRET"),
    };
    println!("cargo:rustc-env=GDL_OFFICIAL_CLIENT_ID={id}");
    println!("cargo:rustc-env=GDL_OFFICIAL_CLIENT_SECRET={secret}");
}
