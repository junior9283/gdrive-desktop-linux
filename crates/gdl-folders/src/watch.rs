//! O par em funcionamento: compara quando a pasta do computador sossega depois de mudar
//! (inotify), quando o motor da conta avisa que itens do par mudaram no Drive e de hora em hora.

use std::path::Path;
use std::time::Duration;

use gdl_engine::events::EngineEvent;
use gdl_store::ItemId;
use notify::{RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{Instant, sleep_until};
use tokio_util::sync::CancellationToken;

use crate::sync::FolderSync;

/// Os tempos do laço (os testes encurtam a espera).
#[derive(Debug, Clone, Copy)]
pub struct Timing {
    /// Sem mudança por este tempo, a pasta é comparada: arquivo ainda sendo gravado não sobe
    /// pela metade.
    pub quiet: Duration,
    /// A comparação completa periódica.
    pub every: Duration,
    /// Sem observadores do inotify (limite do sistema), a comparação periódica fica mais curta.
    pub without_watch: Duration,
}

impl Default for Timing {
    fn default() -> Timing {
        Timing {
            quiet: Duration::from_secs(2),
            every: Duration::from_secs(3600),
            without_watch: Duration::from_secs(300),
        }
    }
}

impl FolderSync {
    /// Roda o par até `shutdown`. A partida também espera a pasta sossegar.
    pub async fn run(
        &self,
        mut events: broadcast::Receiver<EngineEvent>,
        timing: Timing,
        shutdown: CancellationToken,
    ) {
        // Um aviso pendente basta: uma rajada de mudanças só adia a comparação.
        let (tx, mut changed) = mpsc::channel(1);
        // Fica vivo até o fim do laço; cada aviso (inclusive o de fila estourada) pede a
        // comparação completa.
        let watcher = watch(&self.pair().local, tx);
        let every = match &watcher {
            Ok(_) => timing.every,
            Err(e) => {
                tracing::warn!(
                    pair = self.pair().id,
                    "sem observar a pasta ({e}): comparando a cada {:?}",
                    timing.without_watch
                );
                timing.without_watch
            }
        };
        let mut ticks = tokio::time::interval_at(Instant::now() + every, every);
        let mut due = Some(Instant::now() + timing.quiet);
        loop {
            let wait = async {
                match due {
                    Some(at) => sleep_until(at).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                () = shutdown.cancelled() => break,
                Some(()) = changed.recv() => due = Some(Instant::now() + timing.quiet),
                event = events.recv() => match event {
                    Ok(EngineEvent::ItemsChanged { ids }) if !self.concerns(&ids) => {}
                    Ok(EngineEvent::ItemsChanged { .. }) | Err(broadcast::error::RecvError::Lagged(_)) => {
                        due.get_or_insert_with(Instant::now);
                    }
                    Ok(_) => {}
                    // A conta saiu: o par para com ela.
                    Err(broadcast::error::RecvError::Closed) => break,
                },
                _ = ticks.tick() => {
                    due.get_or_insert_with(Instant::now);
                }
                () = wait => {
                    due = None;
                    if let Err(e) = self.reconcile().await {
                        tracing::warn!(pair = self.pair().id, "falha ao comparar a pasta: {e}");
                    }
                }
            }
        }
    }

    /// Algum dos itens é a pasta do par, está nela (pelos pais no banco) ou estava (pela base).
    fn concerns(&self, ids: &[ItemId]) -> bool {
        let (store, pair) = (self.store(), self.pair());
        let under_pair = |id: &ItemId| {
            let mut current = Some(id.clone());
            // A profundidade é limitada: um ciclo no banco não trava o laço.
            for _ in 0..256 {
                match current {
                    Some(c) if c == pair.remote_id => return true,
                    Some(c) => current = store.get(&c).ok().flatten().and_then(|i| i.parent),
                    None => return false,
                }
            }
            false
        };
        ids.iter()
            .any(|id| under_pair(id) || store.synced_has(pair.id, id).unwrap_or(true))
    }
}

fn watch(root: &Path, tx: mpsc::Sender<()>) -> notify::Result<RecommendedWatcher> {
    // Ler um arquivo (o usuário, ou a própria comparação) não é mudança: sem isso, uma leitura
    // frequente adiaria a comparação para sempre.
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        if event.is_err() || event.is_ok_and(|e| !e.kind.is_access()) {
            let _ = tx.try_send(());
        }
    })?;
    watcher.watch(root, RecursiveMode::Recursive)?;
    Ok(watcher)
}
