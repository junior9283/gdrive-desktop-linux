//! Testes da navegação e da leitura com uma montagem FUSE real sobre o Drive simulado.

use std::path::PathBuf;

use gdl_fake_drive::{FakeDrive, Fault};
use gdl_store::ItemId;

use crate::testutil::{MIB, Mounted, mode, names, xattr};

// As operações de arquivo bloqueiam a thread; o runtime com várias threads mantém o Drive
// simulado e o motor respondendo enquanto isso.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn root_shows_localized_my_drive() {
    let m = Mounted::new(FakeDrive::start().await).await;
    assert_eq!(
        names(&m.mnt),
        [
            ".Lixeira do Drive",
            ".Lost and Found",
            ".shortcut-targets-by-id",
            "Meu Drive"
        ]
    );
    let en = Mounted::with_lang(FakeDrive::start().await, "en").await;
    assert!(names(&en.mnt).contains(&"My Drive".to_string()));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn readdir_and_stat_never_download_content() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let docs = fake.add_folder(&root, "Documentos");
    fake.add_file(&docs, "a.bin", vec![1; MIB]);
    let sub = fake.add_folder(&docs, "Sub");
    fake.add_file(&sub, "b.txt", b"conteudo".to_vec());
    fake.add_file(&root, "c.txt", b"raiz".to_vec());
    let m = Mounted::new(fake).await;

    let mut seen = 0;
    for entry in walkdir::WalkDir::new(&m.mnt) {
        let entry = entry.unwrap();
        entry.metadata().unwrap();
        seen += 1;
    }
    assert!(seen >= 9, "percorreu {seen} entradas");
    assert_eq!(
        std::fs::metadata(m.drive().join("Documentos/a.bin"))
            .unwrap()
            .len(),
        MIB as u64
    );
    assert_eq!(m.fake.downloaded_bytes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn reading_file_returns_remote_bytes() {
    let fake = FakeDrive::start().await;
    let data: Vec<u8> = (0..3 * MIB + 17).map(|n| (n % 251) as u8).collect();
    fake.add_file(&fake.root_id(), "video.mp4", data.clone());
    let m = Mounted::new(fake).await;

    assert_eq!(std::fs::read(m.drive().join("video.mp4")).unwrap(), data);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gdoc_reads_as_pointer_json() {
    let fake = FakeDrive::start().await;
    let id = fake.add_gdoc(
        &fake.root_id(),
        "Plano",
        "application/vnd.google-apps.document",
    );
    let m = Mounted::new(fake).await;

    let path = m.drive().join("Plano.gdoc");
    let text = std::fs::read_to_string(&path).unwrap();
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["doc_id"], id.as_str());
    assert_eq!(json["url"], format!("https://docs.google.com/open?id={id}"));
    assert_eq!(std::fs::metadata(&path).unwrap().len(), text.len() as u64);
    assert_eq!(m.fake.downloaded_bytes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shortcut_is_symlink_to_target() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let folder = fake.add_folder(&root, "Projetos");
    let target = fake.add_file(&folder, "alvo.txt", b"do alvo".to_vec());
    fake.add_shortcut(&folder, "atalho", &target);
    let m = Mounted::new(fake).await;

    let link = m.drive().join("Projetos/atalho");
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_link(&link).unwrap(),
        PathBuf::from(format!("../../.shortcut-targets-by-id/{target}/alvo.txt"))
    );
    assert_eq!(std::fs::read(&link).unwrap(), b"do alvo");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trash_folder_lists_trashed_items_read_only() {
    let fake = FakeDrive::start().await;
    let id = fake.add_file(&fake.root_id(), "lixo.txt", b"velho".to_vec());
    let m = Mounted::new(fake).await;
    m.fake.trash_remote(&id);
    m.poll_changes().await;

    let trash = m.mnt.join(".Lixeira do Drive");
    assert_eq!(names(&trash), ["lixo.txt"]);
    assert!(!names(&m.drive()).contains(&"lixo.txt".to_string()));
    assert_eq!(mode(&trash), 0o555);
    assert_eq!(mode(&trash.join("lixo.txt")), 0o444);
    assert!(std::fs::File::create(trash.join("novo.txt")).is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn readonly_items_have_readonly_mode() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let shared = fake.add_folder(&root, "Compartilhada");
    let file = fake.add_file(&shared, "leitura.txt", b"so ler".to_vec());
    fake.add_file(&root, "meu.txt", b"meu".to_vec());
    fake.make_read_only(&shared);
    fake.make_read_only(&file);
    let m = Mounted::new(fake).await;

    assert_eq!(mode(&m.drive().join("Compartilhada")), 0o555);
    assert_eq!(mode(&m.drive().join("Compartilhada/leitura.txt")), 0o444);
    assert_eq!(mode(&m.drive().join("meu.txt")), 0o644);
    assert_eq!(mode(&m.drive()), 0o755);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offline_unread_file_gives_eio_but_dirs_still_list() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let folder = fake.add_folder(&root, "Pasta");
    fake.add_file(&folder, "nunca_lido.txt", b"longe".to_vec());
    let m = Mounted::new(fake).await;
    m.fake.inject(Fault::Offline);

    let err = std::fs::read(m.drive().join("Pasta/nunca_lido.txt")).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EIO));
    assert_eq!(names(&m.drive().join("Pasta")), ["nunca_lido.txt"]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn xattr_state_reports_cloud_then_offline_after_pin() {
    let fake = FakeDrive::start().await;
    let id = fake.add_file(&fake.root_id(), "nota.txt", b"texto".to_vec());
    let m = Mounted::new(fake).await;
    let path = m.drive().join("nota.txt");

    assert_eq!(xattr(&path, "user.gdl.id"), id);
    assert_eq!(xattr(&path, "user.gdl.state"), "cloud");
    assert_eq!(
        xattr(&path, "user.gdl.web_url"),
        format!("https://drive.google.com/file/d/{id}/view")
    );
    m.engine.pins().pin(&ItemId(id)).unwrap();
    // O TTL de 1 s vale para atributos, não para xattrs: o kernel sempre pergunta.
    assert_eq!(xattr(&path, "user.gdl.state"), "offline");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ten_thousand_items_readdir_queries_the_store_once() {
    let fake = FakeDrive::start().await;
    let folder = fake.add_folder(&fake.root_id(), "Grande");
    for n in 0..10_000 {
        fake.add_file(&folder, &format!("f{n:05}.txt"), vec![]);
    }
    let m = Mounted::new(fake).await;
    let before = m.fs.listings();

    assert_eq!(
        std::fs::read_dir(m.drive().join("Grande")).unwrap().count(),
        10_000
    );
    // Desempenho pela contagem de consultas, não pelo relógio: a listagem inteira, em muitas
    // chamadas de readdir do kernel, sai de uma consulta só ao banco.
    assert_eq!(m.fs.listings() - before, 1);
}
