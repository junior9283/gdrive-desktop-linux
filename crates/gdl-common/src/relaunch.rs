//! A troca de programa depois de uma atualização: o serviço, a bandeja e a janela voltam a rodar
//! o programa instalado agora no mesmo caminho (`paths::startup_exe`), com os mesmos argumentos.

use crate::paths::startup_exe;

/// A versão do serviço para a qual o programa já se reiniciou: se o programa no disco ainda não
/// for o dela (um build de desenvolvimento, uma instalação pela metade), não reinicia de novo.
const RESTARTED_FOR: &str = "GDL_RESTARTED_FOR";

/// O serviço que respondeu (`ServiceVersion`) é de outra versão que a deste programa (todos os
/// crates têm a versão do workspace). Vazio: um serviço antigo, sem o método.
pub fn outdated_for(service_version: &str) -> bool {
    needs_restart(
        service_version,
        std::env::var(RESTARTED_FOR).ok().as_deref(),
    )
}

fn needs_restart(service_version: &str, restarted_for: Option<&str>) -> bool {
    !service_version.is_empty()
        && service_version != env!("CARGO_PKG_VERSION")
        && restarted_for != Some(service_version)
}

/// Troca este processo pelo programa de `startup_exe`; `for_version`, a versão do serviço que
/// pediu a troca (ver `outdated_for`). Só volta se a troca falhar, com o erro.
pub fn exec_startup_exe(for_version: Option<&str>) -> std::io::Error {
    use std::os::unix::process::CommandExt;
    let mut command = std::process::Command::new(startup_exe());
    command.args(std::env::args_os().skip(1));
    if let Some(version) = for_version {
        command.env(RESTARTED_FOR, version);
    }
    command.exec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restarts_only_for_another_service_version() {
        assert!(!needs_restart("", None), "serviço antigo, sem o método");
        assert!(!needs_restart(env!("CARGO_PKG_VERSION"), None));
        assert!(needs_restart("99.0.0", None));
        assert!(needs_restart("99.0.0", Some("98.0.0")));
        assert!(
            !needs_restart("99.0.0", Some("99.0.0")),
            "já reiniciou para ela: sem laço"
        );
    }
}
