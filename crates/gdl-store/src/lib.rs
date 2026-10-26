//! Banco SQLite por conta: metadados do Drive, intervalos em cache, fila de envios.

mod schema;
pub mod types;

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

use gdl_common::names::{self, Sibling, fold};
use rusqlite::functions::FunctionFlags;
use rusqlite::{
    Connection, OptionalExtension, Row, TransactionBehavior, named_params, params, params_from_iter,
};

pub use types::*;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("erro no banco: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("valor inválido no banco: {0}")]
    Corrupt(String),
}

pub type Result<T> = std::result::Result<T, StoreError>;

fn thumb_from_row(r: &Row) -> rusqlite::Result<ThumbRec> {
    Ok(ThumbRec {
        id: ItemId(r.get(0)?),
        revision: r.get(1)?,
        modified_ms: r.get(2)?,
        uri: r.get(3)?,
    })
}

/// `SELECT` de itens com o filtro dado.
macro_rules! select_items {
    ($filter:literal) => {
        concat!(
            "SELECT id, inode, parent, remote_name, local_name, kind, mime, target_id, size, md5, \
             head_revision, base_revision, base_md5, modified_ms, created_ms, trashed, can_edit, \
             local_state, pin, remote_size FROM items ",
            $filter
        )
    };
}

/// Banco de uma conta. Pode ser compartilhado entre threads.
/// `FROM … WHERE` dos arquivos fixados, fora da lixeira e sem edição local, que ainda não estão
/// inteiros no cache (`?1` = `PinState::None`, `?2` = `LocalState::Clean`).
const INCOMPLETE_PINNED: &str = "FROM items i \
     WHERE i.pin != ?1 AND i.kind = 'file' AND i.trashed = 0 AND i.local_state = ?2 \
     AND i.size > (SELECT COALESCE(SUM(r.end - r.start), 0) FROM ranges r \
                   WHERE r.item_id = i.id)";

/// `WHERE` (de `items`) dos fixados mais altos, sem pasta fixada acima (`?1` =
/// `PinState::Pinned`, `?2` = `PinState::None`).
const TOP_PINNED: &str = "WHERE pin = ?1 AND trashed = 0 AND NOT EXISTS ( \
     SELECT 1 FROM items p WHERE p.id = items.parent AND p.pin != ?2)";

/// Resumo da fila de envio (`Store::queue_stats`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct QueueStats {
    pub pending: u32,
    /// Itens com envio com erro registrado (os mesmos de `Store::job_errors`).
    pub failed: u32,
    /// Bytes dos itens com envio na fila.
    pub bytes: u64,
}

pub struct Store {
    conn: Mutex<Connection>,
}

impl Store {
    /// Abre (ou cria) o banco em modo WAL com `synchronous=FULL` e aplica as migrações.
    pub fn open(path: &Path) -> Result<Store> {
        let mut conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        // Comparação da pesquisa (`search_names`): sem maiúsculas nem acentos.
        conn.create_scalar_function(
            "gdl_fold",
            1,
            FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC,
            |ctx| Ok(fold(ctx.get_raw(0).as_str()?)),
        )?;
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
        self.modify(|batch| batch.put_all(items))
    }

    /// Lê, decide e grava numa transação só: nenhuma outra escrita (uma gravação local que
    /// acabou de terminar, por exemplo) cai entre a leitura e a escrita e se perde.
    pub fn modify<R, E: From<StoreError>>(
        &self,
        f: impl FnOnce(&Batch<'_>) -> std::result::Result<R, E>,
    ) -> std::result::Result<R, E> {
        let mut conn = self.conn();
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(StoreError::from)?;
        let result = f(&Batch { conn: &tx })?;
        tx.commit().map_err(StoreError::from)?;
        Ok(result)
    }

    fn query_items(&self, sql: &str, args: impl rusqlite::Params) -> Result<Vec<Item>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(sql)?;
        let rows = stmt.query_and_then(args, item_from_row)?;
        rows.collect()
    }

