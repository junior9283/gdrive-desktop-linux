//! Pastas locais ligadas a pastas do Drive (Espelho, pastas do computador e dispositivos) e o
//! último estado sincronizado de cada arquivo delas.

use std::path::{Path, PathBuf};

use rusqlite::{Row, params};

use crate::{Item, ItemId, Result, Store, StoreError, item_from_row};

/// Onde fica guardado o id da pasta de apoio "Computadores".
pub(crate) const COMPUTERS_KEY: &str = "folders.computers";

/// Para que serve o par.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairKind {
    /// O Meu Drive inteiro numa pasta local.
    Mirror,
    /// Uma pasta do computador em "Computadores".
    Folder,
    /// Backup de um dispositivo removível.
    Device,
}

/// Em que sentido as mudanças andam.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    TwoWay,
    /// Só do computador para o Drive; nada do Drive volta nem é apagado no computador.
    UploadOnly,
}

/// Por que um par parou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StopReason {
    /// A pasta local não existe mais (disco desmontado, pasta apagada).
    Missing,
    NoPermission,
    /// A pasta local foi movida ou renomeada.
    Moved,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairState {
    Active,
    Stopped(StopReason),
    /// Uma comparação acharia este tanto de exclusões: espera o usuário confirmar.
    ConfirmDeletes(u64),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pair {
    pub id: i64,
    pub local: PathBuf,
    pub remote_id: ItemId,
    pub kind: PairKind,
    pub direction: Direction,
    pub state: PairState,
}

/// O último estado sincronizado de um caminho do par (caminho relativo com `/`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Synced {
    pub rel_path: String,
    pub item_id: ItemId,
    pub is_dir: bool,
    pub size: u64,
    pub mtime_ns: i64,
    pub inode: u64,
    pub md5: Option<String>,
}

