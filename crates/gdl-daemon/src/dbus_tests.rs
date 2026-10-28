//! Testes da interface D-Bus do serviço, contra o Drive simulado e uma montagem real.

use gdl_drive::token_store::TokenStore;
use std::path::PathBuf;
use std::time::Duration;

use gdl_ipc::ServiceError;
use gdl_store::PinState;

use crate::testutil::{Harness, blocking, status_until, wait_until};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_over_dbus_emits_login_finished() {
    let h = Harness::new().await;

    let (ok, email, error) = h.login().await;

    assert!(ok, "{error}");
    assert_eq!(email, h.fake.email());
    assert!(h.tokens.load(&email).await.unwrap().is_some());
    let status = h.proxy.status("").await.unwrap();
    assert_eq!(status.account, email);
    assert!(h.drive().is_dir());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_rejects_account_outside_allowed_pattern() {
    let h = Harness::with_policy(r#"AllowedAccountsPattern = ".*@empresa\\.com""#).await;

    let (ok, _, error) = h.login().await;

    assert!(!ok);
    assert!(!error.is_empty());
    assert!(h.tokens.load(&h.fake.email()).await.unwrap().is_none());
    assert_eq!(h.proxy.status("").await.unwrap().account, "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_reports_pending_and_paused() {
    let h = Harness::new().await;
    h.logged_in().await;

    h.proxy.pause().await.unwrap();
    let path = h.drive().join("novo.txt");
    blocking(move || std::fs::write(path, b"pendente").unwrap()).await;

    let status = h.proxy.status("").await.unwrap();
    assert_eq!(status.pending_uploads, 1);
    assert!(status.paused);
    assert!(status.mount_point.ends_with("Google Drive"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pin_by_path_resolves_item() {
    let h = Harness::new().await;
    let folder = h.fake.add_folder(&h.fake.root_id(), "Fotos");
    h.fake.add_file(&folder, "praia.jpg", b"jpeg".to_vec());
    h.logged_in().await;

    let path = h.drive().join("Fotos");
    h.proxy.pin(path.to_str().unwrap()).await.unwrap();

    let engine = h.daemon.engine().await.unwrap();
    let item = engine
        .store()
        .get(&gdl_store::ItemId(folder))
        .unwrap()
        .unwrap();
    assert_eq!(item.pin, PinState::Pinned);
    let err = h.proxy.pin("/fora/da/montagem").await.unwrap_err();
    assert!(err.to_string().contains("NotFound"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_with_pending_requires_mode() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy.pause().await.unwrap();
    let path = h.drive().join("rascunho.txt");
    blocking(move || std::fs::write(path, b"ainda nao subiu").unwrap()).await;

    let err = h.proxy.logout("", "", "").await.unwrap_err();
    assert!(err.to_string().contains("PendingUploads"), "{err}");
    assert_eq!(h.proxy.status("").await.unwrap().account, h.fake.email());

    h.proxy.logout("", "discard", "").await.unwrap();
    assert_eq!(h.proxy.status("").await.unwrap().account, "");
    assert!(h.tokens.load(&h.fake.email()).await.unwrap().is_none());
    assert!(!h.drive().exists(), "a montagem saiu");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_removes_seeded_thumbnails() {
    let h = Harness::new().await;
    let folder = h.fake.add_folder(&h.fake.root_id(), "Fotos");
    h.fake
        .add_file_with_mime(&folder, "praia.jpg", "image/jpeg", vec![1; 10]);
    h.logged_in().await;
    let engine = h.daemon.engine().await.unwrap();
    engine
        .seed_thumbnails(&gdl_store::ItemId(folder), h.drive().join("Fotos"))
        .expect("nenhuma semeadura em andamento")
        .await
        .unwrap();

    // Os PNGs de três tamanhos, mais um de outro caminho que não é do serviço.
    let thumbs = h.paths().thumbnails_dir;
    let seeded = seeded_pngs(&thumbs);
    assert_eq!(seeded.len(), 3, "{seeded:?}");
    let other = thumbs.join("normal").join("de-outro-caminho.png");
    std::fs::write(&other, b"png de outro programa").unwrap();

    h.proxy.logout("", "discard", "").await.unwrap();

    for png in &seeded {
        assert!(!png.exists(), "{png:?} ficou depois de sair da conta");
    }
    assert!(
        other.exists(),
        "apagou uma miniatura que não era do serviço"
    );
}

/// Os PNGs gravados nos três tamanhos de miniatura.
fn seeded_pngs(thumbs: &std::path::Path) -> Vec<std::path::PathBuf> {
    ["normal", "large", "x-large"]
        .iter()
        .filter_map(|s| std::fs::read_dir(thumbs.join(s)).ok())
        .flatten()
        .map(|e| e.unwrap().path())
        .collect()
}

/// Quantos itens a pasta lista pela montagem (fora do runtime: o FUSE responde nele).
async fn listed(dir: std::path::PathBuf) -> usize {
    tokio::task::spawn_blocking(move || std::fs::read_dir(dir).unwrap().count())
        .await
        .unwrap()
}

/// A garantia das miniaturas: a primeira listagem de uma pasta de fotos pela montagem só
/// responde com as miniaturas gravadas (o Dolphin as acha sem baixar as fotos). Uma pasta de
/// código não espera nem chama o Google.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn first_listing_of_a_photo_folder_has_the_thumbnails() {
    let h = Harness::new().await;
    let photos = h.fake.add_folder(&h.fake.root_id(), "Camera");
    for name in ["a.jpg", "b.jpg"] {
        h.fake
            .add_file_with_mime(&photos, name, "image/jpeg", vec![1; 10]);
    }
    let code = h.fake.add_folder(&h.fake.root_id(), "src");
    for name in ["a.js", "b.js"] {
        h.fake.add_file(&code, name, b"x".to_vec());
    }
    h.logged_in().await;
    let lists = h.fake.requests("/drive/v3/files");
    assert_eq!(listed(h.drive().join("src")).await, 2);
    assert_eq!(
        h.fake.requests("/drive/v3/files"),
        lists,
        "a pasta de código chamou o Google"
    );

    assert_eq!(listed(h.drive().join("Camera")).await, 2);
    let seeded = seeded_pngs(&h.paths().thumbnails_dir).len();
    assert_eq!(seeded, 2 * 3, "a listagem respondeu antes das miniaturas");
    assert_eq!(h.fake.downloaded_bytes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn busy_mount_point_falls_back_to_numbered_folder() {
    let h = Harness::new().await;
    let busy = h.home.join("Google Drive");
    std::fs::create_dir_all(&busy).unwrap();
    std::fs::write(busy.join("meu.txt"), b"de antes").unwrap();

    h.logged_in().await;

    let status = h.proxy.status("").await.unwrap();
    assert!(
        status.mount_point.ends_with("Google Drive (1)"),
        "{status:?}"
    );
    assert_eq!(std::fs::read(busy.join("meu.txt")).unwrap(), b"de antes");
    let moved = h.notes();
    assert!(
        moved.iter().any(|n| n.body.contains("Google Drive (1)")),
        "{moved:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn notifications_are_sent_for_lost_and_found() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "contrato.txt", b"v1".to_vec());
    h.logged_in().await;

    let path = h.drive().join("contrato.txt");
    blocking(move || std::fs::write(path, b"editado aqui").unwrap()).await;
    h.fake.modify_remote(&id, b"editado na nuvem".to_vec());

    wait_until("aviso do Lost and Found", || {
        h.notes()
            .iter()
            .any(|n| n.actions.iter().any(|a| a == "open-lost-found"))
    })
    .await;
    let note = h
        .notes()
        .into_iter()
        .find(|n| n.actions.iter().any(|a| a == "open-lost-found"))
        .unwrap();
    assert_eq!(note.summary, "Edição guardada em Lost and Found");
    assert!(note.body.contains("contrato"), "{note:?}");
    assert!(
        note.actions.contains(&"Ver arquivos".to_string()),
        "{note:?}"
    );
    assert_eq!(h.proxy.list_lost_and_found("").await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_move_copies_pending_files_without_overwriting() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy.pause().await.unwrap();
    let path = h.drive().join("rascunho.txt");
    blocking(move || std::fs::write(path, b"ainda nao subiu").unwrap()).await;
    let saved = h.home.join("Salvos");
    std::fs::create_dir_all(&saved).unwrap();
    std::fs::write(saved.join("rascunho.txt"), b"de antes").unwrap();

    h.proxy
        .logout("", "move", saved.to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(
        std::fs::read(saved.join("rascunho.txt")).unwrap(),
        b"de antes"
    );
    assert_eq!(
        std::fs::read(saved.join("rascunho (1).txt")).unwrap(),
        b"ainda nao subiu"
    );
    assert_eq!(h.proxy.status("").await.unwrap().account, "");
}

/// Bug: com o armazenamento cheio, cada tentativa de cada arquivo abria um aviso novo.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn repeated_warnings_replace_the_previous_one() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.fake
        .inject(gdl_fake_drive::Fault::QuotaExceeded { times: 1000 });

    let dir = h.drive();
    blocking(move || {
        for name in ["a.txt", "b.txt"] {
            std::fs::write(dir.join(name), b"sem espaco").unwrap();
        }
    })
    .await;

    let quota = || -> Vec<_> {
        h.notes()
            .into_iter()
            .filter(|n| n.summary.contains("cheio"))
            .collect()
    };
    wait_until("dois avisos de armazenamento cheio", || quota().len() >= 2).await;
    let notes = quota();
    assert_eq!(notes[1].replaces_id, notes[0].id, "{notes:?}");
}

/// "Mover tudo para…": as cópias do Lost and Found vão para a pasta escolhida; dentro da montagem
/// ou do próprio Lost and Found, não.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn move_lost_found_refuses_inside_the_mount() {
    let h = Harness::new().await;
    h.logged_in().await;
    let engine = h.daemon.engine().await.unwrap();
    let src = h.home.join("edicao.txt");
    std::fs::write(&src, b"guardada").unwrap();
    engine.lost_and_found().preserve(&src, "edicao.txt").unwrap();
    let kept = engine.lost_and_found().dir().to_path_buf();
    drop(engine);
    let dest = h.home.join("escolhida");
    std::fs::create_dir(&dest).unwrap();

    let inside = h.drive().join("Meu Drive");
    let refused = h
        .proxy
        .move_lost_and_found("", inside.to_str().unwrap())
        .await
        .unwrap_err();
    let into_itself = h
        .proxy
        .move_lost_and_found("", kept.to_str().unwrap())
        .await
        .unwrap_err();
    let relative = h.proxy.move_lost_and_found("", "escolhida").await.unwrap_err();
    let moved = h
        .proxy
        .move_lost_and_found("", dest.to_str().unwrap())
        .await
        .unwrap();

    for err in [refused, into_itself, relative] {
        assert!(matches!(err, ServiceError::InvalidArgs(_)), "{err}");
    }
    assert_eq!(moved, 1);
    assert_eq!(std::fs::read_dir(&kept).unwrap().count(), 0);
    assert_eq!(std::fs::read_dir(&dest).unwrap().count(), 1);
}

/// Bug: sair da conta sem envios na fila apagava o Lost and Found, a única cópia das edições
/// preservadas por conflito.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_never_deletes_lost_and_found_silently() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "contrato.txt", b"v1".to_vec());
    h.logged_in().await;
    let path = h.drive().join("contrato.txt");
    blocking(move || std::fs::write(path, b"editado aqui").unwrap()).await;
    h.fake.modify_remote(&id, b"editado na nuvem".to_vec());
    let engine = h.daemon.engine().await.unwrap();
    wait_until("edição no Lost and Found", || {
        engine.lost_and_found().list().len() == 1
    })
    .await;
    drop(engine);
    let saved = h.proxy.list_lost_and_found("").await.unwrap();

    let err = h.proxy.logout("", "", "").await.unwrap_err();
    assert!(err.to_string().contains("PendingUploads"), "{err}");
    let copies = h.home.join("Copias");
    h.proxy
        .logout("", "move", copies.to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(h.proxy.status("").await.unwrap().account, "");
    assert_eq!(std::fs::read(&saved[0]).unwrap(), b"editado aqui");
    let copied = std::fs::read_dir(&copies).unwrap().count();
    assert_eq!(copied, 1, "o Lost and Found vai junto para a pasta");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_reports_state_and_syncing_count() {
    let h = Harness::new().await;
    assert_eq!(h.proxy.status("").await.unwrap().state, "signed-out");
    h.logged_in().await;
    status_until(&h, "sincronizado", |s| s.state == "synced").await;

    let path = h.drive().join("novo.txt");
    blocking(move || std::fs::write(path, b"para subir").unwrap()).await;

    // O envio só começa 2 s depois do fechamento: até lá, está na fila.
    let status = h.proxy.status("").await.unwrap();
    assert_eq!(status.state, "syncing", "{status:?}");
    assert_eq!(status.pending_uploads, 1);
    assert_eq!(status.syncing, 1);
    status_until(&h, "envio concluído", |s| s.state == "synced").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn account_info_has_name_quota_and_photo() {
    let h = Harness::new().await;
    h.logged_in().await;

    let info = h.proxy.account_info("").await.unwrap();

    assert_eq!(info.email, h.fake.email());
    assert_eq!(info.name, "Pessoa de Teste");
    assert_eq!(info.quota_total, 15 * 1024 * 1024 * 1024);
    assert!(
        info.photo_png.starts_with(b"\x89PNG"),
        "{:?}",
        &info.photo_png[..8.min(info.photo_png.len())]
    );
    // A foto fica guardada: a segunda consulta não baixa de novo.
    let photos = h.fake.requests("/photo.png");
    assert_eq!(photos, 1);
    h.proxy.account_info("").await.unwrap();
    assert_eq!(h.fake.requests("/photo.png"), photos);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn activity_over_dbus() {
    use futures_util::StreamExt;

    let h = Harness::new().await;
    h.logged_in().await;
    let mut changed = h.proxy.receive_activity_changed().await.unwrap();

    let path = h.drive().join("a.txt");
    blocking(move || std::fs::write(path, b"atividade").unwrap()).await;

    // Na fila antes de começar.
    let rows = h.proxy.activity("").await.unwrap();
    assert_eq!(rows[0].name, "a.txt");
    assert_eq!(rows[0].state, "upload-queued");
    tokio::time::timeout(std::time::Duration::from_secs(10), changed.next())
        .await
        .expect("ActivityChanged");
    status_until(&h, "envio concluído", |s| s.pending_uploads == 0).await;
    let rows = h.proxy.activity("").await.unwrap();
    let row = rows.iter().find(|r| r.name == "a.txt").unwrap();
    assert_eq!(row.state, "uploaded");
    assert_eq!(row.percent, 100);
    assert_eq!(row.size, 9);
    assert!(
        row.path.ends_with("Google Drive/Meu Drive/a.txt"),
        "{}",
        row.path
    );
    assert!(!row.item_id.starts_with("local:"), "{}", row.item_id);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn paused_queue_shows_as_paused() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy.pause().await.unwrap();
    let path = h.drive().join("parado.txt");
    blocking(move || std::fs::write(path, b"esperando").unwrap()).await;
    status_until(&h, "envio na fila", |s| s.pending_uploads > 0).await;

    // Pausado, o envio aparece como pausado, sem sumir da lista.
    let state = |rows: Vec<gdl_ipc::ActivityItem>| {
        rows.into_iter().find(|r| r.name == "parado.txt").map(|r| r.state)
    };
    let paused = state(h.proxy.activity("").await.unwrap());
    h.proxy.resume().await.unwrap();
    let resumed = state(h.proxy.activity("").await.unwrap());

    assert_eq!(paused.as_deref(), Some("upload-paused"));
    assert_ne!(resumed.as_deref(), Some("upload-paused"));
    assert!(resumed.is_some(), "a linha continua depois de retomar");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn upload_progress_reaches_activity() {
    const MIB: usize = 1024 * 1024;
    let h = Harness::new().await;
    h.logged_in().await;
    // O primeiro trecho (8 MiB) chega e o envio empaca no segundo.
    h.fake
        .inject(gdl_fake_drive::Fault::StallConnectionAfterBytes(
            10 * MIB as u64,
        ));

    let path = h.drive().join("grande.bin");
    blocking(move || std::fs::write(path, vec![7u8; 20 * MIB]).unwrap()).await;
    wait_until("envio empacado na metade", || h.fake.byte_limit_reached()).await;

    let start = std::time::Instant::now();
    loop {
        let rows = h.proxy.activity("").await.unwrap();
        let row = rows.iter().find(|r| r.name == "grande.bin").unwrap();
        if row.state == "uploading" && row.percent == 40 {
            break;
        }
        assert!(start.elapsed().as_secs() < 10, "{row:?}");
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_upload_over_dbus() {
    const MIB: usize = 1024 * 1024;
    let h = Harness::new().await;
    h.logged_in().await;
    h.fake
        .inject(gdl_fake_drive::Fault::StallConnectionAfterBytes(
            10 * MIB as u64,
        ));
    let path = h.drive().join("grande.bin");
    let file = path.clone();
    blocking(move || std::fs::write(file, vec![7u8; 20 * MIB]).unwrap()).await;
    wait_until("envio empacado na metade", || h.fake.byte_limit_reached()).await;
    let rows = h.proxy.activity("").await.unwrap();
    let id = rows
        .iter()
        .find(|r| r.name == "grande.bin")
        .unwrap()
        .item_id
        .clone();

    assert_eq!(h.proxy.cancel_upload("", &id).await.unwrap(), "canceled");

    assert!(!blocking(move || path.exists()).await);
    let rows = h.proxy.activity("").await.unwrap();
    assert_eq!(rows[0].name, "grande.bin");
    assert_eq!(rows[0].state, "upload-canceled");
    assert_eq!(h.proxy.cancel_upload("", &id).await.unwrap(), "too-late");
    let note = h.proxy.notifications().await.unwrap().remove(0);
    assert_eq!(
        (note.kind.as_str(), note.action_key.as_str()),
        ("upload-canceled", "open-canceled")
    );
    assert!(note.body.contains("grande.bin"), "{}", note.body);
    let engine = h.daemon.engine().await.unwrap();
    let canceled = engine
        .kept()
        .get(gdl_engine::kept::KeptFolder::CanceledUploads);
    assert_eq!(canceled.list().len(), 1);
    drop(engine);
    // A cópia cancelada só existe aqui: sair da conta não a apaga sem perguntar.
    let err = h.proxy.logout("", "", "").await.unwrap_err();
    assert!(err.to_string().contains("PendingUploads"), "{err}");
    let copies = h.home.join("Copias");
    h.proxy
        .logout("", "move", copies.to_str().unwrap())
        .await
        .unwrap();
    assert_eq!(std::fs::read_dir(&copies).unwrap().count(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn errors_over_dbus() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "contrato.txt", b"v1".to_vec());
    h.logged_in().await;
    // Uma edição em conflito (Lost and Found) e um envio barrado pela cota.
    let path = h.drive().join("contrato.txt");
    blocking(move || std::fs::write(path, b"editado aqui").unwrap()).await;
    h.fake.modify_remote(&id, b"editado na nuvem".to_vec());
    status_until(&h, "conflito guardado", |s| s.errors == 1).await;
    // A consulta de mudanças também recebe o erro: a cota fica cheia até o teste liberar.
    h.fake
        .inject(gdl_fake_drive::Fault::QuotaExceeded { times: 1_000 });
    let path = h.drive().join("novo.txt");
    blocking(move || std::fs::write(path, b"sem espaco").unwrap()).await;
    status_until(&h, "erro de cota", |s| s.errors == 2).await;

    let errors = h.proxy.errors("").await.unwrap();

    let categories: Vec<_> = errors.iter().map(|e| e.category.as_str()).collect();
    assert_eq!(categories, ["quota", "lost-and-found"]);
    assert_eq!(errors[0].name, "novo.txt");
    assert!(
        errors[0].path.ends_with("Google Drive/Meu Drive/novo.txt"),
        "{}",
        errors[0].path
    );
    assert!(
        errors[1]
            .path
            .contains("Google Drive/.Lost and Found/contrato (local "),
        "{}",
        errors[1].path
    );
    assert_eq!(errors[1].item_id, "");
    h.fake.clear_faults();
    h.proxy.retry("", &errors[0].item_id).await.unwrap();
    status_until(&h, "envio refeito", |s| {
        s.errors == 1 && s.pending_uploads == 0
    })
    .await;
    assert_eq!(h.proxy.errors("").await.unwrap().len(), 1);
}

/// Copia o ponteiro `Plano.gdoc` como `nome`.gdoc (vira atalho) e espera o atalho no Drive.
async fn copy_gdoc(h: &Harness, name: &str) {
    let (src, dst) = (
        h.drive().join("Plano.gdoc"),
        h.drive().join(format!("{name}.gdoc")),
    );
    blocking(move || std::fs::copy(src, dst).unwrap()).await;
    wait_until("atalho no Drive", || h.fake.find_live(name).len() == 1).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabled_kind_is_neither_shown_nor_listed() {
    let h = Harness::new().await;
    let root = h.fake.root_id();
    h.fake
        .add_gdoc(&root, "Plano", "application/vnd.google-apps.document");
    h.logged_in().await;

    h.proxy
        .set_notification_enabled("copied-as-shortcut", false)
        .await
        .unwrap();
    let settings = h.proxy.notification_settings().await.unwrap();
    let shortcut = settings
        .iter()
        .find(|s| s.kind == "copied-as-shortcut")
        .unwrap();
    assert!(!shortcut.enabled);
    assert!(!shortcut.label.is_empty());
    copy_gdoc(&h, "Copia1").await;
    // O aviso é decidido quando o serviço trata o evento, não quando ele nasce: antes de religar,
    // um aviso ligado gerado depois serve de barreira (os eventos são tratados em ordem).
    h.proxy.pause().await.unwrap();
    let barrier = h.drive().join("barreira.txt");
    blocking(move || std::fs::write(barrier, b"x").unwrap()).await;
    wait_until("aviso da pausa", || {
        h.notes().iter().any(|n| n.body.contains("barreira.txt"))
    })
    .await;
    h.proxy.resume().await.unwrap();
    // Ligado de novo, o próximo aparece.
    h.proxy
        .set_notification_enabled("copied-as-shortcut", true)
        .await
        .unwrap();
    copy_gdoc(&h, "Copia2").await;
    wait_until("aviso do segundo atalho", || {
        h.notes().iter().any(|n| n.body.contains("Copia2"))
    })
    .await;

    assert!(h.notes().iter().all(|n| !n.body.contains("Copia1")));
    let listed = h.proxy.notifications().await.unwrap();
    let shortcuts: Vec<_> = listed
        .iter()
        .filter(|n| n.kind == "copied-as-shortcut")
        .collect();
    assert_eq!(shortcuts.len(), 1, "{listed:?}");
    assert!(shortcuts[0].body.contains("Copia2"));
    let prefs = std::fs::read_to_string(h.paths().config_dir.join("preferences.toml")).unwrap();
    assert!(prefs.contains("DoNotShowNotifications"), "{prefs}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn required_kinds_cannot_be_disabled() {
    let h = Harness::new().await;
    for kind in ["lost-and-found", "quota-full", "nao-existe"] {
        let err = h
            .proxy
            .set_notification_enabled(kind, false)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("InvalidArgs"), "{kind}: {err}");
    }
    let settings = h.proxy.notification_settings().await.unwrap();
    assert!(settings.iter().all(|s| s.kind != "lost-and-found"));
    assert!(settings.iter().all(|s| s.enabled));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn invoking_resume_action_resumes_sync() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy.pause().await.unwrap();

    for name in ["a.txt", "b.txt"] {
        let path = h.drive().join(name);
        blocking(move || std::fs::write(path, b"pausado").unwrap()).await;
    }
    wait_until("aviso para retomar", || {
        h.notes()
            .iter()
            .any(|n| n.actions.iter().any(|a| a == "resume"))
    })
    .await;

    let listed = h.proxy.notifications().await.unwrap();
    let resume: Vec<_> = listed
        .iter()
        .filter(|n| n.kind == "resume-syncing")
        .collect();
    assert_eq!(resume.len(), 1, "um aviso por pausa: {listed:?}");
    assert_eq!(resume[0].action_key, "resume");
    assert!(!resume[0].action_label.is_empty());
    assert!(!resume[0].read);
    h.proxy
        .invoke_notification_action(resume[0].id, "resume")
        .await
        .unwrap();

    assert!(!h.proxy.status("").await.unwrap().paused);
    let listed = h.proxy.notifications().await.unwrap();
    assert!(listed.iter().find(|n| n.id == resume[0].id).unwrap().read);
    let err = h
        .proxy
        .invoke_notification_action(resume[0].id, "open-lost-found")
        .await
        .unwrap_err();
    assert!(err.to_string().contains("InvalidArgs"), "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn notifications_unread_count() {
    use futures_util::StreamExt;

    let h = Harness::new().await;
    let root = h.fake.root_id();
    h.fake
        .add_gdoc(&root, "Plano", "application/vnd.google-apps.document");
    h.logged_in().await;
    let mut changed = h.proxy.receive_notifications_changed().await.unwrap();
    // Dois atalhos: dois avisos (sem chave, um não substitui o outro).
    for name in ["Cópia 1", "Cópia 2"] {
        copy_gdoc(&h, name).await;
    }
    wait_until("dois avisos de atalho", || h.notes().len() == 2).await;
    tokio::time::timeout(std::time::Duration::from_secs(10), changed.next())
        .await
        .expect("NotificationsChanged");
    let unread = |items: &[gdl_ipc::NotificationItem]| items.iter().filter(|n| !n.read).count();
    assert_eq!(unread(&h.proxy.notifications().await.unwrap()), 2);

    h.proxy.mark_notifications_read().await.unwrap();

    assert_eq!(unread(&h.proxy.notifications().await.unwrap()), 0);
    let first = h.proxy.notifications().await.unwrap()[1].id;
    h.proxy.dismiss_notification(first).await.unwrap();
    assert_eq!(h.proxy.notifications().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_with_me_lists_and_adds_shortcut() {
    let h = Harness::new().await;
    let shared = h
        .fake
        .add_shared_file("Orçamento.xlsx", "Ana Souza", b"planilha".to_vec());
    h.logged_in().await;

    let items = h.proxy.shared_with_me("").await.unwrap();
    assert_eq!(items.len(), 1, "{items:?}");
    assert_eq!(
        (items[0].id.as_str(), items[0].name.as_str()),
        (shared.as_str(), "Orçamento.xlsx")
    );
    assert_eq!(items[0].owner, "Ana Souza");
    assert!(items[0].shared_ms > 0);
    let path = h.drive().join("Orçamento.xlsx");
    let exists = move || {
        let path = path.clone();
        blocking(move || path.symlink_metadata().is_ok())
    };
    assert!(!exists().await);

    h.proxy.add_shortcut("", &shared, "").await.unwrap();

    assert!(exists().await, "o atalho aparece no Meu Drive");
    let err = h
        .proxy
        .add_shortcut("", "nao-existe", "")
        .await
        .unwrap_err();
    assert_eq!(err.short_name(), "NotFound", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_item_uses_mount_path() {
    let h = Harness::new().await;
    let root = h.fake.root_id();
    let folder = h.fake.add_folder(&root, "Pasta");
    let file = h.fake.add_file(&folder, "nota.txt", b"x".to_vec());
    let top = h.fake.add_file(&root, "solta.txt", b"y".to_vec());
    h.logged_in().await;
    let before = h.opened().len();

    for (id, target) in [
        (&file, "file"),
        (&file, "folder"),
        (&file, "web"),
        (&top, "folder"),
    ] {
        h.proxy.open_item("", id, target).await.unwrap();
    }

    wait_until("quatro aberturas", || h.opened().len() == before + 4).await;
    let mut opened = h.opened().split_off(before);
    opened.sort();
    let web = format!("https://drive.google.com/file/d/{file}/view");
    let drive = h.drive();
    let mut expected = vec![
        drive.join("Pasta/nota.txt").display().to_string(),
        drive.join("Pasta").display().to_string(),
        web.clone(),
        drive.display().to_string(),
    ];
    expected.sort();
    assert_eq!(opened, expected);
    assert_eq!(h.proxy.web_link("", &file).await.unwrap(), web);
    let err = h.proxy.open_item("", &file, "janela").await.unwrap_err();
    assert_eq!(err.short_name(), "InvalidArgs", "{err}");
    let err = h
        .proxy
        .open_item("", "nao-existe", "file")
        .await
        .unwrap_err();
    assert_eq!(err.short_name(), "NotFound", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offline_files_over_dbus() {
    let h = Harness::new().await;
    let root = h.fake.root_id();
    let folder = h.fake.add_folder(&root, "Pasta");
    let a = h.fake.add_file(&folder, "a.txt", b"12345".to_vec());
    h.fake.add_file(&folder, "b.txt", b"fora".to_vec());
    h.logged_in().await;
    let engine = h.daemon.engine().await.unwrap();

    let a_path = h.drive().join("Pasta/a.txt").display().to_string();
    h.proxy.pin(&a_path).await.unwrap();
    wait_until("a.txt baixado", || {
        engine.store().cached_bytes().unwrap() == 5
    })
    .await;

    let top = h.proxy.offline_items("", "").await.unwrap();
    assert_eq!(top.len(), 1, "{top:?}");
    assert_eq!(
        (
            top[0].id.as_str(),
            top[0].path.as_str(),
            top[0].pin.as_str()
        ),
        (a.as_str(), a_path.as_str(), "on")
    );
    assert_eq!((top[0].size, top[0].is_folder), (5, false));
    let in_root = h.proxy.offline_items("", &root).await.unwrap();
    assert_eq!(in_root.len(), 1, "{in_root:?}");
    assert_eq!(
        (in_root[0].name.as_str(), in_root[0].pin.as_str()),
        ("Pasta", "mixed")
    );
    assert!(in_root[0].is_folder);
    let summary = h.proxy.offline_summary("").await.unwrap();
    assert_eq!((summary.pinned_count, summary.pinned_bytes), (1, 5));
    assert_eq!(summary.cache_bytes, 5);
    assert!(summary.free_bytes > 0);
    assert_eq!(h.proxy.clear_cache("").await.unwrap(), 0, "off-line fica");

    h.proxy.unpin_item("", &a).await.unwrap();
    assert!(h.proxy.offline_items("", "").await.unwrap().is_empty());
    h.proxy.pin(&a_path).await.unwrap();
    h.proxy.clear_offline_files("").await.unwrap();
    assert!(h.proxy.offline_items("", "").await.unwrap().is_empty());
    assert_eq!(h.proxy.clear_cache("").await.unwrap(), 5);

    let found = h.proxy.search("", "A.TX").await.unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].path.as_str(), found[0].is_folder),
        (a_path.as_str(), false)
    );
    let folder_hit = h.proxy.search("", "past").await.unwrap();
    assert_eq!(folder_hit[0].mime, "application/vnd.google-apps.folder");
    let recent = h.proxy.recent_items("").await.unwrap();
    assert!(recent.iter().all(|r| !r.is_folder));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn managed_keys_are_reported_and_locked() {
    let h = Harness::with_policy("AutoStartOnLogin = false\n").await;

    let prefs = h.proxy.preferences().await.unwrap();
    let auto = prefs.iter().find(|p| p.key == "AutoStartOnLogin").unwrap();
    assert!(auto.managed);
    assert_eq!(auto.value, "false");
    let theme = prefs.iter().find(|p| p.key == "UITheme").unwrap();
    assert!(!theme.managed);
    assert_eq!(theme.value, "null");

    let err = h
        .proxy
        .set_preferences(&[("UITheme", "\"dark\""), ("AutoStartOnLogin", "true")])
        .await
        .unwrap_err();
    assert_eq!(err.short_name(), "Managed", "{err}");
    let theme = h.proxy.preferences().await.unwrap();
    let theme = theme.iter().find(|p| p.key == "UITheme").unwrap();
    assert_eq!(theme.value, "null", "nada gravado");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn restart_flag_only_for_restart_keys() {
    use futures_util::StreamExt;

    let h = Harness::new().await;
    let mut changed = h.proxy.receive_preferences_changed().await.unwrap();

    assert!(
        !h.proxy
            .set_preferences(&[("UITheme", "\"dark\"")])
            .await
            .unwrap()
    );
    tokio::time::timeout(std::time::Duration::from_secs(10), changed.next())
        .await
        .expect("PreferencesChanged");
    assert!(
        h.proxy
            .set_preferences(&[("ContentCacheMaxKbytes", "1000")])
            .await
            .unwrap()
    );
    let prefs = h.proxy.preferences().await.unwrap();
    let get = |key: &str| prefs.iter().find(|p| p.key == key).unwrap().clone();
    assert_eq!(get("UITheme").value, "\"dark\"");
    assert!(get("ContentCacheMaxKbytes").restart && !get("UITheme").restart);
    for bad in [
        ("BandwidthRxKBPS", "\"rápido\""),
        ("UITheme", "{"),
        ("Inexistente", "1"),
    ] {
        let err = h.proxy.set_preferences(&[bad]).await.unwrap_err();
        assert_eq!(err.short_name(), "InvalidArgs", "{bad:?}: {err}");
    }

    h.proxy.reset_preferences().await.unwrap();
    let prefs = h.proxy.preferences().await.unwrap();
    assert_eq!(
        prefs.iter().find(|p| p.key == "UITheme").unwrap().value,
        "null"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn autostart_preference_writes_the_session_file() {
    let h = Harness::new().await;
    let paths = h.paths();
    let file = paths
        .config_dir
        .parent()
        .unwrap()
        .join("autostart")
        .join(crate::desktop::AUTOSTART_FILE);
    assert!(file.exists(), "padrão: abre na sessão");

    h.proxy
        .set_preferences(&[("AutoStartOnLogin", "false")])
        .await
        .unwrap();
    assert!(!file.exists());
    h.proxy.reset_preferences().await.unwrap();
    assert!(file.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn quit_emits_quitting_and_ends_the_service() {
    use futures_util::StreamExt;

    let h = Harness::new().await;
    let mut quitting = h.proxy.receive_quitting().await.unwrap();

    h.proxy.quit().await.unwrap();

    tokio::time::timeout(std::time::Duration::from_secs(10), quitting.next())
        .await
        .expect("Quitting");
    let exit = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        h.daemon.exit_requested(),
    )
    .await
    .unwrap();
    assert_eq!(exit, crate::daemon::Exit::Quit);
    h.proxy.restart().await.unwrap();
    assert_eq!(
        h.daemon.exit_requested().await,
        crate::daemon::Exit::Restart
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cache_path_change_moves_cache_on_start() {
    let mut h = Harness::new().await;
    let root = h.fake.root_id();
    h.fake
        .add_file(&root, "fixo.txt", b"conteudo off-line".to_vec());
    h.logged_in().await;
    let path = h.drive().join("fixo.txt");
    h.proxy.pin(&path.display().to_string()).await.unwrap();
    let engine = h.daemon.engine().await.unwrap();
    wait_until("baixado", || engine.store().cached_bytes().unwrap() == 17).await;
    drop(engine);
    let downloaded = h.fake.downloaded_bytes();
    let new_cache = h.home.join("outro disco");
    let value = serde_json::to_string(&new_cache.display().to_string()).unwrap();

    assert!(
        h.proxy
            .set_preferences(&[("ContentCachePath", &value)])
            .await
            .unwrap()
    );
    h.restart(None).await;

    let read = blocking(move || std::fs::read(path).unwrap()).await;
    assert_eq!(read, b"conteudo off-line");
    assert_eq!(
        h.fake.downloaded_bytes(),
        downloaded,
        "nada baixado de novo"
    );
    let engine = h.daemon.engine().await.unwrap();
    assert!(engine.cache().dir().starts_with(&new_cache));
    let notes = h.proxy.notifications().await.unwrap();
    assert_eq!(notes[0].kind, "cache-moved", "{notes:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn metered_connection_pauses_and_resumes() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy
        .set_preferences(&[("PauseSyncingOnMeteredConnection", "true")])
        .await
        .unwrap();

    // 3: "provavelmente limitada", que também conta.
    h.set_metered(3).await;
    status_until(&h, "pausa na conexão limitada", |s| s.paused).await;
    h.set_metered(2).await;
    status_until(&h, "retoma ao sair dela", |s| !s.paused).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn metered_pause_does_not_undo_user_pause() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy
        .set_preferences(&[("PauseSyncingOnMeteredConnection", "true")])
        .await
        .unwrap();

    h.proxy.pause().await.unwrap();
    let engine = h.daemon.engine().await.unwrap();
    h.set_metered(1).await;
    wait_until("conexão vista como limitada", || h.daemon.is_metered()).await;
    h.set_metered(2).await;
    wait_until("conexão vista como não limitada", || {
        !h.daemon.is_metered()
    })
    .await;
    assert!(engine.is_paused(), "a pausa do usuário continua");

    // Sem a preferência, a conexão limitada não pausa.
    h.proxy.resume().await.unwrap();
    h.proxy
        .set_preferences(&[("PauseSyncingOnMeteredConnection", "false")])
        .await
        .unwrap();
    h.set_metered(1).await;
    wait_until("conexão vista como limitada", || h.daemon.is_metered()).await;
    assert!(!engine.is_paused());
}

/// `null` apaga a chave do usuário: volta a valer o padrão (a banda vazia é sem limite).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn null_value_removes_user_preference() {
    let h = Harness::new().await;
    h.proxy
        .set_preferences(&[("BandwidthRxKBPS", "100"), ("UITheme", "\"light\"")])
        .await
        .unwrap();

    h.proxy
        .set_preferences(&[("BandwidthRxKBPS", "null")])
        .await
        .unwrap();

    let prefs = h.proxy.preferences().await.unwrap();
    let get = |key: &str| prefs.iter().find(|p| p.key == key).unwrap().value.clone();
    assert_eq!(get("BandwidthRxKBPS"), "null");
    assert_eq!(get("UITheme"), "\"light\"", "as outras ficam");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uninstall_refuses_to_purge_with_account_connected() {
    let h = Harness::new().await;
    h.logged_in().await;

    let err = h.proxy.uninstall(true).await.unwrap_err();

    assert!(err.to_string().contains("Failed"), "{err}");
    assert_eq!(h.proxy.status("").await.unwrap().account, h.fake.email());
    assert!(h.drive().is_dir());
}

/// Sem apagar os dados: desmonta, tira o início automático e encerra; a conta continua guardada
/// para uma reinstalação.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uninstall_unmounts_and_keeps_the_account() {
    let h = Harness::new().await;
    // Numa sessão do KDE, a montagem sai do indexador ao montar e volta ao desinstalar.
    let config_home = h.paths().config_home.clone();
    std::fs::create_dir_all(&config_home).unwrap();
    std::fs::write(config_home.join("kdeglobals"), "").unwrap();
    let baloo = || std::fs::read_to_string(config_home.join("baloofilerc")).unwrap_or_default();
    h.logged_in().await;
    let mount = format!("{}/", h.home.join("Google Drive").display());
    assert!(baloo().contains(&mount), "{}", baloo());
    let autostart = h
        .paths()
        .config_home
        .join("autostart")
        .join(crate::desktop::AUTOSTART_FILE);
    crate::desktop::set_autostart(
        &h.paths().config_home,
        std::path::Path::new("/usr/bin"),
        true,
    )
    .unwrap();

    h.proxy.uninstall(false).await.unwrap();

    assert!(!h.drive().exists(), "a montagem ficou");
    assert!(!h.home.join("Google Drive").exists(), "a pasta vazia ficou");
    assert!(!autostart.exists(), "o início automático ficou");
    assert!(!baloo().contains(&mount), "a exclusão do Baloo ficou");
    assert!(h.tokens.load(&h.fake.email()).await.unwrap().is_some());
    assert!(h.paths().config_dir.exists() || h.paths().data_dir.exists());
    let exit = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        h.daemon.exit_requested(),
    )
    .await
    .unwrap();
    assert_eq!(exit, crate::daemon::Exit::Quit);
}

/// Apagando os dados: some a configuração, o estado e os dados, menos as cópias guardadas.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uninstall_purge_keeps_only_the_kept_copies() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy.logout("", "discard", "").await.unwrap();
    let paths = h.paths();
    let account = paths.data_dir.join(h.fake.email());
    let kept = account.join("lost_and_found").join("edição.txt");
    std::fs::create_dir_all(kept.parent().unwrap()).unwrap();
    std::fs::write(&kept, b"a unica copia").unwrap();
    std::fs::create_dir_all(account.join("canceled_uploads")).unwrap();
    std::fs::write(account.join("metadata.db"), b"").unwrap();
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(paths.config_dir.join("preferences.toml"), b"").unwrap();
    std::fs::create_dir_all(&paths.state_dir).unwrap();

    h.proxy.uninstall(true).await.unwrap();

    assert_eq!(std::fs::read(&kept).unwrap(), b"a unica copia");
    assert!(!account.join("metadata.db").exists());
    assert!(
        !account.join("canceled_uploads").exists(),
        "pasta vazia ficou"
    );
    assert!(!paths.config_dir.exists());
    assert!(!paths.state_dir.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn states_changed_names_the_items_whose_state_changed() {
    use futures_util::StreamExt;

    let h = Harness::new().await;
    let folder = h.fake.add_folder(&h.fake.root_id(), "Pasta");
    h.fake.add_file(&folder, "a.txt", b"a".to_vec());
    h.logged_in().await;
    let mut changed = h.proxy.receive_states_changed().await.unwrap();
    let a = h.drive().join("Pasta/a.txt").display().to_string();

    h.proxy.pin(&a).await.unwrap();

    let signal = tokio::time::timeout(std::time::Duration::from_secs(10), changed.next())
        .await
        .expect("StatesChanged")
        .unwrap();
    assert_eq!(signal.args().unwrap().paths, [a]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn menu_shows_only_inside_my_drive_with_offline_marks() {
    let h = Harness::new().await;
    let folder = h.fake.add_folder(&h.fake.root_id(), "Pasta");
    h.fake.add_file(&folder, "a.txt", b"a".to_vec());
    h.logged_in().await;
    let path = |p: &str| h.drive().join(p).display().to_string();
    h.proxy.pin(&path("Pasta/a.txt")).await.unwrap();

    let ids = |menu: &[gdl_ipc::MenuEntry]| menu.iter().map(|e| e.id.clone()).collect::<Vec<_>>();
    let state = |menu: &[gdl_ipc::MenuEntry], id: &str| {
        menu.iter().find(|e| e.id == id).unwrap().state.clone()
    };
    let folder_menu = h.proxy.menu(&[&path("Pasta")]).await.unwrap();
    assert!(
        ids(&folder_menu).contains(&"new-document".into()),
        "{folder_menu:?}"
    );
    assert_eq!(state(&folder_menu, "offline"), "mixed");
    let offline = folder_menu.iter().find(|e| e.id == "offline").unwrap();
    assert_eq!(offline.label, "Disponível off-line");
    assert!(
        folder_menu
            .iter()
            .all(|e| (e.id == "-") == e.label.is_empty())
    );

    let file_menu = h.proxy.menu(&[&path("Pasta/a.txt")]).await.unwrap();
    assert_eq!(
        ids(&file_menu)[..5],
        ["open-web", "share", "copy-link", "add-shortcut", "versions"]
    );
    assert_eq!(state(&file_menu, "offline"), "on");
    assert_eq!(state(&file_menu, "online-only"), "off");

    let outside = h
        .proxy
        .menu(&[&path("Pasta"), "/fora/do/drive"])
        .await
        .unwrap();
    assert!(outside.is_empty(), "um item de fora esconde o submenu");
    assert!(h.proxy.menu(&[]).await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn menu_actions_link_pin_create_refresh_and_trash() {
    let h = Harness::new().await;
    let root = h.fake.root_id();
    let folder = h.fake.add_folder(&root, "Pasta");
    let file = h.fake.add_file(&folder, "a.txt", b"a".to_vec());
    h.logged_in().await;
    let engine = h.daemon.engine().await.unwrap();
    let pin_of = |id: &str| {
        let item = engine.store().get(&gdl_store::ItemId(id.into())).unwrap();
        item.unwrap().pin
    };
    let a = h.drive().join("Pasta/a.txt").display().to_string();
    let pasta = h.drive().join("Pasta").display().to_string();
    let before = h.opened().len();

    let link = h.proxy.menu_action("copy-link", &[&a]).await.unwrap();
    assert_eq!(link, format!("https://drive.google.com/file/d/{file}/view"));
    h.proxy.menu_action("offline", &[&a, &pasta]).await.unwrap();
    assert_eq!(
        (pin_of(&file), pin_of(&folder)),
        (PinState::Pinned, PinState::Pinned)
    );
    h.proxy.menu_action("online-only", &[&pasta]).await.unwrap();
    assert_eq!(pin_of(&folder), PinState::None);

    h.proxy
        .menu_action("new-spreadsheet", &[&pasta])
        .await
        .unwrap();
    let children = engine
        .store()
        .children(&gdl_store::ItemId(folder.clone()))
        .unwrap();
    let sheet = children
        .iter()
        .find(|i| {
            i.kind
                == gdl_store::ItemKind::GoogleDoc {
                    mime: "application/vnd.google-apps.spreadsheet".into(),
                }
        })
        .expect("a planilha nova aparece na pasta");
    assert!(
        sheet.local_name.starts_with("Planilha sem título"),
        "{}",
        sheet.local_name
    );

    h.fake.add_file(&folder, "remoto.txt", b"r".to_vec());
    h.proxy.menu_action("refresh", &[&pasta]).await.unwrap();
    let names: Vec<_> = engine
        .store()
        .children(&gdl_store::ItemId(folder.clone()))
        .unwrap()
        .into_iter()
        .map(|i| i.local_name)
        .collect();
    assert!(names.contains(&"remoto.txt".into()), "{names:?}");

    h.proxy.menu_action("open-web", &[&pasta]).await.unwrap();
    h.proxy.menu_action("trash", &[]).await.unwrap();
    wait_until("três aberturas", || h.opened().len() == before + 3).await;
    let opened = h.opened().split_off(before);
    assert!(opened.contains(&format!("https://docs.google.com/open?id={}", sheet.id.0)));
    assert!(opened.contains(&format!("https://drive.google.com/drive/folders/{folder}")));
    assert!(opened.contains(&gdl_common::links::DRIVE_TRASH.into()));

    for (action, paths) in [
        ("copy-link", vec![a.as_str(), pasta.as_str()]),
        ("new-document", vec![a.as_str()]),
        ("refresh", vec![a.as_str()]),
        ("apagar-tudo", vec![a.as_str()]),
    ] {
        let err = h.proxy.menu_action(action, &paths).await.unwrap_err();
        assert_eq!(err.short_name(), "InvalidArgs", "{action}: {err}");
    }
    let err = h
        .proxy
        .menu_action("offline", &["/fora"])
        .await
        .unwrap_err();
    assert_eq!(err.short_name(), "NotFound", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn menu_opens_share_versions_shortcut_and_office_editor() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "Contas.xlsx", b"x".to_vec());
    h.logged_in().await;
    let path = h.drive().join("Contas.xlsx").display().to_string();
    let menu = h.proxy.menu(&[&path]).await.unwrap();
    let ids: Vec<_> = menu.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids[..2], ["open-web", "open-sheets"]);
    let before = h.opened().len();

    for action in ["share", "versions", "add-shortcut", "open-sheets"] {
        h.proxy.menu_action(action, &[&path]).await.unwrap();
    }
    wait_until("quatro aberturas", || h.opened().len() == before + 4).await;
    let opened = h.opened().split_off(before);
    for wanted in [
        format!("--open share:{}/{id}", h.fake.email()),
        format!("--open versions:{}/{id}", h.fake.email()),
        format!("--open shortcut:{}/{id}", h.fake.email()),
        format!("https://docs.google.com/spreadsheets/d/{id}/edit?rtpof=true"),
    ] {
        assert!(opened.contains(&wanted), "{wanted} em {opened:?}");
    }
    let err = h
        .proxy
        .menu_action("open-docs", &[&path])
        .await
        .unwrap_err();
    assert_eq!(err.short_name(), "InvalidArgs", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copying_a_link_says_who_can_open_it() {
    let h = Harness::new().await;
    let id = h.fake.add_file(&h.fake.root_id(), "a.txt", b"x".to_vec());
    h.logged_in().await;
    let path = h.drive().join("a.txt").display().to_string();
    // O aviso sai depois da cópia: espera o corpo dele conter `wanted`.
    let proxy = &h.proxy;
    let note_with = |wanted: &'static str| async move {
        for _ in 0..200 {
            let notes = proxy.notifications().await.unwrap();
            let note = notes.into_iter().find(|n| n.kind == "link-copied");
            if let Some(note) = note.filter(|n| n.body.contains(wanted)) {
                return note.body;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        panic!("sem aviso de link copiado com {wanted:?}");
    };

    h.proxy.menu_action("copy-link", &[&path]).await.unwrap();
    assert_eq!(
        note_with("acesso").await,
        "Só as pessoas com acesso podem abrir este link."
    );

    h.proxy
        .share_set_general("", &id, "anyone", "commenter")
        .await
        .unwrap();
    h.proxy.menu_action("copy-link", &[&path]).await.unwrap();
    let text = note_with("comentar").await;
    assert!(
        text.starts_with("Qualquer pessoa com o link pode"),
        "{text}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn share_window_adds_people_and_changes_general_access() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "Plano.txt", b"x".to_vec());
    h.logged_in().await;

    let before = h.proxy.sharing("", &id).await.unwrap();
    assert_eq!(before.name, "Plano.txt");
    assert!(before.can_share);
    assert_eq!(
        (before.general.as_str(), before.domain.as_str()),
        ("restricted", "example.com"),
        "conta de domínio (a do Drive simulado)"
    );
    assert_eq!(before.people.len(), 1);
    assert_eq!(before.people[0].role, "owner");

    h.proxy
        .share_add(
            "",
            &id,
            &[" ana@example.com ", "bia@example.com"],
            "commenter",
            true,
            "Veja",
        )
        .await
        .unwrap();
    h.proxy
        .share_set_general("", &id, "anyone", "reader")
        .await
        .unwrap();
    h.proxy
        .share_set_general("", &id, "anyone", "writer")
        .await
        .unwrap();

    let after = h.proxy.sharing("", &id).await.unwrap();
    let people: Vec<_> = after
        .people
        .iter()
        .map(|p| (p.email.as_str(), p.role.as_str()))
        .collect();
    assert_eq!(
        people[1..],
        [
            ("ana@example.com", "commenter"),
            ("bia@example.com", "commenter")
        ]
    );
    assert_eq!(
        (after.general.as_str(), after.general_role.as_str()),
        ("anyone", "writer")
    );
    assert_eq!(h.fake.share_emails().len(), 2, "um aviso por pessoa");

    h.proxy
        .share_set_general("", &id, "domain", "reader")
        .await
        .unwrap();
    let domain = h.proxy.sharing("", &id).await.unwrap();
    assert_eq!(
        (domain.general.as_str(), domain.general_role.as_str()),
        ("domain", "reader")
    );
    let general =
        |list: Vec<serde_json::Value>| list.iter().filter(|p| p["type"] != "user").count();
    assert_eq!(
        general(h.fake.permissions(&id)),
        1,
        "o de domínio substitui o do link"
    );

    h.proxy
        .share_set_role("", &id, &after.people[1].id, "")
        .await
        .unwrap();
    h.proxy
        .share_set_general("", &id, "restricted", "")
        .await
        .unwrap();
    let last = h.proxy.sharing("", &id).await.unwrap();
    assert_eq!(last.people.len(), 2, "tirou a Ana");
    assert_eq!(last.general, "restricted");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn share_window_refuses_bad_input() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "Plano.txt", b"x".to_vec());
    h.logged_in().await;

    let name = |e: gdl_ipc::ServiceError| e.short_name();
    let bad_email = h
        .proxy
        .share_add("", &id, &["sem-arroba"], "reader", false, "")
        .await;
    assert_eq!(name(bad_email.unwrap_err()), "InvalidArgs");
    let owner = h
        .proxy
        .share_add("", &id, &["a@example.com"], "owner", false, "")
        .await;
    assert_eq!(
        name(owner.unwrap_err()),
        "InvalidArgs",
        "dono não se dá por aqui"
    );
    let kind = h.proxy.share_set_general("", &id, "todos", "reader").await;
    assert_eq!(name(kind.unwrap_err()), "InvalidArgs");
    assert_eq!(h.fake.permissions(&id).len(), 1, "nada mudou");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn versions_window_downloads_keeps_and_restores() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "notas.txt", b"primeira".to_vec());
    h.fake.modify_remote(&id, b"segunda".to_vec());
    h.logged_in().await;

    let v = h.proxy.versions("", &id).await.unwrap();
    assert_eq!(v.name, "notas.txt");
    let sizes: Vec<_> = v.versions.iter().map(|x| (x.size, x.current)).collect();
    assert_eq!(sizes, [(7, true), (8, false)]);
    let old = &v.versions[1];

    let saved = h.proxy.version_download("", &id, &old.id).await.unwrap();
    let saved = std::path::PathBuf::from(saved);
    assert_eq!(saved.parent().unwrap(), h.home.join("Downloads"));
    let file_name = saved.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        file_name.starts_with("notas (") && file_name.ends_with(").txt"),
        "{file_name}"
    );
    assert_eq!(std::fs::read(&saved).unwrap(), b"primeira");
    let again = h.proxy.version_download("", &id, &old.id).await.unwrap();
    assert_ne!(again, saved.to_string_lossy(), "não sobrescreve");

    h.proxy.version_keep("", &id, &old.id, true).await.unwrap();
    assert!(h.proxy.versions("", &id).await.unwrap().versions[1].keep_forever);

    h.proxy.version_restore("", &id, &old.id).await.unwrap();
    let content = h.drive().join("notas.txt");
    assert_eq!(
        blocking(move || std::fs::read(content).unwrap()).await,
        b"primeira"
    );
    wait_until("a versão restaurada subir", || {
        h.fake.content(&id) == b"primeira"
    })
    .await;
    assert_eq!(h.proxy.versions("", &id).await.unwrap().versions.len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn current_account_file_is_migrated() {
    let mut h = Harness::new().await;
    h.logged_in().await;
    let data_dir = h.paths().data_dir;
    let email = std::fs::read_to_string(data_dir.join("accounts")).unwrap();
    // Estado da 0.7: só o `current-account`.
    h.daemon.shutdown().await;
    std::fs::remove_file(data_dir.join("accounts")).unwrap();
    std::fs::write(data_dir.join("current-account"), &email).unwrap();

    h.restart(None).await;

    assert!(h.daemon.engine().await.is_some(), "a conta subiu");
    assert_eq!(
        std::fs::read_to_string(data_dir.join("accounts"))
            .unwrap()
            .trim(),
        email.trim()
    );
    assert!(!data_dir.join("current-account").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn second_account_mounts_beside_the_first() {
    let h = Harness::new().await;
    h.fake
        .add_file(&h.fake.root_id(), "da-primeira.txt", b"um".to_vec());
    let ana = h.fake.add_account("ana@example.com");
    ana.add_file(&ana.root_id(), "da-ana.txt", b"dois".to_vec());

    h.logged_in().await;
    h.logged_in_as("ana@example.com").await;

    let first = h.drive().join("da-primeira.txt");
    let second = h
        .mount_of("ana@example.com")
        .join("Meu Drive")
        .join("da-ana.txt");
    let (a, b) = blocking(move || (std::fs::read(first), std::fs::read(second))).await;
    assert_eq!(a.unwrap(), b"um");
    assert_eq!(b.unwrap(), b"dois");
    assert!(!h.drive().join("da-ana.txt").exists());
    assert_eq!(
        h.saved_accounts(),
        [h.fake.email(), "ana@example.com".into()]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_of_one_keeps_the_other_mounted() {
    let h = Harness::new().await;
    let ana = h.fake.add_account("ana@example.com");
    ana.add_file(&ana.root_id(), "da-ana.txt", b"dois".to_vec());
    h.logged_in().await;
    h.logged_in_as("ana@example.com").await;

    h.daemon.sign_out(&h.fake.email()).await.unwrap();

    let kept = h
        .mount_of("ana@example.com")
        .join("Meu Drive")
        .join("da-ana.txt");
    assert_eq!(
        blocking(move || std::fs::read(kept)).await.unwrap(),
        b"dois"
    );
    assert_eq!(h.saved_accounts(), ["ana@example.com"]);
    assert!(h.tokens.load(&h.fake.email()).await.unwrap().is_none());
    assert!(h.tokens.load("ana@example.com").await.unwrap().is_some());
    let first = h.home.join("Google Drive");
    assert!(blocking(move || std::fs::read_dir(first).unwrap().next().is_none()).await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn fifth_account_is_refused() {
    let h = Harness::new().await;
    h.logged_in().await;
    for n in 2..=4 {
        let email = format!("conta{n}@example.com");
        h.fake.add_account(&email);
        h.logged_in_as(&email).await;
    }
    h.fake.add_account("conta5@example.com");
    let opened = h.opened().len();

    let err = h
        .proxy
        .begin_login("id-de-teste", "segredo", "")
        .await
        .unwrap_err();

    assert_eq!(err.short_name(), "Refused", "{err}");
    assert_eq!(err.into_message(), "too-many-accounts");
    assert_eq!(h.opened().len(), opened, "o navegador não abriu");
    assert_eq!(h.saved_accounts().len(), 4);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_account_twice_is_refused() {
    let h = Harness::new().await;
    h.logged_in().await;
    let engine = h.daemon.engine().await.unwrap();

    let (ok, email, error) = h.login().await;

    assert!(!ok);
    assert_eq!(email, h.fake.email());
    assert_eq!(error, "already-connected");
    assert!(std::sync::Arc::ptr_eq(
        &engine,
        &h.daemon.engine().await.unwrap()
    ));
    assert_eq!(h.saved_accounts(), [h.fake.email()]);
    assert!(h.drive().is_dir());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_add_account_keeps_only_the_first() {
    let h = Harness::with_policy("DisableAddAccount = true").await;
    h.fake.add_account("ana@example.com");
    // Sem conta nenhuma, o primeiro login vale.
    h.logged_in().await;

    let err = h
        .proxy
        .begin_login("id-de-teste", "segredo", "")
        .await
        .unwrap_err();

    assert_eq!(err.short_name(), "Refused", "{err}");
    assert_eq!(err.into_message(), "add-account-disabled");
    assert_eq!(h.saved_accounts(), [h.fake.email()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn all_accounts_resume_after_restart() {
    let mut h = Harness::new().await;
    h.fake
        .add_file(&h.fake.root_id(), "da-primeira.txt", b"um".to_vec());
    for (n, name) in ["b", "c"].iter().enumerate() {
        let acct = h.fake.add_account(&format!("{name}@example.com"));
        acct.add_file(&acct.root_id(), &format!("arquivo-{n}.txt"), b"x".to_vec());
    }
    h.logged_in().await;
    h.logged_in_as("b@example.com").await;
    h.logged_in_as("c@example.com").await;
    let order = h.saved_accounts();

    h.restart(None).await;

    assert_eq!(h.saved_accounts(), order);
    let (first, b, c) = (
        h.drive().join("da-primeira.txt"),
        h.mount_of("b@example.com")
            .join("Meu Drive")
            .join("arquivo-0.txt"),
        h.mount_of("c@example.com")
            .join("Meu Drive")
            .join("arquivo-1.txt"),
    );
    let read = blocking(move || (std::fs::read(first), std::fs::read(b), std::fs::read(c))).await;
    assert_eq!(read.0.unwrap(), b"um");
    assert!(read.1.is_ok() && read.2.is_ok());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_lists_every_account() {
    let h = Harness::new().await;
    h.two_accounts("ana@example.com").await;
    let ana_mount = h.mount_of("ana@example.com").display().to_string();

    let all = h.proxy.status("").await.unwrap();
    let ana = h.proxy.status("ana@example.com").await.unwrap();

    assert_eq!(all.account, h.fake.email());
    assert_eq!(ana.account, "ana@example.com");
    assert_eq!(ana.mount_point, ana_mount);
    let listed: Vec<_> = all
        .accounts
        .iter()
        .map(|a| (a.email.as_str(), a.mount_point.as_str()))
        .collect();
    assert_eq!(
        listed,
        [
            (h.fake.email().as_str(), all.mount_point.as_str()),
            ("ana@example.com", ana_mount.as_str())
        ]
    );
    assert_eq!(ana.accounts, all.accounts);
    assert!(all.accounts.iter().all(|a| a.pending_uploads == 0));
    let err = h.proxy.status("ninguem@example.com").await.unwrap_err();
    assert_eq!(err.short_name(), "NotLoggedIn", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_state_is_the_worst_of_all_accounts() {
    let h = Harness::new().await;
    h.two_accounts("ana@example.com").await;
    h.proxy.pause().await.unwrap();

    let all = h.proxy.status("").await.unwrap();

    assert_eq!(all.state, "paused");
    assert!(all.accounts.iter().all(|a| a.state == "paused"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn id_methods_act_on_the_requested_account() {
    let h = Harness::new().await;
    h.fake
        .add_file(&h.fake.root_id(), "da-primeira.txt", b"um".to_vec());
    let ana = h.fake.add_account("ana@example.com");
    ana.add_file(&ana.root_id(), "da-ana.txt", b"dois".to_vec());
    h.logged_in().await;
    h.logged_in_as("ana@example.com").await;

    let first = h.proxy.search("", "da-").await.unwrap();
    let second = h.proxy.search("ana@example.com", "da-").await.unwrap();
    let info = h.proxy.account_info("ana@example.com").await.unwrap();

    assert_eq!(first.len(), 1);
    assert_eq!(first[0].name, "da-primeira.txt");
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].name, "da-ana.txt");
    assert!(
        second[0]
            .path
            .starts_with(&h.mount_of("ana@example.com").display().to_string())
    );
    assert_eq!(info.email, "ana@example.com");
    // O id da conta de fora não vale na primeira.
    let err = h.proxy.web_link("", &second[0].id).await.unwrap_err();
    assert_eq!(err.short_name(), "NotFound", "{err}");
    assert!(
        h.proxy
            .web_link("ana@example.com", &second[0].id)
            .await
            .is_ok()
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn notification_action_acts_on_its_account() {
    let h = Harness::new().await;
    h.two_accounts("ana@example.com").await;
    h.proxy.pause().await.unwrap();

    let path = h
        .mount_of("ana@example.com")
        .join("Meu Drive")
        .join("a.txt");
    blocking(move || std::fs::write(path, b"pausado").unwrap()).await;
    wait_until("aviso da segunda conta", || {
        h.notes()
            .iter()
            .any(|n| n.actions.iter().any(|a| a == "resume"))
    })
    .await;
    let listed = h.proxy.notifications().await.unwrap();
    let note = listed.iter().find(|n| n.kind == "resume-syncing").unwrap();
    assert_eq!(note.account, "ana@example.com");
    // A área de trabalho diz a conta no texto; a janela a mostra pelo campo.
    assert!(!note.body.contains("ana@example.com"), "{note:?}");
    let shown = h.notes();
    let desktop = shown
        .iter()
        .find(|n| n.actions.iter().any(|a| a == "resume"));
    assert!(desktop.unwrap().body.contains("ana@example.com"));

    h.proxy
        .invoke_notification_action(note.id, "resume")
        .await
        .unwrap();

    assert!(!h.proxy.status("ana@example.com").await.unwrap().paused);
    assert!(h.proxy.status(&h.fake.email()).await.unwrap().paused);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn logout_all_signs_out_every_account() {
    let h = Harness::new().await;
    h.two_accounts("ana@example.com").await;

    h.proxy.logout_all("discard", "").await.unwrap();

    assert!(h.saved_accounts().is_empty());
    assert!(h.proxy.status("").await.unwrap().accounts.is_empty());
}

const ANA: &str = "ana@example.com";

/// Espera a conta `email` aparecer em `Status` com o estado `state`.
async fn account_state_until(h: &Harness, email: &str, state: &str) {
    status_until(h, &format!("{email} em {state}"), |s| {
        s.accounts
            .iter()
            .any(|a| a.email == email && a.state == state)
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expired_account_does_not_stop_the_others() {
    let h = Harness::new().await;
    h.two_accounts(ANA).await;

    h.fake.revoke(ANA);
    account_state_until(&h, ANA, "needs-sign-in").await;

    // O pior estado vale no agregado, e a outra conta segue como estava.
    let all = h.proxy.status("").await.unwrap();
    assert_eq!(all.state, "needs-sign-in");
    assert_eq!(
        h.proxy.status(&h.fake.email()).await.unwrap().state,
        "synced"
    );
    // A conta que venceu avisa, com o botão "Entrar".
    wait_until("aviso de conta sem login", || {
        h.notes()
            .iter()
            .any(|n| n.actions.contains(&"sign-in".into()))
    })
    .await;
    let note = h
        .notes()
        .into_iter()
        .find(|n| n.actions.contains(&"sign-in".into()))
        .unwrap();
    assert!(note.body.contains("ana"), "{}", note.body);
    // A primeira continua enviando.
    let path = h.drive().join("depois.txt");
    blocking(move || std::fs::write(path, b"ainda sobe"))
        .await
        .unwrap();
    wait_until("envio da primeira conta", || {
        !h.fake.find_live("depois.txt").is_empty()
    })
    .await;
    // A que venceu espera: nada vai para o Lost and Found nem para a lista de erros.
    let path = h.mount_of(ANA).join("Meu Drive").join("espera.txt");
    blocking(move || std::fs::write(path, b"na fila"))
        .await
        .unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(400)).await;
    let ana = h.proxy.status(ANA).await.unwrap();
    assert_eq!(ana.state, "needs-sign-in");
    assert_eq!(ana.pending_uploads, 1);
    let listed = h.proxy.status("").await.unwrap().accounts;
    let pending: Vec<_> = listed.iter().map(|a| a.pending_uploads).collect();
    assert_eq!(pending, [0, 1], "a lista traz os envios de cada conta");
    assert_eq!(ana.errors, 0);
    assert!(h.proxy.list_lost_and_found(ANA).await.unwrap().is_empty());
    // O botão "Entrar" pede à janela o login dessa conta.
    let listed = h.proxy.notifications().await.unwrap();
    let signed_out = listed
        .iter()
        .find(|n| n.kind == "account-signed-out")
        .unwrap();
    assert_eq!(signed_out.account, ANA);
    let before = h.opened().len();
    h.proxy
        .invoke_notification_action(signed_out.id, "sign-in")
        .await
        .unwrap();
    wait_until("janela no login", || {
        h.opened()[before..]
            .iter()
            .any(|o| o.contains("onboarding:resume:ana@example.com"))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn expired_account_stays_readable_from_the_cache() {
    let h = Harness::new().await;
    let ana = h.fake.add_account(ANA);
    ana.add_file(&ana.root_id(), "da-ana.txt", b"no cache".to_vec());
    h.logged_in().await;
    h.logged_in_as(ANA).await;
    let file = h.mount_of(ANA).join("Meu Drive").join("da-ana.txt");
    let again = file.clone();
    assert_eq!(
        blocking(move || std::fs::read(file)).await.unwrap(),
        b"no cache"
    );

    h.fake.revoke(ANA);
    account_state_until(&h, ANA, "needs-sign-in").await;

    assert_eq!(
        blocking(move || std::fs::read(again)).await.unwrap(),
        b"no cache"
    );
    assert!(h.mount_of(ANA).join("Meu Drive").is_dir());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_token_resumes_the_others() {
    let mut h = Harness::new().await;
    h.fake
        .add_file(&h.fake.root_id(), "da-primeira.txt", b"um".to_vec());
    h.two_accounts(ANA).await;
    h.tokens.delete(ANA).await.unwrap();

    h.restart(None).await;

    // Ela continua no arquivo e na lista, pedindo para entrar de novo; a outra sobe.
    assert_eq!(h.saved_accounts(), [h.fake.email(), ANA.into()]);
    let all = h.proxy.status("").await.unwrap();
    let listed: Vec<_> = all
        .accounts
        .iter()
        .map(|a| (a.email.as_str(), a.state.as_str()))
        .collect();
    assert_eq!(
        listed,
        [(h.fake.email().as_str(), "synced"), (ANA, "needs-sign-in")]
    );
    assert_eq!(all.state, "needs-sign-in");
    let first = h.drive().join("da-primeira.txt");
    assert_eq!(blocking(move || std::fs::read(first)).await.unwrap(), b"um");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_says_starting_until_the_saved_accounts_reconnect() {
    let mut h = Harness::new().await;
    h.logged_in().await;

    // A bandeja pergunta logo que liga o serviço, antes de a conta guardada subir: não pode
    // ouvir "sem conta" (abriria o primeiro uso).
    h.restart_without_resuming(None).await;
    assert_eq!(h.proxy.status("").await.unwrap().state, "starting");

    h.daemon.resume_saved_accounts().await.unwrap();
    assert_ne!(h.proxy.status("").await.unwrap().state, "starting");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn without_saved_accounts_status_says_signed_out() {
    let mut h = Harness::new().await;
    h.restart_without_resuming(None).await;
    assert_eq!(h.proxy.status("").await.unwrap().state, "signed-out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn when_no_saved_account_comes_back_status_says_signed_out() {
    use futures_util::StreamExt;
    let mut h = Harness::new().await;
    h.logged_in().await;
    let email = h.fake.email();
    // Sem o login e sem o índice, a conta não tem como subir.
    h.tokens.delete(&email).await.unwrap();
    h.daemon.shutdown().await;
    std::fs::remove_file(h.paths().data_dir.join(&email).join("metadata.db")).unwrap();
    h.restart_without_resuming(None).await;
    let mut changed = h.proxy.receive_status_changed().await.unwrap();

    h.daemon.resume_saved_accounts().await.unwrap();

    // A bandeja, que esperava em "conectando", fica sabendo e pode abrir o primeiro uso.
    tokio::time::timeout(std::time::Duration::from_secs(10), changed.next())
        .await
        .expect("StatusChanged");
    assert_eq!(h.proxy.status("").await.unwrap().state, "signed-out");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn an_account_that_did_not_start_stays_saved() {
    let mut h = Harness::new().await;
    h.two_accounts(ANA).await;
    // Sem o login e sem o índice, ela não tem como subir na próxima partida.
    h.tokens.delete(ANA).await.unwrap();
    h.daemon.shutdown().await;
    std::fs::remove_file(h.paths().data_dir.join(ANA).join("metadata.db")).unwrap();
    h.restart(None).await;

    h.daemon.sign_out(&h.fake.email()).await.unwrap();

    assert_eq!(
        h.saved_accounts(),
        [ANA],
        "tentada de novo na próxima partida"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_second_account_keeps_its_folder_after_the_first_leaves() {
    let mut h = Harness::new().await;
    let ana = h.fake.add_account(ANA);
    ana.add_file(&ana.root_id(), "da-ana.txt", b"dois".to_vec());
    h.logged_in().await;
    h.logged_in_as(ANA).await;
    h.daemon.sign_out(&h.fake.email()).await.unwrap();

    h.restart(None).await;

    let kept = h.mount_of(ANA).join("Meu Drive").join("da-ana.txt");
    assert_eq!(
        blocking(move || std::fs::read(kept)).await.unwrap(),
        b"dois",
        "a pasta não muda de lugar"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn pause_reaches_an_account_added_later() {
    let h = Harness::new().await;
    h.fake.add_account(ANA);
    h.logged_in().await;
    h.daemon.set_paused(true).await.unwrap();

    h.logged_in_as(ANA).await;

    let ana = h.proxy.status(ANA).await.unwrap();
    assert!(
        ana.paused,
        "pausar vale para todas as contas, também as que chegam depois"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_of_every_account_adds_the_counts_up() {
    let h = Harness::new().await;
    h.two_accounts(ANA).await;
    h.daemon.set_paused(true).await.unwrap();
    let file = h.mount_of(ANA).join("Meu Drive").join("novo.txt");
    blocking(move || std::fs::write(file, b"novo"))
        .await
        .unwrap();
    status_until(&h, "o envio pendente da Ana", |s| {
        s.accounts
            .iter()
            .any(|a| a.email == ANA && a.pending_uploads == 1)
    })
    .await;

    let all = h.proxy.status("").await.unwrap();

    assert_eq!(all.pending_uploads, 1, "a segunda conta também conta");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_lists_errors_per_account() {
    let h = Harness::new().await;
    h.two_accounts(ANA).await;
    let engine = h.daemon.engine().await.unwrap();
    let src = h.home.join("edicao.txt");
    std::fs::write(&src, b"guardada").unwrap();
    engine.lost_and_found().preserve(&src, "edicao.txt").unwrap();
    drop(engine);

    let status = h.proxy.status(ANA).await.unwrap();

    let errors = |email: &str| {
        let account = status.accounts.iter().find(|a| a.email == email);
        account.unwrap().errors
    };
    assert_eq!(errors(&h.fake.email()), 1);
    assert_eq!(errors(ANA), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signing_in_again_resumes_the_queue() {
    let h = Harness::new().await;
    let ana = h.fake.add_account(ANA);
    h.logged_in().await;
    h.logged_in_as(ANA).await;
    let before = h.tokens.load(ANA).await.unwrap().unwrap().1;
    h.fake.revoke(ANA);
    account_state_until(&h, ANA, "needs-sign-in").await;
    let path = h.mount_of(ANA).join("Meu Drive").join("espera.txt");
    blocking(move || std::fs::write(path, b"na fila"))
        .await
        .unwrap();

    let (ok, email, error) = h.sign_in_again_as(ANA, ANA).await;

    assert!(ok, "{error}");
    assert_eq!(email, ANA);
    assert_ne!(h.tokens.load(ANA).await.unwrap().unwrap().1, before);
    wait_until("o envio que esperava", || {
        !ana.find_live("espera.txt").is_empty()
    })
    .await;
    account_state_until(&h, ANA, "synced").await;
    // Continua a mesma montagem, e a conta não entrou duas vezes.
    assert_eq!(h.saved_accounts(), [h.fake.email(), ANA.into()]);
    assert_eq!(h.proxy.status("").await.unwrap().accounts.len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signing_in_again_with_another_account_is_refused() {
    let h = Harness::new().await;
    h.two_accounts(ANA).await;
    h.fake.revoke(ANA);
    account_state_until(&h, ANA, "needs-sign-in").await;
    let before = h.tokens.load(ANA).await.unwrap().unwrap().1;

    let (ok, email, error) = h.sign_in_again_as(&h.fake.email(), ANA).await;

    assert!(!ok);
    assert_eq!(email, h.fake.email());
    assert_eq!(error, "other-account");
    assert_eq!(h.tokens.load(ANA).await.unwrap().unwrap().1, before);
    assert_eq!(h.proxy.status(ANA).await.unwrap().state, "needs-sign-in");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_says_whether_another_account_can_be_added() {
    let h = Harness::new().await;
    assert!(h.proxy.status("").await.unwrap().can_add_account);
    h.logged_in().await;
    for n in 2..=4 {
        assert!(h.proxy.status("").await.unwrap().can_add_account);
        let email = format!("conta{n}@example.com");
        h.fake.add_account(&email);
        h.logged_in_as(&email).await;
    }

    assert!(!h.proxy.status("").await.unwrap().can_add_account);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_add_account_hides_adding_after_the_first() {
    let h = Harness::with_policy("DisableAddAccount = true").await;
    assert!(h.proxy.status("").await.unwrap().can_add_account);
    h.logged_in().await;

    assert!(!h.proxy.status("").await.unwrap().can_add_account);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_new_account_can_reuse_the_credentials_of_the_first() {
    let h = Harness::new().await;
    h.fake.add_account("ana@example.com");
    h.logged_in().await;
    h.fake.login_as("ana@example.com");

    // Client ID vazio: o serviço usa o que a primeira conta guardou.
    let (ok, _, error) = h.login_with("", "", "").await;

    assert!(ok, "{error}");
    assert_eq!(h.saved_accounts().len(), 2);
    let stored = h.tokens.load("ana@example.com").await.unwrap().unwrap().0;
    assert_eq!(stored.client_id, "id-de-teste");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn empty_credentials_without_any_account_are_refused() {
    let h = Harness::new().await;

    let err = h.proxy.begin_login("", "", "").await.unwrap_err();

    assert_eq!(err.short_name(), "InvalidArgs", "{err}");
}

/// Um Drive compartilhado "Equipe" da Ana, de que a conta principal também é membro, com
/// `Planos/plano.txt`. Devolve o ID do arquivo, que é o mesmo nas duas contas.
fn team_drive(h: &Harness) -> String {
    let ana = h.fake.add_account(ANA);
    let drive = ana.add_shared_drive("Equipe");
    h.fake.share_drive(&drive, &h.fake.email());
    let folder = ana.add_folder(&drive, "Planos");
    ana.add_file(&folder, "plano.txt", b"plano".to_vec())
}

/// `path` dentro do Drive compartilhado "Equipe" da montagem `mount`.
fn in_team(mount: &std::path::Path, path: &str) -> String {
    let team = mount.join("Drives compartilhados/Equipe");
    team.join(path).display().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shortcut_goes_to_the_chosen_folder() {
    let h = Harness::new().await;
    let plan = team_drive(&h);
    h.logged_in().await;
    h.logged_in_as(ANA).await;

    // No alto, o Meu Drive (com o nome da pasta da montagem) e o Drive compartilhado.
    let roots = h.proxy.folders(ANA, "").await.unwrap();
    let names: Vec<_> = roots.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Meu Drive", "Equipe"]);
    assert!(roots[1].has_children);
    let inside = h.proxy.folders(ANA, &roots[1].id).await.unwrap();
    let names: Vec<_> = inside.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(names, ["Planos"]);

    h.proxy
        .add_shortcut(ANA, &plan, &roots[1].id)
        .await
        .unwrap();

    let path = h
        .mount_of(ANA)
        .join("Drives compartilhados/Equipe/plano.txt");
    assert!(
        blocking(move || path.symlink_metadata().is_ok()).await,
        "o atalho na pasta escolhida, na montagem da conta pedida"
    );
    // Só pasta é destino.
    let err = h.proxy.add_shortcut(ANA, &plan, &plan).await.unwrap_err();
    assert_eq!(err.short_name(), "NotFound", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn menu_works_inside_a_shared_drive() {
    let h = Harness::new().await;
    let file = team_drive(&h);
    h.logged_in().await;
    let mount = h.home.join("Google Drive");
    let (folder, plan) = (
        in_team(&mount, "Planos"),
        in_team(&mount, "Planos/plano.txt"),
    );

    let file_menu = h.proxy.menu(&[&plan]).await.unwrap();
    let folder_menu = h.proxy.menu(&[&folder]).await.unwrap();

    let ids: Vec<_> = file_menu.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(
        ids[..5],
        ["open-web", "share", "copy-link", "add-shortcut", "versions"]
    );
    assert!(folder_menu.iter().any(|e| e.id == "new-document"));
    // A raiz do drive também é uma pasta do menu; a pasta virtual que os reúne tem o menu dela.
    let team = in_team(&mount, "");
    assert!(!h.proxy.menu(&[&team]).await.unwrap().is_empty());
    // As ações acham o item pelo caminho.
    let link = h.proxy.menu_action("copy-link", &[&plan]).await.unwrap();
    assert_eq!(link, format!("https://drive.google.com/file/d/{file}/view"));
    h.proxy.menu_action("offline", &[&plan]).await.unwrap();
    let pinned = h.proxy.menu(&[&plan]).await.unwrap();
    assert_eq!(
        pinned.iter().find(|e| e.id == "offline").unwrap().state,
        "on"
    );
    // O que é pesquisado e fixado dentro do drive aparece com o caminho da montagem.
    let found = h.proxy.search("", "plano.txt").await.unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].path, plan);
    let listed = h.proxy.offline_items("", "").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, file);
    assert_eq!(listed[0].path, plan);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn same_file_in_two_accounts_acts_on_the_chosen_one() {
    let h = Harness::new().await;
    let file = team_drive(&h);
    h.logged_in().await;
    h.logged_in_as(ANA).await;
    let first = h.fake.email();
    let (first_mount, ana_mount) = (h.home.join("Google Drive"), h.mount_of(ANA));
    let (first_path, ana_path) = (
        in_team(&first_mount, "Planos/plano.txt"),
        in_team(&ana_mount, "Planos/plano.txt"),
    );
    let pinned = |account: &'static str| {
        let proxy = h.proxy.clone();
        async move { proxy.offline_items(account, "").await.unwrap().len() }
    };

    // Pelo caminho, vale a montagem de cada conta.
    h.proxy.pin(&ana_path).await.unwrap();
    assert_eq!((pinned("").await, pinned(ANA).await), (0, 1));
    h.proxy.pin(&first_path).await.unwrap();
    assert_eq!((pinned("").await, pinned(ANA).await), (1, 1));
    // Pelo ID, vale a conta pedida: o mesmo ID é de um item diferente em cada banco.
    h.proxy.unpin_item(&first, &file).await.unwrap();
    assert_eq!((pinned("").await, pinned(ANA).await), (0, 1));
    let ana_items = h.proxy.offline_items(ANA, "").await.unwrap();
    assert_eq!(ana_items[0].path, ana_path);

    let before = h.opened().len();
    h.proxy.open_item(ANA, &file, "file").await.unwrap();
    h.proxy.open_item(&first, &file, "folder").await.unwrap();
    wait_until("os dois itens abertos", || h.opened().len() == before + 2).await;
    let opened = h.opened();
    // Cada um abre na montagem da conta pedida (o lançador roda sem ordem entre os dois).
    assert!(opened.contains(&ana_path), "{opened:?}");
    assert!(
        opened.contains(&in_team(&first_mount, "Planos")),
        "{opened:?}"
    );
    for account in [first.as_str(), ANA] {
        let found = h.proxy.search(account, "plano.txt").await.unwrap();
        assert_eq!(found.len(), 1);
        let mount = if account == ANA {
            &ana_mount
        } else {
            &first_mount
        };
        assert_eq!(found[0].path, in_team(mount, "Planos/plano.txt"));
        assert!(h.proxy.web_link(account, &file).await.is_ok());
    }
}

/// Move `name` do Meu Drive para o Drive compartilhado "Equipe" pela montagem.
async fn move_into_team(h: &Harness, name: &str) {
    let mount = h.home.join("Google Drive");
    let (from, to) = (h.drive().join(name), in_team(&mount, name));
    blocking(move || std::fs::rename(from, to).unwrap()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn moved_to_shared_drive_warns_until_the_kind_is_turned_off() {
    let h = Harness::new().await;
    team_drive(&h);
    for name in ["um.txt", "dois.txt"] {
        h.fake.add_file(&h.fake.root_id(), name, b"x".to_vec());
    }
    h.logged_in().await;
    let settings = h.proxy.notification_settings().await.unwrap();
    let setting = settings.iter().find(|s| s.kind == "moved-to-shared-drive");
    assert!(setting.unwrap().enabled);

    move_into_team(&h, "um.txt").await;
    wait_until("aviso da mudança de drive", || {
        h.notes().iter().any(|n| n.body.contains("um.txt"))
    })
    .await;
    let listed = h.proxy.notifications().await.unwrap();
    let moved: Vec<_> = listed
        .iter()
        .filter(|n| n.kind == "moved-to-shared-drive")
        .collect();
    assert_eq!(moved.len(), 1, "{listed:?}");

    h.proxy
        .set_notification_enabled("moved-to-shared-drive", false)
        .await
        .unwrap();
    move_into_team(&h, "dois.txt").await;
    // Os eventos são tratados em ordem: o aviso da pausa, depois do movimento, é a barreira.
    h.proxy.pause().await.unwrap();
    let barrier = h.drive().join("barreira.txt");
    blocking(move || std::fs::write(barrier, b"x").unwrap()).await;
    wait_until("aviso da pausa", || {
        h.notes().iter().any(|n| n.body.contains("barreira.txt"))
    })
    .await;

    assert!(h.notes().iter().all(|n| !n.body.contains("dois.txt")));
    let listed = h.proxy.notifications().await.unwrap();
    assert!(
        listed.iter().all(|n| !n.body.contains("dois.txt")),
        "{listed:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn refused_folder_move_warns_and_cannot_be_turned_off() {
    let h = Harness::new().await;
    team_drive(&h);
    h.fake.add_folder(&h.fake.root_id(), "Pasta");
    h.logged_in().await;
    let mount = h.home.join("Google Drive");
    let (from, to) = (h.drive().join("Pasta"), in_team(&mount, "Pasta"));

    let err = blocking(move || std::fs::rename(from, to).unwrap_err()).await;

    assert_eq!(
        err.raw_os_error(),
        Some(rustix::io::Errno::PERM.raw_os_error())
    );
    wait_until("aviso da pasta", || {
        h.notes().iter().any(|n| n.body.contains("Pasta"))
    })
    .await;
    let err = h
        .proxy
        .set_notification_enabled("folder-move-unsupported", false)
        .await
        .unwrap_err();
    assert!(err.to_string().contains("InvalidArgs"), "{err}");
}

/// O que `SharedDrives` diz da conta `account`: `(nome, ligado)` de cada drive.
async fn drive_list(h: &Harness, account: &str) -> Vec<(String, bool)> {
    let drives = h.proxy.shared_drives(account).await.unwrap();
    drives.into_iter().map(|d| (d.name, d.enabled)).collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_drives_list_toggle_and_create_on_the_requested_account() {
    let h = Harness::new().await;
    team_drive(&h);
    h.logged_in().await;
    h.logged_in_as(ANA).await;
    let team = h.proxy.shared_drives("").await.unwrap()[0].id.clone();
    let team_dir = in_team(&h.home.join("Google Drive"), "");
    let exists = || {
        let dir = std::path::PathBuf::from(&team_dir);
        blocking(move || dir.symlink_metadata().is_ok())
    };
    assert_eq!(drive_list(&h, "").await, [("Equipe".into(), true)]);
    assert!(exists().await);

    // Desligar na primeira conta: a pasta some da montagem dela, e a conta da Ana não muda.
    h.proxy.set_shared_drives("", &[]).await.unwrap();
    assert_eq!(drive_list(&h, "").await, [("Equipe".into(), false)]);
    assert_eq!(drive_list(&h, ANA).await, [("Equipe".into(), true)]);
    wait_until("a pasta do drive sair da montagem", || {
        let dir = std::path::PathBuf::from(&team_dir);
        !dir.exists()
    })
    .await;
    h.proxy.set_shared_drives("", &[&team]).await.unwrap();
    assert_eq!(drive_list(&h, "").await, [("Equipe".into(), true)]);
    wait_until("a pasta do drive voltar", || {
        std::path::Path::new(&team_dir).exists()
    })
    .await;

    // Um nome já usado ganha o sufixo, e o drive novo nasce ligado só na conta pedida.
    let made = h.proxy.create_shared_drive("", " Equipe ").await.unwrap();
    assert_eq!((made.name.as_str(), made.enabled), ("Equipe (2)", true));
    assert!(h.proxy.shared_drives("").await.unwrap().contains(&made));
    let other = h.proxy.create_shared_drive(ANA, "Só da Ana").await.unwrap();
    assert!(h.proxy.shared_drives(ANA).await.unwrap().contains(&other));
    let mine = h.proxy.shared_drives("").await.unwrap();
    assert!(mine.iter().all(|d| d.id != other.id));
    let err = h.proxy.create_shared_drive("", "  ").await.unwrap_err();
    assert_eq!(err.short_name(), "InvalidArgs", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disabling_a_shared_drive_with_pending_uploads_is_refused() {
    let h = Harness::new().await;
    team_drive(&h);
    h.logged_in().await;
    let mount = h.home.join("Google Drive");
    h.proxy.pause().await.unwrap();
    let draft = in_team(&mount, "rascunho.txt");
    blocking(move || std::fs::write(draft, b"ainda nao subiu").unwrap()).await;

    let err = h.proxy.set_shared_drives("", &[]).await.unwrap_err();

    assert_eq!(err.short_name(), "PendingUploads", "{err}");
    assert!(err.to_string().contains("enviados"), "{err}");
    assert_eq!(drive_list(&h, "").await, [("Equipe".into(), true)]);
    // Manter o drive ligado, mesmo com envio pendente, é salvar sem mudar nada.
    let team = h.proxy.shared_drives("").await.unwrap()[0].id.clone();
    h.proxy.set_shared_drives("", &[&team]).await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shared_drives_folder_menu_opens_the_screens_of_its_account() {
    let h = Harness::new().await;
    team_drive(&h);
    h.logged_in().await;
    h.logged_in_as(ANA).await;
    let (first, ana) = (h.fake.email(), ANA.to_string());
    let folder = |account: &str| {
        let mount = if account == ANA {
            h.mount_of(ANA)
        } else {
            h.home.join("Google Drive")
        };
        mount.join("Drives compartilhados").display().to_string()
    };

    let menu = h.proxy.menu(&[&folder(&first)]).await.unwrap();
    let ids: Vec<_> = menu.iter().map(|e| e.id.as_str()).collect();
    assert_eq!(ids, ["new-shared-drive", "manage-shared-drives"]);
    assert_eq!(menu[0].label, "Novo Drive compartilhado");
    assert_eq!(menu[1].label, "Gerenciar drives compartilhados");
    let before = h.opened().len();
    h.proxy
        .menu_action("new-shared-drive", &[&folder(&first)])
        .await
        .unwrap();
    h.proxy
        .menu_action("manage-shared-drives", &[&folder(&ana)])
        .await
        .unwrap();
    wait_until("as duas telas abertas", || h.opened().len() == before + 2).await;
    let opened = h.opened().split_off(before);
    for wanted in [
        format!("--open shared-drives:{first}/new"),
        format!("--open shared-drives:{ana}"),
    ] {
        assert!(opened.contains(&wanted), "{wanted} em {opened:?}");
    }

    // Só vale na própria pasta, e sozinha: o resto do menu não existe lá, e vice-versa.
    let inside = in_team(&h.home.join("Google Drive"), "Planos");
    let both = [folder(&first), inside.clone()];
    assert!(
        h.proxy
            .menu(&[&both[0], &both[1]])
            .await
            .unwrap()
            .is_empty()
    );
    let err = h
        .proxy
        .menu_action("manage-shared-drives", &[&inside])
        .await
        .unwrap_err();
    assert_eq!(err.short_name(), "InvalidArgs", "{err}");
    let err = h
        .proxy
        .menu_action("trash", &[&folder(&first)])
        .await
        .unwrap_err();
    assert_eq!(err.short_name(), "NotFound", "{err}");
}

// ---- Pastas do computador ----

/// A pasta "Computadores/<este computador>" no Drive simulado, como o app a cria.
fn computer_folder(h: &Harness) -> String {
    let found = h.fake.find_live(crate::testutil::COMPUTER_NAME);
    assert_eq!(found.len(), 1, "pasta do computador: {found:?}");
    found[0].clone()
}

fn local_folder(h: &Harness, name: &str) -> PathBuf {
    let path = h.home.join(name);
    std::fs::create_dir_all(&path).unwrap();
    path
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn added_folder_syncs_to_computers() {
    let h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    std::fs::write(docs.join("a.txt"), b"um").unwrap();
    let path = docs.to_str().unwrap();

    let check = h.proxy.check_local_folder("", path).await.unwrap();
    assert_eq!(
        (
            check.refusal.as_str(),
            check.files,
            check.size,
            check.exists
        ),
        ("", 1, 2, false)
    );
    h.proxy.add_local_folder("", path, false).await.unwrap();

    wait_until("a.txt no Drive", || h.fake.find_live("a.txt").len() == 1).await;
    let file = h.fake.meta(&h.fake.find_live("a.txt")[0]);
    let folder = h.fake.meta(file["parents"][0].as_str().unwrap());
    assert_eq!(folder["name"], "Documentos");
    let computer = h.fake.meta(folder["parents"][0].as_str().unwrap());
    assert_eq!(computer["id"], computer_folder(&h).as_str());
    assert_eq!(
        computer["appProperties"]["gdl.computer"],
        crate::testutil::COMPUTER_ID
    );
    let computers = h.fake.meta(computer["parents"][0].as_str().unwrap());
    assert_eq!(computers["appProperties"]["gdl.role"], "computers");
    let listed = h.proxy.local_folders().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        (listed[0].path.as_str(), listed[0].state.as_str()),
        (path, "active")
    );
    let again = h.proxy.check_local_folder("", path).await.unwrap();
    assert_eq!(
        again.refusal, "nested",
        "a mesma pasta não entra duas vezes"
    );
    let mounted = h.drive().parent().unwrap().join(format!(
        "Computadores/{}/Documentos/a.txt",
        crate::testutil::COMPUTER_NAME
    ));
    wait_until("a.txt em Computadores na montagem", || mounted.exists()).await;
    assert!(!h.drive().join("Computadores").exists());
}

/// Excluir no Drive um arquivo de uma pasta do computador manda a cópia local para a lixeira do
/// sistema e avisa quantos itens foram.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn items_trashed_from_drive_warn() {
    let h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    std::fs::write(docs.join("a.txt"), b"um").unwrap();
    h.proxy
        .add_local_folder("", docs.to_str().unwrap(), false)
        .await
        .unwrap();
    wait_until("a.txt no Drive", || h.fake.find_live("a.txt").len() == 1).await;

    h.fake.trash_remote(&h.fake.find_live("a.txt")[0]);

    wait_until("a.txt na lixeira", || !docs.join("a.txt").exists()).await;
    wait_until("aviso da lixeira", || {
        h.notes()
            .iter()
            .any(|n| n.summary == "Itens movidos para a lixeira")
    })
    .await;
    let note = h
        .notes()
        .into_iter()
        .find(|n| n.summary == "Itens movidos para a lixeira")
        .unwrap();
    assert!(note.body.contains("Um item"), "{note:?}");
}

/// Vinte arquivos com cara de cifrados gravados na montagem pausam os envios e avisam com os três
/// botões; "Eu reconheço esta atividade" retoma.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn encrypted_burst_pauses_and_can_be_recognized() {
    const SUSPICIOUS: &str = "Atividade suspeita nos arquivos";
    let h = Harness::new().await;
    h.logged_in().await;
    let drive = h.drive();
    blocking(move || {
        for n in 0..20u64 {
            let mut x = n * 2 + 1;
            let bytes: Vec<u8> = (0..8192)
                .map(|_| {
                    x ^= x << 13;
                    x ^= x >> 7;
                    x ^= x << 17;
                    (x >> 24) as u8
                })
                .collect();
            std::fs::write(drive.join(format!("doc{n}.txt")), bytes).unwrap();
        }
    })
    .await;

    wait_until("aviso de atividade suspeita", || {
        h.notes().iter().any(|n| n.summary == SUSPICIOUS)
    })
    .await;
    let note = h.notes().into_iter().find(|n| n.summary == SUSPICIOUS).unwrap();
    let keys: Vec<_> = note.actions.iter().step_by(2).map(String::as_str).collect();
    assert_eq!(keys, ["review-activity", "recognize-activity", "resume"]);
    assert!(h.proxy.status("").await.unwrap().paused);

    h.daemon.on_desktop_action(note.id, "recognize-activity").await;
    assert!(!h.proxy.status("").await.unwrap().paused);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn other_computers_show_in_the_mount() {
    let h = Harness::new().await;
    let root = h.fake.root_id();
    let computers = h.fake.add_folder(&root, "Computadores");
    h.fake
        .set_app_properties(&computers, &[("gdl.role", "computers")]);
    let other = h.fake.add_folder(&computers, "Outro PC");
    h.fake.add_file(&other, "b.txt", b"b".to_vec());
    h.logged_in().await;

    let mounted = h
        .drive()
        .parent()
        .unwrap()
        .join("Computadores/Outro PC/b.txt");
    wait_until("o backup do outro computador", || mounted.exists()).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn folders_resume_after_restart() {
    let mut h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    h.proxy
        .add_local_folder("", docs.to_str().unwrap(), false)
        .await
        .unwrap();
    wait_until("pasta no Drive", || {
        h.fake.find_live("Documentos").len() == 1
    })
    .await;

    h.restart(None).await;
    std::fs::write(docs.join("depois.txt"), b"dois").unwrap();

    wait_until("depois.txt no Drive", || {
        h.fake.find_live("depois.txt").len() == 1
    })
    .await;
    assert_eq!(h.proxy.local_folders().await.unwrap().len(), 1);
    assert_eq!(
        h.fake.find_live(crate::testutil::COMPUTER_NAME).len(),
        1,
        "a mesma pasta de apoio"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn merge_joins_an_existing_folder() {
    let h = Harness::new().await;
    // Uma instalação anterior deste computador deixou a pasta no Drive.
    let root = h.fake.root_id();
    let computers = h.fake.add_folder(&root, "Computadores");
    h.fake
        .set_app_properties(&computers, &[("gdl.role", "computers")]);
    let computer = h
        .fake
        .add_folder(&computers, crate::testutil::COMPUTER_NAME);
    h.fake
        .set_app_properties(&computer, &[("gdl.computer", crate::testutil::COMPUTER_ID)]);
    let old = h.fake.add_folder(&computer, "Documentos");
    h.fake.add_file(&old, "nuvem.txt", b"guardado".to_vec());
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    let path = docs.to_str().unwrap();

    let check = h.proxy.check_local_folder("", path).await.unwrap();
    assert!(check.exists, "o configurador pergunta se junta");
    h.proxy.add_local_folder("", path, true).await.unwrap();

    wait_until("nuvem.txt no computador", || {
        docs.join("nuvem.txt").exists()
    })
    .await;
    assert_eq!(h.fake.find_live("Documentos"), [old]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_mirrored_folders_refuses() {
    let h = Harness::with_policy("DisableMirroredFolders = true").await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");

    let err = h
        .proxy
        .add_local_folder("", docs.to_str().unwrap(), false)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ServiceError::Refused(r) if r == "folders-disabled"),
        "{err:?}"
    );
    assert!(h.proxy.local_folders().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_mirrored_folders_stops_existing_folders() {
    let mut h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    let id = h
        .proxy
        .add_local_folder("", docs.to_str().unwrap(), false)
        .await
        .unwrap();
    wait_until("pasta no Drive", || {
        h.fake.find_live("Documentos").len() == 1
    })
    .await;

    h.restart(Some("DisableMirroredFolders = true")).await;
    std::fs::write(docs.join("depois.txt"), b"dois").unwrap();
    let refused =
        |e: &ServiceError| matches!(e, ServiceError::Refused(r) if r == "folders-disabled");
    let moved = local_folder(&h, "Outra");
    let err = h
        .proxy
        .relocate_local_folder("", id, moved.to_str().unwrap())
        .await
        .unwrap_err();
    assert!(refused(&err), "{err:?}");

    tokio::time::sleep(std::time::Duration::from_millis(800)).await;
    assert!(
        h.fake.find_live("depois.txt").is_empty(),
        "o par não roda com a política"
    );
    assert_eq!(
        h.proxy.local_folders().await.unwrap().len(),
        1,
        "o par continua guardado"
    );
}

/// Bug da revisão final: "Escolher novo local" numa pasta errada (vazia) tratava cada arquivo
/// sincronizado como excluído e mandava tudo para a lixeira do Drive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn relocating_to_another_folder_merges_and_trashes_nothing() {
    let h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    std::fs::write(docs.join("a.txt"), b"um").unwrap();
    let id = h
        .proxy
        .add_local_folder("", docs.to_str().unwrap(), false)
        .await
        .unwrap();
    wait_until("a.txt no Drive", || h.fake.find_live("a.txt").len() == 1).await;
    let wrong = local_folder(&h, "Vazia");

    h.proxy
        .relocate_local_folder("", id, wrong.to_str().unwrap())
        .await
        .unwrap();

    wait_until("a.txt baixado na pasta nova", || wrong.join("a.txt").exists()).await;
    assert_eq!(h.fake.find_live("a.txt").len(), 1, "nada foi para a lixeira do Drive");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn missing_folder_is_announced() {
    let h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    h.proxy
        .add_local_folder("", docs.to_str().unwrap(), false)
        .await
        .unwrap();
    wait_until("pasta no Drive", || {
        h.fake.find_live("Documentos").len() == 1
    })
    .await;

    std::fs::remove_dir_all(&docs).unwrap();
    let mut note = None;
    for _ in 0..100 {
        let notes = h.proxy.notifications().await.unwrap();
        note = notes.into_iter().find(|n| n.kind == "folder-missing");
        if note.is_some() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    let note = note.expect("o aviso da pasta que sumiu");

    assert!(note.body.contains("Documentos"), "{}", note.body);
    assert_eq!(note.action_key, "open-folders");
    let folders = h.proxy.local_folders().await.unwrap();
    assert_eq!(folders[0].state, "stopped-missing");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn signing_out_stops_the_folders_and_keeps_the_files() {
    let h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    std::fs::write(docs.join("a.txt"), b"um").unwrap();
    h.proxy
        .add_local_folder("", docs.to_str().unwrap(), false)
        .await
        .unwrap();
    wait_until("a.txt no Drive", || h.fake.find_live("a.txt").len() == 1).await;

    h.proxy.logout("", "discard", "").await.unwrap();
    std::fs::write(docs.join("depois.txt"), b"dois").unwrap();
    tokio::time::sleep(Duration::from_millis(800)).await;

    assert!(
        h.fake.find_live("depois.txt").is_empty(),
        "o par parou com a conta"
    );
    assert_eq!(
        std::fs::read(docs.join("a.txt")).unwrap(),
        b"um",
        "os arquivos ficam"
    );
    assert!(h.proxy.local_folders().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn menu_offers_to_sync_a_folder_outside_the_drive() {
    let h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    std::fs::write(docs.join("a.txt"), b"um").unwrap();
    let path = docs.to_str().unwrap();
    let file = docs.join("a.txt").display().to_string();
    let ids = |menu: Vec<gdl_ipc::MenuEntry>| menu.into_iter().map(|e| e.id).collect::<Vec<_>>();

    let menu = h.proxy.menu(&[path]).await.unwrap();
    assert_eq!(menu.len(), 1, "{menu:?}");
    assert_eq!(
        (menu[0].id.as_str(), menu[0].label.as_str()),
        ("sync-folder", "Sincronizar esta pasta com o Google Drive")
    );
    assert!(ids(h.proxy.menu(&[&file]).await.unwrap()).is_empty());
    assert!(ids(h.proxy.menu(&[path, path]).await.unwrap()).is_empty());
    let before = h.opened().len();
    h.proxy.menu_action("sync-folder", &[path]).await.unwrap();
    wait_until("o configurador aberto", || h.opened().len() > before).await;
    assert_eq!(
        h.opened()[before],
        format!("--open preferences:folder:{path}")
    );

    h.proxy.add_local_folder("", path, false).await.unwrap();
    assert!(
        ids(h.proxy.menu(&[path]).await.unwrap()).is_empty(),
        "pasta já sincronizada não oferece de novo"
    );
    let inner = local_folder(&h, "Documentos/Notas");
    assert!(ids(h.proxy.menu(&[inner.to_str().unwrap()]).await.unwrap()).is_empty());
    let drive = h.drive().display().to_string();
    assert!(
        h.proxy.menu_action("sync-folder", &[&drive]).await.is_err(),
        "a montagem não vira pasta sincronizada"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn states_inside_a_pair() {
    use futures_util::StreamExt;

    let h = Harness::new().await;
    h.logged_in().await;
    let docs = local_folder(&h, "Documentos");
    std::fs::write(docs.join("a.txt"), b"um").unwrap();
    let path = |rel: &str| docs.join(rel).display().to_string();
    let root = docs.display().to_string();
    h.proxy.add_local_folder("", &root, false).await.unwrap();
    wait_until("a.txt no Drive", || h.fake.find_live("a.txt").len() == 1).await;
    assert_eq!(
        h.proxy.folder_roots().await.unwrap(),
        std::slice::from_ref(&root)
    );
    // O arquivo aparece no Drive antes de o par anotá-lo como sincronizado.
    for _ in 0..100 {
        if h.proxy.folder_states(&[&path("a.txt")]).await.unwrap() == ["offline"] {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    let states = h
        .proxy
        .folder_states(&[&root, &path("a.txt"), &path("nada.txt"), "/fora"])
        .await
        .unwrap();
    assert_eq!(states, ["offline", "offline", "syncing", ""]);

    let mut changed = h.proxy.receive_states_changed().await.unwrap();
    std::fs::write(docs.join("b.txt"), b"dois").unwrap();
    let b = path("b.txt");
    let mut seen = vec![];
    while !seen.contains(&b) {
        let signal = tokio::time::timeout(std::time::Duration::from_secs(10), changed.next())
            .await
            .expect("StatesChanged com b.txt")
            .unwrap();
        seen.extend(signal.args().unwrap().paths);
    }
    assert_eq!(h.proxy.folder_states(&[&b]).await.unwrap(), ["offline"]);

    std::fs::remove_dir_all(&docs).unwrap();
    for _ in 0..100 {
        if h.proxy.folder_states(&[&root]).await.unwrap() == ["conflict"] {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("a pasta parada não mostra o emblema de erro");
}

// ---- Espelho do Meu Drive ----

/// Os nomes na pasta `dir`, em ordem.
fn names_in(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .map(|d| {
            d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mirror_into_a_folder_with_files_merges_safely() {
    let h = Harness::new().await;
    let root = h.fake.root_id();
    h.fake.add_file(&root, "igual.txt", b"mesmo".to_vec());
    h.fake.add_file(&root, "diferente.txt", b"nuvem".to_vec());
    h.fake
        .add_file(&root, "da-nuvem.txt", b"so na nuvem".to_vec());
    h.logged_in().await;
    let mirror = local_folder(&h, "Espelho");
    std::fs::write(mirror.join("igual.txt"), b"mesmo").unwrap();
    std::fs::write(mirror.join("diferente.txt"), b"local").unwrap();
    std::fs::write(mirror.join("so-local.txt"), b"daqui").unwrap();
    let path = mirror.to_str().unwrap();

    let check = h.proxy.mirror_check("", path).await.unwrap();
    assert_eq!(check.refusal, "");
    assert_eq!(check.size, 5 + 5 + 11, "o que vai ser baixado");
    assert!(check.free > 0);
    h.proxy.set_mirror("", path).await.unwrap();
    assert_eq!(h.proxy.mirror("").await.unwrap(), path);

    wait_until("só-local no Drive e a cópia em conflito", || {
        h.fake.find_live("so-local.txt").len() == 1
            && mirror.join("da-nuvem.txt").exists()
            && mirror.join("diferente.txt").exists()
            && names_in(&mirror)
                .iter()
                .any(|n| n.starts_with("diferente (conflito"))
    })
    .await;
    assert_eq!(
        std::fs::read(mirror.join("da-nuvem.txt")).unwrap(),
        b"so na nuvem"
    );
    assert_eq!(
        h.fake.find_live("igual.txt").len(),
        1,
        "iguais não duplicam"
    );
    assert_eq!(
        std::fs::read(mirror.join("diferente.txt")).unwrap(),
        b"nuvem"
    );
    let conflict = names_in(&mirror)
        .into_iter()
        .find(|n| n.starts_with("diferente (conflito"))
        .unwrap();
    assert_eq!(std::fs::read(mirror.join(&conflict)).unwrap(), b"local");
    wait_until("a versão local também no Drive", || {
        h.fake.find_live(&conflict).len() == 1
    })
    .await;
    assert_eq!(h.fake.find_live("diferente.txt").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn my_drive_leaves_the_mount_in_mirror_mode() {
    let h = Harness::new().await;
    h.logged_in().await;
    assert!(h.drive().exists());
    let mirror = local_folder(&h, "Espelho");

    h.proxy
        .set_mirror("", mirror.to_str().unwrap())
        .await
        .unwrap();

    wait_until("Meu Drive fora da montagem", || !h.drive().exists()).await;
    let mount = h.drive().parent().unwrap().to_path_buf();
    assert!(names_in(&mount).contains(&"Drives compartilhados".to_string()));
    assert!(
        h.proxy
            .set_mirror("", mirror.to_str().unwrap())
            .await
            .is_err(),
        "um Espelho por conta"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mirror_resumes_the_first_download() {
    let mut h = Harness::new().await;
    let root = h.fake.root_id();
    for n in 0..30 {
        h.fake
            .add_file(&root, &format!("f{n:02}.txt"), vec![b'x'; 20_000]);
    }
    h.logged_in().await;
    let mirror = local_folder(&h, "Espelho");
    h.proxy
        .set_mirror("", mirror.to_str().unwrap())
        .await
        .unwrap();
    wait_until("o download começou", || !names_in(&mirror).is_empty()).await;

    h.restart(None).await;

    wait_until("os 30 arquivos", || {
        names_in(&mirror)
            .iter()
            .filter(|n| n.ends_with(".txt"))
            .count()
            == 30
    })
    .await;
    let names = names_in(&mirror);
    assert!(
        names
            .iter()
            .all(|n| !n.contains("conflito") && !n.ends_with(".gdl-part")),
        "{names:?}"
    );
    for n in 0..30 {
        assert_eq!(h.fake.find_live(&format!("f{n:02}.txt")).len(), 1);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn switching_back_to_stream_keeps_the_old_folder() {
    let h = Harness::new().await;
    h.fake.add_file(&h.fake.root_id(), "a.txt", b"a".to_vec());
    h.logged_in().await;
    let mirror = local_folder(&h, "Espelho");
    let path = mirror.to_str().unwrap();
    h.proxy.set_mirror("", path).await.unwrap();
    wait_until("a.txt no Espelho", || mirror.join("a.txt").exists()).await;

    h.proxy.set_stream("", false).await.unwrap();
    assert_eq!(h.proxy.mirror("").await.unwrap(), "");
    assert!(mirror.join("a.txt").exists(), "a pasta antiga fica");
    wait_until("Meu Drive de volta", || h.drive().join("a.txt").exists()).await;

    h.proxy.set_mirror("", path).await.unwrap();
    let a = mirror.join("a.txt").display().to_string();
    for _ in 0..100 {
        if h.proxy.folder_states(&[&a]).await.unwrap() == ["offline"] {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    h.proxy.set_stream("", true).await.unwrap();
    assert!(
        !mirror.exists(),
        "remover a pasta antiga a manda para a lixeira"
    );
    let trashed = h.paths().data_home.join("Trash/files/Espelho/a.txt");
    assert!(trashed.exists(), "na lixeira do sistema: {trashed:?}");
    assert_eq!(h.fake.find_live("a.txt").len(), 1, "nada sai do Drive");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stream_switch_waits_for_pending_uploads() {
    let h = Harness::new().await;
    h.logged_in().await;
    let mirror = local_folder(&h, "Espelho");
    h.proxy
        .set_mirror("", mirror.to_str().unwrap())
        .await
        .unwrap();
    h.fake.inject(gdl_fake_drive::Fault::Offline);
    std::fs::write(mirror.join("novo.txt"), b"ainda aqui").unwrap();

    let err = h.proxy.set_stream("", false).await.unwrap_err();
    assert!(
        matches!(&err, ServiceError::Refused(r) if r == "mirror-pending"),
        "{err:?}"
    );
    assert_eq!(h.proxy.mirror("").await.unwrap(), mirror.to_str().unwrap());

    h.fake.clear_faults();
    wait_until("novo.txt no Drive", || {
        h.fake.find_live("novo.txt").len() == 1
    })
    .await;
    for _ in 0..100 {
        if h.proxy.set_stream("", false).await.is_ok() {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    panic!("sem pendências, volta para o Stream");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn disable_mirrored_my_drive_refuses() {
    let h = Harness::with_policy("DisableMirroredMyDrive = true").await;
    h.logged_in().await;
    let mirror = local_folder(&h, "Espelho");
    let path = mirror.to_str().unwrap();

    let check = h.proxy.mirror_check("", path).await.unwrap();
    assert_eq!(check.refusal, "mirror-disabled");
    let err = h.proxy.set_mirror("", path).await.unwrap_err();
    assert!(
        matches!(&err, ServiceError::Refused(r) if r == "mirror-disabled"),
        "{err:?}"
    );
    assert!(h.drive().exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn verbose_logging_is_switched_over_dbus() {
    let h = Harness::new().await;
    let state = h.paths().state_dir;
    let (_subscriber, log) = crate::logging::subscriber(&state, gdl_common::time::now_ms());
    h.daemon.set_log(log);

    assert!(!h.proxy.verbose_logging().await.unwrap());
    h.proxy.set_verbose_logging(true).await.unwrap();
    assert!(h.proxy.verbose_logging().await.unwrap());
    assert!(state.join("verbose-until").exists(), "a escolha fica guardada");
    h.proxy.set_verbose_logging(false).await.unwrap();
    assert!(!h.proxy.verbose_logging().await.unwrap());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn package_has_the_expected_files_and_no_secrets() {
    let h = Harness::new().await;
    h.logged_in().await;
    let logs = crate::logging::logs_dir(&h.paths().state_dir);
    std::fs::create_dir_all(&logs).unwrap();
    std::fs::write(
        logs.join("gdrive-desktop-linuxd.log"),
        "Authorization: Bearer ya29.conhecido\nrefresh 1//0guardado\nconta usuario@exemplo.com\n",
    )
    .unwrap();
    std::fs::create_dir(h.home.join("Desktop")).unwrap();
    let before = h.opened().len();

    let path = PathBuf::from(h.proxy.generate_diagnostics().await.unwrap());

    assert_eq!(path.parent().unwrap(), h.home.join("Desktop"));
    let name = path.file_name().unwrap().to_string_lossy().into_owned();
    assert!(name.starts_with("gdrive-desktop-linux-diagnostico-"), "{name}");
    assert!(name.ends_with(".tar.gz"), "{name}");
    wait_until("a pasta aberta", || h.opened().len() == before + 1).await;
    assert_eq!(h.opened()[before], h.home.join("Desktop").display().to_string());
    let files = crate::testutil::read_package(&path);
    let names: Vec<_> = files.iter().map(|(n, _)| n.as_str()).collect();
    for expected in [
        "sistema.txt",
        "preferencias.txt",
        "contas.txt",
        "pastas.txt",
        "logs/gdrive-desktop-linuxd.log",
    ] {
        assert!(names.contains(&expected), "falta {expected}: {names:?}");
    }
    let all: String = files.iter().map(|(_, t)| t.as_str()).collect();
    for secret in ["ya29.", "1//", "usuario@exemplo.com", "conhecido", "guardado"] {
        assert!(!all.contains(secret), "{secret} no pacote");
    }
    assert!(all.contains("u***@exemplo.com"), "a conta aparece mascarada");
    assert!(all.contains(env!("CARGO_PKG_VERSION")), "a versão vai no sistema.txt");
}

/// Um relatório de falha da janela, com segredos que não podem sair dele.
fn seeded_crash(h: &Harness) -> PathBuf {
    let dir = gdl_common::paths::crashes_dir(&h.paths().state_dir);
    std::fs::create_dir_all(&dir).unwrap();
    let report = dir.join("gdrive-desktop-linux-ui-2026-10-27-120000.txt");
    std::fs::write(
        &report,
        "gdrive-desktop-linux-ui 0.9.0\n\npanicked at src/x.rs:1:1:\nfalhou para ana@exemplo.com com Bearer ya29.segredo\n",
    )
    .unwrap();
    report
}

async fn crash_notes(h: &Harness) -> Vec<gdl_ipc::NotificationItem> {
    let all = h.proxy.notifications().await.unwrap();
    all.into_iter().filter(|n| n.kind == "crash").collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crash_is_offered_once_when_reporting_is_on() {
    let h = Harness::new().await;
    let report = seeded_crash(&h);
    h.proxy
        .set_preferences(&[("AutomaticErrorReporting", "true")])
        .await
        .unwrap();

    h.daemon.offer_crash_reports().await;
    h.daemon.offer_crash_reports().await;

    let crashes = crash_notes(&h).await;
    assert_eq!(crashes.len(), 1, "um aviso, uma vez só");
    assert!(!report.exists(), "o relatório fica marcado como visto");
    let before = h.opened().len();
    h.proxy
        .invoke_notification_action(crashes[0].id, crate::notify::SEND_CRASH_REPORT)
        .await
        .unwrap();
    wait_until("a issue aberta", || h.opened().len() == before + 1).await;
    let url = &h.opened()[before];
    let issues = "https://github.com/junior9283/gdrive-desktop-linux/issues/new?";
    assert!(url.starts_with(issues), "{url}");
    assert!(url.contains("falhou"), "o relatório vai no texto: {url}");
    assert!(!url.contains("ya29") && !url.contains("ana%40"), "{url}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crash_is_silent_when_reporting_is_off() {
    let h = Harness::new().await;
    let report = seeded_crash(&h);

    h.daemon.offer_crash_reports().await;

    assert!(crash_notes(&h).await.is_empty());
    assert!(report.exists(), "fica para quando o envio for ligado");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reset_local_data_refuses_with_pending_uploads() {
    let h = Harness::new().await;
    h.logged_in().await;
    h.proxy.pause().await.unwrap();
    let path = h.drive().join("rascunho.txt");
    blocking(move || std::fs::write(path, b"ainda nao subiu").unwrap()).await;

    let err = h.proxy.reset_local_data("").await.unwrap_err();

    assert!(err.to_string().contains("PendingUploads"), "{err}");
    let draft = h.drive().join("rascunho.txt");
    assert!(blocking(move || draft.exists()).await, "nada foi apagado");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reset_local_data_reindexes() {
    let h = Harness::new().await;
    h.fake.add_file(&h.fake.root_id(), "remoto.txt", b"do drive".to_vec());
    h.logged_in().await;
    let docs = h.home.join("Documentos");
    std::fs::create_dir_all(&docs).unwrap();
    std::fs::write(docs.join("a.txt"), b"oi").unwrap();
    let path = docs.to_str().unwrap();
    h.proxy.add_local_folder("", path, false).await.unwrap();
    wait_until("a.txt no Drive", || h.fake.find_live("a.txt").len() == 1).await;
    let before = h.daemon.engine().await.unwrap();

    h.proxy.reset_local_data("").await.unwrap();

    let after = h.daemon.engine().await.unwrap();
    assert!(!std::sync::Arc::ptr_eq(&before, &after), "a conta subiu de novo, do zero");
    assert_eq!(h.proxy.status("").await.unwrap().account, h.fake.email());
    let remote = h.drive().join("remoto.txt");
    assert_eq!(
        blocking(move || std::fs::read(remote).unwrap()).await,
        b"do drive"
    );
    let listed = h.proxy.local_folders().await.unwrap();
    assert_eq!(
        listed.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        [path],
        "a pasta do computador continua"
    );
    std::fs::write(docs.join("b.txt"), b"depois").unwrap();
    wait_until("b.txt no Drive", || h.fake.find_live("b.txt").len() == 1).await;
    assert_eq!(h.fake.find_live("a.txt").len(), 1, "sem cópia repetida");
}

/// Sem conexão, a conta não subiria de novo depois de apagar o banco (e as pastas do computador
/// se perderiam): a redefinição é recusada antes de apagar qualquer coisa.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reset_local_data_refuses_offline() {
    let h = Harness::new().await;
    h.logged_in().await;
    let docs = h.home.join("Documentos");
    std::fs::create_dir_all(&docs).unwrap();
    let path = docs.to_str().unwrap();
    h.proxy.add_local_folder("", path, false).await.unwrap();
    let before = h.daemon.engine().await.unwrap();
    h.fake.inject(gdl_fake_drive::Fault::Offline);

    let err = h.proxy.reset_local_data("").await.unwrap_err();

    h.fake.clear_faults();
    assert_eq!(err.short_name(), "Failed", "{err:?}");
    let after = h.daemon.engine().await.unwrap();
    assert!(std::sync::Arc::ptr_eq(&before, &after), "nada foi apagado");
    let listed = h.proxy.local_folders().await.unwrap();
    assert_eq!(
        listed.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(),
        [path]
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reset_preferences_keeps_policies() {
    let h = Harness::with_policy("BandwidthRxKBPS = 500\n").await;
    h.proxy
        .set_preferences(&[("UITheme", "\"dark\"")])
        .await
        .unwrap();

    h.proxy.reset_preferences().await.unwrap();

    let prefs = h.proxy.preferences().await.unwrap();
    let get = |key: &str| prefs.iter().find(|p| p.key == key).unwrap().clone();
    assert_eq!(get("UITheme").value, "null", "a escolha do usuário sai");
    let managed = get("BandwidthRxKBPS");
    assert_eq!((managed.value.as_str(), managed.managed), ("500", true));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn localized_folders_can_be_turned_off() {
    let h = Harness::with_policy("DisableLocalizedVirtualFolders = true\n").await;
    h.fake.add_file(&h.fake.root_id(), "a.txt", b"oi".to_vec());
    h.logged_in().await;

    let mount = h.home.join("Google Drive");
    let names = blocking(move || names_in(&mount)).await;
    assert!(names.contains(&"My Drive".to_string()), "{names:?}");
    assert!(names.contains(&"Shared drives".to_string()), "{names:?}");
    assert!(!names.contains(&"Meu Drive".to_string()), "{names:?}");
    let file = h.home.join("Google Drive/My Drive/a.txt");
    assert!(blocking(move || file.exists()).await);
}