    fn query_item(&self, sql: &str, args: impl rusqlite::Params) -> Result<Option<Item>> {
        query_item(&self.conn(), sql, args)
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

    /// A pasta tem algum filho fora da lixeira.
    pub fn has_children(&self, parent: &ItemId) -> Result<bool> {
        let found = self.conn().query_row(
            "SELECT EXISTS(SELECT 1 FROM items WHERE parent = ?1 AND trashed = 0)",
            [&parent.0],
            |r| r.get(0),
        )?;
        Ok(found)
    }

    pub fn trashed(&self) -> Result<Vec<Item>> {
        self.query_items(select_items!("WHERE trashed = 1 ORDER BY local_name"), [])
    }

    /// Item da lixeira pelo nome local.
    pub fn lookup_trashed(&self, local_name: &str) -> Result<Option<Item>> {
        self.query_item(
            select_items!("WHERE trashed = 1 AND local_name = ?1"),
            [local_name],
        )
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
        self.modify(|batch| batch.ranges_add(id, start, end))
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

    /// O arquivo de `size` bytes está inteiro no cache? (Tamanho 0 é sempre inteiro.)
    pub fn is_complete(&self, id: &ItemId, size: u64) -> Result<bool> {
        Ok(self.ranges_missing(id, 0, size)?.is_empty())
    }

    pub fn ranges_clear(&self, id: &ItemId) -> Result<()> {
        self.modify(|batch| batch.ranges_clear(id))
    }

    pub fn enqueue(&self, job: NewUploadJob) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO upload_queue (item_id, kind, next_try_ms, file_id) VALUES (?1, ?2, ?3, ?4)",
            params![job.item.0, job.kind, job.next_try_ms, job.file_id],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// O envio pronto para tentar agora (`next_try_ms <= now_ms`), o mais antigo primeiro.
    pub fn next_job(&self, now_ms: i64) -> Result<Option<UploadJob>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT job_id, item_id, kind, attempts, next_try_ms, session_url, file_id \
             FROM upload_queue \
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
                    file_id: r.get(6)?,
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

    /// Os itens com envio com erro registrado (um por item: o erro mais recente da fila).
    pub fn job_errors(&self) -> Result<Vec<JobError>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT q.item_id, i.local_name, q.error_category, q.last_error \
             FROM upload_queue q JOIN items i ON i.id = q.item_id \
             WHERE q.job_id IN (SELECT MAX(job_id) FROM upload_queue \
                                WHERE error_category IS NOT NULL GROUP BY item_id)",
        )?;
        let errors = stmt
            .query_map([], |r| {
                Ok(JobError {
                    item: ItemId(r.get(0)?),
                    name: r.get(1)?,
                    category: r.get(2)?,
                    message: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(errors)
    }

    /// "Tentar de novo": os envios do item vão para agora, com as tentativas zeradas.
    pub fn retry_now(&self, id: &ItemId) -> Result<()> {
        self.conn().execute(
            "UPDATE upload_queue SET next_try_ms = 0, attempts = 0, waiting_network = 0, \
             error_category = NULL, last_error = NULL WHERE item_id = ?1",
            [&id.0],
        )?;
        Ok(())
    }

    /// Tira da fila todos os envios do item.
    pub fn drop_jobs(&self, id: &ItemId) -> Result<()> {
        self.conn()
            .execute("DELETE FROM upload_queue WHERE item_id = ?1", [&id.0])?;
        Ok(())
    }

    /// Mais uma tentativa às `next_try_ms`; `error` (categoria e mensagem) registra o erro desta
    /// tentativa para a lista de erros (sem ele, fica o erro anterior, se houver).
    pub fn retry_job(
        &self,
        job_id: i64,
        next_try_ms: i64,
        error: Option<(&str, &str)>,
    ) -> Result<()> {
        let (category, message) = error.unzip();
        self.conn().execute(
            "UPDATE upload_queue SET attempts = attempts + 1, next_try_ms = ?2, waiting_network = 0, \
             error_category = COALESCE(?3, error_category), last_error = COALESCE(?4, last_error) \
             WHERE job_id = ?1",
            params![job_id, next_try_ms, category, message],
        )?;
        Ok(())
    }

    pub fn set_local_state(&self, id: &ItemId, state: LocalState) -> Result<()> {
        self.conn().execute(
            "UPDATE items SET local_state = ?2 WHERE id = ?1",
            params![id.0, state],
        )?;
        Ok(())
    }

    /// Muda o estado só se ele ainda for `from`.
    pub fn transition_local_state(
        &self,
        id: &ItemId,
        from: LocalState,
        to: LocalState,
    ) -> Result<bool> {
        let changed = self.conn().execute(
            "UPDATE items SET local_state = ?3 WHERE id = ?1 AND local_state = ?2",
            params![id.0, from, to],
        )?;
        Ok(changed == 1)
    }

    /// Adia o envio por falta de rede até `next_try_ms` ou até `wake_network_waiters`.
    pub fn wait_for_network(&self, job_id: i64, next_try_ms: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE upload_queue SET next_try_ms = ?2, waiting_network = 1 WHERE job_id = ?1",
            [job_id, next_try_ms],
        )?;
        Ok(())
    }

    /// A rede voltou (ou o serviço reiniciou): os envios que esperavam por ela ficam para já.
    pub fn wake_network_waiters(&self, now_ms: i64) -> Result<usize> {
        Ok(self.conn().execute(
            "UPDATE upload_queue SET next_try_ms = MIN(next_try_ms, ?1), waiting_network = 0
             WHERE waiting_network = 1",
            [now_ms],
        )?)
    }

    /// Adia o envio sem contar tentativa (a pasta-mãe ainda não subiu, por exemplo).
    pub fn postpone_job(&self, job_id: i64, next_try_ms: i64) -> Result<()> {
        self.conn().execute(
            "UPDATE upload_queue SET next_try_ms = ?2, waiting_network = 0 WHERE job_id = ?1",
            [job_id, next_try_ms],
        )?;
        Ok(())
    }

    /// Itens com edição local (`Dirty`, `Uploading` ou `Conflict`) sem envio na fila.
    pub fn dirty_items_without_job(&self) -> Result<Vec<ItemId>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id FROM items WHERE local_state IN (?1, ?2, ?3) \
             AND id NOT IN (SELECT item_id FROM upload_queue) ORDER BY id",
        )?;
        let rows = stmt.query_map(params_from_iter(EDITED), |r| Ok(ItemId(r.get(0)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Troca o ID de um item (o temporário `local:` pelo do Drive) em todas as tabelas, mantendo o
    /// inode, os filhos, o cache e a fila.
    pub fn replace_id(&self, old: &ItemId, new: &ItemId) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        // A consulta de mudanças pode ter trazido o item novo antes do fim do envio: essa cópia
        // sai e o item local, com o inode que o kernel já conhece, fica com o ID.
        purge_rows(&tx, new)?;
        tx.execute("DELETE FROM inodes WHERE item_id = ?1", [&new.0])?;
        for sql in [
            "UPDATE items SET id = ?2 WHERE id = ?1",
            "UPDATE items SET parent = ?2 WHERE parent = ?1",
            "UPDATE inodes SET item_id = ?2 WHERE item_id = ?1",
            "UPDATE ranges SET item_id = ?2 WHERE item_id = ?1",
            "UPDATE access SET item_id = ?2 WHERE item_id = ?1",
            "UPDATE upload_queue SET item_id = ?2 WHERE item_id = ?1",
        ] {
            tx.execute(sql, [&old.0, &new.0])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Apaga o item local e tudo o que é dele. O inode continua reservado e nunca volta a ser usado.
    pub fn remove_item(&self, id: &ItemId) -> Result<()> {
        self.modify(|b| b.remove_item(id))
    }

    /// Salvar por cima: `dst` assume o inode de `src` (o nome agora aponta para o inode que o
    /// kernel conhece) e `src` sai do banco. O inode antigo de `dst` é aposentado.
    pub fn take_over(&self, src: &ItemId, dst: &ItemId) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        let inode: u64 = tx.query_row("SELECT inode FROM items WHERE id = ?1", [&src.0], |r| {
            r.get(0)
        })?;
        purge_rows(&tx, src)?;
        tx.execute("DELETE FROM inodes WHERE item_id = ?1", [&dst.0])?;
        tx.execute(
            "UPDATE inodes SET item_id = ?1 WHERE item_id = ?2",
            [&dst.0, &src.0],
        )?;
        tx.execute(
            "UPDATE items SET inode = ?1 WHERE id = ?2",
            params![inode, dst.0],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn set_job_file_id(&self, job_id: i64, file_id: &str) -> Result<()> {
        self.conn().execute(
            "UPDATE upload_queue SET file_id = ?2 WHERE job_id = ?1",
            params![job_id, file_id],
        )?;
        Ok(())
    }

    /// Guarda a sessão retomável do envio, só enquanto o item ainda está sendo enviado: depois
    /// de uma gravação nova, a sessão (com o conteúdo velho) não volta para a fila.
    pub fn set_session_url(&self, job_id: i64, url: Option<&str>) -> Result<()> {
        self.conn().execute(
            "UPDATE upload_queue SET session_url = ?2 WHERE job_id = ?1 AND (?2 IS NULL OR \
             (SELECT local_state FROM items WHERE id = upload_queue.item_id) = ?3)",
            params![job_id, url, LocalState::Uploading],
        )?;
        Ok(())
    }

    /// ID já reservado para criar o item, por qualquer envio dele na fila.
    pub fn reserved_file_id(&self, id: &ItemId) -> Result<Option<String>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT file_id FROM upload_queue WHERE item_id = ?1 AND file_id IS NOT NULL \
                 ORDER BY job_id LIMIT 1",
                [&id.0],
                |r| r.get(0),
            )
            .optional()?)
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

    /// Muda o estado de fixação de vários itens numa transação.
    pub fn set_pins(&self, ids: &[ItemId], pin: PinState) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut stmt = tx.prepare_cached("UPDATE items SET pin = ?1 WHERE id = ?2")?;
            for id in ids {
                stmt.execute(params![pin, id.0])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Arquivos fixados, fora da lixeira e sem edição local, que ainda não estão inteiros no
    /// cache (nunca baixados ou descartados por uma revisão nova).
    pub fn incomplete_pinned_files(&self) -> Result<Vec<ItemId>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare_cached(&format!("SELECT i.id {INCOMPLETE_PINNED} ORDER BY i.id"))?;
        let rows = stmt.query_map(params![PinState::None, LocalState::Clean], |r| {
            Ok(ItemId(r.get(0)?))
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Quantos são os `incomplete_pinned_files`, sem carregá-los.
    pub fn count_incomplete_pinned(&self) -> Result<u32> {
        Ok(self.conn().query_row(
            &format!("SELECT COUNT(*) {INCOMPLETE_PINNED}"),
            params![PinState::None, LocalState::Clean],
            |r| r.get(0),
        )?)
    }

    /// Arquivos off-line para a lista de Preferências. Sem `folder`, os fixados mais altos (sem
    /// pasta fixada acima), de qualquer lugar; numa pasta, os filhos fixados e as pastas sem
    /// fixação própria com algo fixado dentro (`pin == PinState::None`, "misto").
    pub fn pinned_children(&self, folder: Option<&ItemId>) -> Result<Vec<Item>> {
        let Some(folder) = folder else {
            return self.query_items(
                &format!("{} {TOP_PINNED} ORDER BY local_name", select_items!("")),
                params![PinState::Pinned, PinState::None],
            );
        };
        // `up`: as pastas acima de algum item fixado.
        self.query_items(
            concat!(
                "WITH RECURSIVE up(id) AS ( \
                     SELECT parent FROM items WHERE pin = ?2 AND trashed = 0 \
                     UNION SELECT i.parent FROM items i JOIN up ON i.id = up.id \
                     WHERE i.parent IS NOT NULL) ",
                select_items!(
                    "WHERE parent = ?1 AND trashed = 0 AND (pin != ?3 OR id IN up) \
                     ORDER BY local_name"
                )
            ),
            params![folder.0, PinState::Pinned, PinState::None],
        )
    }

    /// Se há algo fixado dentro de `folder` (a pasta "mista" do menu off-line). Sobe dos itens
    /// fixados, que são poucos, em vez de descer a pasta, que pode ser o Drive inteiro.
    pub fn has_pinned_inside(&self, folder: &ItemId) -> Result<bool> {
        Ok(self.conn().query_row(
            "WITH RECURSIVE up(id) AS ( \
                 SELECT parent FROM items WHERE pin = ?2 AND trashed = 0 \
                 UNION SELECT i.parent FROM items i JOIN up ON i.id = up.id \
                 WHERE i.parent IS NOT NULL) \
             SELECT EXISTS (SELECT 1 FROM up WHERE id = ?1)",
            params![folder.0, PinState::Pinned],
            |r| r.get(0),
        )?)
    }

    /// Quantos são os fixados mais altos (`pinned_children(None)`), sem carregá-los.
    pub fn count_top_pinned(&self) -> Result<u32> {
        Ok(self.conn().query_row(
            &format!("SELECT COUNT(*) FROM items {TOP_PINNED}"),
            params![PinState::Pinned, PinState::None],
            |r| r.get(0),
        )?)
    }

    /// Bytes dos arquivos off-line (fixados por conta própria ou por herança).
    pub fn pinned_bytes(&self) -> Result<u64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(size), 0) FROM items \
             WHERE pin != ?1 AND kind = 'file' AND trashed = 0",
            params![PinState::None],
            |r| r.get(0),
        )?)
    }

    /// Desfixa tudo ("Remover arquivos off-line"); o conteúdo fica como cache comum.
    pub fn unpin_all(&self) -> Result<()> {
        self.conn().execute(
            "UPDATE items SET pin = ?1 WHERE pin != ?1",
            params![PinState::None],
        )?;
        Ok(())
    }

    /// Até `limit` itens fora da lixeira cujo nome contém `query`, sem diferenciar maiúsculas
    /// nem acentos (`fold`). A raiz não conta.
    pub fn search_names(&self, query: &str, limit: usize) -> Result<Vec<Item>> {
        let mut pattern = String::from("%");
        for c in fold(query).chars() {
            if matches!(c, '%' | '_' | '\\') {
                pattern.push('\\');
            }
            pattern.push(c);
        }
        pattern.push('%');
        self.query_items(
            select_items!(
                "WHERE trashed = 0 AND parent IS NOT NULL \
                 AND gdl_fold(local_name) LIKE ?1 ESCAPE '\\' ORDER BY local_name LIMIT ?2"
            ),
            params![pattern, limit],
        )
    }

    /// Os `limit` arquivos abertos por último (pela tabela `access`), o mais recente primeiro.
    pub fn recent(&self, limit: usize) -> Result<Vec<Item>> {
        self.query_items(
            select_items!(
                "JOIN access a ON a.item_id = items.id \
                 WHERE trashed = 0 AND kind != 'folder' \
                 ORDER BY a.last_access_ms DESC LIMIT ?1"
            ),
            params![limit],
        )
    }

    /// Itens que ainda não estão no Drive como estão aqui (com envio na fila ou edição local),
    /// em ordem de nome local.
    pub fn pending_items(&self) -> Result<Vec<Item>> {
        self.query_items(
            select_items!(
                "WHERE id IN (SELECT item_id FROM upload_queue) OR local_state IN (?1, ?2, ?3) \
                 ORDER BY local_name"
            ),
            params_from_iter(EDITED),
        )
    }

    /// Caminho de `id` a partir da raiz do Meu Drive (`Pasta/arquivo.txt`), pelos nomes locais;
    /// `None` se algum pai estiver faltando ou se a cadeia não termina na raiz (itens de pastas
    /// compartilhadas por outras pessoas, que não aparecem na montagem).
    pub fn path_of(&self, id: &ItemId) -> Result<Option<String>> {
        // Sobe pelos pais numa consulta só; `depth` ordena do item para a raiz.
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "WITH RECURSIVE up(id, parent, name, inode, depth) AS ( \
                 SELECT id, parent, local_name, inode, 0 FROM items WHERE id = ?1 \
                 UNION ALL \
                 SELECT i.id, i.parent, i.local_name, i.inode, up.depth + 1 \
                 FROM items i JOIN up ON i.id = up.parent) \
             SELECT parent, name, inode FROM up ORDER BY depth DESC",
        )?;
        let rows: Vec<(Option<String>, String, u64)> = stmt
            .query_map(params![id.0], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
            .collect::<rusqlite::Result<_>>()?;
        // Chegou à raiz do Meu Drive só se nenhum pai faltou e a cadeia termina nela.
        match rows.first() {
            Some((None, _, inode)) if *inode == Inode::ROOT.0 => Ok(Some(
                rows.iter()
                    .skip(1)
                    .map(|(_, name, _)| name.as_str())
                    .collect::<Vec<_>>()
                    .join("/"),
            )),
            _ => Ok(None),
        }
    }

    /// Envios na fila, quantos deles já falharam ao menos uma vez e os bytes dos itens deles.
    pub fn queue_stats(&self) -> Result<QueueStats> {
        Ok(self.conn().query_row(
            "SELECT COUNT(*), COUNT(DISTINCT item_id) FILTER (WHERE error_category IS NOT NULL), \
             (SELECT COALESCE(SUM(size), 0) FROM items \
              WHERE id IN (SELECT item_id FROM upload_queue)) \
             FROM upload_queue",
            [],
            |r| {
                Ok(QueueStats {
                    pending: r.get(0)?,
                    failed: r.get(1)?,
                    bytes: r.get(2)?,
                })
            },
        )?)
    }

    /// Total de bytes de conteúdo no cache, somando os intervalos de todos os itens.
    pub fn cached_bytes(&self) -> Result<u64> {
        Ok(self.conn().query_row(
            "SELECT COALESCE(SUM(end - start), 0) FROM ranges",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn touch_access(&self, id: &ItemId, now_ms: i64) -> Result<()> {
        self.conn().execute(
            "INSERT INTO access (item_id, last_access_ms) VALUES (?1, ?2) \
             ON CONFLICT(item_id) DO UPDATE SET last_access_ms = excluded.last_access_ms",
            params![id.0, now_ms],
        )?;
        Ok(())
    }

    /// Miniatura semeada para o item, se houver.
    pub fn thumb(&self, id: &ItemId) -> Result<Option<ThumbRec>> {
        Ok(self
            .conn()
            .query_row(
                "SELECT item_id, revision, modified_ms, uri FROM thumbs WHERE item_id = ?1",
                [&id.0],
                thumb_from_row,
            )
            .optional()?)
    }

    /// Miniaturas semeadas dos filhos de `parent` fora da lixeira (os mesmos de `children`),
    /// numa consulta só.
    pub fn thumbs_of_children(&self, parent: &ItemId) -> Result<HashMap<ItemId, ThumbRec>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT t.item_id, t.revision, t.modified_ms, t.uri FROM thumbs t \
             JOIN items i ON i.id = t.item_id WHERE i.parent = ?1 AND i.trashed = 0",
        )?;
        let rows = stmt.query_map([&parent.0], thumb_from_row)?;
        Ok(rows
            .map(|r| r.map(|rec| (rec.id.clone(), rec)))
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Grava (ou substitui) a miniatura semeada do item.
    pub fn put_thumb(&self, rec: &ThumbRec) -> Result<()> {
        self.put_thumbs(std::slice::from_ref(rec))
    }

    /// Grava (ou substitui) várias miniaturas semeadas numa transação.
    pub fn put_thumbs(&self, recs: &[ThumbRec]) -> Result<()> {
        let mut conn = self.conn();
        let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        {
            let mut put = tx.prepare_cached(
                "INSERT INTO thumbs (item_id, revision, modified_ms, uri) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(item_id) DO UPDATE SET revision = excluded.revision, \
                 modified_ms = excluded.modified_ms, uri = excluded.uri",
            )?;
            for rec in recs {
                put.execute(params![rec.id.0, rec.revision, rec.modified_ms, rec.uri])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Todas as miniaturas semeadas.
    pub fn all_thumbs(&self) -> Result<Vec<ThumbRec>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT item_id, revision, modified_ms, uri FROM thumbs ORDER BY item_id",
        )?;
        Ok(stmt
            .query_map([], thumb_from_row)?
            .collect::<rusqlite::Result<_>>()?)
    }

    /// Esquece todas as miniaturas semeadas.
    pub fn clear_thumbs(&self) -> Result<()> {
        self.conn().execute("DELETE FROM thumbs", [])?;
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

/// Estados de um item com edição local que ainda não está no Drive.
const EDITED: [LocalState; 3] = [
    LocalState::Dirty,
    LocalState::Uploading,
    LocalState::Conflict,
];

fn query_item(conn: &Connection, sql: &str, args: impl rusqlite::Params) -> Result<Option<Item>> {
    let mut stmt = conn.prepare_cached(sql)?;
    stmt.query_and_then(args, item_from_row)?.next().transpose()
}

/// Leituras e escritas de dentro de `Store::modify`, na mesma transação.
pub struct Batch<'a> {
    conn: &'a Connection,
}

impl Batch<'_> {
    pub fn get(&self, id: &ItemId) -> Result<Option<Item>> {
        query_item(self.conn, select_items!("WHERE id = ?1"), [&id.0])
    }

    pub fn put(&self, item: &Item) -> Result<()> {
        self.put_all(std::slice::from_ref(item))
    }

    pub fn put_all(&self, items: &[Item]) -> Result<()> {
        let mut inode_of = self.conn.prepare_cached(
            "INSERT INTO inodes (inode, item_id) VALUES (?1, ?2) \
             ON CONFLICT(item_id) DO UPDATE SET item_id = item_id RETURNING inode",
        )?;
        let mut upsert = self.conn.prepare_cached(
            "INSERT INTO items (id, inode, parent, remote_name, local_name, kind, mime, target_id, \
             size, md5, head_revision, base_revision, base_md5, modified_ms, created_ms, trashed, \
             can_edit, local_state, pin, remote_size) VALUES (:id, :inode, :parent, :remote_name, \
             :local_name, :kind, :mime, :target_id, :size, :md5, :head_revision, :base_revision, \
             :base_md5, :modified_ms, :created_ms, :trashed, :can_edit, :local_state, :pin, \
             :remote_size) \
             ON CONFLICT(id) DO UPDATE SET parent = excluded.parent, \
             remote_name = excluded.remote_name, local_name = excluded.local_name, \
             kind = excluded.kind, mime = excluded.mime, target_id = excluded.target_id, \
             size = excluded.size, md5 = excluded.md5, head_revision = excluded.head_revision, \
             base_revision = excluded.base_revision, base_md5 = excluded.base_md5, \
             modified_ms = excluded.modified_ms, created_ms = excluded.created_ms, \
             trashed = excluded.trashed, can_edit = excluded.can_edit, \
             local_state = excluded.local_state, pin = excluded.pin, \
             remote_size = excluded.remote_size",
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
                ":remote_size": item.remote_size,
            })?;
        }
        Ok(())
    }

    pub fn ranges_clear(&self, id: &ItemId) -> Result<()> {
        self.conn
            .execute("DELETE FROM ranges WHERE item_id = ?1", [&id.0])?;
        Ok(())
    }

    /// Apaga o item e tudo o que é dele (o inode continua reservado, como em `remove_item`).
    pub fn remove_item(&self, id: &ItemId) -> Result<()> {
        purge_rows(self.conn, id)
    }

    /// O conteúdo mudou: as sessões retomáveis dos envios do item (do conteúdo velho) caem.
    pub fn reset_upload_sessions(&self, id: &ItemId) -> Result<()> {
        self.conn.execute(
            "UPDATE upload_queue SET session_url = NULL WHERE item_id = ?1",
            [&id.0],
        )?;
        Ok(())
    }

    /// Marca `[start, end)` como presente no cache, juntando com intervalos que se tocam.
    pub fn ranges_add(&self, id: &ItemId, start: u64, end: u64) -> Result<()> {
        let (mut lo, mut hi) = (start, end);
        let mut absorb = self.conn.prepare_cached(
            "DELETE FROM ranges WHERE item_id = ?1 AND start <= ?3 AND end >= ?2 RETURNING start, end",
        )?;
        for range in absorb.query_map(params![id.0, start, end], |r| {
            Ok((r.get::<_, u64>(0)?, r.get::<_, u64>(1)?))
        })? {
            let (s, e) = range?;
            lo = lo.min(s);
            hi = hi.max(e);
        }
        self.conn
            .prepare_cached("INSERT INTO ranges (item_id, start, end) VALUES (?1, ?2, ?3)")?
            .execute(params![id.0, lo, hi])?;
        Ok(())
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
        remote_size: r.get(19)?,
    })
}

/// Apaga as linhas do item em todas as tabelas, menos a do inode (cada chamador decide o destino
/// dele).
fn purge_rows(tx: &Connection, id: &ItemId) -> Result<()> {
    for sql in [
        "DELETE FROM items WHERE id = ?1",
        "DELETE FROM ranges WHERE item_id = ?1",
        "DELETE FROM access WHERE item_id = ?1",
        "DELETE FROM upload_queue WHERE item_id = ?1",
    ] {
        tx.execute(sql, [&id.0])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
            remote_size: 0,
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
    fn path_of_only_reaches_the_my_drive_root() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let folder = |name: &str, parent: Option<&str>| Item {
            kind: ItemKind::Folder,
            ..item(name, parent)
        };
        store
            .upsert_item(&Item {
                inode: Inode::ROOT,
                ..folder("root", None)
            })
            .unwrap();
        store.upsert_item(&folder("Docs", Some("root"))).unwrap();
        store.upsert_item(&item("a.txt", Some("Docs"))).unwrap();
        // Pasta de outra pessoa, compartilhada: sem pai, mas não é a raiz.
        store.upsert_item(&folder("Projeto", None)).unwrap();
        store
            .upsert_item(&item("orcamento.xlsx", Some("Projeto")))
            .unwrap();

        assert_eq!(
            store.path_of(&id("a.txt")).unwrap().as_deref(),
            Some("Docs/a.txt")
        );
        assert_eq!(store.path_of(&id("root")).unwrap().as_deref(), Some(""));
        assert_eq!(store.path_of(&id("orcamento.xlsx")).unwrap(), None);
        assert_eq!(store.path_of(&id("Projeto")).unwrap(), None);
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

    fn thumb_rec(item: &str, revision: &str) -> ThumbRec {
        ThumbRec {
            id: id(item),
            revision: Some(revision.into()),
            modified_ms: 1_000,
            uri: format!("file:///mnt/drive/{item}.jpg"),
        }
    }

    #[test]
    fn thumbs_roundtrip_and_upsert() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        assert_eq!(store.thumb(&id("a")).unwrap(), None);

        store.put_thumb(&thumb_rec("a", "r1")).unwrap();
        assert_eq!(store.thumb(&id("a")).unwrap(), Some(thumb_rec("a", "r1")));

        let nova = ThumbRec {
            modified_ms: 2_000,
            ..thumb_rec("a", "r2")
        };
        store.put_thumb(&nova).unwrap();
        assert_eq!(store.thumb(&id("a")).unwrap(), Some(nova.clone()));
        assert_eq!(store.all_thumbs().unwrap(), [nova]);

        let sem_revisao = ThumbRec {
            revision: None,
            ..thumb_rec("b", "x")
        };
        store.put_thumb(&sem_revisao).unwrap();
        assert_eq!(store.thumb(&id("b")).unwrap(), Some(sem_revisao));
        assert_eq!(store.all_thumbs().unwrap().len(), 2);
    }

    #[test]
    fn put_thumbs_writes_all_and_replaces_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store.put_thumbs(&[]).unwrap();
        assert!(store.all_thumbs().unwrap().is_empty());

        store.put_thumb(&thumb_rec("a", "r1")).unwrap();
        let recs = [thumb_rec("a", "r2"), thumb_rec("b", "r1")];
        store.put_thumbs(&recs).unwrap();
        assert_eq!(store.all_thumbs().unwrap(), recs);
    }

    #[test]
    fn thumbs_of_children_reads_only_that_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let lixo = Item {
            trashed: true,
            ..item("c", Some("p"))
        };
        store
            .upsert_items(&[
                item("p", None),
                item("a", Some("p")),
                item("b", Some("p")),
                lixo,
                item("x", Some("outra")),
            ])
            .unwrap();
        assert!(store.thumbs_of_children(&id("p")).unwrap().is_empty());

        let recs = ["a", "c", "x", "sem-item"].map(|i| thumb_rec(i, "r1"));
        store.put_thumbs(&recs).unwrap();
        let found = store.thumbs_of_children(&id("p")).unwrap();
        // Só os filhos fora da lixeira, como em `children`; "b" não tem miniatura.
        assert_eq!(
            found,
            std::collections::HashMap::from([(id("a"), thumb_rec("a", "r1"))])
        );
    }

    #[test]
    fn clear_thumbs_empties_table() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store.put_thumb(&thumb_rec("a", "r1")).unwrap();
        store.put_thumb(&thumb_rec("b", "r1")).unwrap();
        store.clear_thumbs().unwrap();
        assert!(store.all_thumbs().unwrap().is_empty());
        assert_eq!(store.thumb(&id("a")).unwrap(), None);
    }

    #[test]
    fn is_complete_follows_the_cached_ranges() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let a = id("a");
        assert!(store.is_complete(&a, 0).unwrap());
        assert!(!store.is_complete(&a, 40).unwrap());
        store.ranges_add(&a, 0, 30).unwrap();
        assert!(!store.is_complete(&a, 40).unwrap());
        store.ranges_add(&a, 30, 40).unwrap();
        assert!(store.is_complete(&a, 40).unwrap());
        assert!(!store.is_complete(&a, 41).unwrap());
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
        // A sessão só é guardada enquanto o item está sendo enviado.
        store
            .upsert_item(&Item {
                local_state: LocalState::Uploading,
                ..item("a", Some("root"))
            })
            .unwrap();
        let a = store
            .enqueue(NewUploadJob {
                item: id("a"),
                kind: UploadKind::Update,
                next_try_ms: 0,
                file_id: None,
            })
            .unwrap();
        let b = store
            .enqueue(NewUploadJob {
                item: id("b"),
                kind: UploadKind::Create,
                next_try_ms: 1000,
                file_id: None,
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
        store.retry_job(a, 2000, None).unwrap();
        assert!(store.next_job(500).unwrap().is_none());
        assert_eq!(store.next_job(1500).unwrap().unwrap().job_id, b);
        store.finish_job(b).unwrap();
        let job = store.next_job(3000).unwrap().unwrap();
        assert_eq!((job.job_id, job.attempts), (a, 1));
    }

    #[test]
    fn take_over_gives_destination_the_source_inode_and_drops_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let get = |name: &str| store.get(&id(name)).unwrap().unwrap();
        store
            .upsert_items(&[item("doc", Some("root")), item("tmp", Some("root"))])
            .unwrap();
        let (old_dst, src) = (get("doc").inode, get("tmp").inode);
        store.ranges_add(&id("tmp"), 0, 10).unwrap();

        store.take_over(&id("tmp"), &id("doc")).unwrap();

        assert_eq!(get("doc").inode, src);
        assert!(store.get(&id("tmp")).unwrap().is_none());
        assert!(store.by_inode(old_dst).unwrap().is_none());
        assert_eq!(store.by_inode(src).unwrap().unwrap().id, id("doc"));
        assert!(store.ranges_missing(&id("tmp"), 0, 10).unwrap().len() == 1);
        // O inode antigo do destino nunca volta.
        store.upsert_item(&item("novo", Some("root"))).unwrap();
        assert!(get("novo").inode.0 > src.0);
    }

    #[test]
    fn pending_items_are_the_ones_with_an_upload_job() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store
            .upsert_items(&[item("a", Some("root")), item("b", Some("root"))])
            .unwrap();
        store
            .enqueue(NewUploadJob {
                item: id("b"),
                kind: UploadKind::Update,
                next_try_ms: 0,
                file_id: None,
            })
            .unwrap();

        let pending: Vec<_> = store
            .pending_items()
            .unwrap()
            .into_iter()
            .map(|i| i.id)
            .collect();

        assert_eq!(pending, [id("b")]);
    }

    #[test]
    fn replace_id_drops_the_copy_that_changes_already_brought() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store
            .upsert_items(&[
                item("local:1", Some("root")),
                item("filho", Some("local:1")),
            ])
            .unwrap();
        let inode = store.get(&id("local:1")).unwrap().unwrap().inode;
        // A consulta de mudanças viu a pasta recém-criada antes do fim do envio.
        store.upsert_item(&item("real", Some("root"))).unwrap();
        let copy = store.get(&id("real")).unwrap().unwrap().inode;

        store.replace_id(&id("local:1"), &id("real")).unwrap();

        assert_eq!(store.get(&id("real")).unwrap().unwrap().inode, inode);
        assert!(store.by_inode(copy).unwrap().is_none());
        assert_eq!(store.children(&id("real")).unwrap().len(), 1);
    }

