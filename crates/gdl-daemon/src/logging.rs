//! O log do serviço: no stderr e em `<state_dir>/logs/gdrive-desktop-linuxd.log`, girando por
//! tamanho, com o registro detalhado (`debug` nos crates do app) ligado e desligado sem reiniciar.
//! Ligado, ele vale por um dia e volta sozinho ao normal, mesmo depois de reiniciar.

use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use tracing_subscriber::filter::{LevelFilter, Targets};
use tracing_subscriber::reload;

/// O nome do arquivo de log dentro de `<state_dir>/logs`.
pub const LOG_FILE: &str = "gdrive-desktop-linuxd.log";
/// Quanto o registro detalhado fica ligado.
pub const VERBOSE_FOR_MS: i64 = 24 * 60 * 60 * 1000;
const MAX_BYTES: u64 = 10 * 1024 * 1024;
/// O arquivo atual e mais 4 girados.
const KEEP: usize = 5;

/// O arquivo que guarda até quando o registro detalhado vale (milissegundos, em texto).
fn verbose_file(state_dir: &Path) -> PathBuf {
    state_dir.join("verbose-until")
}

pub fn logs_dir(state_dir: &Path) -> PathBuf {
    state_dir.join("logs")
}

/// `debug` nos crates do app (`gdl_*`), `info` no resto; ou `info` em tudo.
fn filter(verbose: bool) -> Targets {
    let app = if verbose {
        LevelFilter::DEBUG
    } else {
        LevelFilter::INFO
    };
    Targets::new()
        .with_default(LevelFilter::INFO)
        .with_target("gdl_", app)
        .with_target("gdrive_desktop_linux", app)
}

/// A escolha guardada, se ainda vale em `now_ms`.
fn saved_until(state_dir: &Path, now_ms: i64) -> Option<i64> {
    let text = std::fs::read_to_string(verbose_file(state_dir)).ok()?;
    text.trim().parse().ok().filter(|until| *until > now_ms)
}

/// Liga e desliga o registro detalhado; ligado, desliga sozinho no fim do prazo.
#[derive(Clone)]
pub struct LogHandle(Arc<Verbose>);

struct Verbose {
    reload: reload::Handle<Targets, tracing_subscriber::Registry>,
    state_dir: PathBuf,
    /// Até quando (ms) o registro detalhado vale; 0 desligado. O arquivo guarda o mesmo para a
    /// próxima partida.
    until: AtomicI64,
    /// O temporizador que desliga no fim do prazo (um só: ligar de novo troca o anterior).
    expiry: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl LogHandle {
    /// Até quando o registro detalhado vale (`None` desligado), visto em `now_ms`.
    pub fn verbose_until(&self, now_ms: i64) -> Option<i64> {
        let until = self.0.until.load(Ordering::SeqCst);
        (until > now_ms).then_some(until)
    }

    /// Liga (por `VERBOSE_FOR_MS` a partir de `now_ms`) ou desliga, e guarda a escolha.
    pub fn set_verbose(&self, on: bool, now_ms: i64) -> io::Result<()> {
        self.apply(on.then_some(now_ms + VERBOSE_FOR_MS))?;
        self.schedule_expiry(now_ms);
        Ok(())
    }

    fn apply(&self, until: Option<i64>) -> io::Result<()> {
        let file = verbose_file(&self.0.state_dir);
        match until {
            Some(until) => gdl_common::fs::write_atomic(&file, until.to_string().as_bytes())?,
            None => gdl_common::fs::remove_file_if_exists(&file)?,
        }
        self.0.until.store(until.unwrap_or(0), Ordering::SeqCst);
        let reloaded = self.0.reload.reload(filter(until.is_some()));
        reloaded.map_err(io::Error::other)
    }

    /// Desliga no fim do prazo. Fora de um runtime do tokio (testes síncronos), vale só o prazo
    /// guardado, aplicado na próxima partida.
    fn schedule_expiry(&self, now_ms: i64) {
        let mut expiry = crate::lock(&self.0.expiry);
        if let Some(old) = expiry.take() {
            old.abort();
        }
        let (Some(until), Ok(runtime)) = (
            self.verbose_until(now_ms),
            tokio::runtime::Handle::try_current(),
        ) else {
            return;
        };
        let me = Arc::downgrade(&self.0);
        *expiry = Some(runtime.spawn(async move {
            let wait = until.saturating_sub(gdl_common::time::now_ms()).max(0) as u64;
            tokio::time::sleep(std::time::Duration::from_millis(wait)).await;
            if let Some(me) = me.upgrade() {
                let _ = LogHandle(me).apply(None);
            }
        }));
    }
}

/// O arquivo de log que gira ao passar de `max_bytes`, guardando `keep` arquivos ao todo.
pub struct RotatingFile {
    path: PathBuf,
    max_bytes: u64,
    keep: usize,
    file: Option<std::fs::File>,
    written: u64,
}

impl RotatingFile {
    pub fn new(path: PathBuf, max_bytes: u64, keep: usize) -> RotatingFile {
        RotatingFile {
            path,
            max_bytes,
            keep,
            file: None,
            written: 0,
        }
    }

