//! Banco da conta estragado: o banco vai para `recuperacao/<data>/` (não é apagado), o
//! conteúdo das edições que não subiram vai para `Lost and Found/Recuperados <data>/` e a conta
//! começa um banco novo, que se reindexa a partir do Drive. Nada é reenviado sozinho.

use std::path::{Path, PathBuf};

use gdl_common::paths::create_private_dir;
use gdl_common::time::local_stamp;
use gdl_store::{Store, StoreError, salvage};

use crate::Result;

pub const RECOVERY_DIR: &str = "recuperacao";
const DB_FILE: &str = "metadata.db";

/// Abre o banco de `data_dir`; estragado, recupera o que der (copiando o conteúdo de
/// `content_dir` para `lost_and_found`) e abre um novo. Devolve também quantos itens foram
/// recuperados.
pub(crate) fn open_store(
    data_dir: &Path,
    content_dir: &Path,
    lost_and_found: &Path,
    now: i64,
) -> Result<(Store, usize)> {
    let db = data_dir.join(DB_FILE);
    match Store::open(&db) {
        Err(StoreError::Corrupt(why)) => {
            tracing::error!(motivo = %why, "banco da conta estragado; recuperando");
            let stamp = local_stamp(now);
            let recovered = keep_edits(
                &db,
                content_dir,
                &lost_and_found.join(format!("Recuperados {stamp}")),
            )?;
            set_aside(&db, &data_dir.join(RECOVERY_DIR).join(&stamp))?;
            // O conteúdo do cache era do banco antigo; o novo começa sem nada baixado.
            unless_missing(std::fs::remove_dir_all(content_dir))?;
            Ok((Store::open(&db)?, recovered))
        }
        opened => Ok((opened?, 0)),
    }
}

/// Copia para `dest` o conteúdo local de cada edição legível do banco `db`, pelo caminho (ou
/// pelo ID, sem caminho legível); devolve quantas foram copiadas.
fn keep_edits(db: &Path, content_dir: &Path, dest: &Path) -> Result<usize> {
    let mut copied = 0;
    for item in salvage::edited_items(db) {
        let src = content_dir.join(&item.id);
        if !src.is_file() {
            continue;
        }
        let target = dest.join(item.path.unwrap_or_else(|| PathBuf::from(&item.id)));
        if let Some(parent) = target.parent() {
            create_private_dir(parent)?;
        }
        std::fs::copy(&src, &target)?;
        copied += 1;
    }
    Ok(copied)
}

/// Move o banco e os arquivos do WAL para `dir`.
fn set_aside(db: &Path, dir: &Path) -> Result<()> {
    create_private_dir(dir)?;
    for suffix in ["", "-wal", "-shm"] {
        let name = format!("{DB_FILE}{suffix}");
        unless_missing(std::fs::rename(db.with_file_name(&name), dir.join(&name)))?;
    }
    Ok(())
}

