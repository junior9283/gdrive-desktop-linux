//! Testes da escrita: criar, editar, salvar por `rename`, excluir para a lixeira do Drive, lixeira
//! do KIO, mover, datas, ponteiros de Documentos Google, pastas sem internet e travas de editores.

use std::io::Write;
use std::time::{Duration, SystemTime};

use gdl_common::time::rfc3339;
use gdl_engine::uploads::CancelOutcome;
use gdl_fake_drive::{FakeDrive, Fault};
use gdl_store::ItemId;

use crate::testutil::{Mounted, mode, names, uid, wait_until, xattr};

fn only(ids: Vec<String>) -> String {
    assert_eq!(ids.len(), 1, "esperava um item só: {ids:?}");
    ids.into_iter().next().unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn create_write_close_uploads_new_file() {
    let m = Mounted::new(FakeDrive::start().await).await;
    m.run_engine();
    let path = m.drive().join("novo.txt");
    std::fs::write(&path, b"escrito no linux").unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"escrito no linux");
    wait_until("envio do arquivo novo", || {
        !m.fake.find_live("novo.txt").is_empty()
    });
    let id = only(m.fake.find_live("novo.txt"));
    wait_until("conteúdo no Drive", || {
        m.fake.content(&id) == b"escrito no linux"
    });
    assert_eq!(m.fake.meta(&id)["parents"][0], m.fake.root_id());
    // O inode é mantido quando o ID temporário vira o do Drive.
    assert_eq!(std::fs::read(&path).unwrap(), b"escrito no linux");
}

