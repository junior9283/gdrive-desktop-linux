//! Testes da interface D-Bus do serviço, contra o Drive simulado e uma montagem real.

use gdl_drive::token_store::TokenStore;
use gdl_store::PinState;

use crate::testutil::{Harness, blocking, wait_until};

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
