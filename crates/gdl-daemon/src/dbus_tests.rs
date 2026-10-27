//! Testes da interface D-Bus do serviço, contra o Drive simulado e uma montagem real.

use gdl_drive::token_store::TokenStore;
use gdl_store::PinState;

use crate::testutil::{Harness, blocking, status_until, wait_until};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn login_over_dbus_emits_login_finished() {
    let h = Harness::new().await;

    let (ok, email, error) = h.login().await;

    assert!(ok, "{error}");
    assert_eq!(email, h.fake.email());
    assert!(h.tokens.load(&email).await.unwrap().is_some());
    let status = h.proxy.status().await.unwrap();
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
    assert_eq!(h.proxy.status().await.unwrap().account, "");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_reports_pending_and_paused() {
    let h = Harness::new().await;
    h.logged_in().await;

    h.proxy.pause().await.unwrap();
    let path = h.drive().join("novo.txt");
    blocking(move || std::fs::write(path, b"pendente").unwrap()).await;

    let status = h.proxy.status().await.unwrap();
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

    let err = h.proxy.logout("", "").await.unwrap_err();
    assert!(err.to_string().contains("PendingUploads"), "{err}");
    assert_eq!(h.proxy.status().await.unwrap().account, h.fake.email());

    h.proxy.logout("discard", "").await.unwrap();
    assert_eq!(h.proxy.status().await.unwrap().account, "");
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

    h.proxy.logout("discard", "").await.unwrap();

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

    let status = h.proxy.status().await.unwrap();
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
    assert_eq!(h.proxy.list_lost_and_found().await.unwrap().len(), 1);
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
        .logout("move", saved.to_str().unwrap())
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
    assert_eq!(h.proxy.status().await.unwrap().account, "");
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
    let saved = h.proxy.list_lost_and_found().await.unwrap();

    let err = h.proxy.logout("", "").await.unwrap_err();
    assert!(err.to_string().contains("PendingUploads"), "{err}");
    let copies = h.home.join("Copias");
    h.proxy
        .logout("move", copies.to_str().unwrap())
        .await
        .unwrap();

    assert_eq!(h.proxy.status().await.unwrap().account, "");
    assert_eq!(std::fs::read(&saved[0]).unwrap(), b"editado aqui");
    let copied = std::fs::read_dir(&copies).unwrap().count();
    assert_eq!(copied, 1, "o Lost and Found vai junto para a pasta");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_reports_state_and_syncing_count() {
    let h = Harness::new().await;
    assert_eq!(h.proxy.status().await.unwrap().state, "signed-out");
    h.logged_in().await;
    status_until(&h, "sincronizado", |s| s.state == "synced").await;

    let path = h.drive().join("novo.txt");
    blocking(move || std::fs::write(path, b"para subir").unwrap()).await;

    // O envio só começa 2 s depois do fechamento: até lá, está na fila.
    let status = h.proxy.status().await.unwrap();
    assert_eq!(status.state, "syncing", "{status:?}");
    assert_eq!(status.pending_uploads, 1);
    assert_eq!(status.syncing, 1);
    status_until(&h, "envio concluído", |s| s.state == "synced").await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn account_info_has_name_quota_and_photo() {
    let h = Harness::new().await;
    h.logged_in().await;

    let info = h.proxy.account_info().await.unwrap();

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
    h.proxy.account_info().await.unwrap();
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
    let rows = h.proxy.activity().await.unwrap();
    assert_eq!(rows[0].name, "a.txt");
    assert_eq!(rows[0].state, "upload-queued");
    tokio::time::timeout(std::time::Duration::from_secs(10), changed.next())
        .await
        .expect("ActivityChanged");
    status_until(&h, "envio concluído", |s| s.pending_uploads == 0).await;
    let rows = h.proxy.activity().await.unwrap();
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
        let rows = h.proxy.activity().await.unwrap();
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
    let rows = h.proxy.activity().await.unwrap();
    let id = rows
        .iter()
        .find(|r| r.name == "grande.bin")
        .unwrap()
        .item_id
        .clone();

    assert_eq!(h.proxy.cancel_upload(&id).await.unwrap(), "canceled");

    assert!(!blocking(move || path.exists()).await);
    let rows = h.proxy.activity().await.unwrap();
    assert_eq!(rows[0].name, "grande.bin");
    assert_eq!(rows[0].state, "upload-canceled");
    assert_eq!(h.proxy.cancel_upload(&id).await.unwrap(), "too-late");
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
    let err = h.proxy.logout("", "").await.unwrap_err();
    assert!(err.to_string().contains("PendingUploads"), "{err}");
    let copies = h.home.join("Copias");
    h.proxy
        .logout("move", copies.to_str().unwrap())
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

    let errors = h.proxy.errors().await.unwrap();

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
    h.proxy.retry(&errors[0].item_id).await.unwrap();
    status_until(&h, "envio refeito", |s| {
        s.errors == 1 && s.pending_uploads == 0
    })
    .await;
    assert_eq!(h.proxy.errors().await.unwrap().len(), 1);
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
    // Ligado de novo, o próximo aparece: os eventos chegam em ordem, então o primeiro já passou.
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

    assert!(!h.proxy.status().await.unwrap().paused);
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

    let items = h.proxy.shared_with_me().await.unwrap();
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

    h.proxy.add_shortcut(&shared).await.unwrap();

    assert!(exists().await, "o atalho aparece no Meu Drive");
    let err = h.proxy.add_shortcut("nao-existe").await.unwrap_err();
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
        h.proxy.open_item(id, target).await.unwrap();
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
    assert_eq!(h.proxy.web_link(&file).await.unwrap(), web);
    let err = h.proxy.open_item(&file, "janela").await.unwrap_err();
    assert_eq!(err.short_name(), "InvalidArgs", "{err}");
    let err = h.proxy.open_item("nao-existe", "file").await.unwrap_err();
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

    let top = h.proxy.offline_items("").await.unwrap();
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
    let in_root = h.proxy.offline_items(&root).await.unwrap();
    assert_eq!(in_root.len(), 1, "{in_root:?}");
    assert_eq!(
        (in_root[0].name.as_str(), in_root[0].pin.as_str()),
        ("Pasta", "mixed")
    );
    assert!(in_root[0].is_folder);
    let summary = h.proxy.offline_summary().await.unwrap();
    assert_eq!((summary.pinned_count, summary.pinned_bytes), (1, 5));
    assert_eq!(summary.cache_bytes, 5);
    assert!(summary.free_bytes > 0);
    assert_eq!(h.proxy.clear_cache().await.unwrap(), 0, "off-line fica");

    h.proxy.unpin_item(&a).await.unwrap();
    assert!(h.proxy.offline_items("").await.unwrap().is_empty());
    h.proxy.pin(&a_path).await.unwrap();
    h.proxy.clear_offline_files().await.unwrap();
    assert!(h.proxy.offline_items("").await.unwrap().is_empty());
    assert_eq!(h.proxy.clear_cache().await.unwrap(), 5);

    let found = h.proxy.search("A.TX").await.unwrap();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(
        (found[0].path.as_str(), found[0].is_folder),
        (a_path.as_str(), false)
    );
    let folder_hit = h.proxy.search("past").await.unwrap();
    assert_eq!(folder_hit[0].mime, "application/vnd.google-apps.folder");
    let recent = h.proxy.recent_items().await.unwrap();
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
    assert_eq!(h.proxy.status().await.unwrap().account, h.fake.email());
    assert!(h.drive().is_dir());
}

/// Sem apagar os dados: desmonta, tira o início automático e encerra; a conta continua guardada
/// para uma reinstalação.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn uninstall_unmounts_and_keeps_the_account() {
    let h = Harness::new().await;
    h.logged_in().await;
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
    h.proxy.logout("discard", "").await.unwrap();
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
    assert_eq!(ids(&file_menu)[..2], ["open-web", "copy-link"]);
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
async fn share_window_adds_people_and_changes_general_access() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "Plano.txt", b"x".to_vec());
    h.logged_in().await;

    let before = h.proxy.sharing(&id).await.unwrap();
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
            &id,
            &[" ana@example.com ", "bia@example.com"],
            "commenter",
            true,
            "Veja",
        )
        .await
        .unwrap();
    h.proxy
        .share_set_general(&id, "anyone", "reader")
        .await
        .unwrap();
    h.proxy
        .share_set_general(&id, "anyone", "writer")
        .await
        .unwrap();

    let after = h.proxy.sharing(&id).await.unwrap();
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
        .share_set_general(&id, "domain", "reader")
        .await
        .unwrap();
    let domain = h.proxy.sharing(&id).await.unwrap();
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
        .share_set_role(&id, &after.people[1].id, "")
        .await
        .unwrap();
    h.proxy
        .share_set_general(&id, "restricted", "")
        .await
        .unwrap();
    let last = h.proxy.sharing(&id).await.unwrap();
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
        .share_add(&id, &["sem-arroba"], "reader", false, "")
        .await;
    assert_eq!(name(bad_email.unwrap_err()), "InvalidArgs");
    let owner = h
        .proxy
        .share_add(&id, &["a@example.com"], "owner", false, "")
        .await;
    assert_eq!(
        name(owner.unwrap_err()),
        "InvalidArgs",
        "dono não se dá por aqui"
    );
    let kind = h.proxy.share_set_general(&id, "todos", "reader").await;
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

    let v = h.proxy.versions(&id).await.unwrap();
    assert_eq!(v.name, "notas.txt");
    let sizes: Vec<_> = v.versions.iter().map(|x| (x.size, x.current)).collect();
    assert_eq!(sizes, [(7, true), (8, false)]);
    let old = &v.versions[1];

    let saved = h.proxy.version_download(&id, &old.id).await.unwrap();
    let saved = std::path::PathBuf::from(saved);
    assert_eq!(saved.parent().unwrap(), h.home.join("Downloads"));
    let file_name = saved.file_name().unwrap().to_string_lossy().into_owned();
    assert!(
        file_name.starts_with("notas (") && file_name.ends_with(").txt"),
        "{file_name}"
    );
    assert_eq!(std::fs::read(&saved).unwrap(), b"primeira");
    let again = h.proxy.version_download(&id, &old.id).await.unwrap();
    assert_ne!(again, saved.to_string_lossy(), "não sobrescreve");

    h.proxy.version_keep(&id, &old.id, true).await.unwrap();
    assert!(h.proxy.versions(&id).await.unwrap().versions[1].keep_forever);

    h.proxy.version_restore(&id, &old.id).await.unwrap();
    let content = h.drive().join("notas.txt");
    assert_eq!(
        blocking(move || std::fs::read(content).unwrap()).await,
        b"primeira"
    );
    wait_until("a versão restaurada subir", || {
        h.fake.content(&id) == b"primeira"
    })
    .await;
    assert_eq!(h.proxy.versions(&id).await.unwrap().versions.len(), 3);
}