/// O que não existia não conta como erro.
fn unless_missing(done: std::io::Result<()>) -> std::io::Result<()> {
    match done {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use gdl_common::config::load_preferences;
    use gdl_common::paths::Paths;
    use gdl_fake_drive::FakeDrive;
    use gdl_store::{ItemId, LocalState};

    use super::*;
    use crate::engine::Engine;
    use crate::events::EngineEvent;
    use crate::indexer::START_PAGE_TOKEN;
    use crate::testutil::fast_client;

    const ACCOUNT: &str = "teste@example.com";

    fn open(paths: &Paths, fake: &FakeDrive) -> Engine {
        let (prefs, _) = load_preferences(paths).unwrap();
        Engine::open(paths, &prefs, ACCOUNT, fast_client(fake, 3)).unwrap()
    }

    /// As subpastas de `dir`.
    fn subdirs(dir: &Path) -> Vec<PathBuf> {
        std::fs::read_dir(dir).map_or(vec![], |entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
    }

    #[tokio::test]
    async fn corrupt_database_keeps_pending_content() {
        let fake = FakeDrive::start().await;
        fake.add_file(&fake.root_id(), "do-drive.txt", b"nuvem".to_vec());
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let account = paths.data_dir.join(ACCOUNT);
        let engine = open(&paths, &fake);
        engine.ensure_indexed().await.unwrap();
        let note = engine
            .create_file(&ItemId(fake.root_id()), "nota.txt")
            .unwrap();
        std::fs::write(engine.cache().local_path(&note.id), "não subiu ainda").unwrap();
        engine
            .store()
            .set_local_state(&note.id, LocalState::Dirty)
            .unwrap();
        drop(engine);
        salvage::damage_table(&account.join(DB_FILE), "access");

        let engine = open(&paths, &fake);
        let mut events = engine.subscribe();

        let kept = subdirs(&account.join("lost_and_found"));
        assert_eq!(kept.len(), 1, "{kept:?}");
        let name = kept[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("Recuperados "), "{name}");
        assert_eq!(
            std::fs::read_to_string(kept[0].join("nota.txt")).unwrap(),
            "não subiu ainda"
        );
        let aside = subdirs(&account.join(RECOVERY_DIR));
        assert_eq!(aside.len(), 1, "o banco estragado fica guardado: {aside:?}");
        assert!(aside[0].join(DB_FILE).is_file());
        assert!(
            !engine.cache().local_path(&note.id).exists(),
            "o cache recomeça"
        );
        // Banco novo: nada da edição, e nada para subir sozinho.
        assert!(engine.store().get(&note.id).unwrap().is_none());
        assert!(engine.store().kv_get(START_PAGE_TOKEN).unwrap().is_none());
        engine.ensure_indexed().await.unwrap();
        assert!(engine.store().kv_get(START_PAGE_TOKEN).unwrap().is_some());

        let shutdown = tokio_util::sync::CancellationToken::new();
        let engine = std::sync::Arc::new(engine);
        let run = tokio::spawn({
            let (engine, shutdown) = (engine.clone(), shutdown.clone());
            async move { engine.run(shutdown).await }
        });
        let announced = tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if let Ok(EngineEvent::ItemsRecovered { count }) = events.recv().await {
                    return count;
                }
            }
        })
        .await
        .unwrap();
        assert_eq!(announced, 1);
        shutdown.cancel();
        run.await.unwrap();
    }

    #[tokio::test]
    async fn unreadable_file_is_set_aside_and_reindexed() {
        let fake = FakeDrive::start().await;
        fake.add_file(&fake.root_id(), "a.txt", b"oi".to_vec());
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let account = paths.data_dir.join(ACCOUNT);
        std::fs::create_dir_all(&account).unwrap();
        std::fs::write(account.join(DB_FILE), vec![0x5A; 4096]).unwrap();

        let engine = open(&paths, &fake);
        engine.ensure_indexed().await.unwrap();

        let aside = subdirs(&account.join(RECOVERY_DIR));
        assert_eq!(aside.len(), 1, "{aside:?}");
        assert_eq!(
            std::fs::read(aside[0].join(DB_FILE)).unwrap(),
            vec![0x5A; 4096]
        );
        assert!(subdirs(&account.join("lost_and_found")).is_empty());
        assert!(engine.store().kv_get(START_PAGE_TOKEN).unwrap().is_some());
        assert_eq!(engine.recovered, 0, "nada a avisar sem edições salvas");
    }

    #[tokio::test]
    async fn healthy_database_is_untouched() {
        let fake = FakeDrive::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let account = paths.data_dir.join(ACCOUNT);
        let engine = open(&paths, &fake);
        engine.ensure_indexed().await.unwrap();
        let token = engine.store().kv_get(START_PAGE_TOKEN).unwrap();
        drop(engine);

        let engine = open(&paths, &fake);

        assert!(!account.join(RECOVERY_DIR).exists());
        assert_eq!(engine.store().kv_get(START_PAGE_TOKEN).unwrap(), token);
        assert_eq!(engine.recovered, 0);
    }
}
