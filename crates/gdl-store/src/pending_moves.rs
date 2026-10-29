//! Mudanças de lugar de itens do Drive já aplicadas aqui e ainda não confirmadas por ele.
//!
//! Mover um item num Drive lento demora (cada chamada espera a rede), e o kernel segura as duas
//! pastas enquanto a montagem não responde. Por isso o lugar novo vale aqui na hora e o Drive é
//! avisado em segundo plano. Enquanto a linha existe, as respostas do Drive (a consulta
//! periódica, a resposta de outra operação) não desfazem o lugar novo; e como ela é do banco,
//! uma queda do serviço no meio não deixa o banco diferente do Drive sem ninguém saber: a
//! partida seguinte retoma.

use rusqlite::{OptionalExtension, params};

use crate::{Batch, ItemId, Result, Store};

/// O lugar que a pessoa quer para um item e o que o Drive ainda tem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingMove {
    /// Pasta e nome (no Drive) pedidos.
    pub parent: ItemId,
    pub remote_name: String,
    /// Pasta e nome que o Drive tem agora (`None`: sem pasta de cima).
    pub drive_parent: Option<ItemId>,
    pub drive_name: String,
}

fn read(conn: &rusqlite::Connection, id: &ItemId) -> Result<Option<PendingMove>> {
    Ok(conn
        .prepare_cached(
            "SELECT parent, remote_name, drive_parent, drive_name FROM pending_moves \
             WHERE item_id = ?1",
        )?
        .query_row([&id.0], |r| {
            Ok(PendingMove {
                parent: ItemId(r.get(0)?),
                remote_name: r.get(1)?,
                drive_parent: r.get::<_, Option<String>>(2)?.map(ItemId),
                drive_name: r.get(3)?,
            })
        })
        .optional()?)
}

impl Batch<'_> {
    /// Guarda o pedido de `id`. Devolve `true` se não havia pedido (quem chama confirma com o
    /// Drive); com um já em andamento, só troca o lugar pedido e o que já roda o alcança.
    pub fn pend_move(&self, id: &ItemId, want: &PendingMove) -> Result<bool> {
        if read(self.conn, id)?.is_some() {
            self.conn.execute(
                "UPDATE pending_moves SET parent = ?2, remote_name = ?3 WHERE item_id = ?1",
                params![id.0, want.parent.0, want.remote_name],
            )?;
            return Ok(false);
        }
        self.conn.execute(
            "INSERT INTO pending_moves (item_id, parent, remote_name, drive_parent, drive_name) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                id.0,
                want.parent.0,
                want.remote_name,
                want.drive_parent.as_ref().map(|p| &p.0),
                want.drive_name
            ],
        )?;
        Ok(true)
    }

    /// Se `id` tem um pedido sem confirmar (sem montar o pedido: é consultado a cada resposta do
    /// Drive).
    pub fn has_pending_move(&self, id: &ItemId) -> Result<bool> {
        Ok(self
            .conn
            .prepare_cached("SELECT EXISTS(SELECT 1 FROM pending_moves WHERE item_id = ?1)")?
            .query_row([&id.0], |r| r.get(0))?)
    }
}

impl Store {
    pub fn pending_move(&self, id: &ItemId) -> Result<Option<PendingMove>> {
        read(&self.conn(), id)
    }

