//! Dispositivos removíveis pelo UDisks2 falso: pergunta, backup, lembrar, ignorar e a política.

use std::path::PathBuf;

use gdl_ipc::DeviceItem;

use crate::testutil::{Harness, Note, poll_until, wait_until};

const UUID: &str = "1234-ABCD";
const LABEL: &str = "PENDRIVE";

/// Um pendrive com um arquivo, ainda não conectado.
fn pendrive(h: &Harness) -> PathBuf {
    let mount = h.home.join("media").join(LABEL);
    std::fs::create_dir_all(mount.join("fotos")).unwrap();
    std::fs::write(mount.join("fotos/praia.jpg"), b"praia").unwrap();
    mount
}

/// Os avisos deste pendrive com o título `title`.
fn notes_titled(h: &Harness, title: &str) -> Vec<Note> {
    h.notes()
        .into_iter()
        .filter(|n| n.summary == title && n.body.contains(LABEL))
        .collect()
}

/// Espera a lista de `Devices` satisfazer `done`.
async fn devices_until(h: &Harness, what: &str, done: impl Fn(&Vec<DeviceItem>) -> bool) {
    poll_until(what, || h.proxy.devices(), done).await;
}

/// Desconecta o pendrive e espera o serviço ver a saída.
async fn unplug(h: &Harness) {
    h.unplug(UUID).await;
    devices_until(h, "desconectado", |d| d.iter().all(|d| d.path.is_empty())).await;
}

const ASK: &str = "Fazer backup do dispositivo?";
const DONE: &str = "Backup concluído";