    #[test]
    fn replace_id_keeps_inode_children_ranges_and_jobs() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store
            .upsert_items(&[
                item("local:1", Some("root")),
                item("filho", Some("local:1")),
            ])
            .unwrap();
        let inode = store.get(&id("local:1")).unwrap().unwrap().inode;
        store.ranges_add(&id("local:1"), 0, 10).unwrap();
        store.touch_access(&id("local:1"), 5).unwrap();
        store
            .enqueue(NewUploadJob {
                item: id("local:1"),
                kind: UploadKind::Update,
                next_try_ms: 0,
                file_id: None,
            })
            .unwrap();

        store.replace_id(&id("local:1"), &id("real")).unwrap();

        assert!(store.get(&id("local:1")).unwrap().is_none());
        let real = store.get(&id("real")).unwrap().unwrap();
        assert_eq!(real.inode, inode);
        assert_eq!(store.by_inode(inode).unwrap().unwrap().id, id("real"));
        assert_eq!(store.children(&id("real")).unwrap()[0].id, id("filho"));
        assert!(store.ranges_missing(&id("real"), 0, 10).unwrap().is_empty());
        assert_eq!(store.next_job(0).unwrap().unwrap().item, id("real"));
    }

    #[test]
    fn removed_item_leaves_no_trace_and_its_inode_is_never_reused() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store.upsert_item(&item("a", Some("root"))).unwrap();
        let inode = store.get(&id("a")).unwrap().unwrap().inode;
        store.ranges_add(&id("a"), 0, 10).unwrap();
        store
            .enqueue(NewUploadJob {
                item: id("a"),
                kind: UploadKind::Create,
                next_try_ms: 0,
                file_id: None,
            })
            .unwrap();

        store.remove_item(&id("a")).unwrap();

        assert!(store.get(&id("a")).unwrap().is_none());
        assert_eq!(store.ranges_missing(&id("a"), 0, 10).unwrap(), [(0, 10)]);
        assert!(store.next_job(0).unwrap().is_none());
        store.upsert_item(&item("b", Some("root"))).unwrap();
        assert!(store.get(&id("b")).unwrap().unwrap().inode > inode);
    }

    #[test]
    fn postponing_a_job_does_not_count_an_attempt() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let job = store
            .enqueue(NewUploadJob {
                item: id("a"),
                kind: UploadKind::Update,
                next_try_ms: 0,
                file_id: None,
            })
            .unwrap();
        store.postpone_job(job, 100).unwrap();
        assert!(store.next_job(50).unwrap().is_none());
        assert_eq!(store.next_job(100).unwrap().unwrap().attempts, 0);
    }

    /// Um envio que esperou a rede e depois foi adiado por outro motivo mantém o novo prazo
    /// quando a rede volta de novo.
    #[test]
    fn rescheduled_job_no_longer_waits_for_network() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let new_job = || NewUploadJob {
            item: id("a"),
            kind: UploadKind::Update,
            next_try_ms: 0,
            file_id: None,
        };
        let retried = store.enqueue(new_job()).unwrap();
        let postponed = store.enqueue(new_job()).unwrap();
        for job in [retried, postponed] {
            store.wait_for_network(job, 100).unwrap();
        }
        store.retry_job(retried, 500, None).unwrap();
        store.postpone_job(postponed, 500).unwrap();

        assert_eq!(store.wake_network_waiters(0).unwrap(), 0);
        assert!(store.next_job(499).unwrap().is_none());
    }

    #[test]
    fn enqueue_keeps_the_reserved_file_id() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        store
            .enqueue(NewUploadJob {
                item: id("local-a"),
                kind: UploadKind::Create,
                next_try_ms: 0,
                file_id: Some("reservado".into()),
            })
            .unwrap();
        let job = store.next_job(0).unwrap().unwrap();
        assert_eq!(job.file_id.as_deref(), Some("reservado"));
    }

    #[test]
    fn finds_pending_edits_without_a_job() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let mut queued = item("queued", Some("root"));
        queued.local_state = LocalState::Dirty;
        let mut orphan = item("orphan", Some("root"));
        orphan.local_state = LocalState::Uploading;
        let mut local_only = item("lock", Some("root"));
        local_only.local_state = LocalState::LocalOnly;
        store
            .upsert_items(&[queued, orphan, local_only, item("clean", Some("root"))])
            .unwrap();
        store
            .enqueue(NewUploadJob {
                item: id("queued"),
                kind: UploadKind::Update,
                next_try_ms: 0,
                file_id: None,
            })
            .unwrap();
        assert_eq!(store.dirty_items_without_job().unwrap(), [id("orphan")]);
    }

    /// Raiz, `Pasta` (não fixada) com `fixo.txt` (fixado) e `solto.txt`, e `Toda` (fixada) com
    /// `dentro.txt` (herda).
    fn pinned_tree(store: &Store) {
        let folder = |name: &str, parent| Item {
            kind: ItemKind::Folder,
            ..item(name, parent)
        };
        store
            .upsert_items(&[
                Item {
                    inode: Inode::ROOT,
                    ..folder("root", None)
                },
                folder("Pasta", Some("root")),
                Item {
                    size: 10,
                    pin: PinState::Pinned,
                    ..item("fixo.txt", Some("Pasta"))
                },
                item("solto.txt", Some("Pasta")),
                Item {
                    pin: PinState::Pinned,
                    ..folder("Toda", Some("root"))
                },
                Item {
                    size: 5,
                    pin: PinState::Inherited,
                    ..item("dentro.txt", Some("Toda"))
                },
            ])
            .unwrap();
    }

    #[test]
    fn pinned_children_reports_mixed_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        pinned_tree(&store);
        let names = |items: Vec<Item>| -> Vec<(String, PinState)> {
            items.into_iter().map(|i| (i.local_name, i.pin)).collect()
        };

        // No topo, os fixados mais altos, de qualquer pasta.
        assert_eq!(
            names(store.pinned_children(None).unwrap()),
            [
                ("Toda".to_string(), PinState::Pinned),
                ("fixo.txt".to_string(), PinState::Pinned)
            ]
        );
        // Numa pasta, os filhos fixados e as pastas com algo fixado dentro (sem fixação própria).
        assert_eq!(
            names(store.pinned_children(Some(&id("root"))).unwrap()),
            [
                ("Pasta".to_string(), PinState::None),
                ("Toda".to_string(), PinState::Pinned)
            ]
        );
        assert_eq!(
            names(store.pinned_children(Some(&id("Toda"))).unwrap()),
            [("dentro.txt".to_string(), PinState::Inherited)]
        );
        assert_eq!(store.pinned_bytes().unwrap(), 15);
        assert_eq!(store.count_top_pinned().unwrap(), 2);
        assert!(store.has_pinned_inside(&id("root")).unwrap());
        assert!(store.has_pinned_inside(&id("Pasta")).unwrap());
        assert!(
            !store.has_pinned_inside(&id("Toda")).unwrap(),
            "só a herança"
        );

        store.unpin_all().unwrap();
        assert!(!store.has_pinned_inside(&id("Pasta")).unwrap());
        assert!(store.pinned_children(None).unwrap().is_empty());
        assert_eq!(store.pinned_bytes().unwrap(), 0);
    }

    #[test]
    fn search_names_ignores_case_and_accents() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        pinned_tree(&store);
        store
            .upsert_items(&[
                item("Relatório Anual.pdf", Some("Pasta")),
                item("100%_pronto.txt", Some("Pasta")),
                Item {
                    trashed: true,
                    ..item("relatorio velho.pdf", Some("Pasta"))
                },
            ])
            .unwrap();
        let found = |q: &str| -> Vec<String> {
            let items = store.search_names(q, 50).unwrap();
            items.into_iter().map(|i| i.local_name).collect()
        };

        assert_eq!(found("RELATORIO"), ["Relatório Anual.pdf"]);
        assert_eq!(found("relatório"), ["Relatório Anual.pdf"]);
        assert_eq!(found("%"), ["100%_pronto.txt"], "% é texto, não curinga");
        assert_eq!(found("_p"), ["100%_pronto.txt"]);
        assert!(found("root").is_empty(), "a raiz não é resultado");
        assert_eq!(store.search_names("o", 2).unwrap().len(), 2);
    }

    #[test]
    fn recent_lists_last_opened_files() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        pinned_tree(&store);
        store.touch_access(&id("solto.txt"), 1).unwrap();
        store.touch_access(&id("dentro.txt"), 3).unwrap();
        store.touch_access(&id("fixo.txt"), 2).unwrap();
        store.touch_access(&id("Pasta"), 4).unwrap();

        let recent: Vec<_> = store
            .recent(2)
            .unwrap()
            .into_iter()
            .map(|i| i.local_name)
            .collect();
        assert_eq!(recent, ["dentro.txt", "fixo.txt"]);
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
    fn listing_children_uses_the_parent_index_without_sorting_in_memory() {
        let tmp = tempfile::tempdir().unwrap();
        let store = Store::open(&tmp.path().join("db.sqlite")).unwrap();
        let items: Vec<_> = (0..10_000)
            .map(|n| item(&format!("f{n:05}"), Some("root")))
            .collect();
        store.upsert_items(&items).unwrap();
        assert_eq!(store.children(&id("root")).unwrap().len(), 10_000);
        // Desempenho pelo plano da consulta, não pelo relógio: o tempo varia com a carga da máquina.
        let sql = format!(
            "EXPLAIN QUERY PLAN {}",
            select_items!("WHERE parent = ?1 AND trashed = 0 ORDER BY local_name")
        );
        let conn = store.conn();
        let mut stmt = conn.prepare(&sql).unwrap();
        let plan: Vec<String> = stmt
            .query_map(["root"], |r| r.get(3))
            .unwrap()
            .collect::<rusqlite::Result<_>>()
            .unwrap();
        let plan = plan.join(" | ");
        assert!(plan.contains("items_parent_name"), "plano: {plan}");
        assert!(!plan.contains("TEMP B-TREE"), "plano: {plan}");
    }
}
