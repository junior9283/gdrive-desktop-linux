//! Relatórios de falha: quando um programa do app entra em pânico, a versão, a mensagem, o local
//! e o *backtrace* vão para `<dir>/<programa>-<data>.txt`, guardando os 20 mais novos. O serviço
//! oferece enviar os que ainda não viu e os marca como vistos (`.visto.txt`).

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use crate::names::create_numbered;
use crate::paths::{Paths, crashes_dir, create_private_dir};
use crate::time::{local_stamp, now_ms};

/// Quantos relatórios ficam guardados.
pub const KEEP: usize = 20;

const SEEN: &str = ".visto.txt";

/// Grava um relatório a cada pânico deste processo em `<state_dir>/crashes`, sem mudar o que
/// acontece depois (a mensagem no stderr e o fim da thread).
pub fn install(program: &'static str) {
    install_in(program, crashes_dir(&Paths::from_env().state_dir));
}

fn install_in(program: &'static str, dir: PathBuf) {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        let text = format!(
            "{program} {}\n\n{info}\n\n{backtrace}\n",
            env!("CARGO_PKG_VERSION")
        );
        if let Err(e) = save(&dir, program, &text) {
            eprintln!("{program}: não deu para gravar o relatório de falha: {e}");
        }
        previous(info);
    }));
}

fn save(dir: &Path, program: &str, text: &str) -> std::io::Result<()> {
    // O relatório pode ter caminhos e nomes de arquivo: só para o usuário.
    create_private_dir(dir)?;
    let stem = format!("{program}-{}", local_stamp(now_ms()).replace(' ', "-"));
    let (_, mut file) = create_numbered(dir, &stem, ".txt")?;
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    std::io::Write::write_all(&mut file, text.as_bytes())?;
    for old in reports(dir).iter().rev().skip(KEEP) {
        let _ = std::fs::remove_file(old);
    }
    Ok(())
}

/// Os relatórios de `dir`, do mais velho ao mais novo.
pub fn reports(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return vec![];
    };
    let mut found: Vec<(SystemTime, PathBuf)> = entries
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().ends_with(".txt"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .collect();
    found.sort();
    found.into_iter().map(|(_, path)| path).collect()
}

/// Marca como vistos os relatórios ainda não vistos de `dir` e diz quantos eram.
pub fn take_unseen(dir: &Path) -> usize {
    reports(dir)
        .iter()
        .filter(|p| !is_seen(p) && mark_seen(p).is_ok())
        .count()
}

fn is_seen(path: &Path) -> bool {
    path.to_string_lossy().ends_with(SEEN)
}

/// Renomeia `x.txt` para `x.visto.txt`.
fn mark_seen(path: &Path) -> std::io::Result<PathBuf> {
    let name = path.to_string_lossy();
    let seen = PathBuf::from(format!("{}{SEEN}", name.trim_end_matches(".txt")));
    std::fs::rename(path, &seen)?;
    Ok(seen)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn panic_writes_a_report_and_keeps_twenty() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("crashes");
        std::fs::create_dir(&dir).unwrap();
        let old = SystemTime::now() - Duration::from_secs(3600);
        for n in 0..KEEP {
            let path = dir.join(format!("antigo-{n:02}.txt"));
            std::fs::write(&path, "velho").unwrap();
            let file = std::fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(old + Duration::from_secs(n as u64))
                .unwrap();
        }

        install_in("programa-de-teste", dir.clone());
        let joined = std::thread::spawn(|| panic!("falha de propósito")).join();
        drop(std::panic::take_hook());

        assert!(joined.is_err());
        let all = reports(&dir);
        assert_eq!(all.len(), KEEP, "{all:?}");
        assert!(!dir.join("antigo-00.txt").exists(), "o mais velho sai");
        let newest = all.last().unwrap();
        let name = newest.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("programa-de-teste-"), "{name}");
        let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&dir), 0o700, "a pasta é só do usuário");
        assert_eq!(mode(newest), 0o600, "o relatório é só do usuário");
        let text = std::fs::read_to_string(newest).unwrap();
        assert!(text.starts_with(&format!("programa-de-teste {}", env!("CARGO_PKG_VERSION"))));
        assert!(text.contains("falha de propósito"), "{text}");
        assert!(text.contains("crash.rs"), "o local vai junto: {text}");
        assert!(!is_seen(newest));
        let seen = mark_seen(newest).unwrap();
        assert!(is_seen(&seen) && seen.exists() && !newest.exists());
    }
}