/// O arquivo `name` chegou a "Computadores/<este computador>/PENDRIVE" no Drive.
async fn backed_up(h: &Harness, name: &str) {
    wait_until(name, || h.fake.find_live(name).len() == 1).await;
    let file = h.fake.meta(&h.fake.find_live(name)[0]);
    let mut parent = file["parents"][0].as_str().unwrap().to_owned();
    // Sobe até a pasta do dispositivo.
    loop {
        let meta = h.fake.meta(&parent);
        if meta["name"] == LABEL {
            let computer = h.fake.meta(meta["parents"][0].as_str().unwrap());
            assert_eq!(computer["name"], crate::testutil::COMPUTER_NAME);
            return;
        }
        parent = meta["parents"][0].as_str().unwrap().to_owned();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn plugged_device_asks_for_backup() {
    let h = Harness::new().await;
    h.logged_in().await;
    let mount = pendrive(&h);

    h.plug(UUID, LABEL, &mount).await;

    wait_until("pergunta", || notes_titled(&h, ASK).len() == 1).await;
    let ask = &notes_titled(&h, ASK)[0];
    assert!(ask.actions.contains(&"open-folders".to_string()), "{ask:?}");
    let listed = h.proxy.devices().await.unwrap();
    assert_eq!(
        listed,
        vec![DeviceItem {
            id: UUID.into(),
            label: LABEL.into(),
            path: mount.display().to_string(),
            ..Default::default()
        }]
    );

    h.proxy.backup_device(UUID, false).await.unwrap();

    backed_up(&h, "praia.jpg").await;
    wait_until("aviso de concluído", || notes_titled(&h, DONE).len() == 1).await;
    let done = &notes_titled(&h, DONE)[0];
    assert!(done.body.contains("remover"), "{done:?}");
    assert_eq!(
        done.replaces_id, ask.id,
        "o aviso de concluído substitui a pergunta"
    );
    let listed = h.proxy.devices().await.unwrap();
    assert_eq!(listed[0].choice, "", "sem lembrar, pergunta de novo");
    assert!(!listed[0].busy);
    assert_eq!(
        h.proxy.local_folders().await.unwrap(),
        vec![],
        "o backup não vira pasta do computador"
    );
    assert_eq!(
        std::fs::read(mount.join("fotos/praia.jpg")).unwrap(),
        b"praia"
    );

    unplug(&h).await;
    assert_eq!(
        h.proxy.devices().await.unwrap(),
        vec![],
        "sem escolha, sai da lista"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn remembered_device_backs_up_and_says_it_is_safe() {
    let h = Harness::new().await;
    h.logged_in().await;
    let mount = pendrive(&h);
    h.plug(UUID, LABEL, &mount).await;
    wait_until("pergunta", || notes_titled(&h, ASK).len() == 1).await;

    h.proxy.backup_device(UUID, true).await.unwrap();
    wait_until("primeiro backup", || notes_titled(&h, DONE).len() == 1).await;
    unplug(&h).await;
    std::fs::write(mount.join("novo.txt"), b"novo").unwrap();
    h.plug(UUID, LABEL, &mount).await;

    backed_up(&h, "novo.txt").await;
    wait_until("segundo backup", || notes_titled(&h, DONE).len() == 2).await;
    assert_eq!(notes_titled(&h, ASK).len(), 1, "lembrado não pergunta");
    assert_eq!(
        h.fake.find_live(LABEL).len(),
        1,
        "a mesma pasta no Drive, sem \"PENDRIVE (2)\""
    );
    assert_eq!(
        h.fake.find_live("praia.jpg").len(),
        1,
        "nada sobe duas vezes"
    );
    let listed = h.proxy.devices().await.unwrap();
    assert_eq!(listed[0].choice, "backup");

    h.proxy.forget_device(UUID).await.unwrap();
    assert_eq!(h.proxy.devices().await.unwrap()[0].choice, "");
}

/// A pergunta oferece, como no original, "Fazer backup sempre" e "Ignorar este dispositivo" no
/// próprio aviso (as mesmas escolhas de Preferências > Meu computador).
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn device_notice_offers_always_and_ignore() {
    let h = Harness::new().await;
    h.logged_in().await;
    let mount = pendrive(&h);
    h.plug(UUID, LABEL, &mount).await;
    wait_until("pergunta", || notes_titled(&h, ASK).len() == 1).await;
    let ask = notes_titled(&h, ASK)[0].clone();
    let keys: Vec<_> = ask.actions.iter().step_by(2).map(String::as_str).collect();
    assert_eq!(keys, ["open-folders", "device-always", "device-ignore"]);

    h.daemon.on_desktop_action(ask.id, "device-always").await;

    backed_up(&h, "praia.jpg").await;
    let listed = h.proxy.devices().await.unwrap();
    assert_eq!(listed[0].choice, "backup", "lembrado");
    wait_until("aviso de concluído", || notes_titled(&h, DONE).len() == 1).await;

    // O outro botão, num aviso novo: o dispositivo passa a ser ignorado.
    h.proxy.forget_device(UUID).await.unwrap();
    unplug(&h).await;
    h.plug(UUID, LABEL, &mount).await;
    wait_until("pergunta de novo", || notes_titled(&h, ASK).len() == 2).await;
    let again = notes_titled(&h, ASK)[1].clone();
    h.daemon.on_desktop_action(again.id, "device-ignore").await;
    assert_eq!(h.proxy.devices().await.unwrap()[0].choice, "ignored");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn ignored_device_is_not_asked() {
    let h = Harness::new().await;
    h.logged_in().await;
    let mount = pendrive(&h);
    h.plug(UUID, LABEL, &mount).await;
    wait_until("pergunta", || notes_titled(&h, ASK).len() == 1).await;

    h.proxy.ignore_device(UUID).await.unwrap();
    unplug(&h).await;
    h.plug(UUID, LABEL, &mount).await;
    devices_until(&h, "de volta na lista", |d| {
        d.first().is_some_and(|d| !d.path.is_empty())
    })
    .await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert_eq!(notes_titled(&h, ASK).len(), 1, "ignorado não pergunta");
    assert!(h.fake.find_live("praia.jpg").is_empty());
    let listed = h.proxy.devices().await.unwrap();
    assert_eq!(listed[0].choice, "ignored");
    let saved = std::fs::read_to_string(h.paths().data_dir.join("devices")).unwrap();
    assert!(saved.contains(UUID), "{saved}");

    // Esquecido, volta a perguntar na próxima vez.
    h.proxy.forget_device(UUID).await.unwrap();
    unplug(&h).await;
    h.plug(UUID, LABEL, &mount).await;
    wait_until("pergunta de novo", || notes_titled(&h, ASK).len() == 2).await;
    let err = h.proxy.forget_device("outro").await.unwrap_err();
    assert_eq!(err.short_name(), "NotFound", "{err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn external_media_policy_turns_it_off() {
    let h = Harness::with_policy("DisableExternalMediaSync = true").await;
    h.logged_in().await;
    let mount = pendrive(&h);

    h.plug(UUID, LABEL, &mount).await;
    devices_until(&h, "na lista", |d| d.len() == 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    assert!(notes_titled(&h, ASK).is_empty(), "a política não pergunta");
    let err = h.proxy.backup_device(UUID, false).await.unwrap_err();
    assert_eq!(err.short_name(), "Refused", "{err}");
    assert!(err.to_string().contains("devices-disabled"), "{err}");
    assert!(h.fake.find_live("praia.jpg").is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn turning_off_the_question_still_lets_the_user_back_up() {
    let h = Harness::with_policy("PromptToBackupDevices = false").await;
    h.logged_in().await;
    let mount = pendrive(&h);

    h.plug(UUID, LABEL, &mount).await;
    devices_until(&h, "na lista", |d| d.len() == 1).await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    assert!(notes_titled(&h, ASK).is_empty());

    h.proxy.backup_device(UUID, false).await.unwrap();
    backed_up(&h, "praia.jpg").await;
}

/// Bug: o rótulo do pendrive (escolhido por quem o formatou) ia como marcação para o corpo do
/// aviso, que o servidor de notificações interpreta.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn device_label_is_not_markup() {
    let h = Harness::new().await;
    h.logged_in().await;
    let mount = pendrive(&h);

    h.plug(UUID, "<a href='x'>A&B</a>", &mount).await;

    wait_until("pergunta", || h.notes().iter().any(|n| n.summary == ASK)).await;
    let ask = h.notes().into_iter().find(|n| n.summary == ASK).unwrap();
    assert!(
        ask.body.contains("&lt;a href='x'&gt;A&amp;B&lt;/a&gt;"),
        "{}",
        ask.body
    );
}

/// Bug da revisão final: o par do backup ficava com o ponto de montagem. Outro pendrive com o
/// mesmo rótulo montava no mesmo lugar e não fazia backup (caminho já de outro par), e o lugar
/// aparecia como pasta sincronizada no Dolphin.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn another_device_with_the_same_label_backs_up_too() {
    let h = Harness::new().await;
    h.logged_in().await;
    let mount = pendrive(&h);
    h.plug(UUID, LABEL, &mount).await;
    wait_until("pergunta", || notes_titled(&h, ASK).len() == 1).await;
    h.proxy.backup_device(UUID, false).await.unwrap();
    wait_until("primeiro backup", || notes_titled(&h, DONE).len() == 1).await;
    unplug(&h).await;

    assert_eq!(h.proxy.folder_roots().await.unwrap(), Vec::<String>::new());
    let shown = mount.join("fotos/praia.jpg").display().to_string();
    assert_eq!(h.proxy.folder_states(&[&shown]).await.unwrap(), vec![""]);

    std::fs::write(mount.join("outra.txt"), b"outra").unwrap();
    h.plug("9999-FFFF", LABEL, &mount).await;
    wait_until("pergunta", || notes_titled(&h, ASK).len() == 2).await;
    h.proxy.backup_device("9999-FFFF", false).await.unwrap();

    wait_until("segundo backup", || notes_titled(&h, DONE).len() == 2).await;
    wait_until("outra.txt", || h.fake.find_live("outra.txt").len() == 1).await;
}
