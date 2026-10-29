//! Mover um item do Drive: o lugar novo vale aqui na hora e o Drive é avisado em segundo plano.
//!
//! Cada mudança de lugar espera a rede (~1 s), e o kernel segura as duas pastas enquanto a
//! montagem não responde: um lote de `mv` deixava as pastas sem responder por vários segundos, e
//! o Dolphin as via vazias ou ilegíveis (o cadeado). Ver `gdl_store::PendingMove`: o pedido fica
//! no banco até o Drive confirmar, nenhuma resposta dele o desfaz antes disso, e a partida
//! seguinte retoma o que a queda deixou no meio.

use std::sync::Arc;
use std::time::Duration;

use gdl_drive::client::{Change, DriveClient, DriveError};
use gdl_store::{Item, ItemId, PendingMove, Store};
use tokio::sync::{Semaphore, broadcast};
use tokio_util::sync::CancellationToken;

use crate::changes::apply_known;
use crate::engine::Engine;
use crate::events::{DeniedAction, EngineEvent};
use crate::ops::{never_uploaded, notify_denied};
use crate::uploads::is_local_id;
use crate::{EngineError, Result};

/// De quanto em quanto tempo uma confirmação que esbarrou num erro passageiro tenta de novo.
const PASSING_RETRY: Duration = if cfg!(test) {
    Duration::from_millis(100)
} else {
    Duration::from_secs(5)
};

/// Quantas confirmações falam com o Drive ao mesmo tempo.
pub(crate) const MOVES_AT_ONCE: usize = 4;

impl Engine {
    /// Vale para o item que já está no Drive e que a conta pode editar (o resto o Drive recusa na
    /// hora, e quem pediu fica sabendo), indo para uma pasta que também está, com o Drive no ar
    /// e algo a mudar.
    pub(crate) fn moves_in_background(
        &self,
        item: &Item,
        parent: &ItemId,
        remote_name: &str,
    ) -> bool {
        !never_uploaded(item)
            && item.can_edit
            && !is_local_id(parent)
            && !self.poller.is_offline()
            && (item.parent.as_ref() != Some(parent) || item.remote_name != remote_name)
    }

    /// Aplica o lugar novo aqui e confirma com o Drive em segundo plano.
    pub(crate) fn move_in_background(
        &self,
        item: Item,
        parent: &ItemId,
        remote_name: String,
    ) -> Result<Item> {
        let id = item.id.clone();
        let old_parent = item.parent.clone();
        let started = self.store.modify(|batch| -> Result<bool> {
            let current = batch.get(&id)?.ok_or(EngineError::NotFound)?;
            let want = PendingMove {
                parent: parent.clone(),
                remote_name: remote_name.clone(),
                drive_parent: current.parent.clone(),
                drive_name: current.remote_name.clone(),
            };
            let started = batch.pend_move(&id, &want)?;
            batch.put(&Item {
                parent: Some(parent.clone()),
                remote_name: remote_name.clone(),
                ..current
            })?;
            Ok(started)
        })?;
        self.store
            .recompute_local_names(old_parent.iter().chain([parent]))?;
        crate::pins::reconcile(&self.store, &id)?;
        if started {
            self.confirm_in_background(id.clone());
        }
        self.item(&id)
    }

    /// Retoma os pedidos que a partida anterior deixou sem confirmar.
    pub(crate) fn resume_pending_moves(&self) {
        for id in self.store.pending_moves().unwrap_or_default() {
            self.confirm_in_background(id);
        }
    }

    fn confirm_in_background(&self, id: ItemId) {
        tokio::spawn(confirm(
            self.store.clone(),
            self.client.clone(),
            self.events.clone(),
            self.moves_gate.clone(),
            self.moves_stop.clone(),
            id,
        ));
    }
}

/// Erro que passa sozinho (o cliente já tentou de novo e desistiu): a confirmação espera e tenta
/// outra vez, em vez de desfazer o que a pessoa já viu dar certo.
fn passing(e: &DriveError) -> bool {
    matches!(
        e,
        DriveError::Offline | DriveError::RateLimited | DriveError::Http(500..=599)
    )
}

