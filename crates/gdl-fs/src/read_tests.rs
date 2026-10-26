//! Testes da navegação e da leitura com uma montagem FUSE real sobre o Drive simulado.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use gdl_engine::thumbs::freedesktop::{ThumbSize, file_for, stored_mtime, uri_of};
use gdl_fake_drive::{FakeDrive, Fault};
use gdl_store::ItemId;
use tokio_util::sync::CancellationToken;

use crate::testutil::{MIB, Mounted, mode, names, wait_until, xattr};

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
            ".Uploads cancelados",
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

/// A miniatura `normal` de `path` (caminho na montagem), se já foi gravada.
fn thumb_of(m: &Mounted, path: &Path) -> PathBuf {
    file_for(&m.thumbs, ThumbSize::Normal, &uri_of(path))
}

/// Lista `dir` numa thread de bloqueio e devolve os nomes e quanto demorou.
async fn timed_names(dir: PathBuf) -> (Vec<String>, Duration) {
    tokio::task::spawn_blocking(move || {
        let start = Instant::now();
        let names = names(&dir);
        (names, start.elapsed())
    })
    .await
    .unwrap()
}

/// Meu Drive com `foto (1).jpg` em `Fotos de Março #1`, `raiz.png` na raiz e `outro.txt`.
async fn photos() -> Mounted {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let folder = fake.add_folder(&root, "Fotos de Março #1");
    fake.add_file_with_mime(&folder, "foto (1).jpg", "image/jpeg", vec![7; 1000]);
    fake.add_file_with_mime(&root, "raiz.png", "image/png", vec![8; 500]);
    fake.add_file(&root, "outro.txt", b"texto".to_vec());
    Mounted::new(fake).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn opening_folder_seeds_thumbnails_with_mount_uri_and_mtime() {
    let m = photos().await;
    let folder = m.drive().join("Fotos de Março #1");

    let (listed, _) = timed_names(folder.clone()).await;
    assert_eq!(listed, ["foto (1).jpg"]);
    // A listagem esperou a semeadura: a miniatura já está lá, com a data que a montagem mostra.
    let photo = folder.join("foto (1).jpg");
    let png = thumb_of(&m, &photo);
    assert!(png.exists(), "sem miniatura em {}", png.display());
    let mtime = std::fs::metadata(&photo).unwrap().mtime();
    assert_eq!(stored_mtime(&png), Some(mtime));

    // A raiz do Meu Drive também semeia.
    timed_names(m.drive()).await;
    let root_png = thumb_of(&m, &m.drive().join("raiz.png"));
    assert!(root_png.exists(), "sem miniatura em {}", root_png.display());
    assert_eq!(m.fake.downloaded_bytes(), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn slow_thumbnails_do_not_hold_listing_past_deadline() {
    let m = photos().await;
    m.fake.inject(Fault::DelayThumbs(Duration::from_secs(5)));
    let folder = m.drive().join("Fotos de Março #1");

    let (listed, took) = timed_names(folder.clone()).await;
    assert_eq!(listed, ["foto (1).jpg"]);
    // Esperou o prazo, e não mais que ele.
    assert!(took >= Duration::from_millis(1400), "demorou só {took:?}");
    assert!(took < Duration::from_millis(2500), "demorou {took:?}");
    // A semeadura seguiu em segundo plano.
    let png = thumb_of(&m, &folder.join("foto (1).jpg"));
    wait_until("miniatura em segundo plano", || png.exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn other_fuse_ops_are_not_blocked_while_seeding_thumbnails() {
    let m = photos().await;
    m.fake.inject(Fault::DelayThumbs(Duration::from_secs(5)));
    let listing = tokio::spawn(timed_names(m.drive().join("Fotos de Março #1")));
    tokio::time::sleep(Duration::from_millis(300)).await;

    let other = m.drive().join("outro.txt");
    let took = tokio::task::spawn_blocking(move || {
        let start = Instant::now();
        std::fs::metadata(&other).unwrap();
        start.elapsed()
    })
    .await
    .unwrap();
    assert!(!listing.is_finished(), "a listagem não esperou a semeadura");
    assert!(took < Duration::from_millis(200), "stat demorou {took:?}");
    listing.await.unwrap();
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trash_and_mount_root_do_not_seed_thumbnails() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    fake.add_file_with_mime(&root, "raiz.png", "image/png", vec![8; 500]);
    let gone = fake.add_folder(&root, "Velhas");
    fake.add_file_with_mime(&gone, "antiga.jpg", "image/jpeg", vec![9; 500]);
    let m = Mounted::new(fake).await;
    m.fake.trash_remote(&gone);
    m.poll_changes().await;

    let trash = m.mnt.join(".Lixeira do Drive");
    timed_names(m.mnt.clone()).await;
    assert_eq!(timed_names(trash.clone()).await.0, ["Velhas"]);
    timed_names(trash.join("Velhas")).await;
    timed_names(m.mnt.join(".shortcut-targets-by-id")).await;
    assert_eq!(m.fake.requests("/thumb"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn many_slow_folders_do_not_hold_all_fuse_threads_for_thumbnails() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    for n in 0..6 {
        let folder = fake.add_folder(&root, &format!("Fotos {n}"));
        fake.add_file_with_mime(&folder, "foto.jpg", "image/jpeg", vec![n; 300]);
    }
    fake.add_file(&root, "outro.txt", b"texto".to_vec());
    let m = Mounted::new(fake).await;
    m.fake.inject(Fault::DelayThumbs(Duration::from_secs(5)));

    let listings: Vec<_> = (0..6)
        .map(|n| tokio::spawn(timed_names(m.drive().join(format!("Fotos {n}")))))
        .collect();
    tokio::time::sleep(Duration::from_millis(300)).await;
    let other = m.drive().join("outro.txt");
    let took = tokio::task::spawn_blocking(move || {
        let start = Instant::now();
        std::fs::metadata(&other).unwrap();
        start.elapsed()
    })
    .await
    .unwrap();
    assert!(took < Duration::from_millis(200), "stat demorou {took:?}");

    let mut times = vec![];
    for listing in listings {
        let (names, took) = listing.await.unwrap();
        assert_eq!(names, ["foto.jpg"]);
        times.push(took);
    }
    times.sort();
    // Só `MAX_THUMB_WAITERS` (4) esperam o prazo; as outras listam na hora.
    assert!(
        times[..2].iter().all(|t| *t < Duration::from_millis(700)),
        "tempos: {times:?}"
    );
    assert!(
        times[2..].iter().all(|t| *t >= Duration::from_millis(1400)),
        "tempos: {times:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn offline_engine_does_not_wait_for_thumbnails() {
    let m = photos().await;
    // O motor percebe a falta de rede pela consulta de mudanças e para de consultar.
    m.fake.inject(Fault::Offline);
    let stop = CancellationToken::new();
    let run = tokio::spawn({
        let (engine, stop) = (m.engine.clone(), stop.clone());
        async move { engine.run(stop).await }
    });
    let engine = m.engine.clone();
    tokio::task::spawn_blocking(move || wait_until("motor sem rede", || engine.is_offline()))
        .await
        .unwrap();
    stop.cancel();
    run.await.unwrap();
    // A rede volta só para as miniaturas, lentas; o motor ainda se acha sem rede.
    m.fake.clear_faults();
    m.fake.inject(Fault::DelayThumbs(Duration::from_secs(5)));

    let (listed, took) = timed_names(m.drive().join("Fotos de Março #1")).await;
    assert_eq!(listed, ["foto (1).jpg"]);
    assert!(took < Duration::from_millis(700), "demorou {took:?}");
}