impl Store {
    /// Liga `local` a `remote_id`; recusa se `local` já é um par.
    pub fn add_pair(
        &self,
        local: &Path,
        remote_id: &ItemId,
        kind: PairKind,
        direction: Direction,
    ) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO folder_pairs (local, remote_id, kind, direction, created_ms) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                path_text(local)?,
                remote_id.0,
                kind.as_str(),
                direction.as_str(),
                gdl_common::time::now_ms()
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn pairs(&self) -> Result<Vec<Pair>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, local, remote_id, kind, direction, state FROM folder_pairs ORDER BY id",
        )?;
        let rows = stmt.query_and_then([], pair_from_row)?;
        rows.collect()
    }

    pub fn pair(&self, id: i64) -> Result<Option<Pair>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, local, remote_id, kind, direction, state FROM folder_pairs WHERE id = ?1",
        )?;
        let mut rows = stmt.query_and_then([id], pair_from_row)?;
        rows.next().transpose()
    }

    /// O par do Espelho do Meu Drive, se a conta está no modo Espelho.
    pub fn mirror(&self) -> Result<Option<Pair>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, local, remote_id, kind, direction, state FROM folder_pairs \
             WHERE kind = 'mirror' LIMIT 1",
        )?;
        let mut rows = stmt.query_and_then([], pair_from_row)?;
        rows.next().transpose()
    }

    /// Desfaz o par e esquece o estado sincronizado dele (os arquivos ficam onde estão).
    pub fn remove_pair(&self, id: i64) -> Result<()> {
        self.modify(|batch| {
            let conn = batch.conn;
            conn.execute("DELETE FROM synced WHERE pair_id = ?1", [id])?;
            conn.execute("DELETE FROM folder_pairs WHERE id = ?1", [id])?;
            Ok::<_, StoreError>(())
        })
    }

    pub fn set_pair_state(&self, id: i64, state: PairState) -> Result<()> {
        self.conn().execute(
            "UPDATE folder_pairs SET state = ?2 WHERE id = ?1",
            params![id, state.to_text()],
        )?;
        Ok(())
    }

    /// O par passa a outra pasta do computador. O último estado sincronizado era da pasta antiga
    /// e sai junto: a próxima comparação é um primeiro encontro (junta, nunca exclui).
    pub fn set_pair_local(&self, id: i64, local: &Path) -> Result<()> {
        let local = path_text(local)?;
        self.modify(|batch| {
            let conn = batch.conn;
            conn.execute(
                "UPDATE folder_pairs SET local = ?2 WHERE id = ?1",
                params![id, local],
            )?;
            conn.execute("DELETE FROM synced WHERE pair_id = ?1", [id])?;
            Ok::<_, StoreError>(())
        })
    }

    /// O estado sincronizado do par, em ordem de caminho.
    pub fn synced_of(&self, pair: i64) -> Result<Vec<Synced>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT rel_path, item_id, is_dir, size, mtime_ns, inode, md5 FROM synced \
             WHERE pair_id = ?1 ORDER BY rel_path",
        )?;
        let rows = stmt.query_map([pair], synced_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// A pasta de apoio "Computadores" desta conta, quando já conhecida.
    pub fn computers_folder(&self) -> Result<Option<ItemId>> {
        Ok(self.kv_get(COMPUTERS_KEY)?.map(ItemId))
    }

    pub fn set_computers_folder(&self, id: &ItemId) -> Result<()> {
        self.kv_set(COMPUTERS_KEY, &id.0)
    }

    /// O item `item` está na base do par (sincronizado por ele).
    pub fn synced_has(&self, pair: i64, item: &ItemId) -> Result<bool> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM synced WHERE pair_id = ?1 AND item_id = ?2)",
        )?;
        Ok(stmt.query_row(params![pair, &item.0], |r| r.get(0))?)
    }

    /// O caminho relativo `rel_path` está na base do par.
    pub fn is_synced(&self, pair: i64, rel_path: &str) -> Result<bool> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT EXISTS (SELECT 1 FROM synced WHERE pair_id = ?1 AND rel_path = ?2)",
        )?;
        Ok(stmt.query_row(params![pair, rel_path], |r| r.get(0))?)
    }

    /// Grava vários caminhos numa transação só (uma sincronização grava milhares de uma vez).
    pub fn put_synced(&self, pair: i64, synced: &[Synced]) -> Result<()> {
        self.modify(|batch| {
            let mut stmt = batch.conn.prepare_cached(
                "INSERT OR REPLACE INTO synced \
                 (pair_id, rel_path, item_id, is_dir, size, mtime_ns, inode, md5) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for s in synced {
                stmt.execute(params![
                    pair,
                    s.rel_path,
                    s.item_id.0,
                    s.is_dir,
                    s.size,
                    s.mtime_ns,
                    s.inode,
                    s.md5
                ])?;
            }
            Ok::<_, StoreError>(())
        })
    }

    /// Esquece o caminho e tudo abaixo dele.
    pub fn forget_synced(&self, pair: i64, rel_path: &str) -> Result<()> {
        self.conn().execute(
            &format!("DELETE FROM synced WHERE pair_id = ?1 AND {UNDER}"),
            params![pair, rel_path],
        )?;
        Ok(())
    }

    /// Leva o caminho e tudo abaixo dele para `to`.
    pub fn rename_synced(&self, pair: i64, from: &str, to: &str) -> Result<()> {
        self.conn().execute(
            &format!(
                "UPDATE synced SET rel_path = ?3 || substr(rel_path, length(?2) + 1) \
                 WHERE pair_id = ?1 AND {UNDER}"
            ),
            params![pair, from, to],
        )?;
        Ok(())
    }

    /// Tudo abaixo da pasta `root` fora da lixeira, com o caminho relativo (nomes locais
    /// unidos por `/`), pais antes dos filhos (a recursão anda por níveis).
    pub fn descendants(&self, root: &ItemId) -> Result<Vec<(String, Item)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(concat!(
            "WITH RECURSIVE tree(id, rel) AS ( \
               SELECT id, local_name FROM items WHERE parent = ?1 AND trashed = 0 \
               UNION ALL \
               SELECT i.id, tree.rel || '/' || i.local_name FROM items i \
               JOIN tree ON i.parent = tree.id WHERE i.trashed = 0) \
             SELECT ",
            item_columns!(),
            ", tree.rel FROM tree JOIN items USING (id)"
        ))?;
        let rows = stmt.query_and_then([&root.0], |r| {
            let rel: String = r.get("rel")?;
            Ok::<_, StoreError>((rel, item_from_row(r)?))
        })?;
        rows.collect()
    }
}