    /// `a.log.N` com N de 1 a `keep - 1`.
    fn rotated(&self, n: usize) -> PathBuf {
        let mut name = self.path.clone().into_os_string();
        name.push(format!(".{n}"));
        name.into()
    }

    /// O arquivo aberto para acrescentar, criado (com a pasta) se preciso.
    fn open(&mut self) -> io::Result<&mut std::fs::File> {
        if self.file.is_none() {
            if let Some(dir) = self.path.parent() {
                gdl_common::paths::create_private_dir(dir)?;
            }
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&self.path)?;
            self.written = file.metadata()?.len();
            self.file = Some(file);
        }
        Ok(self.file.as_mut().expect("aberto acima"))
    }

    /// `a.log` vira `a.log.1`, que vira `a.log.2`…; o mais velho sai.
    fn rotate(&mut self) -> io::Result<()> {
        self.file = None;
        for n in (1..self.keep).rev() {
            let from = if n == 1 {
                self.path.clone()
            } else {
                self.rotated(n - 1)
            };
            match std::fs::rename(&from, self.rotated(n)) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
                _ => {}
            }
        }
        Ok(())
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.open()?;
        if self.written > 0 && self.written + buf.len() as u64 > self.max_bytes {
            self.rotate()?;
        }
        let n = self.open()?.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.as_mut().map_or(Ok(()), |f| f.flush())
    }
}

/// O assinante do serviço (stderr e arquivo) e o controle do registro detalhado, já com a escolha
/// guardada aplicada em `now_ms`.
pub fn subscriber(
    state_dir: &Path,
    now_ms: i64,
) -> (impl tracing::Subscriber + Send + Sync + 'static, LogHandle) {
    use tracing_subscriber::layer::SubscriberExt;
    let verbose = saved_until(state_dir, now_ms).is_some();

    let (filter, reload) = reload::Layer::new(filter(verbose));
    let file = Arc::new(Mutex::new(RotatingFile::new(
        logs_dir(state_dir).join(LOG_FILE),
        MAX_BYTES,
        KEEP,
    )));
    let to_file = move || SharedFile(file.clone());
    let subscriber = tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().with_writer(io::stderr))
        .with(
            tracing_subscriber::fmt::layer()
                .with_ansi(false)
                .with_writer(to_file),
        );
    let handle = LogHandle(Arc::new(Verbose {
        reload,
        state_dir: state_dir.to_path_buf(),
        until: AtomicI64::new(saved_until(state_dir, now_ms).unwrap_or(0)),
        expiry: Mutex::default(),
    }));
    handle.schedule_expiry(now_ms);
    (subscriber, handle)
}

/// Um escritor do arquivo compartilhado entre as linhas do log.
struct SharedFile(Arc<Mutex<RotatingFile>>);

