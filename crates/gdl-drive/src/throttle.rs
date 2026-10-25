//! Limite de banda (`BandwidthRxKBPS`, `BandwidthTxKBPS`): um balde de fichas por direção, que
//! o cliente esvazia ao ler o conteúdo baixado e ao mandar o conteúdo enviado.

use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::Notify;
use tokio::time::Instant;

/// Balde de fichas de uma direção. Cabe um segundo de tráfego; quem pede mais do que há fica
/// devendo e espera a dívida ser paga. Trocar o limite vale na hora, inclusive para quem já está
/// esperando.
pub struct Throttle {
    state: Mutex<State>,
    changed: Notify,
}

struct State {
    /// Bytes por segundo; `None` é sem limite.
    rate: Option<u64>,
    /// Fichas disponíveis (negativo: dívida).
    tokens: f64,
    last: Instant,
    /// Bytes que já passaram com limite.
    taken: u64,
}

impl State {
    fn refill(&mut self) {
        let now = Instant::now();
        if let Some(rate) = self.rate {
            let earned = now.duration_since(self.last).as_secs_f64() * rate as f64;
            self.tokens = (self.tokens + earned).min(rate as f64);
        }
        self.last = now;
    }
}

impl Default for Throttle {
    fn default() -> Throttle {
        Throttle {
            state: Mutex::new(State {
                rate: None,
                tokens: 0.0,
                last: Instant::now(),
                taken: 0,
            }),
            changed: Notify::new(),
        }
    }
}

impl Throttle {
    fn state(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Limite em KB/s (`None` ou 0: sem limite).
    pub fn set_rate(&self, kbps: Option<u64>) {
        let rate = kbps.filter(|&r| r > 0).map(|r| r * 1024);
        let mut s = self.state();
        if s.rate == rate {
            return;
        }
        s.refill();
        s.tokens = match (s.rate, rate) {
            // Começa com o balde cheio.
            (None, Some(new)) => new as f64,
            // A dívida continua; a sobra não passa do balde novo.
            (Some(_), Some(new)) => s.tokens.min(new as f64),
            (_, None) => 0.0,
        };
        s.rate = rate;
        drop(s);
        self.changed.notify_waiters();
    }

    pub fn is_limited(&self) -> bool {
        self.state().rate.is_some()
    }

    /// Bytes que já passaram pelo balde com limite.
    pub fn taken(&self) -> u64 {
        self.state().taken
    }

    /// Espera até poder passar `bytes`.
    pub async fn take(&self, bytes: usize) {
        let mut charge = bytes;
        loop {
            // Inscrito antes de olhar o estado: uma troca de limite no meio não se perde.
            let changed = self.changed.notified();
            let wait = {
                let mut s = self.state();
                let Some(rate) = s.rate else {
                    return;
                };
                s.refill();
                s.tokens -= charge as f64;
                s.taken += charge as u64;
                charge = 0;
                if s.tokens >= 0.0 {
                    return;
                }
                Duration::from_secs_f64(-s.tokens / rate as f64)
            };
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = changed => {}
            }
        }
    }
}

/// Os dois baldes do app, compartilhados por todos os clientes da conta.
#[derive(Default)]
pub struct Bandwidth {
    /// Download.
    pub rx: Throttle,
    /// Upload.
    pub tx: Throttle,
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;

    const KB: usize = 1024;

    #[tokio::test(start_paused = true)]
    async fn throttle_limits_rate() {
        let throttle = Throttle::default();
        throttle.set_rate(Some(10));
        let start = Instant::now();

        throttle.take(30 * KB).await;

        // 10 KB do balde cheio e 20 KB a 10 KB/s.
        let elapsed = start.elapsed().as_secs_f64();
        assert!((1.9..=2.1).contains(&elapsed), "{elapsed} s");
        assert_eq!(throttle.taken(), 30 * KB as u64);
    }

    #[tokio::test(start_paused = true)]
    async fn unlimited_never_waits() {
        let throttle = Throttle::default();
        let start = Instant::now();
        throttle.take(100 * 1024 * KB).await;
        assert_eq!(start.elapsed(), Duration::ZERO);
        assert_eq!(throttle.taken(), 0);
    }

    #[tokio::test(start_paused = true)]
    async fn removing_the_limit_releases_who_is_waiting() {
        let throttle = Arc::new(Throttle::default());
        throttle.set_rate(Some(1));
        let start = Instant::now();
        let waiting = tokio::spawn({
            let throttle = throttle.clone();
            async move { throttle.take(1000 * KB).await }
        });
        tokio::time::sleep(Duration::from_secs(1)).await;

        throttle.set_rate(None);
        waiting.await.unwrap();

        assert!(
            start.elapsed() < Duration::from_secs(2),
            "{:?}",
            start.elapsed()
        );
    }
}
