//! Esquema do banco e migrações, versionadas por `PRAGMA user_version`.

use rusqlite::Connection;

/// Cada posição é uma migração; a versão do banco é quantas já foram aplicadas.
const MIGRATIONS: &[&str] = &[
    "
    CREATE TABLE items (
        id            TEXT PRIMARY KEY,
        inode         INTEGER NOT NULL UNIQUE,
        parent        TEXT,
        remote_name   TEXT NOT NULL,
        local_name    TEXT NOT NULL,
        kind          TEXT NOT NULL,
        mime          TEXT,
        target_id     TEXT,
        size          INTEGER NOT NULL,
        md5           TEXT,
        head_revision TEXT,
        base_revision TEXT,
        base_md5      TEXT,
        modified_ms   INTEGER NOT NULL,
        created_ms    INTEGER NOT NULL,
        trashed       INTEGER NOT NULL,
        can_edit      INTEGER NOT NULL,
        local_state   INTEGER NOT NULL,
        pin           INTEGER NOT NULL
    );
    CREATE INDEX items_parent_name ON items (parent, local_name);
    CREATE INDEX items_in_trash ON items (local_name) WHERE trashed = 1;

    -- Inodes nunca são reaproveitados (AUTOINCREMENT); o 1 fica reservado para a raiz.
    CREATE TABLE inodes (
        inode   INTEGER PRIMARY KEY AUTOINCREMENT,
        item_id TEXT NOT NULL UNIQUE
    );
    INSERT INTO sqlite_sequence (name, seq) VALUES ('inodes', 1);

    CREATE TABLE ranges (
        item_id TEXT NOT NULL,
        start   INTEGER NOT NULL,
        end     INTEGER NOT NULL,
        PRIMARY KEY (item_id, start)
    ) WITHOUT ROWID;

    CREATE TABLE upload_queue (
        job_id      INTEGER PRIMARY KEY AUTOINCREMENT,
        item_id     TEXT NOT NULL,
        kind        INTEGER NOT NULL,
        attempts    INTEGER NOT NULL DEFAULT 0,
        next_try_ms INTEGER NOT NULL,
        session_url TEXT
    );
    CREATE INDEX upload_queue_next ON upload_queue (next_try_ms, job_id);

    CREATE TABLE kv (key TEXT PRIMARY KEY, value TEXT NOT NULL) WITHOUT ROWID;

    CREATE TABLE access (item_id TEXT PRIMARY KEY, last_access_ms INTEGER NOT NULL) WITHOUT ROWID;
",
    "
    -- Envios adiados por falta de rede: voltam na hora em que ela volta.
    ALTER TABLE upload_queue ADD COLUMN waiting_network INTEGER NOT NULL DEFAULT 0;
    -- ID reservado no Drive para a criação: repetida, ela não duplica o item.
    ALTER TABLE upload_queue ADD COLUMN file_id TEXT;
",
    "
    -- Tamanho da versão da nuvem: durante uma edição local, `size` é o da edição.
    ALTER TABLE items ADD COLUMN remote_size INTEGER NOT NULL DEFAULT 0;
    UPDATE items SET remote_size = size;
",
    "
    -- Último erro de cada envio, para a lista de erros.
    ALTER TABLE upload_queue ADD COLUMN last_error TEXT;
    ALTER TABLE upload_queue ADD COLUMN error_category TEXT;
    CREATE INDEX upload_queue_item ON upload_queue (item_id);
",
    "
    -- Arquivos off-line e recentes sem varrer as tabelas inteiras.
    CREATE INDEX items_pinned ON items (parent) WHERE pin != 0;
    CREATE INDEX access_recent ON access (last_access_ms);
",
    "
    -- Miniaturas já gravadas no cache do freedesktop, para não refazer nem repetir downloads.
    CREATE TABLE thumbs (
        item_id     TEXT PRIMARY KEY,
        revision    TEXT,
        modified_ms INTEGER NOT NULL,
        uri         TEXT NOT NULL
    ) WITHOUT ROWID;
",
    "
    -- Itens com edição local (sujos, subindo, em conflito) sem varrer todos os itens.
    CREATE INDEX items_edited ON items (local_name) WHERE local_state IN (1, 2, 3);
",
    "
    -- Drives compartilhados da conta; a raiz de cada um é um item pasta sem pai, com o id dele.
    CREATE TABLE shared_drives (
        id      TEXT PRIMARY KEY,
        name    TEXT NOT NULL,
        enabled INTEGER NOT NULL
    ) WITHOUT ROWID;
",
    "
    -- Pastas locais ligadas a pastas do Drive (Espelho, pastas do computador, dispositivos).
    CREATE TABLE folder_pairs (
        id         INTEGER PRIMARY KEY AUTOINCREMENT,
        local      TEXT NOT NULL UNIQUE,
        remote_id  TEXT NOT NULL,
        kind       TEXT NOT NULL,
        direction  TEXT NOT NULL,
        state      TEXT NOT NULL DEFAULT 'active',
        created_ms INTEGER NOT NULL
    );
    -- Último estado sincronizado de cada caminho de um par (a base da comparação em três vias).
    CREATE TABLE synced (
        pair_id  INTEGER NOT NULL,
        rel_path TEXT NOT NULL,
        item_id  TEXT NOT NULL,
        is_dir   INTEGER NOT NULL,
        size     INTEGER NOT NULL,
        mtime_ns INTEGER NOT NULL,
        inode    INTEGER NOT NULL,
        md5      TEXT,
        PRIMARY KEY (pair_id, rel_path)
    ) WITHOUT ROWID;
",
    "
    -- De quem é o item (falso no que outra pessoa criou); os já indexados contam como do usuário
    -- até a próxima mudança deles.
    ALTER TABLE items ADD COLUMN owned_by_me INTEGER NOT NULL DEFAULT 1;
",
];

