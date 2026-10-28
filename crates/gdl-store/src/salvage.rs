//! Banco quebrado: a checagem na abertura e a leitura do que der para salvar dele.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, ErrorCode, OpenFlags, params, params_from_iter};

use crate::{EDITED, Result, StoreError};

/// Um item com edição local que ainda não subiu, lido de um banco quebrado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Salvaged {
    pub id: String,
    /// Os nomes locais a partir da raiz; `None` se algum nome não for legível.
    pub path: Option<PathBuf>,
}

/// `PRAGMA quick_check`: um banco com páginas estragadas, ou um arquivo que nem é SQLite, vira
/// `StoreError::Corrupt`.
pub(crate) fn check(conn: &Connection) -> Result<()> {
    let verdict: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(corrupt_or_sqlite)?;
    if verdict == "ok" {
        Ok(())
    } else {
        Err(StoreError::Corrupt(verdict))
    }
}

/// Os erros que querem dizer arquivo estragado viram `Corrupt`; o resto continua `Sqlite`.
pub(crate) fn corrupt_or_sqlite(e: rusqlite::Error) -> StoreError {
    match e.sqlite_error_code() {
        Some(ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt) => {
            StoreError::Corrupt(e.to_string())
        }
        _ => StoreError::Sqlite(e),
    }
}

/// O que deu para ler de um banco quebrado.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Salvage {
    /// Os itens com edição local (sujos, subindo ou em conflito) lidos.
    pub items: Vec<Salvaged>,
    /// A lista foi lida inteira; sem isso, pode haver edições que não estão em `items`.
    pub complete: bool,
}

/// As edições locais que dá para ler de `path`, só lendo; a leitura para no primeiro erro, com o
/// que já tinha.
pub fn edited_items(path: &Path) -> Salvage {
    let Ok(conn) = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY) else {
        return Salvage::default();
    };
    let (ids, complete) = edited_ids(&conn);
    let items = ids
        .into_iter()
        .map(|id| Salvaged {
            path: path_of(&conn, &id),
            id,
        })
        .collect();
    Salvage { items, complete }
}

/// Os IDs lidos e se a leitura chegou ao fim.
fn edited_ids(conn: &Connection) -> (Vec<String>, bool) {
    let mut ids = vec![];
    let Ok(mut stmt) = conn.prepare("SELECT id FROM items WHERE local_state IN (?1, ?2, ?3)")
    else {
        return (ids, false);
    };
    let Ok(rows) = stmt.query_map(params_from_iter(EDITED), |r| r.get(0)) else {
        return (ids, false);
    };
    // Para no primeiro erro: depois de um erro de página, o passo seguinte pode repeti-lo.
    for row in rows {
        match row {
            Ok(id) => ids.push(id),
            Err(_) => return (ids, false),
        }
    }
    (ids, true)
}

/// `name` serve de componente de caminho (nem vazio, nem `.`/`..`, sem `/` nem NUL).
pub fn usable(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\0'])
}

/// Sobe pelos pais de `id` juntando os nomes locais, sem a raiz (inode 1); `None` se algum
/// nome não servir de componente de caminho.
fn path_of(conn: &Connection, id: &str) -> Option<PathBuf> {
    let mut stmt = conn
        .prepare_cached(
            "WITH RECURSIVE up(id, parent, name, inode, depth) AS ( \
                 SELECT id, parent, local_name, inode, 0 FROM items WHERE id = ?1 \
                 UNION ALL \
                 SELECT i.id, i.parent, i.local_name, i.inode, up.depth + 1 \
                 FROM items i JOIN up ON i.id = up.parent WHERE up.depth < 256) \
             SELECT name FROM up WHERE inode != 1 ORDER BY depth DESC",
        )
        .ok()?;
    let names: Vec<String> = stmt
        .query_map(params![id], |r| r.get(0))
        .ok()?
        .collect::<rusqlite::Result<_>>()
        .ok()?;
    (!names.is_empty() && names.iter().all(|n| usable(n))).then(|| names.iter().collect())
}

/// Estraga a primeira página da tabela `table` de `db` (fechado), sem mexer nas outras: o
/// `quick_check` acusa e o resto continua legível.
#[cfg(any(test, feature = "testing"))]
pub fn damage_table(db: &Path, table: &str) {
    use std::io::{Seek, SeekFrom, Write};
    let (page_size, root): (u64, u64) = {
        let conn = Connection::open(db).unwrap();
        let page_size = conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .unwrap();
        let root = conn
            .query_row(
                "SELECT rootpage FROM sqlite_master WHERE name = ?1",
                [table],
                |r| r.get(0),
            )
            .unwrap();
        (page_size, root)
    };
    let mut file = std::fs::OpenOptions::new().write(true).open(db).unwrap();
    file.seek(SeekFrom::Start((root - 1) * page_size)).unwrap();
    file.write_all(&vec![0xA5; page_size as usize]).unwrap();
}
