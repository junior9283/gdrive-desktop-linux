//! "Gerar informações de diagnóstico": o pacote com logs, relatórios de falha, sistema,
//! preferências, contas e pastas, filtrado pelo `scrub`, na Área de trabalho.

use std::fmt::Write as _;

use gdl_common::crash;
use gdl_common::links::new_issue;
use gdl_common::names::create_numbered;
use gdl_common::paths::{crashes_dir, user_dir};
use gdl_common::time::{local_stamp, now_ms};
use gdl_ipc::ServiceError;

use super::{Daemon, failed};
use crate::diagnostics::{Package, scrub};
use crate::logging::logs_dir;
use crate::notify;

impl Daemon {
    /// Grava o pacote na Área de trabalho (sem ela, na pasta pessoal), abre a pasta e devolve o
    /// caminho do arquivo.
    pub async fn generate_diagnostics(&self) -> Result<String, ServiceError> {
        let (accounts, folders) = tokio::try_join!(self.accounts_text(), self.folders_text())?;
        let mut package = Package::default();
        package.add("sistema.txt", self.system_text());
        package.add("preferencias.txt", self.preferences_text()?);
        package.add("contas.txt", accounts);
        package.add("pastas.txt", folders);
        let (state_dir, config_home, home) = (
            self.config.paths.state_dir.clone(),
            self.config.paths.config_home.clone(),
            self.config.home.clone(),
        );
        let (dir, path) = tokio::task::spawn_blocking(move || -> std::io::Result<_> {
            package.add_dir("logs", &logs_dir(&state_dir));
            package.add_dir("crashes", &crashes_dir(&state_dir));
            let desktop = user_dir(&config_home, &home, "DESKTOP", "Desktop");
            let dir = if desktop.is_dir() { desktop } else { home };
            let stem = format!(
                "gdrive-desktop-linux-diagnostico-{}",
                local_stamp(now_ms()).replace(' ', "-")
            );
            let (path, file) = create_numbered(&dir, &stem, ".tar.gz")?;
            package.write(file)?;
            Ok((dir, path))
        })
        .await
        .map_err(failed)?
        .map_err(failed)?;
        self.open_local(&dir);
        Ok(path.display().to_string())
    }

    /// Na partida, com `AutomaticErrorReporting`: um aviso para as falhas ainda não vistas, que
    /// passam a vistas (cada uma é oferecida uma vez só). Desligado, elas esperam.
    pub async fn offer_crash_reports(&self) {
        if !self.prefs().automatic_error_reporting {
            return;
        }
        if crash::take_unseen(&crashes_dir(&self.config.paths.state_dir)) > 0 {
            self.notify(&notify::crash(&self.i18n)).await;
        }
    }

    /// "Enviar relatório": abre a *issue* com a falha mais recente, já filtrada, para o usuário
    /// revisar e enviar.
    pub(super) fn send_crash_report(&self) -> Result<(), ServiceError> {
        let dir = crashes_dir(&self.config.paths.state_dir);
        let latest = crash::reports(&dir)
            .pop()
            .ok_or_else(|| ServiceError::NotFound("relatório de falha".into()))?;
        let text = std::fs::read_to_string(&latest).map_err(failed)?;
        let title = text.lines().next().unwrap_or_default();
        let body = format!("```\n{}\n```", scrub(&text));
        self.open(new_issue(&format!("Falha: {title}"), &body));
        Ok(())
    }

    fn system_text(&self) -> String {
        let read = |path: &str| std::fs::read_to_string(path).unwrap_or_default();
        let pretty = read("/etc/os-release")
            .lines()
            .find_map(|l| l.strip_prefix("PRETTY_NAME="))
            .map(|v| v.trim_matches('"').to_owned())
            .unwrap_or_default();
        let env = |key: &str| std::env::var(key).unwrap_or_default();
        let fuse = ["/dev/fuse", "/usr/bin/fusermount3"]
            .iter()
            .map(|p| format!("{p}: {}", std::path::Path::new(p).exists()))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "gdrive-desktop-linux {}\nsistema: {pretty}\nkernel: {}\narquitetura: {}\n\
             área de trabalho: {}\nsessão: {}\nidioma: {}\nFUSE: {fuse}\n",
            env!("CARGO_PKG_VERSION"),
            read("/proc/sys/kernel/osrelease").trim(),
            std::env::consts::ARCH,
            env("XDG_CURRENT_DESKTOP"),
            env("XDG_SESSION_TYPE"),
            self.config.lang,
        )
    }

    fn preferences_text(&self) -> Result<String, ServiceError> {
        let mut out = String::new();
        for p in self.preferences()? {
            let managed = if p.managed { " (política)" } else { "" };
            let _ = writeln!(out, "{} = {}{managed}", p.key, p.value);
        }
        Ok(out)
    }

    async fn accounts_text(&self) -> Result<String, ServiceError> {
        let status = self.status("").await?;
        let mut out = format!(
            "estado: {}\nsincronizando: {}\nenvios pendentes: {}\nerros: {}\npausado: {}\n",
            status.state, status.syncing, status.pending_uploads, status.errors, status.paused
        );
        for a in &status.accounts {
            let _ = writeln!(
                out,
                "conta {}: {} ({} envios pendentes) em {}",
                a.email, a.state, a.pending_uploads, a.mount_point
            );
        }
        Ok(out)
    }

    async fn folders_text(&self) -> Result<String, ServiceError> {
        let mut out = String::new();
        for f in self.local_folders().await? {
            let _ = writeln!(out, "{} #{}: {} [{}]", f.account, f.id, f.path, f.state);
        }
        Ok(out)
    }
}
