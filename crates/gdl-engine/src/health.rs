//! Conferências periódicas da conta: a faixa de uso do armazenamento (90 e 95 %) e os envios
//! parados há mais de 3 dias. Cada uma avisa uma vez; o que já foi avisado fica no `kv`.

use std::collections::HashMap;

use crate::Result;
use crate::engine::Engine;
use crate::events::EngineEvent;

/// Envio na fila há mais que isto está parado.
pub const STUCK_AFTER_MS: i64 = 3 * 24 * 60 * 60 * 1000;
/// A faixa de uso já avisada.
const QUOTA_BAND: &str = "health.quota-band";
/// Cada envio pendente: `id \t visto desde \t já avisado`, uma linha por item.
const PENDING_SINCE: &str = "health.pending-since";

/// A faixa de aviso do uso do armazenamento: 0, 90 ou 95.
fn band(used: u64, total: Option<u64>) -> u8 {
    match total
        .filter(|&t| t > 0)
        .map(|t| used.saturating_mul(100) / t)
    {
        Some(95..) => 95,
        Some(90..) => 90,
        _ => 0,
    }
}

impl Engine {
    /// Avisa quando o uso sobe de faixa (`QuotaAlmostFull`, de novo só depois de baixar) e quando
    /// envios passam de 3 dias na fila (`UploadsStuck`, um aviso por item).
    pub async fn check_health(&self, now: i64) -> Result<()> {
        self.check_stuck(now)?;
        let about = self.client.about().await?;
        let band = band(about.quota_used, about.quota_total);
        let before: u8 = (self.store.kv_get(QUOTA_BAND)?)
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        if band > before {
            let _ = self
                .events
                .send(EngineEvent::QuotaAlmostFull { percent: band });
        }
        if band != before {
            self.store.kv_set(QUOTA_BAND, &band.to_string())?;
        }
        Ok(())
    }

    fn check_stuck(&self, now: i64) -> Result<()> {
        let saved = self.store.kv_get(PENDING_SINCE)?.unwrap_or_default();
        let known: HashMap<&str, (i64, bool)> = saved
            .lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');
                let (id, since, told) = (parts.next()?, parts.next()?, parts.next()?);
                Some((id, (since.parse().ok()?, told == "1")))
            })
            .collect();
        let mut lines = vec![];
        let mut fresh = 0;
        for item in self.store.pending_items()?.iter().filter(|i| !i.trashed) {
            let (since, told) = known
                .get(item.id.0.as_str())
                .copied()
                .unwrap_or((now, false));
            let stuck = now - since >= STUCK_AFTER_MS;
            fresh += usize::from(stuck && !told);
            lines.push(format!(
                "{}\t{since}\t{}",
                item.id.0,
                u8::from(stuck || told)
            ));
        }
        // A fila vem por nome, e empates podem trocar de ordem: sem isto, regravaria à toa.
        lines.sort();
        let lines = lines.join("\n");
        if lines != saved {
            self.store.kv_set(PENDING_SINCE, &lines)?;
        }
        if fresh > 0 {
            let _ = self.events.send(EngineEvent::UploadsStuck { count: fresh });
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use gdl_fake_drive::FakeDrive;
    use gdl_store::ItemId;

    use super::*;
    use crate::testutil::indexed_engine;

    fn drained(events: &mut tokio::sync::broadcast::Receiver<EngineEvent>) -> Vec<EngineEvent> {
        std::iter::from_fn(|| events.try_recv().ok())
            .filter(|e| !matches!(e, EngineEvent::SyncStateChanged(_)))
            .collect()
    }

    #[test]
    fn bands_start_at_ninety_and_ninety_five() {
        assert_eq!(band(89, Some(100)), 0);
        assert_eq!(band(90, Some(100)), 90);
        assert_eq!(band(94, Some(100)), 90);
        assert_eq!(band(95, Some(100)), 95);
        assert_eq!(band(200, Some(100)), 95);
        assert_eq!(band(10, None), 0, "sem limite, nunca enche");
        assert_eq!(band(10, Some(0)), 0);
    }

    #[tokio::test]
    async fn quota_warning_at_90_and_95_once() {
        let fake = FakeDrive::start().await;
        fake.add_file(&fake.root_id(), "a.bin", vec![0; 100]);
        let (engine, _tmp) = indexed_engine(&fake).await;
        let mut events = engine.subscribe();
        let at = |limit| {
            fake.set_quota_limit(limit);
            engine.check_health(0)
        };

        at(1000).await.unwrap();
        assert!(drained(&mut events).is_empty(), "10 % não avisa");
        at(110).await.unwrap();
        at(108).await.unwrap();
        assert_eq!(
            drained(&mut events),
            [EngineEvent::QuotaAlmostFull { percent: 90 }],
            "uma vez na faixa dos 90"
        );
        at(105).await.unwrap();
        at(100).await.unwrap();
        assert_eq!(
            drained(&mut events),
            [EngineEvent::QuotaAlmostFull { percent: 95 }]
        );
        at(110).await.unwrap();
        assert!(drained(&mut events).is_empty(), "descer de faixa não avisa");
        at(1000).await.unwrap();
        at(110).await.unwrap();
        assert_eq!(
            drained(&mut events),
            [EngineEvent::QuotaAlmostFull { percent: 90 }],
            "depois de baixar, subir avisa de novo"
        );
    }

    #[tokio::test]
    async fn stuck_uploads_warn_after_three_days() {
        let fake = FakeDrive::start().await;
        let (engine, _tmp) = indexed_engine(&fake).await;
        let root = ItemId(fake.root_id());
        let mut events = engine.subscribe();
        const DAY: i64 = 24 * 60 * 60 * 1000;
        let t0 = 1_000 * DAY;

        engine.create_file(&root, "a.txt").unwrap();
        engine.check_health(t0).await.unwrap();
        engine.create_file(&root, "b.txt").unwrap();
        engine.check_health(t0 + DAY).await.unwrap();
        engine.check_health(t0 + 3 * DAY - 1).await.unwrap();
        assert!(drained(&mut events).is_empty(), "antes de 3 dias, nada");

        engine.check_health(t0 + 3 * DAY).await.unwrap();
        engine.check_health(t0 + 3 * DAY + 1).await.unwrap();
        assert_eq!(
            drained(&mut events),
            [EngineEvent::UploadsStuck { count: 1 }],
            "a.txt, uma vez"
        );
        engine.check_health(t0 + 4 * DAY).await.unwrap();
        assert_eq!(
            drained(&mut events),
            [EngineEvent::UploadsStuck { count: 1 }],
            "b.txt, uma vez"
        );
        engine.check_health(t0 + 9 * DAY).await.unwrap();
        assert!(drained(&mut events).is_empty());
    }
}