/// Um programa que deixou o arquivo aberto enquanto ele subia continua lendo depois que o ID
/// temporário vira o do Drive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn open_file_still_reads_after_the_upload_finishes() {
    use std::io::{Read, Seek};
    let m = Mounted::new(FakeDrive::start().await).await;
    m.fake
        .inject(Fault::DelayUploads(Duration::from_millis(300)));
    m.run_engine();
    let path = m.drive().join("aberto.txt");
    std::fs::write(&path, b"lido depois do envio").unwrap();
    let mut open = std::fs::File::open(&path).unwrap();
    assert!(xattr(&path, "user.gdl.id").starts_with("local:"));

    wait_until("ID do Drive", || {
        !xattr(&path, "user.gdl.id").starts_with("local:")
    });
    let mut read = Vec::new();
    open.rewind().unwrap();
    open.read_to_end(&mut read).unwrap();
    assert_eq!(read, b"lido depois do envio");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn edit_existing_file_updates_same_id() {
    let fake = FakeDrive::start().await;
    let id = fake.add_file(&fake.root_id(), "notas.txt", b"linha 1\n".to_vec());
    let m = Mounted::new(fake).await;
    m.run_engine();

    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(m.drive().join("notas.txt"))
        .unwrap();
    file.write_all(b"linha 2\n").unwrap();
    drop(file);

    wait_until("edição no Drive", || {
        m.fake.content(&id) == b"linha 1\nlinha 2\n"
    });
    assert_eq!(m.fake.find_live("notas.txt"), [id]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn save_via_rename_keeps_drive_id() {
    let fake = FakeDrive::start().await;
    let id = fake.add_file(&fake.root_id(), "doc.odt", b"versao 1".to_vec());
    let m = Mounted::new(fake).await;
    m.run_engine();

    let tmp = m.drive().join("doc.odt.tmp123");
    std::fs::write(&tmp, b"versao 2 salva pelo editor").unwrap();
    std::fs::rename(&tmp, m.drive().join("doc.odt")).unwrap();

    assert_eq!(names(&m.drive()), ["doc.odt"]);
    wait_until("conteúdo novo no mesmo ID", || {
        m.fake.content(&id) == b"versao 2 salva pelo editor"
    });
    assert_eq!(m.fake.find_live("doc.odt"), [id]);
    // Ponto de atenção 1: o temporário nunca fica vivo no Drive.
    std::thread::sleep(Duration::from_millis(300));
    assert!(m.fake.find_live("doc.odt.tmp123").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn unlink_moves_to_drive_trash_never_deletes() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let file = fake.add_file(&root, "velho.txt", b"x".to_vec());
    let full = fake.add_folder(&root, "Cheia");
    fake.add_file(&full, "dentro.txt", b"y".to_vec());
    let empty = fake.add_folder(&root, "Vazia");
    let m = Mounted::new(fake).await;

    std::fs::remove_file(m.drive().join("velho.txt")).unwrap();
    let err = std::fs::remove_dir(m.drive().join("Cheia")).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::ENOTEMPTY));
    std::fs::remove_dir(m.drive().join("Vazia")).unwrap();

    assert_eq!(m.fake.meta(&file)["trashed"], true);
    assert_eq!(m.fake.meta(&empty)["trashed"], true);
    assert_eq!(m.fake.meta(&full)["trashed"], false);
    assert_eq!(names(&m.drive()), ["Cheia"]);
    assert_eq!(
        names(&m.mnt.join(".Lixeira do Drive")),
        ["Vazia", "velho.txt"]
    );
    assert_eq!(m.fake.requests_by_method("DELETE"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn kio_topdir_trash_maps_to_drive_trash() {
    let fake = FakeDrive::start().await;
    let id = fake.add_file(&fake.root_id(), "x.pdf", vec![7; 100_000]);
    let m = Mounted::new(fake).await;
    let trash = m.mnt.join(format!(".Trash-{}", uid()));
    let meta = std::fs::symlink_metadata(&trash).unwrap();
    assert!(meta.is_dir());
    assert_eq!(
        std::os::unix::fs::PermissionsExt::mode(&meta.permissions()) & 0o777,
        0o700
    );

    // O que o Dolphin faz ao mandar para a lixeira: move para files/ e grava o .trashinfo.
    std::fs::rename(m.drive().join("x.pdf"), trash.join("files/x.pdf")).unwrap();
    std::fs::write(
        trash.join("info/x.pdf.trashinfo"),
        "[Trash Info]\nPath=Meu%20Drive/x.pdf\nDeletionDate=2026-10-25T10:00:00\n",
    )
    .unwrap();

    assert_eq!(m.fake.meta(&id)["trashed"], true);
    assert_eq!(
        m.fake.downloaded_bytes(),
        0,
        "mandar para a lixeira não baixa nada"
    );
    assert_eq!(names(&trash.join("files")), ["x.pdf"]);
    assert_eq!(names(&trash.join("info")), ["x.pdf.trashinfo"]);
    assert!(!names(&m.drive()).contains(&"x.pdf".to_string()));

    // Restaurar: move de volta.
    std::fs::rename(trash.join("files/x.pdf"), m.drive().join("x.pdf")).unwrap();
    std::fs::remove_file(trash.join("info/x.pdf.trashinfo")).unwrap();
    assert_eq!(m.fake.meta(&id)["trashed"], false);
    assert_eq!(names(&m.drive()), ["x.pdf"]);
    assert!(names(&trash.join("files")).is_empty());
    assert_eq!(m.fake.requests_by_method("DELETE"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn emptying_kio_trash_keeps_item_in_drive_trash() {
    let fake = FakeDrive::start().await;
    let id = fake.add_file(&fake.root_id(), "y.txt", b"y".to_vec());
    let m = Mounted::new(fake).await;
    let trash = m.mnt.join(format!(".Trash-{}", uid()));
    std::fs::rename(m.drive().join("y.txt"), trash.join("files/y.txt")).unwrap();

    std::fs::remove_file(trash.join("files/y.txt")).unwrap();

    assert!(names(&trash.join("files")).is_empty());
    assert_eq!(
        m.fake.meta(&id)["trashed"],
        true,
        "continua na lixeira do Drive"
    );
    assert_eq!(m.fake.requests_by_method("DELETE"), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn rename_and_move_between_folders() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let a = fake.add_folder(&root, "A");
    let b = fake.add_folder(&root, "B");
    let id = fake.add_file(&a, "rel.txt", b"r".to_vec());
    let m = Mounted::new(fake).await;

    std::fs::rename(
        m.drive().join("A/rel.txt"),
        m.drive().join("A/relatorio.txt"),
    )
    .unwrap();
    assert_eq!(m.fake.meta(&id)["name"], "relatorio.txt");
    std::fs::rename(
        m.drive().join("A/relatorio.txt"),
        m.drive().join("B/final.txt"),
    )
    .unwrap();

    let meta = m.fake.meta(&id);
    assert_eq!(meta["name"], "final.txt");
    assert_eq!(meta["parents"], serde_json::json!([b]));
    assert!(names(&m.drive().join("A")).is_empty());
    assert_eq!(names(&m.drive().join("B")), ["final.txt"]);
    assert_eq!(std::fs::read(m.drive().join("B/final.txt")).unwrap(), b"r");
    // Para fora do Meu Drive não dá.
    let err = std::fs::rename(m.drive().join("B/final.txt"), m.mnt.join("final.txt")).unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EXDEV));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mtime_set_by_app_is_kept_after_upload() {
    let m = Mounted::new(FakeDrive::start().await).await;
    m.run_engine();
    let path = m.drive().join("foto.jpg");
    std::fs::write(&path, b"jpeg").unwrap();
    let when = SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    std::fs::File::open(&path)
        .unwrap()
        .set_modified(when)
        .unwrap();

    assert_eq!(std::fs::metadata(&path).unwrap().modified().unwrap(), when);
    wait_until("envio da foto", || !m.fake.find_live("foto.jpg").is_empty());
    let id = only(m.fake.find_live("foto.jpg"));
    wait_until("data enviada", || {
        m.fake.meta(&id)["modifiedTime"] == rfc3339(1_600_000_000_000)
    });
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn copying_gdoc_pointer_creates_shortcut() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let doc = fake.add_gdoc(&root, "Plano", "application/vnd.google-apps.document");
    let folder = fake.add_folder(&root, "Pasta");
    let m = Mounted::new(fake).await;

    std::fs::copy(
        m.drive().join("Plano.gdoc"),
        m.drive().join("Pasta/Plano.gdoc"),
    )
    .unwrap();

    let shortcut = only(
        m.fake
            .find_live("Plano")
            .into_iter()
            .filter(|i| *i != doc)
            .collect(),
    );
    let meta = m.fake.meta(&shortcut);
    assert_eq!(meta["mimeType"], "application/vnd.google-apps.shortcut");
    assert_eq!(meta["parents"], serde_json::json!([folder]));
    assert_eq!(meta["shortcutDetails"]["targetId"], doc.as_str());
    let link = m.drive().join("Pasta/Plano");
    assert!(
        std::fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink()
    );
    assert_eq!(names(&m.drive().join("Pasta")), ["Plano"]);
    // Editar um ponteiro existente não é permitido.
    let err = std::fs::OpenOptions::new()
        .write(true)
        .open(m.drive().join("Plano.gdoc"))
        .unwrap_err();
    assert_eq!(err.raw_os_error(), Some(libc::EPERM));
}

/// Um `fork` (de qualquer app) duplica o descritor; o `close` da cópia chega antes como `flush`,
/// às vezes junto com o do original.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn closing_a_duplicated_pointer_descriptor_makes_one_shortcut() {
    let fake = FakeDrive::start().await;
    let root = fake.root_id();
    let doc = fake.add_gdoc(&root, "Plano", "application/vnd.google-apps.document");
    let m = Mounted::new(fake).await;
    let json = std::fs::read(m.drive().join("Plano.gdoc")).unwrap();

    let mut file = std::fs::File::create(m.drive().join("Copia.gdoc")).unwrap();
    file.write_all(&json).unwrap();
    drop(file.try_clone().unwrap());
    file.write_all(b"\n").unwrap();
    file.sync_all().unwrap();
    drop(file);

    let shortcuts = m.fake.find_live("Copia");
    assert_eq!(shortcuts.len(), 1);
    assert_eq!(
        m.fake.meta(&shortcuts[0])["shortcutDetails"]["targetId"],
        doc.as_str()
    );
    assert!(m.fake.find_live("Copia.gdoc").is_empty());
}

/// Bug: sem a resposta da criação, a pasta era criada duas vezes no Drive.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mkdir_with_lost_response_creates_one_folder() {
    let m = Mounted::new(FakeDrive::start().await).await;
    m.fake.inject(Fault::LoseCreateResponses { times: 1 });

    std::fs::create_dir(m.drive().join("Viagem")).unwrap();

    assert_eq!(m.fake.find_live("Viagem").len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn mkdir_offline_is_created_later() {
    let m = Mounted::new(FakeDrive::start().await).await;
    m.fake.inject(Fault::Offline);
    std::fs::create_dir(m.drive().join("Viagem")).unwrap();
    std::fs::write(m.drive().join("Viagem/roteiro.txt"), b"dia 1").unwrap();
    assert_eq!(names(&m.drive()), ["Viagem"]);
    assert!(m.fake.find_live("Viagem").is_empty());

    m.fake.clear_faults();
    m.run_engine();
    wait_until("pasta criada no Drive", || {
        !m.fake.find_live("Viagem").is_empty()
    });
    let folder = only(m.fake.find_live("Viagem"));
    wait_until("arquivo dentro da pasta", || {
        !m.fake.find_live("roteiro.txt").is_empty()
    });
    let file = only(m.fake.find_live("roteiro.txt"));
    assert_eq!(m.fake.meta(&file)["parents"], serde_json::json!([folder]));
    assert_eq!(m.fake.content(&file), b"dia 1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lock_files_exist_locally_but_never_upload() {
    let m = Mounted::new(FakeDrive::start().await).await;
    m.run_engine();
    let lock = m.drive().join(".~lock.planilha.ods#");
    std::fs::write(&lock, b"usuario,maquina").unwrap();
    std::fs::write(m.drive().join("planilha.ods"), b"dados").unwrap();

    wait_until("envio da planilha", || {
        !m.fake.find_live("planilha.ods").is_empty()
    });
    assert_eq!(std::fs::read(&lock).unwrap(), b"usuario,maquina");
    assert!(m.fake.find_live(".~lock.planilha.ods#").is_empty());
    std::fs::remove_file(&lock).unwrap();
    assert_eq!(names(&m.drive()), ["planilha.ods"]);
}

/// Bug: o download do Chrome (`.crdownload` renomeado para o nome final) ficava só no
/// computador para sempre; e o contrário, um arquivo pendente renomeado para nome de trava, subia.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn crdownload_renamed_to_final_name_uploads() {
    let m = Mounted::new(FakeDrive::start().await).await;
    m.run_engine();
    let partial = m.drive().join("relatorio.pdf.crdownload");
    std::fs::write(&partial, b"%PDF baixado").unwrap();
    std::fs::rename(&partial, m.drive().join("relatorio.pdf")).unwrap();
    let draft = m.drive().join("rascunho.txt");
    std::fs::write(&draft, b"vira trava").unwrap();
    std::fs::rename(&draft, m.drive().join("rascunho.txt.swp")).unwrap();

    wait_until("envio do download", || {
        !m.fake.find_live("relatorio.pdf").is_empty()
    });
    let id = only(m.fake.find_live("relatorio.pdf"));
    assert_eq!(m.fake.content(&id), b"%PDF baixado");
    assert!(m.fake.find_live("relatorio.pdf.crdownload").is_empty());
    assert!(m.fake.find_live("rascunho.txt").is_empty());
    assert!(m.fake.find_live("rascunho.txt.swp").is_empty());
}

/// Bug: esvaziar a lixeira do Dolphin com uma pasta dentro dava "permissão negada" (o KIO apaga
/// o conteúdo e depois a pasta).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn emptying_kio_trash_with_folder() {
    let fake = FakeDrive::start().await;
    let folder = fake.add_folder(&fake.root_id(), "Fotos");
    let photo = fake.add_file(&folder, "praia.jpg", b"jpeg".to_vec());
    let m = Mounted::new(fake).await;
    let trash = m.mnt.join(format!(".Trash-{}", uid()));
    std::fs::rename(m.drive().join("Fotos"), trash.join("files/Fotos")).unwrap();

    std::fs::remove_dir_all(trash.join("files/Fotos")).unwrap();

    assert!(names(&trash.join("files")).is_empty());
    assert_eq!(m.fake.meta(&folder)["trashed"], true);
    assert_eq!(
        m.fake.meta(&photo)["trashed"],
        false,
        "vai junto com a pasta"
    );
    assert_eq!(m.fake.requests_by_method("DELETE"), 0);
}

/// Bug: mandar para a lixeira do Dolphin um arquivo que ainda não tinha subido o apagava de vez,
/// e restaurar falhava.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn trashing_a_file_not_yet_uploaded_can_be_restored() {
    let m = Mounted::new(FakeDrive::start().await).await;
    let trash = m.mnt.join(format!(".Trash-{}", uid()));
    let path = m.drive().join("rascunho.txt");
    std::fs::write(&path, b"ainda nao subiu").unwrap();

    std::fs::rename(&path, trash.join("files/rascunho.txt")).unwrap();
    assert!(!names(&m.drive()).contains(&"rascunho.txt".to_string()));
    std::fs::rename(trash.join("files/rascunho.txt"), &path).unwrap();

    assert_eq!(std::fs::read(&path).unwrap(), b"ainda nao subiu");
    m.run_engine();
    wait_until("envio depois de restaurar", || {
        !m.fake.find_live("rascunho.txt").is_empty()
    });
    let id = only(m.fake.find_live("rascunho.txt"));
    assert_eq!(m.fake.content(&id), b"ainda nao subiu");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn canceled_uploads_folder_is_hidden_and_read_only() {
    let m = Mounted::new(FakeDrive::start().await).await;
    m.fake.inject(Fault::Offline);
    let file = m.drive().join("novo.txt");
    std::fs::write(&file, b"rascunho").unwrap();
    let id = ItemId(xattr(&file, "user.gdl.id"));

    assert_eq!(
        m.engine.cancel_upload(&id).await.unwrap(),
        CancelOutcome::Canceled
    );

    assert!(!file.exists(), "o arquivo que nunca subiu sai do Meu Drive");
    assert!(names(&m.mnt).contains(&".Uploads cancelados".to_string()));
    let dir = m.mnt.join(".Uploads cancelados");
    let saved = names(&dir);
    assert_eq!(saved.len(), 1, "{saved:?}");
    assert!(saved[0].starts_with("novo (local "), "{saved:?}");
    let copy = dir.join(&saved[0]);
    assert_eq!(std::fs::read(&copy).unwrap(), b"rascunho");
    assert_eq!(mode(&copy) & 0o777, 0o444);
    assert!(std::fs::write(&copy, b"outra coisa").is_err());
    assert!(std::fs::write(dir.join("outro.txt"), b"x").is_err());
    assert_eq!(names(&m.mnt.join(".Lost and Found")), Vec::<String>::new());
}

/// Cancelar com o arquivo aberto tiraria o conteúdo do programa que grava: só depois de fechar.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancel_waits_for_the_file_to_close() {
    use std::io::Write;
    let m = Mounted::new(FakeDrive::start().await).await;
    m.fake.inject(Fault::Offline);
    let file = m.drive().join("aberto.txt");
    std::fs::write(&file, b"primeira parte").unwrap();
    let id = ItemId(xattr(&file, "user.gdl.id"));
    let mut open = std::fs::OpenOptions::new()
        .append(true)
        .open(&file)
        .unwrap();

    assert_eq!(
        m.engine.cancel_upload(&id).await.unwrap(),
        CancelOutcome::InUse
    );
    open.write_all(b" e segunda").unwrap();
    drop(open);
    assert_eq!(std::fs::read(&file).unwrap(), b"primeira parte e segunda");

    assert_eq!(
        m.engine.cancel_upload(&id).await.unwrap(),
        CancelOutcome::Canceled
    );
    assert!(!file.exists());
}