/// Leva o Drive ao lugar pedido (e a cada pedido novo que chegar no meio-tempo). Recusado, o
/// lugar volta ao que o Drive tinha.
async fn confirm(
    store: Arc<Store>,
    client: Arc<DriveClient>,
    events: broadcast::Sender<EngineEvent>,
    gate: Arc<Semaphore>,
    stop: CancellationToken,
    id: ItemId,
) {
    loop {
        let Ok(Some(want)) = store.pending_move(&id) else {
            return;
        };
        let moves = want.drive_parent.as_ref() != Some(&want.parent);
        let renames = want.remote_name != want.drive_name;
        if moves || renames {
            // Poucas chamadas ao Drive de uma vez: um lote grande não vira rajada.
            let Ok(_turn) = gate.acquire().await else {
                return;
            };
            let call = client.rename_move(
                &id.0,
                renames.then_some(want.remote_name.as_str()),
                moves.then_some(want.parent.0.as_str()),
                want.drive_parent
                    .as_ref()
                    .filter(|_| moves)
                    .map(|p| p.0.as_str()),
            );
            let sent = tokio::select! {
                () = stop.cancelled() => return,
                sent = call => sent,
            };
            match sent {
                // O pedido ainda de pé mantém o lugar daqui; isto só traz a versão nova.
                Ok(remote) => {
                    let _ = apply_known(&store, vec![Change::Upsert(Box::new(remote))]);
                }
                Err(e) if passing(&e) => {
                    drop(_turn);
                    tokio::select! {
                        () = stop.cancelled() => return,
                        () = tokio::time::sleep(PASSING_RETRY) => {}
                    }
                    continue;
                }
                Err(e) => {
                    tracing::warn!(item = id.0, erro = %e, "o Drive recusou a mudança de lugar");
                    if let Ok(Some(refused)) = store.drop_move(&id, &want) {
                        refuse(&store, &events, &id, &refused, &e);
                        return;
                    }
                    // O pedido já era outro: segue com ele.
                    continue;
                }
            }
        }
        if !store
            .settle_move(&id, &want.parent, &want.remote_name)
            .unwrap_or(false)
        {
            return;
        }
    }
}

