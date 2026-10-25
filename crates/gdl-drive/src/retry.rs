//! Erros da Drive API e repetição com recuo exponencial.

use std::time::Duration;

use rand::Rng;

use crate::oauth::AuthError;

#[derive(Debug, thiserror::Error)]
pub enum DriveError {
    #[error("sem conexão com o Google Drive")]
    Offline,
    #[error("o item não existe mais no Drive")]
    NotFound,
    #[error("sem permissão para esta ação no Drive")]
    PermissionDenied,
    #[error("o armazenamento da conta está cheio")]
    QuotaExceeded,
    #[error("limite diário de envio do Google Drive atingido")]
    DailyUploadLimit,
    #[error("limite de download deste arquivo atingido; tente mais tarde")]
    DownloadQuotaExceeded,
    #[error("o Google pediu para diminuir o ritmo")]
    RateLimited,
    #[error("o Google não aceitou o acesso da conta")]
    Unauthorized,
    #[error("o Google Drive respondeu {0}")]
    Http(u16),
    #[error("resposta inesperada do Google Drive: {0}")]
    Protocol(String),
    #[error("erro ao ler o arquivo local: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Auth(#[from] AuthError),
}

impl From<reqwest::Error> for DriveError {
    fn from(e: reqwest::Error) -> Self {
        // Resposta ilegível ou pedido mal montado não se resolvem esperando a rede voltar.
        if e.is_decode() || e.is_builder() {
            DriveError::Protocol(e.to_string())
        } else {
            DriveError::Offline
        }
    }
}

impl DriveError {
    /// Falhas passageiras: sem rede, limite de taxa e erro do servidor.
    pub fn is_retryable(&self) -> bool {
        matches!(
            self,
            DriveError::Offline | DriveError::RateLimited | DriveError::Http(500..=599)
        )
    }
}

/// Recuo exponencial com variação aleatória. Padrão: de 1 s a 64 s, no máximo 8 tentativas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Backoff {
    pub base: Duration,
    pub max: Duration,
    pub tries: u32,
}

impl Default for Backoff {
    fn default() -> Backoff {
        Backoff {
            base: Duration::from_secs(1),
            max: Duration::from_secs(64),
            tries: 8,
        }
    }
}

impl Backoff {
    /// Espera antes da tentativa `attempt + 1`: entre metade e o total de `base·2^attempt`, até `max`.
    pub fn delay(&self, attempt: u32) -> Duration {
        let cap = self.base.saturating_mul(1 << attempt.min(16)).min(self.max);
        cap.mul_f64(rand::rng().random_range(0.5..=1.0))
    }
}

/// Repete `op` enquanto a falha for passageira, até `backoff.tries` tentativas.
pub async fn with_backoff<T>(
    backoff: &Backoff,
    mut op: impl AsyncFnMut() -> Result<T, DriveError>,
) -> Result<T, DriveError> {
    let mut attempt = 0;
    loop {
        match op().await {
            Err(e) if e.is_retryable() && attempt + 1 < backoff.tries => {
                tracing::debug!(tentativa = attempt + 1, erro = %e, "repetindo chamada ao Drive");
                tokio::time::sleep(backoff.delay(attempt)).await;
                attempt += 1;
            }
            result => return result,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn delays_grow_and_stop_at_the_maximum() {
        let b = Backoff::default();
        for (attempt, cap) in [(0, 1), (1, 2), (3, 8), (6, 64), (10, 64)] {
            let d = b.delay(attempt);
            let cap = Duration::from_secs(cap);
            assert!(d >= cap / 2 && d <= cap, "tentativa {attempt}: {d:?}");
        }
    }

    #[tokio::test]
    async fn only_passing_failures_are_retried() {
        let fast = Backoff {
            base: Duration::from_millis(1),
            max: Duration::from_millis(1),
            tries: 5,
        };
        let mut calls = 0;
        let r: Result<(), _> = with_backoff(&fast, async || {
            calls += 1;
            Err(DriveError::NotFound)
        })
        .await;
        assert!(matches!(r, Err(DriveError::NotFound)));
        assert_eq!(calls, 1);

        let mut calls = 0;
        let r = with_backoff(&fast, async || {
            calls += 1;
            if calls < 3 {
                Err(DriveError::RateLimited)
            } else {
                Ok(calls)
            }
        })
        .await;
        assert_eq!(r.unwrap(), 3);
    }
}