/// O caminho `?2` e tudo abaixo dele, como faixa da chave (`'0'` vem logo depois de `'/'`):
/// usa o índice, e nomes com `%` e `_` não atrapalham.
const UNDER: &str = "(rel_path = ?2 OR (rel_path >= ?2 || '/' AND rel_path < ?2 || '0'))";

fn path_text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| StoreError::Corrupt(format!("caminho não UTF-8: {}", path.display())))
}

impl PairKind {
    fn as_str(self) -> &'static str {
        match self {
            PairKind::Mirror => "mirror",
            PairKind::Folder => "folder",
            PairKind::Device => "device",
        }
    }

    fn parse(text: &str) -> Option<PairKind> {
        [PairKind::Mirror, PairKind::Folder, PairKind::Device]
            .into_iter()
            .find(|k| k.as_str() == text)
    }
}

impl Direction {
    fn as_str(self) -> &'static str {
        match self {
            Direction::TwoWay => "two-way",
            Direction::UploadOnly => "upload-only",
        }
    }

    fn parse(text: &str) -> Option<Direction> {
        [Direction::TwoWay, Direction::UploadOnly]
            .into_iter()
            .find(|d| d.as_str() == text)
    }
}

impl PairState {
    fn to_text(self) -> String {
        match self {
            PairState::Active => "active".into(),
            PairState::Stopped(StopReason::Missing) => "missing".into(),
            PairState::Stopped(StopReason::NoPermission) => "no-permission".into(),
            PairState::Stopped(StopReason::Moved) => "moved".into(),
            PairState::ConfirmDeletes(n) => format!("confirm:{n}"),
        }
    }

    fn parse(text: &str) -> Option<PairState> {
        Some(match text {
            "active" => PairState::Active,
            "missing" => PairState::Stopped(StopReason::Missing),
            "no-permission" => PairState::Stopped(StopReason::NoPermission),
            "moved" => PairState::Stopped(StopReason::Moved),
            _ => PairState::ConfirmDeletes(text.strip_prefix("confirm:")?.parse().ok()?),
        })
    }
}

fn pair_from_row(r: &Row) -> Result<Pair> {
    let text = |i: usize| r.get::<_, String>(i);
    let corrupt = |what: String| StoreError::Corrupt(format!("par de pastas: {what}"));
    let (kind, direction, state) = (text(3)?, text(4)?, text(5)?);
    Ok(Pair {
        id: r.get(0)?,
        local: PathBuf::from(text(1)?),
        remote_id: ItemId(text(2)?),
        kind: PairKind::parse(&kind).ok_or_else(|| corrupt(kind))?,
        direction: Direction::parse(&direction).ok_or_else(|| corrupt(direction))?,
        state: PairState::parse(&state).ok_or_else(|| corrupt(state))?,
    })
}

