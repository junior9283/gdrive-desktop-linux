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