    /// Os itens com pedido por confirmar (a retomada da partida).
    pub fn pending_moves(&self) -> Result<Vec<ItemId>> {
        Ok(self
            .conn()
            .prepare_cached("SELECT item_id FROM pending_moves ORDER BY item_id")?
            .query_map([], |r| Ok(ItemId(r.get(0)?)))?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// O Drive passou a ter `parent` e `name` para `id`. Se era o pedido, a linha some; se o
    /// pedido mudou no meio-tempo, guarda o que o Drive tem agora e devolve `true` (falta levá-lo
    /// ao pedido novo).
    pub fn settle_move(&self, id: &ItemId, parent: &ItemId, name: &str) -> Result<bool> {
        let conn = self.conn();
        let done = conn.execute(
            "DELETE FROM pending_moves WHERE item_id = ?1 AND parent = ?2 AND remote_name = ?3",
            params![id.0, parent.0, name],
        )?;
        if done > 0 {
            return Ok(false);
        }
        let more = conn.execute(
            "UPDATE pending_moves SET drive_parent = ?2, drive_name = ?3 WHERE item_id = ?1",
            params![id.0, parent.0, name],
        )?;
        Ok(more > 0)
    }

    /// O Drive recusou `refused`: apaga o pedido e devolve o que ele tinha, para o lugar voltar.
    /// `None` se o pedido já é outro (mais novo): esse segue.
    pub fn drop_move(&self, id: &ItemId, refused: &PendingMove) -> Result<Option<PendingMove>> {
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let pending = read(&tx, id)?
            .filter(|p| p.parent == refused.parent && p.remote_name == refused.remote_name);
        if pending.is_some() {
            tx.execute("DELETE FROM pending_moves WHERE item_id = ?1", [&id.0])?;
        }
        tx.commit()?;
        Ok(pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(s: &str) -> ItemId {
        ItemId(s.into())
    }

    fn want(parent: &str, name: &str) -> PendingMove {
        PendingMove {
            parent: id(parent),
            remote_name: name.into(),
            drive_parent: Some(id("velha")),
            drive_name: "a.txt".into(),
        }
    }

    fn store() -> (Store, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        (Store::open(&dir.path().join("t.db")).unwrap(), dir)
    }

    fn pend(store: &Store, item: &str, wanted: &PendingMove) -> bool {
        store.modify(|b| b.pend_move(&id(item), wanted)).unwrap()
    }

    #[test]
    fn a_second_request_only_changes_what_is_wanted() {
        let (store, _dir) = store();
        assert!(pend(&store, "x", &want("p1", "a.txt")));
        assert!(!pend(&store, "x", &want("p2", "b.txt")));

        let kept = store.pending_move(&id("x")).unwrap().unwrap();
        assert_eq!(
            (kept.parent, kept.remote_name.as_str()),
            (id("p2"), "b.txt")
        );
        assert_eq!(
            kept.drive_parent,
            Some(id("velha")),
            "o que o Drive tem não muda"
        );
        assert_eq!(store.pending_moves().unwrap(), [id("x")]);
    }

    #[test]
    fn settling_the_wanted_place_ends_the_request() {
        let (store, _dir) = store();
        pend(&store, "x", &want("p1", "a.txt"));

        assert!(!store.settle_move(&id("x"), &id("p1"), "a.txt").unwrap());

        assert!(store.pending_move(&id("x")).unwrap().is_none());
    }

    #[test]
    fn settling_an_old_request_keeps_the_newer_one() {
        let (store, _dir) = store();
        pend(&store, "x", &want("p1", "a.txt"));
        pend(&store, "x", &want("p2", "a.txt"));

        assert!(
            store.settle_move(&id("x"), &id("p1"), "a.txt").unwrap(),
            "falta o pedido novo"
        );

        let kept = store.pending_move(&id("x")).unwrap().unwrap();
        assert_eq!(kept.parent, id("p2"));
        assert_eq!(kept.drive_parent, Some(id("p1")), "o Drive já está em p1");
    }

    #[test]
    fn a_refusal_never_drops_a_newer_request() {
        let (store, _dir) = store();
        pend(&store, "x", &want("p1", "a.txt"));
        pend(&store, "x", &want("p2", "a.txt"));

        assert!(
            store
                .drop_move(&id("x"), &want("p1", "a.txt"))
                .unwrap()
                .is_none()
        );
        assert!(store.pending_move(&id("x")).unwrap().is_some());
        assert!(
            store
                .drop_move(&id("x"), &want("p2", "a.txt"))
                .unwrap()
                .is_some()
        );
        assert!(store.pending_move(&id("x")).unwrap().is_none());
    }
}
