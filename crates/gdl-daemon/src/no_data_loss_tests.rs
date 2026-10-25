//! Suíte "nunca perder dados": serviço completo, montagem real e Drive simulado com falhas.

use std::collections::BTreeSet;
use std::time::Duration;

use gdl_fake_drive::Fault;

use crate::testutil::{Harness, blocking, wait_until};

const MIB: usize = 1024 * 1024;
const FLAP_FILES: usize = 200;

fn flap_name(i: usize) -> String {
    format!("f{i:03}.txt")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kill_during_upload_then_restart_finishes_upload() {
    let mut h = Harness::new().await;
    h.logged_in().await;
    let data: Vec<u8> = (0..20 * MIB).map(|n| (n % 251) as u8).collect();
    // A transferência empaca no meio e o serviço para com o envio em andamento. A parada é a
    // normal (`restart`), mas o envio empacado não termina: o que fica é o que já estava gravado.
    h.fake
        .inject(Fault::StallConnectionAfterBytes(10 * MIB as u64));

    let path = h.drive().join("grande.bin");
    let written = data.clone();
    blocking(move || std::fs::write(path, written).unwrap()).await;
    let engine = h.daemon.engine().await.unwrap();
    wait_until("envio em andamento", || {
        let item = engine
            .store()
            .lookup(&gdl_store::ItemId(h.fake.root_id()), "grande.bin");
        item.unwrap()
            .is_some_and(|i| i.local_state == gdl_store::LocalState::Uploading)
    })
    .await;
    drop(engine);
    h.fake.clear_faults();

    h.restart(None).await;

    wait_until("envio concluído depois de reiniciar", || {
        h.fake.find_live("grande.bin").len() == 1
    })
    .await;
    let id = h.fake.find_live("grande.bin").remove(0);
    assert!(h.fake.content(&id) == data, "conteúdo íntegro");
    assert_eq!(
        h.fake
            .requests("/upload/drive/v3/files?uploadType=resumable"),
        1,
        "continuou a mesma sessão em vez de recomeçar"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn network_flaps_during_many_writes_all_arrive() {
    let h = Harness::new().await;
    h.logged_in().await;
    let dir = h.drive();
    let writes = tokio::task::spawn_blocking(move || {
        for i in 0..FLAP_FILES {
            std::fs::write(dir.join(flap_name(i)), format!("conteúdo {i}")).unwrap();
            std::thread::sleep(Duration::from_millis(5));
        }
    });
    // A rede cai e volta enquanto os arquivos são gravados e até metade deles ter subido.
    let arrived = || {
        (0..FLAP_FILES)
            .filter(|&i| !h.fake.find_live(&flap_name(i)).is_empty())
            .count()
    };
    let start = std::time::Instant::now();
    while !writes.is_finished() || arrived() < FLAP_FILES / 2 {
        assert!(start.elapsed() < Duration::from_secs(30), "envios parados");
        h.fake.inject(Fault::Offline);
        tokio::time::sleep(Duration::from_millis(200)).await;
        h.fake.clear_faults();
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    writes.await.unwrap();

    wait_until("os 200 arquivos no Drive, sem cópias", || {
        (0..FLAP_FILES).all(|i| h.fake.find_live(&flap_name(i)).len() == 1)
    })
    .await;
    for i in 0..FLAP_FILES {
        let id = h.fake.find_live(&flap_name(i)).remove(0);
        assert_eq!(h.fake.content(&id), format!("conteúdo {i}").into_bytes());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disk_full_never_corrupts_existing_cache() {
    let mut h = Harness::new().await;
    let cached: Vec<u8> = (0..MIB).map(|n| (n % 241) as u8).collect();
    h.fake
        .add_file(&h.fake.root_id(), "guardado.bin", cached.clone());
    h.fake.add_file(&h.fake.root_id(), "novo.bin", vec![7; MIB]);
    h.logged_in().await;
    let path = h.drive().join("guardado.bin");
    assert!(blocking(move || std::fs::read(path).unwrap()).await == cached);

    // Disco "cheio": o mínimo livre exigido passa do que qualquer disco tem.
    h.restart(Some("MinFreeDiskSpaceKBytes = 9000000000000"))
        .await;

    let path = h.drive().join("guardado.bin");
    assert!(
        blocking(move || std::fs::read(path).unwrap()).await == cached,
        "o que já estava no cache continua íntegro"
    );
    let path = h.drive().join("novo.bin");
    // O FUSE responde ENOSPC; pelo cache de páginas do kernel, o programa recebe EIO. O que
    // importa: falha em vez de entregar dados errados.
    assert!(blocking(move || std::fs::read(path)).await.is_err());
    wait_until("aviso de disco cheio", || {
        h.notes().iter().any(|n| n.summary.contains("Disco"))
    })
    .await;

    h.restart(Some("")).await;
    let path = h.drive().join("novo.bin");
    assert_eq!(
        blocking(move || std::fs::read(path).unwrap()).await,
        vec![7; MIB]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn conflict_storm_preserves_every_local_version() {
    let h = Harness::new().await;
    let ids: Vec<String> = (0..20)
        .map(|i| {
            h.fake
                .add_file(&h.fake.root_id(), &format!("c{i:02}.txt"), b"v1".to_vec())
        })
        .collect();
    h.logged_in().await;
    h.proxy.pause().await.unwrap();

    let dir = h.drive();
    blocking(move || {
        for i in 0..20 {
            std::fs::write(dir.join(format!("c{i:02}.txt")), format!("local {i}")).unwrap();
        }
    })
    .await;
    for (i, id) in ids.iter().enumerate() {
        h.fake.modify_remote(id, format!("nuvem {i}").into_bytes());
    }
    h.proxy.resume().await.unwrap();

    let engine = h.daemon.engine().await.unwrap();
    wait_until("as 20 edições locais no Lost and Found", || {
        engine.lost_and_found().list().len() == 20
    })
    .await;
    let saved: BTreeSet<String> = h
        .proxy
        .list_lost_and_found()
        .await
        .unwrap()
        .iter()
        .map(|p| std::fs::read_to_string(p).unwrap())
        .collect();
    let expected: BTreeSet<String> = (0..20).map(|i| format!("local {i}")).collect();
    assert_eq!(saved, expected);
    for (i, id) in ids.iter().enumerate() {
        assert_eq!(h.fake.content(id), format!("nuvem {i}").into_bytes());
    }
}
