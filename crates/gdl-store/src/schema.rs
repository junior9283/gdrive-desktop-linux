//! Esquema do banco e migrações, versionadas por `PRAGMA user_version`.

use rusqlite::Connection;

/// Cada posição é uma migração; a versão do banco é quantas já foram aplicadas.
const MIGRATIONS: &[&str] = &["
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
"];

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