fn synced_from_row(r: &Row) -> rusqlite::Result<Synced> {
    Ok(Synced {
        rel_path: r.get(0)?,
        item_id: ItemId(r.get(1)?),
        is_dir: r.get(2)?,
        size: r.get(3)?,
        mtime_ns: r.get(4)?,
        inode: r.get(5)?,
        md5: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Inode, ItemKind, LocalState, PinState};

    fn store() -> (tempfile::TempDir, Store) {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        (tmp, store)
    }

    fn folder_pair(store: &Store, local: &str) -> Result<i64> {
        store.add_pair(
            Path::new(local),
            &ItemId("r".into()),
            PairKind::Folder,
            Direction::TwoWay,
        )
    }

    fn synced(path: &str) -> Synced {
        Synced {
            rel_path: path.into(),
            item_id: ItemId(format!("id-{path}")),
            is_dir: false,
            size: 3,
            mtime_ns: 7,
            inode: 11,
            md5: Some("abc".into()),
        }
    }

    fn item(id: &str, parent: &str, name: &str, kind: ItemKind, trashed: bool) -> Item {
        Item {
            id: ItemId(id.into()),
            inode: Inode::UNASSIGNED,
            parent: Some(ItemId(parent.into())),
            remote_name: name.into(),
            local_name: name.into(),
            kind,
            size: 0,
            remote_size: 0,
            md5: None,
            head_revision: None,
            base_revision: None,
            base_md5: None,
            modified_ms: 0,
            created_ms: 0,
            trashed,
            can_edit: true,
            owned_by_me: true,
            local_state: LocalState::Clean,
            pin: PinState::None,
        }
    }

    #[test]
    fn pairs_are_saved_with_their_state() {
        let (_tmp, store) = store();
        let docs = folder_pair(&store, "/home/a/Documentos").unwrap();
        let usb = store
            .add_pair(
                Path::new("/run/media/a/USB"),
                &ItemId("u".into()),
                PairKind::Device,
                Direction::UploadOnly,
            )
            .unwrap();
        store
            .set_pair_state(docs, PairState::Stopped(StopReason::Missing))
            .unwrap();
        store
            .set_pair_state(usb, PairState::ConfirmDeletes(1200))
            .unwrap();

        let pairs = store.pairs().unwrap();
        assert_eq!(pairs.len(), 2);
        assert_eq!(pairs[0].local, Path::new("/home/a/Documentos"));
        assert_eq!(pairs[0].kind, PairKind::Folder);
        assert_eq!(pairs[0].state, PairState::Stopped(StopReason::Missing));
        assert_eq!(pairs[1].direction, Direction::UploadOnly);
        assert_eq!(pairs[1].state, PairState::ConfirmDeletes(1200));

        store
            .set_pair_local(docs, Path::new("/home/a/Docs"))
            .unwrap();
        assert_eq!(store.pairs().unwrap()[0].local, Path::new("/home/a/Docs"));
    }

    #[test]
    fn the_same_folder_cannot_be_paired_twice() {
        let (_tmp, store) = store();
        folder_pair(&store, "/home/a/Documentos").unwrap();
        assert!(folder_pair(&store, "/home/a/Documentos").is_err());
    }

    #[test]
    fn removing_a_pair_forgets_its_synced_state() {
        let (_tmp, store) = store();
        let pair = folder_pair(&store, "/x").unwrap();
        store.put_synced(pair, &[synced("a.txt")]).unwrap();
        assert_eq!(store.synced_of(pair).unwrap(), [synced("a.txt")]);

        store.remove_pair(pair).unwrap();

        assert!(store.pairs().unwrap().is_empty());
        assert!(store.synced_of(pair).unwrap().is_empty());
    }

    #[test]
    fn forget_and_rename_take_the_whole_subtree_and_only_it() {
        let (_tmp, store) = store();
        let pair = folder_pair(&store, "/x").unwrap();
        let all: Vec<_> = ["a", "a/b.txt", "a/c/d.txt", "a0", "ab.txt", "z"]
            .map(synced)
            .into();
        store.put_synced(pair, &all).unwrap();
        let paths = |store: &Store| {
            store
                .synced_of(pair)
                .unwrap()
                .into_iter()
                .map(|s| s.rel_path)
                .collect::<Vec<_>>()
        };

        store.rename_synced(pair, "a", "n").unwrap();
        assert_eq!(
            paths(&store),
            ["a0", "ab.txt", "n", "n/b.txt", "n/c/d.txt", "z"]
        );

        store.forget_synced(pair, "n").unwrap();
        assert_eq!(paths(&store), ["a0", "ab.txt", "z"]);
    }

    #[test]
    fn descendants_have_relative_paths_and_skip_the_trash() {
        let (_tmp, store) = store();
        store
            .upsert_items(&[
                item("f", "root", "Fotos", ItemKind::Folder, false),
                item("p", "f", "praia.jpg", ItemKind::File, false),
                item("v", "f", "Velha", ItemKind::Folder, true),
                item("w", "v", "dentro.txt", ItemKind::File, false),
                item("t", "root", "nota.txt", ItemKind::File, false),
                item("o", "outra", "fora.txt", ItemKind::File, false),
            ])
            .unwrap();

        let mut found: Vec<_> = store
            .descendants(&ItemId("root".into()))
            .unwrap()
            .into_iter()
            .map(|(path, item)| (path, item.id.0))
            .collect();
        found.sort();

        assert_eq!(
            found,
            [
                ("Fotos".to_string(), "f".to_string()),
                ("Fotos/praia.jpg".into(), "p".into()),
                ("nota.txt".into(), "t".into()),
            ]
        );
    }
}