impl Write for SharedFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        crate::lock(&self.0).write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        crate::lock(&self.0).flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_000_000_000_000;

    fn log_text(state_dir: &Path) -> String {
        std::fs::read_to_string(logs_dir(state_dir).join(LOG_FILE)).unwrap_or_default()
    }

    #[test]
    fn verbose_turns_debug_on_and_off() {
        let tmp = tempfile::tempdir().unwrap();
        let (sub, log) = subscriber(tmp.path(), NOW);
        tracing::subscriber::with_default(sub, || {
            tracing::debug!(target: "gdl_engine::teste", "antes");
            tracing::info!(target: "gdl_engine::teste", "informação");
            log.set_verbose(true, NOW).unwrap();
            tracing::debug!(target: "gdl_engine::teste", "detalhado");
            tracing::debug!(target: "hyper::conn", "de fora");
            log.set_verbose(false, NOW).unwrap();
            tracing::debug!(target: "gdl_engine::teste", "depois");
        });

        let text = log_text(tmp.path());
        assert!(text.contains("informação"), "{text}");
        assert!(text.contains("detalhado"), "{text}");
        for absent in ["antes", "de fora", "depois"] {
            assert!(!text.contains(absent), "{absent}: {text}");
        }
    }

    #[test]
    fn verbose_expires_after_a_day() {
        let tmp = tempfile::tempdir().unwrap();
        let (_sub, log) = subscriber(tmp.path(), NOW);
        log.set_verbose(true, NOW).unwrap();
        assert_eq!(log.verbose_until(NOW + 1), Some(NOW + VERBOSE_FOR_MS));

        // Na partida seguinte, ainda no prazo: continua ligado.
        let (sub, again) = subscriber(tmp.path(), NOW + 1000);
        assert_eq!(again.verbose_until(NOW + 1000), Some(NOW + VERBOSE_FOR_MS));
        tracing::subscriber::with_default(sub, || tracing::debug!(target: "gdl_x", "ainda"));
        assert!(log_text(tmp.path()).contains("ainda"));

        // Passado um dia: desligado, e o filtro também.
        let late = NOW + VERBOSE_FOR_MS + 1;
        let (sub, later) = subscriber(tmp.path(), late);
        assert_eq!(later.verbose_until(late), None);
        tracing::subscriber::with_default(sub, || tracing::debug!(target: "gdl_x", "vencido"));
        assert!(!log_text(tmp.path()).contains("vencido"));
    }

    /// Com o serviço rodando, o prazo vencido desliga o registro sem esperar a próxima partida.
    #[tokio::test]
    async fn verbose_turns_itself_off_when_the_day_ends() {
        let tmp = tempfile::tempdir().unwrap();
        let (_sub, log) = subscriber(tmp.path(), NOW);
        let now = gdl_common::time::now_ms();
        // Ligado "quase um dia atrás": faltam 50 ms.
        log.set_verbose(true, now - VERBOSE_FOR_MS + 50).unwrap();
        assert!(log.verbose_until(now).is_some());

        tokio::time::sleep(std::time::Duration::from_millis(300)).await;

        assert_eq!(log.verbose_until(gdl_common::time::now_ms()), None);
        assert!(
            !verbose_file(tmp.path()).exists(),
            "a escolha guardada sai junto"
        );
    }

    #[test]
    fn log_rotates_by_size() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("a.log");
        let mut file = RotatingFile::new(path.clone(), 100, 3);
        for n in 0..10 {
            file.write_all(format!("{n:0>59}\n").as_bytes()).unwrap();
        }
        file.flush().unwrap();

        let mut names: Vec<String> = std::fs::read_dir(tmp.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["a.log", "a.log.1", "a.log.2"]);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            format!("{:0>59}\n", 9)
        );
        assert_eq!(
            std::fs::read_to_string(tmp.path().join("a.log.1")).unwrap(),
            format!("{:0>59}\n", 8)
        );
    }
}
