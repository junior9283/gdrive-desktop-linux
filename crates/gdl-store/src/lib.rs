//! Banco SQLite por conta: metadados do Drive, intervalos em cache, fila de envios.

mod schema;
pub mod types;

use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use gdl_common::names::{self, Sibling};
use rusqlite::{Connection, OptionalExtension, Row, TransactionBehavior, named_params, params};

pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("erro no banco: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("valor inválido no banco: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

/// `SELECT` de itens com o filtro dado.
macro_rules! select_items {
    ($filter:literal) => {
        concat!(
            "SELECT id, inode, parent, remote_name, local_name, kind, mime, target_id, size, md5, \
             head_revision, base_revision, base_md5, modified_ms, created_ms, trashed, can_edit, \
             local_state, pin FROM items ",
            $filter
        )
    };
}

/// Banco de uma conta. Pode ser compartilhado entre threads.
pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Abre (ou cria) o banco em modo WAL com `synchronous=FULL` e aplica as migrações.
    pub fn open(path: &Path) -> Result<Store> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        schema::migrate(&mut conn)?;
        Ok(Store {
            conn: Mutex::new(conn),
        })
    }

    fn conn(&self) -> MutexGuard<'_, Connection> {
        // Um pânico com o lock não deixa o SQLite inconsistente; seguimos com a conexão.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn upsert_item(&self, item: &Item) -> Result<()> {
        self.upsert_items(std::slice::from_ref(item))
    }

    /// Grava vários itens numa transação. Item já conhecido mantém o inode; item novo com
    /// `Inode::UNASSIGNED` recebe o próximo livre, e nenhum inode é reaproveitado.
    pub fn upsert_items(&self, items: &[Item]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut inode_of = tx.prepare_cached(
                "INSERT INTO inodes (inode, item_id) VALUES (?1, ?2) \
                 ON CONFLICT(item_id) DO UPDATE SET item_id = item_id RETURNING inode",
            )?;
            let mut upsert = tx.prepare_cached(
                "INSERT INTO items (id, inode, parent, remote_name, local_name, kind, mime, target_id, \
                 size, md5, head_revision, base_revision, base_md5, modified_ms, created_ms, trashed, \
                 can_edit, local_state, pin) VALUES (:id, :inode, :parent, :remote_name, :local_name, \
                 :kind, :mime, :target_id, :size, :md5, :head_revision, :base_revision, :base_md5, \
                 :modified_ms, :created_ms, :trashed, :can_edit, :local_state, :pin) \
                 ON CONFLICT(id) DO UPDATE SET parent = excluded.parent, \
                 remote_name = excluded.remote_name, local_name = excluded.local_name, \
                 kind = excluded.kind, mime = excluded.mime, target_id = excluded.target_id, \
                 size = excluded.size, md5 = excluded.md5, head_revision = excluded.head_revision, \
                 base_revision = excluded.base_revision, base_md5 = excluded.base_md5, \
                 modified_ms = excluded.modified_ms, created_ms = excluded.created_ms, \
                 trashed = excluded.trashed, can_edit = excluded.can_edit, \
                 local_state = excluded.local_state, pin = excluded.pin",
            )?;
            for item in items {
                let wanted = (item.inode != Inode::UNASSIGNED).then_some(item.inode.0);
                let inode: u64 = inode_of.query_row(params![wanted, item.id.0], |r| r.get(0))?;
                let (kind, mime, target_id) = item.kind.to_columns();
                upsert.execute(named_params! {
                    ":id": item.id.0,
                    ":inode": inode,
                    ":parent": item.parent.as_ref().map(|p| &p.0),
                    ":remote_name": item.remote_name,
                    ":local_name": item.local_name,
                    ":kind": kind,
                    ":mime": mime,
                    ":target_id": target_id,
                    ":size": item.size,
                    ":md5": item.md5,
                    ":head_revision": item.head_revision,
                    ":base_revision": item.base_revision,
                    ":base_md5": item.base_md5,
                    ":modified_ms": item.modified_ms,
                    ":created_ms": item.created_ms,
                    ":trashed": item.trashed,
                    ":can_edit": item.can_edit,
                    ":local_state": item.local_state,
                    ":pin": item.pin,
                })?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    fn query_items(&self, sql: &str, args: impl rusqlite::Params) -> Result<Vec<Item>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(sql)?;
        let rows = stmt.query_and_then(args, item_from_row)?;
        rows.collect()
    }

    fn query_item(&self, sql: &str, args: impl rusqlite::Params) -> Result<Option<Item>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(sql)?;
        stmt.query_and_then(args, item_from_row)?.next().transpose()
    }

    pub fn get(&self, id: &ItemId) -> Result<Option<Item>> {
        self.query_item(select_items!("WHERE id = ?1"), [&id.0])
    }

    pub fn by_inode(&self, ino: Inode) -> Result<Option<Item>> {
        self.query_item(select_items!("WHERE inode = ?1"), [ino.0])
    }

    /// Filhos fora da lixeira, em ordem de nome local.
    pub fn children(&self, parent: &ItemId) -> Result<Vec<Item>> {
        self.query_items(
            select_items!("WHERE parent = ?1 AND trashed = 0 ORDER BY local_name"),
            [&parent.0],
        )
    }

    pub fn trashed(&self) -> Result<Vec<Item>> {
        self.query_items(select_items!("WHERE trashed = 1 ORDER BY local_name"), [])
    }

    pub fn lookup(&self, parent: &ItemId, local_name: &str) -> Result<Option<Item>> {
        self.query_item(
            select_items!("WHERE parent = ?1 AND local_name = ?2 AND trashed = 0"),
            [&parent.0, local_name],
        )
    }

    /// Refaz os nomes locais dos filhos de cada pasta de `parents`, numa transação só: mapeia
    /// para nomes válidos no Linux (com a extensão do ponteiro nos Documentos Google) e numera os
    /// repetidos por ordem de criação.
    pub fn recompute_local_names<'a>(
        &self,
        parents: impl IntoIterator<Item = &'a ItemId>,
    ) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut select = tx.prepare_cached(
                "SELECT id, remote_name, created_ms, kind, mime, target_id FROM items \
                 WHERE parent = ?1 AND trashed = 0",
            )?;
            let mut update = tx.prepare_cached(
                "UPDATE items SET local_name = ?2 WHERE id = ?1 AND local_name <> ?2",
            )?;
            for parent in parents {
                let siblings = select
                    .query_map([&parent.0], |r| {
                        let remote: String = r.get(1)?;
                        let kind =
                            ItemKind::from_columns(r.get_ref(3)?.as_str()?, r.get(4)?, r.get(5)?);
                        let name = match kind.as_ref().and_then(ItemKind::pointer_extension) {
                            Some(ext) => format!("{remote}.{ext}"),
                            None => remote,
                        };
                        Ok(Sibling {
                            id: r.get(0)?,
                            name: names::to_local(&name),
                            created: r.get(2)?,
                        })
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?;
                for (id, local) in names::dedupe(&siblings) {
                    update.execute([id, local])?;
                }
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Marca `[start, end)` como presente no cache, juntando com intervalos que se tocam.
    pub fn ranges_add(&self, id: &ItemId, start: u64, end: u64) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let (mut lo, mut hi) = (start, end);
        {
            let mut absorb = tx.prepare_cached(
                "DELETE FROM ranges WHERE item_id = ?1 AND start <= ?3 AND end >= ?2 RETURNING start, end",
            )?;
            for range in absorb.query_map(params![id.0, start, end], |r| {
                Ok((r.get::<_, u64>(0)?, r.get::<_, u64>(1)?))
            })? {
                let (s, e) = range?;
                lo = lo.min(s);
                hi = hi.max(e);
            }
            tx.prepare_cached("INSERT INTO ranges (item_id, start, end) VALUES (?1, ?2, ?3)")?
                .execute(params![id.0, lo, hi])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Trechos de `[start, end)` que ainda não estão no cache, em ordem.
    pub fn ranges_missing(&self, id: &ItemId, start: u64, end: u64) -> Result<Vec<(u64, u64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT start, end FROM ranges WHERE item_id = ?1 AND end > ?2 AND start < ?3 ORDER BY start",
        )?;
        let mut missing = vec![];
        let mut pos = start;
        for range in stmt.query_map(params![id.0, start, end], |r| {
            Ok((r.get::<_, u64>(0)?, r.get::<_, u64>(1)?))
        })? {
            let (s, e) = range?;
            if s > pos {
                missing.push((pos, s));
            }
            pos = pos.max(e);
        }
        if pos < end {
            missing.push((pos, end));
        }
        Ok(missing)
    }

    pub fn ranges_clear(&self, id: &ItemId) -> Result<()> {
        self.conn()
            .execute("DELETE FROM ranges WHERE item_id = ?1", [&id.0])?;
        Ok(())
    }

    pub fn enqueue(&self, job: NewUploadJob) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO upload_queue (item_id, kind, next_try_ms) VALUES (?1, ?2, ?3)",
            params![job.item.0, job.kind, job.next_try_ms],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// O envio pronto para tentar agora (`next_try_ms <= now_ms`), o mais antigo primeiro.
    pub fn next_job(&self, now_ms: i64) -> Result<Option<UploadJob>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT job_id, item_id, kind, attempts, next_try_ms, session_url FROM upload_queue \
             WHERE next_try_ms <= ?1 ORDER BY next_try_ms, job_id LIMIT 1",
        )?;
        let job = stmt
            .query_row([now_ms], |r| {
                Ok(UploadJob {
                    job_id: r.get(0)?,
                    item: ItemId(r.get(1)?),
                    kind: r.get(2)?,
                    attempts: r.get(3)?,
                    next_try_ms: r.get(4)?,
                    session_url: r.get(5)?,
                })
            })
            .optional()?;
        Ok(job)
    }

    pub fn finish_job(&self, job_id: i64) -> Result<()> {
        self.conn()
            .execute("DELETE FROM upload_queue WHERE job_id = ?1", [job_id])?;
        Ok(())
    }

    pub fn retry_job(&self, job_id: i64, next_try_ms: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE upload_queue SET attempts = attempts + 1, next_try_ms = ?2 WHERE job_id = ?1",
            [job_id, next_try_ms],
        )?;
        Ok(())
    }

    pub fn set_session_url(&self, job_id: i64, url: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE upload_queue SET session_url = ?2 WHERE job_id = ?1",
            params![job_id, url],
        )?;
        Ok(())
    }

    pub fn kv_get(&self, key: &str) -> Result<Option<String>> {
        let value = self
            .conn()
            .query_row("SELECT value FROM kv WHERE key = ?1", [key], |r| r.get(0))
            .optional()?;
        Ok(value)
    }

    pub fn kv_set(&self, key: &str, value: &str) -> Result<()> {
        self.conn().execute(
            "INSERT INTO kv (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            [key, value],
        )?;
        Ok(())
    }

    pub fn touch_access(&self, id: &ItemId, now_ms: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO access (item_id, last_access_ms) VALUES (?1, ?2) \
             ON CONFLICT(item_id) DO UPDATE SET last_access_ms = excluded.last_access_ms",
            params![id.0, now_ms],
        )?;
        Ok(())
    }

    /// Itens com conteúdo em cache que podem ser liberados, do acesso mais antigo ao mais recente,
    /// com os bytes em cache de cada um. Nunca inclui fixados nem itens com edição local.
    pub fn eviction_candidates(&self, limit: usize) -> Result<Vec<(ItemId, u64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT r.item_id, SUM(r.end - r.start) FROM ranges r \
             JOIN items i ON i.id = r.item_id \
             LEFT JOIN access a ON a.item_id = r.item_id \
             WHERE i.pin = ?1 AND i.local_state = ?2 \
             GROUP BY r.item_id ORDER BY COALESCE(a.last_access_ms, 0), r.item_id LIMIT ?3",
        )?;
        let rows = stmt.query_map(params![PinState::None, LocalState::Clean, limit], |r| {
            Ok((ItemId(r.get(0)?), r.get(1)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}

/// Lê as colunas na ordem de `select_items!` (por posição: ler por nome custa uma busca por coluna).
fn item_from_row(r: &Row) -> Result<Item> {
    let kind: String = r.get(5)?;
    let kind = ItemKind::from_columns(&kind, r.get(6)?, r.get(7)?)
        .ok_or_else(|| StoreError::Corrupt(format!("tipo de item: {kind}")))?;
    Ok(Item {
        id: ItemId(r.get(0)?),
        inode: Inode(r.get(1)?),
        parent: r.get::<_, Option<String>>(2)?.map(ItemId),
        remote_name: r.get(3)?,
        local_name: r.get(4)?,
        kind,
        size: r.get(8)?,
        md5: r.get(9)?,
        head_revision: r.get(10)?,
        base_revision: r.get(11)?,
        base_md5: r.get(12)?,
        modified_ms: r.get(13)?,
        created_ms: r.get(14)?,
        trashed: r.get(15)?,
        can_edit: r.get(16)?,
        local_state: r.get(17)?,
        pin: r.get(18)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn id(s: &str) -> ItemId {
        ItemId(s.into())
    }

    fn item(name: &str, parent: Option<&str>) -> Item {
        Item {
            id: id(name),
            inode: Inode::UNASSIGNED,
            parent: parent.map(id),
            remote_name: name.into(),
            local_name: name.into(),
            kind: ItemKind::File,
            size: 0,
            md5: None,
            head_revision: None,
            base_revision: None,
            base_md5: None,
            modified_ms: 0,
            created_ms: 0,
            trashed: false,
            can_edit: true,
            local_state: LocalState::Clean,
            pin: PinState::None,
        }
    }

    #[test]
    fn inodes_are_stable_across_reopen() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("db.sqlite");
        let store = Store::open(&db).unwrap();
        store
            .upsert_item(&Item {
                inode: Inode::ROOT,
                kind: ItemKind::Folder,
                ..item("root", None)
            })
            .unwrap();
        store.upsert_item(&item("a.txt", Some("root"))).unwrap();
        let ino = store.get(&id("a.txt")).unwrap().unwrap().inode;
        assert!(ino.0 > Inode::ROOT.0);
        store
            .upsert_item(&Item {
                size: 5,
                ..item("a.txt", Some("root"))
            })
            .unwrap();
        assert_eq!(store.get(&id("a.txt")).unwrap().unwrap().inode, ino);
        drop(store);

        let store = Store::open(&db).unwrap();
        assert_eq!(store.by_inode(ino).unwrap().unwrap().id, id("a.txt"));
        assert_eq!(store.by_inode(Inode::ROOT).unwrap().unwrap().id, id("root"));
        store.upsert_item(&item("b.txt", Some("root"))).unwrap();
        assert!(store.get(&id("b.txt")).unwrap().unwrap().inode > ino);
    }

    #[test]
    fn item_round_trips_all_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let mut it = Item {
            kind: ItemKind::Shortcut {
                target_id: id("t"),
                target_mime: "application/pdf".into(),
            },
            size: 42,
            md5: Some("m".into()),
            head_revision: Some("r2".into()),
            base_revision: Some("r1".into()),
            base_md5: Some("m1".into()),
            modified_ms: 7,
            created_ms: 3,
            can_edit: false,
            local_state: LocalState::Conflict,
            pin: PinState::Inherited,
            ..item("s", Some("root"))
        };
        store.upsert_item(&it).unwrap();
        let got = store.get(&id("s")).unwrap().unwrap();
        it.inode = got.inode;
        assert_eq!(got, it);
    }

    #[test]
    fn children_exclude_trashed_and_sort_by_local_name() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store.upsert_item(&item("b.txt", Some("root"))).unwrap();
        store.upsert_item(&item("a.txt", Some("root"))).unwrap();
        store
            .upsert_item(&Item {
                trashed: true,
                ..item("z.txt", Some("root"))
            })
            .unwrap();
        store.upsert_item(&item("outro.txt", Some("x"))).unwrap();
        let names: Vec<_> = store
            .children(&id("root"))
            .unwrap()
            .into_iter()
            .map(|i| i.local_name)
            .collect();
        assert_eq!(names, ["a.txt", "b.txt"]);
        let trashed: Vec<_> = store.trashed().unwrap().into_iter().map(|i| i.id).collect();
        assert_eq!(trashed, [id("z.txt")]);
        assert_eq!(
            store.lookup(&id("root"), "a.txt").unwrap().unwrap().id,
            id("a.txt")
        );
        assert!(store.lookup(&id("root"), "z.txt").unwrap().is_none());
    }

    #[test]
    fn recompute_local_names_maps_and_dedupes_siblings() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let mut old = item("1", Some("root"));
        old.remote_name = "x.txt".into();
        old.created_ms = 1;
        let mut new = item("2", Some("root"));
        new.remote_name = "x.txt".into();
        new.created_ms = 2;
        let mut slash = item("3", Some("root"));
        slash.remote_name = "a/b".into();
        store.upsert_items(&[old, new, slash]).unwrap();
        store.recompute_local_names(&[id("root")]).unwrap();
        let name = |s| store.get(&id(s)).unwrap().unwrap().local_name;
        assert_eq!(name("1"), "x.txt");
        assert_eq!(name("2"), "x (1).txt");
        assert_eq!(name("3"), "a\u{2215}b");
    }

    #[test]
    fn google_docs_keep_their_pointer_extension() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let mut doc = item("1", Some("root"));
        doc.remote_name = "Plano".into();
        doc.created_ms = 1;
        doc.kind = ItemKind::GoogleDoc {
            mime: "application/vnd.google-apps.spreadsheet".into(),
        };
        let mut twin = doc.clone();
        twin.id = id("2");
        twin.created_ms = 2;
        store.upsert_items(&[doc, twin]).unwrap();
        store.recompute_local_names(&[id("root")]).unwrap();
        let name = |s| store.get(&id(s)).unwrap().unwrap().local_name;
        assert_eq!(name("1"), "Plano.gsheet");
        assert_eq!(name("2"), "Plano (1).gsheet");
    }

    #[test]
    fn ranges_merge_and_report_gaps() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let a = id("a");
        store.ranges_add(&a, 0, 10).unwrap();
        store.ranges_add(&a, 20, 30).unwrap();
        assert_eq!(
            store.ranges_missing(&a, 0, 40).unwrap(),
            [(10, 20), (30, 40)]
        );
        assert_eq!(store.ranges_missing(&a, 2, 8).unwrap(), []);
        store.ranges_add(&a, 5, 25).unwrap();
        assert_eq!(store.ranges_missing(&a, 0, 40).unwrap(), [(30, 40)]);
        store.ranges_add(&a, 30, 40).unwrap();
        assert_eq!(store.ranges_missing(&a, 0, 40).unwrap(), []);
        store.ranges_clear(&a).unwrap();
        assert_eq!(store.ranges_missing(&a, 0, 40).unwrap(), [(0, 40)]);
    }

    #[test]
    fn queue_survives_reopen_and_respects_next_try() {
        let tmp = tempfile::tempdir().unwrap();
        let db = tmp.path().join("db.sqlite");
        let store = Store::open(&db).unwrap();
        let a = store
            .enqueue(NewUploadJob {
                item: id("a"),
                kind: UploadKind::Update,
                next_try_ms: 0,
            })
            .unwrap();
        let b = store
            .enqueue(NewUploadJob {
                item: id("b"),
                kind: UploadKind::Create,
                next_try_ms: 1000,
            })
            .unwrap();
        store
            .set_session_url(a, Some("https://upload/sessao"))
            .unwrap();
        drop(store);

        let store = Store::open(&db).unwrap();
        let job = store.next_job(500).unwrap().unwrap();
        assert_eq!(
            (job.job_id, job.kind, job.attempts),
            (a, UploadKind::Update, 0)
        );
        assert_eq!(job.session_url.as_deref(), Some("https://upload/sessao"));
        store.retry_job(a, 2000).unwrap();
        assert!(store.next_job(500).unwrap().is_none());
        assert_eq!(store.next_job(1500).unwrap().unwrap().job_id, b);
        store.finish_job(b).unwrap();
        let job = store.next_job(3000).unwrap().unwrap();
        assert_eq!((job.job_id, job.attempts), (a, 1));
    }

    #[test]
    fn kv_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        assert_eq!(store.kv_get("change_token").unwrap(), None);
        store.kv_set("change_token", "42").unwrap();
        store.kv_set("change_token", "43").unwrap();
        assert_eq!(store.kv_get("change_token").unwrap().as_deref(), Some("43"));
    }

    #[test]
    fn eviction_never_returns_pinned_or_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store.upsert_item(&item("a", Some("root"))).unwrap();
        store.upsert_item(&item("e", Some("root"))).unwrap();
        store
            .upsert_item(&Item {
                pin: PinState::Pinned,
                ..item("b", Some("root"))
            })
            .unwrap();
        store
            .upsert_item(&Item {
                pin: PinState::Inherited,
                ..item("i", Some("root"))
            })
            .unwrap();
        store
            .upsert_item(&Item {
                local_state: LocalState::Dirty,
                ..item("c", Some("root"))
            })
            .unwrap();
        store
            .upsert_item(&Item {
                local_state: LocalState::Uploading,
                ..item("u", Some("root"))
            })
            .unwrap();
        store
            .upsert_item(&Item {
                local_state: LocalState::Conflict,
                ..item("k", Some("root"))
            })
            .unwrap();
        store.upsert_item(&item("d", Some("root"))).unwrap();
        for (s, t) in [
            ("a", 200),
            ("e", 100),
            ("b", 1),
            ("i", 1),
            ("c", 1),
            ("u", 1),
            ("k", 1),
        ] {
            store.ranges_add(&id(s), 0, 10).unwrap();
            store.ranges_add(&id(s), 20, 25).unwrap();
            store.touch_access(&id(s), t).unwrap();
        }
        assert_eq!(
            store.eviction_candidates(10).unwrap(),
            [(id("e"), 15), (id("a"), 15)]
        );
        assert_eq!(store.eviction_candidates(1).unwrap(), [(id("e"), 15)]);
    }

    #[test]
    fn ten_thousand_children_list_under_100ms() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let items: Vec<_> = (0..10_000)
            .map(|n| item(&format!("f{n:05}"), Some("root")))
            .collect();
        store.upsert_items(&items).unwrap();
        let start = Instant::now();
        let children = store.children(&id("root")).unwrap();
        let elapsed = start.elapsed();
        assert_eq!(children.len(), 10_000);
        assert!(
            elapsed.as_millis() < 100,
            "listar 10 mil itens levou {elapsed:?}"
        );
    }
}
