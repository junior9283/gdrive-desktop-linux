//! Abrir endereços e pastas fora do app: `DefaultWebBrowser` quando o administrador definiu,
//! senão `xdg-open` (o navegador ou o gerenciador de arquivos padrão do usuário).

use std::io;
use std::path::Path;
use std::process::{Command, Stdio};

/// Abre `target` (endereço ou pasta) e espera o lançador terminar.
pub fn open(target: &str, browser: Option<&Path>) -> io::Result<()> {
    let program = browser.unwrap_or(Path::new("xdg-open"));
    let status = Command::new(program)
        .arg(target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{} terminou com {status}",
            program.display()
        )))
    }
}
