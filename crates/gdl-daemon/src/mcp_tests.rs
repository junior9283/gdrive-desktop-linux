//! As ferramentas do servidor MCP (`gdl-mcp`) contra o serviço de teste, montagem incluída.

use std::path::Path;

use gdl_mcp::ToolError;
use gdl_mcp::Tools;
use gdl_mcp::drive::DriveTools;
use serde_json::{Value, json};

use crate::testutil::{Harness, blocking, wait_until};

async fn call(h: &Harness, name: &str, args: Value) -> Result<Value, ToolError> {
    DriveTools::new(h.proxy.clone(), h.home.clone())
        .call(name, args)
        .await
}

/// Uma pasta `Projeto` na pasta pessoal com `a.txt` e `sub/b.txt`.
fn project(h: &Harness) -> std::path::PathBuf {
    let dir = h.home.join("Projeto");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("a.txt"), "primeiro").unwrap();
    std::fs::write(dir.join("sub/b.txt"), "segundo").unwrap();
    dir
}

fn text(path: &Path) -> String {
    path.display().to_string()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn status_of_a_synced_path() {
    let h = Harness::new().await;
    h.fake
        .add_file(&h.fake.root_id(), "nota.txt", b"oi".to_vec());
    h.logged_in().await;

    let status = call(
        &h,
        "get_sync_status",
        json!({ "path": text(&h.drive().join("nota.txt")) }),
    )
    .await
    .unwrap();
    assert_eq!(status["state"], "online", "{status}");
    assert_eq!(status["account"], h.fake.email());
    assert!(
        !status["account_state"].as_str().unwrap().is_empty(),
        "{status}"
    );

    let outside = call(&h, "get_sync_status", json!({ "path": text(&h.home) }))
        .await
        .unwrap();
    assert_eq!(outside["state"], "outside");
    assert_eq!(outside["account"], "");
    let missing = call(&h, "get_sync_status", json!({})).await.unwrap_err();
    assert!(matches!(missing, ToolError::InvalidArgs(_)), "{missing:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn drive_url_of_a_path() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "nota.txt", b"oi".to_vec());
    h.logged_in().await;

    let link = call(
        &h,
        "get_drive_url",
        json!({ "path": text(&h.drive().join("nota.txt")) }),
    )
    .await
    .unwrap();
    assert!(link["url"].as_str().unwrap().contains(&id), "{link}");
    let notes = h.notes().len();
    let outside = call(&h, "get_drive_url", json!({ "path": text(&h.home) })).await;
    assert!(matches!(outside, Err(ToolError::Failed(_))), "{outside:?}");
    assert_eq!(
        h.notes().len(),
        notes,
        "sem o aviso de link copiado do menu"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tracked_folders_lists_pairs_and_mounts() {
    let h = Harness::new().await;
    h.logged_in().await;
    let dir = project(&h);
    h.proxy
        .add_local_folder("", &text(&dir), false)
        .await
        .unwrap();

    let tracked = call(&h, "list_tracked_folders", json!({})).await.unwrap();

    assert_eq!(tracked["accounts"][0]["account"], h.fake.email());
    assert_eq!(
        tracked["accounts"][0]["mount_point"],
        text(h.drive().parent().unwrap())
    );
    assert_eq!(tracked["computer_folders"][0]["path"], text(&dir));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn acl_dialog_only_opens() {
    let h = Harness::new().await;
    let id = h
        .fake
        .add_file(&h.fake.root_id(), "nota.txt", b"oi".to_vec());
    h.logged_in().await;
    let shared = h.fake.permissions(&id);
    let before = h.opened().len();

    call(
        &h,
        "open_acl_dialog",
        json!({ "email": "bia@example.com", "item_id": id }),
    )
    .await
    .unwrap();

    wait_until("a janela abriu", || h.opened().len() == before + 1).await;
    assert_eq!(
        h.opened()[before],
        format!("--open share:{}/{id}/bia@example.com", h.fake.email())
    );
    assert_eq!(h.fake.permissions(&id), shared, "nada foi compartilhado");
    let unknown = call(
        &h,
        "open_acl_dialog",
        json!({ "email": "bia@example.com", "item_id": "nada" }),
    )
    .await;
    assert!(matches!(unknown, Err(ToolError::Failed(_))), "{unknown:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn backup_mode_adds_a_pair() {
    let h = Harness::new().await;
    h.logged_in().await;
    let dir = project(&h);

    let added = call(
        &h,
        "add_folder_to_sync",
        json!({ "path": text(&dir), "sync_mode": "backup" }),
    )
    .await
    .unwrap();

    assert_eq!(added["sync_mode"], "backup");
    let folders = h.proxy.local_folders().await.unwrap();
    assert_eq!(folders.len(), 1);
    assert_eq!(folders[0].path, text(&dir));
    assert!(dir.join("a.txt").is_file(), "a pasta fica onde estava");
    let bad = call(
        &h,
        "add_folder_to_sync",
        json!({ "path": text(&dir), "sync_mode": "x" }),
    )
    .await;
    assert!(matches!(bad, Err(ToolError::InvalidArgs(_))), "{bad:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stream_move_leaves_a_link() {
    let h = Harness::new().await;
    h.fake.add_folder(&h.fake.root_id(), "Trabalho");
    h.logged_in().await;
    let dir = project(&h);

    let moved = call(
        &h,
        "add_folder_to_sync",
        json!({ "path": text(&dir), "sync_mode": "stream", "my_drive_path": "Trabalho" }),
    )
    .await
    .unwrap();

    let target = h.drive().join("Trabalho/Projeto");
    assert_eq!(moved["path"], text(&target));
    assert_eq!(
        std::fs::read_link(&dir).unwrap(),
        target,
        "um link no lugar"
    );
    let (a, b) = blocking({
        let target = target.clone();
        move || {
            (
                std::fs::read_to_string(target.join("a.txt")).unwrap(),
                std::fs::read_to_string(target.join("sub/b.txt")).unwrap(),
            )
        }
    })
    .await;
    assert_eq!((a.as_str(), b.as_str()), ("primeiro", "segundo"));
    let trashed = h.paths().data_home.join("Trash/files/Projeto");
    assert!(
        trashed.join("sub/b.txt").is_file(),
        "a original está na lixeira"
    );
    wait_until("os arquivos subiram", || {
        !h.fake.find_live("b.txt").is_empty()
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stream_move_undoes_on_failure() {
    use std::os::unix::fs::PermissionsExt;
    let h = Harness::new().await;
    h.logged_in().await;
    let dir = project(&h);
    let target = h.drive().join("Projeto");

    // Sem espaço no Drive: recusa antes de copiar.
    h.fake.set_quota_limit(1);
    let full = call(
        &h,
        "add_folder_to_sync",
        json!({ "path": text(&dir), "sync_mode": "stream" }),
    )
    .await;
    assert!(
        matches!(&full, Err(ToolError::Failed(m)) if m.contains("no-space")),
        "{full:?}"
    );
    assert!(!dir.is_symlink() && dir.join("a.txt").is_file());
    assert!(
        !blocking({
            let target = target.clone();
            move || target.exists()
        })
        .await
    );

    // Um arquivo que não dá para ler: a cópia pela metade sai e a original fica.
    h.fake.set_quota_limit(u64::MAX / 2);
    let locked = dir.join("sub/b.txt");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let broken = call(
        &h,
        "add_folder_to_sync",
        json!({ "path": text(&dir), "sync_mode": "stream" }),
    )
    .await;
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o644)).unwrap();

    assert!(matches!(broken, Err(ToolError::Failed(_))), "{broken:?}");
    assert!(!dir.is_symlink(), "sem link pela metade");
    assert_eq!(
        std::fs::read_to_string(dir.join("sub/b.txt")).unwrap(),
        "segundo"
    );
    assert!(
        !blocking(move || target.exists()).await,
        "a cópia pela metade saiu"
    );
    assert!(!h.paths().data_home.join("Trash/files/Projeto").exists());
}

/// Um assistente (ou um texto que o engane) não leva ao Drive pastas ocultas, como as chaves de
/// `~/.ssh`, nem nada fora da pasta pessoal.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn agent_cannot_add_hidden_or_outside_folders() {
    let h = Harness::new().await;
    h.logged_in().await;
    let ssh = h.home.join(".ssh");
    let nested = h.home.join(".config/app");
    let outside = h.paths().data_home.join("fora");
    for dir in [&ssh, &nested, &outside] {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("chave"), "segredo").unwrap();
    }

    for dir in [&ssh, &nested, &outside, &h.home] {
        for mode in ["backup", "stream"] {
            let refused = call(
                &h,
                "add_folder_to_sync",
                json!({ "path": text(dir), "sync_mode": mode }),
            )
            .await;
            assert!(
                matches!(&refused, Err(ToolError::Failed(m)) if m.contains("not-allowed")),
                "{} em {mode}: {refused:?}",
                dir.display()
            );
        }
    }
    // Pelo link, a pasta de verdade é a oculta.
    let link = h.home.join("Chaves");
    std::os::unix::fs::symlink(&ssh, &link).unwrap();
    let through_link = call(
        &h,
        "add_folder_to_sync",
        json!({ "path": text(&link), "sync_mode": "backup" }),
    )
    .await;
    assert!(
        matches!(through_link, Err(ToolError::Failed(_))),
        "{through_link:?}"
    );

    assert!(h.proxy.local_folders().await.unwrap().is_empty());
    assert!(!ssh.is_symlink() && ssh.join("chave").is_file());
}
