//! Banco da conta estragado: o banco vai para `recuperacao/<data>/` (não é apagado), o
//! conteúdo das edições que não subiram vai para `Lost and Found/Recuperados <data>/` e a conta
//! começa um banco novo, que se reindexa a partir do Drive. Nada é reenviado sozinho. Se o banco
//! não deu para ler inteiro, o cache também fica guardado, em `content-recuperacao-<data>/`.

use std::path::Path;

use gdl_common::names::{create_numbered, split_ext};
use gdl_common::paths::create_private_dir;
use gdl_common::time::local_stamp;
use gdl_store::salvage::{self, Salvaged};
use gdl_store::{Store, StoreError};

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
            let salvage = salvage::edited_items(&db);
            let recovered = keep_edits(
                &salvage.items,
                content_dir,
                &lost_and_found.join(format!("Recuperados {stamp}")),
            )?;
            set_aside(&db, &data_dir.join(RECOVERY_DIR).join(&stamp))?;
            // O conteúdo do cache era do banco antigo; o novo começa sem nada baixado. Sem a
            // lista inteira das edições, o cache pode ter alguma que não subiu: fica guardado.
            if salvage.complete {
                unless_missing(std::fs::remove_dir_all(content_dir))?;
            } else {
                let name = content_dir
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy();
                let aside = content_dir.with_file_name(format!("{name}-recuperacao-{stamp}"));
                unless_missing(std::fs::rename(content_dir, &aside))?;
                tracing::error!(pasta = %aside.display(), "banco ilegível; o cache ficou guardado");
            }
            Ok((Store::open(&db)?, recovered))
        }
        opened => Ok((opened?, 0)),
    }
}

/// Copia para `dest` o conteúdo local de cada item de `items`, pelo caminho (ou pelo ID, sem
/// caminho legível), numerando nomes repetidos; devolve quantos foram copiados.
fn keep_edits(items: &[Salvaged], content_dir: &Path, dest: &Path) -> Result<usize> {
    let mut copied = 0;
    for item in items {
        // Um ID lido de banco estragado não pode levar para fora das pastas.
        let src = content_dir.join(&item.id);
        if !salvage::usable(&item.id) || !src.is_file() {
            continue;
        }
        let mut src = std::fs::File::open(src)?;
        let target = dest.join(item.path.as_deref().unwrap_or(Path::new(&item.id)));
        let parent = target.parent().unwrap_or(dest);
        create_private_dir(parent)?;
        let name = target.file_name().unwrap_or_default().to_string_lossy();
        let (stem, ext) = split_ext(&name);
        let (_, mut dst) = create_numbered(parent, stem, ext)?;
        std::io::copy(&mut src, &mut dst)?;
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
    use std::path::PathBuf;

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

    /// A pasta do cache guardada ao lado de `content` (o cache fica nos dados da conta, por
    /// padrão), quando o banco não deu para ler inteiro.
    fn content_aside(paths: &Paths) -> Vec<PathBuf> {
        subdirs(&paths.data_dir.join(ACCOUNT))
            .into_iter()
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("content-recuperacao-")
            })
            .collect()
    }

    /// Uma conta indexada com `nota.txt` editada que não subiu; o motor fica fechado.
    async fn unsent_note(paths: &Paths, fake: &FakeDrive) -> gdl_store::Item {
        let engine = open(paths, fake);
        engine.ensure_indexed().await.unwrap();
        let note = engine
            .create_file(&ItemId(fake.root_id()), "nota.txt")
            .unwrap();
        std::fs::write(engine.cache().local_path(&note.id), "não subiu ainda").unwrap();
        engine
            .store()
            .set_local_state(&note.id, LocalState::Dirty)
            .unwrap();
        note
    }

    #[tokio::test]
    async fn damaged_items_table_keeps_the_cache() {
        let fake = FakeDrive::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let note = unsent_note(&paths, &fake).await;
        salvage::damage_table(&paths.data_dir.join(ACCOUNT).join(DB_FILE), "items");

        let engine = open(&paths, &fake);

        let aside = content_aside(&paths);
        assert_eq!(
            aside.len(),
            1,
            "o cache não lido é guardado, não apagado: {aside:?}"
        );
        assert_eq!(
            std::fs::read_to_string(aside[0].join(&note.id.0)).unwrap(),
            "não subiu ainda"
        );
        assert!(!engine.cache().local_path(&note.id).exists());
    }

    #[tokio::test]
    async fn unreadable_file_keeps_the_cache() {
        let fake = FakeDrive::start().await;
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(tmp.path());
        let note = unsent_note(&paths, &fake).await;
        std::fs::write(paths.data_dir.join(ACCOUNT).join(DB_FILE), vec![0x5A; 4096]).unwrap();

        open(&paths, &fake);

        let aside = content_aside(&paths);
        assert_eq!(aside.len(), 1, "{aside:?}");
        assert!(aside[0].join(&note.id.0).is_file());
    }

    #[test]
    fn same_path_edits_are_both_kept_and_bad_ids_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let (content, dest) = (tmp.path().join("content"), tmp.path().join("dest"));
        std::fs::create_dir_all(&content).unwrap();
        for (id, text) in [("a", "primeiro"), ("b", "segundo")] {
            std::fs::write(content.join(id), text).unwrap();
        }
        std::fs::write(tmp.path().join("fora"), "não é do cache").unwrap();
        let item = |id: &str, path: Option<&str>| salvage::Salvaged {
            id: id.into(),
            path: path.map(PathBuf::from),
        };
        let items = [
            item("a", Some("pasta/nota.txt")),
            item("b", Some("pasta/nota.txt")),
            item("../fora", None),
        ];

        assert_eq!(keep_edits(&items, &content, &dest).unwrap(), 2);

        let mut kept: Vec<String> = std::fs::read_dir(dest.join("pasta"))
            .unwrap()
            .map(|e| std::fs::read_to_string(e.unwrap().path()).unwrap())
            .collect();
        kept.sort();
        assert_eq!(kept, ["primeiro", "segundo"]);
        assert_eq!(
            std::fs::read_dir(&dest).unwrap().count(),
            1,
            "nada sai da pasta"
        );
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