/// O Drive não aceitou: o item volta ao lugar que ele tem, e a pessoa é avisada.
fn refuse(
    store: &Store,
    events: &broadcast::Sender<EngineEvent>,
    id: &ItemId,
    refused: &PendingMove,
    e: &DriveError,
) {
    let back = store.modify(|batch| -> Result<Option<String>> {
        let Some(item) = batch.get(id)? else {
            return Ok(None);
        };
        let name = item.local_name.clone();
        batch.put(&Item {
            parent: refused.drive_parent.clone(),
            remote_name: refused.drive_name.clone(),
            ..item
        })?;
        Ok(Some(name))
    });
    let _ = store.recompute_local_names(refused.drive_parent.iter().chain([&refused.parent]));
    let _ = crate::pins::reconcile(store, id);
    let Ok(Some(name)) = back else {
        return;
    };
    if !notify_denied(events, DeniedAction::Move, &name, e) {
        let _ = events.send(EngineEvent::MoveFailed { name });
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use gdl_fake_drive::{FakeDrive, Fault};

    use super::*;
    use crate::testutil::{indexed_engine, wait_until};

    /// Uma pasta de origem com `a.txt` e uma pasta de destino, num Drive que demora 1,5 s para
    /// cada alteração de item.
    async fn slow_drive() -> (FakeDrive, Engine, tempfile::TempDir, ItemId, ItemId, String) {
        let fake = FakeDrive::start().await;
        let root = fake.root_id();
        let from = fake.add_folder(&root, "origem");
        let to = fake.add_folder(&root, "destino");
        let file = fake.add_file(&from, "a.txt", b"conteudo".to_vec());
        let (engine, tmp) = indexed_engine(&fake).await;
        fake.inject(Fault::DelayUpdates(Duration::from_millis(1500)));
        (fake, engine, tmp, ItemId(file), ItemId(to), from)
    }

    fn parents(fake: &FakeDrive, id: &ItemId) -> Vec<String> {
        fake.meta(&id.0)["parents"]
            .as_array()
            .map(|p| {
                p.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Mover não espera o Drive: volta na hora, o item já está no lugar novo, e o Drive o
    /// recebe depois.
    #[tokio::test]
    async fn move_returns_before_the_drive_answers() {
        let (fake, engine, _tmp, file, to, _from) = slow_drive().await;

        let started = Instant::now();
        let moved = engine.move_item(&file, &to, "a.txt").await.unwrap();

        assert!(
            started.elapsed() < Duration::from_millis(700),
            "esperou o Drive"
        );
        assert_eq!(moved.parent.as_ref(), Some(&to));
        assert!(engine.store().pending_move(&file).unwrap().is_some());
        wait_until("o Drive ter o lugar novo", || {
            parents(&fake, &file) == [to.0.clone()]
        })
        .await;
        wait_until("o pedido confirmado", || {
            engine.store().pending_move(&file).unwrap().is_none()
        })
        .await;
        assert_eq!(engine.item(&file).unwrap().parent.as_ref(), Some(&to));
    }

    /// A consulta periódica, que ainda vê o lugar de antes no Drive, não desfaz o pedido.
    #[tokio::test]
    async fn poll_does_not_undo_a_pending_move() {
        let (fake, engine, _tmp, file, to, from) = slow_drive().await;
        engine.move_item(&file, &to, "a.txt").await.unwrap();
        // Uma mudança qualquer no Drive faz o item voltar na lista de mudanças, com o lugar velho.
        fake.modify_remote(&file.0, b"novo conteudo".to_vec());
        assert_eq!(
            parents(&fake, &file),
            std::slice::from_ref(&from),
            "o Drive ainda não moveu"
        );

        engine.poller.poll().await;

        let item = engine.item(&file).unwrap();
        assert_eq!(
            item.parent.as_ref(),
            Some(&to),
            "a resposta velha desfez o lugar"
        );
        wait_until("o Drive ter o lugar novo", || {
            parents(&fake, &file) == [to.0.clone()]
        })
        .await;
    }

    /// Dois pedidos seguidos para o mesmo item: o Drive termina no último.
    #[tokio::test]
    async fn a_second_move_while_pending_wins() {
        let (fake, engine, _tmp, file, to, from) = slow_drive().await;
        engine.move_item(&file, &to, "a.txt").await.unwrap();
        engine
            .move_item(&file, &ItemId(from.clone()), "b.txt")
            .await
            .unwrap();

        wait_until("o pedido confirmado", || {
            engine.store().pending_move(&file).unwrap().is_none()
        })
        .await;
        assert_eq!(parents(&fake, &file), [from]);
        assert_eq!(fake.meta(&file.0)["name"], "b.txt");
    }

    /// Um erro passageiro do Drive (aqui, 503 além das tentativas do cliente) não desfaz o que a
    /// pessoa já viu dar certo: a confirmação espera e tenta de novo.
    #[tokio::test]
    async fn a_passing_error_is_retried_not_undone() {
        let (fake, engine, _tmp, file, to, _from) = slow_drive().await;
        fake.clear_faults();
        let mut events = engine.subscribe();
        fake.inject(Fault::Status {
            code: 503,
            times: 8,
        });
        engine.move_item(&file, &to, "a.txt").await.unwrap();

        tokio::time::sleep(Duration::from_millis(400)).await;
        assert_eq!(engine.item(&file).unwrap().parent.as_ref(), Some(&to));
        wait_until("o Drive ter o lugar novo", || {
            parents(&fake, &file) == [to.0.clone()]
        })
        .await;
        wait_until("o pedido confirmado", || {
            engine.store().pending_move(&file).unwrap().is_none()
        })
        .await;
        while let Ok(event) = events.try_recv() {
            assert!(
                !matches!(event, EngineEvent::MoveFailed { .. }),
                "desfez à toa"
            );
        }
    }

    /// O Drive recusou: o item volta ao lugar de antes e a pessoa é avisada.
    #[tokio::test]
    async fn refused_move_goes_back() {
        let (fake, engine, _tmp, file, to, from) = slow_drive().await;
        let mut events = engine.subscribe();
        fake.inject(Fault::Reason {
            reason: "insufficientFilePermissions",
            times: 1,
        });
        engine.move_item(&file, &to, "a.txt").await.unwrap();

        wait_until("o pedido recusado", || {
            engine.store().pending_move(&file).unwrap().is_none()
        })
        .await;
        let item = engine.item(&file).unwrap();
        assert_eq!(item.parent.as_ref(), Some(&ItemId(from)), "não voltou");
        let mut denied = false;
        while let Ok(event) = events.try_recv() {
            denied |= matches!(event, EngineEvent::NoPermission { .. });
        }
        assert!(denied, "sem aviso");
    }

    /// Um pedido que a partida anterior deixou no banco é confirmado quando o motor volta.
    #[tokio::test]
    async fn pending_move_resumes_after_a_restart() {
        let (fake, engine, _tmp, file, to, from) = slow_drive().await;
        fake.clear_faults();
        // O que a queda no meio deixaria: o lugar novo aqui, o Drive ainda no velho.
        engine
            .store()
            .modify(|batch| -> crate::Result<()> {
                let item = batch.get(&file)?.unwrap();
                batch.pend_move(
                    &file,
                    &PendingMove {
                        parent: to.clone(),
                        remote_name: "a.txt".into(),
                        drive_parent: Some(ItemId(from.clone())),
                        drive_name: "a.txt".into(),
                    },
                )?;
                batch.put(&Item {
                    parent: Some(to.clone()),
                    ..item
                })?;
                Ok(())
            })
            .unwrap();

        engine.resume_pending_moves();

        wait_until("o Drive ter o lugar novo", || {
            parents(&fake, &file) == [to.0.clone()]
        })
        .await;
        wait_until("o pedido confirmado", || {
            engine.store().pending_move(&file).unwrap().is_none()
        })
        .await;
    }
}