/// Aplica as migrações que faltam, cada uma na própria transação.
pub(crate) fn migrate(conn: &mut Connection) -> rusqlite::Result<()> {
    let applied: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (version, sql) in MIGRATIONS.iter().enumerate().skip(applied) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", version + 1)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upgrade_fills_remote_size_from_size() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..2] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 2).unwrap();
        conn.execute(
            "INSERT INTO items (id, inode, remote_name, local_name, kind, size, modified_ms, \
             created_ms, trashed, can_edit, local_state, pin) \
             VALUES ('a', 2, 'a', 'a', 'file', 7, 0, 0, 0, 1, 0, 0)",
            [],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let remote_size: u64 = conn
            .query_row("SELECT remote_size FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remote_size, 7);
    }

    #[test]
    fn migration_4_adds_error_columns_and_keeps_the_queue() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..3] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 3).unwrap();
        conn.execute(
            "INSERT INTO upload_queue (item_id, kind, next_try_ms) VALUES ('a', 0, 5)",
            [],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let row: (String, Option<String>, Option<String>) = conn
            .query_row(
                "SELECT item_id, last_error, error_category FROM upload_queue",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(row, ("a".into(), None, None));
    }

    #[test]
    fn migration_6_adds_thumbs_and_keeps_items() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..5] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 5).unwrap();
        conn.execute(
            "INSERT INTO items (id, inode, remote_name, local_name, kind, size, modified_ms, \
             created_ms, trashed, can_edit, local_state, pin) \
             VALUES ('a', 2, 'a', 'a', 'file', 7, 0, 0, 0, 1, 0, 0)",
            [],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let version: usize = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .unwrap();
        assert_eq!(version, MIGRATIONS.len());
        let items: u32 = conn
            .query_row("SELECT COUNT(*) FROM items", [], |r| r.get(0))
            .unwrap();
        assert_eq!(items, 1);
        conn.execute(
            "INSERT INTO thumbs (item_id, revision, modified_ms, uri) VALUES ('a', NULL, 1, 'file:///a')",
            [],
        )
        .unwrap();
    }

    #[test]
    fn migration_7_indexes_the_edited_items() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..6] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 6).unwrap();
        conn.execute(
            "INSERT INTO items (id, inode, remote_name, local_name, kind, size, modified_ms, \
             created_ms, trashed, can_edit, local_state, pin) \
             VALUES ('a', 2, 'a', 'a', 'file', 7, 0, 0, 0, 1, 1, 0)",
            [],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let edited: u32 = conn
            .query_row(
                "SELECT COUNT(*) FROM items INDEXED BY items_edited WHERE local_state IN (1, 2, 3)",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(edited, 1);
    }

    #[test]
    fn owned_by_me_defaults_to_true() {
        let mut conn = Connection::open_in_memory().unwrap();
        for sql in &MIGRATIONS[..9] {
            conn.execute_batch(sql).unwrap();
        }
        conn.pragma_update(None, "user_version", 9).unwrap();
        conn.execute(
            "INSERT INTO items (id, inode, remote_name, local_name, kind, size, modified_ms, \
             created_ms, trashed, can_edit, local_state, pin) \
             VALUES ('a', 2, 'a', 'a', 'file', 7, 0, 0, 0, 1, 0, 0)",
            [],
        )
        .unwrap();

        migrate(&mut conn).unwrap();

        let owned: bool = conn
            .query_row("SELECT owned_by_me FROM items", [], |r| r.get(0))
            .unwrap();
        assert!(owned, "os itens já indexados contam como do usuário");
    }
}
