//! Ambiente de teste: Drive simulado, motor e uma montagem FUSE real num diretório temporário.

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use gdl_common::config::load_preferences;
use gdl_common::i18n::I18n;
use gdl_common::paths::Paths;
use gdl_drive::client::{Backoff, DriveClient};
use gdl_drive::oauth::{AccessTokenProvider, AuthError};
use gdl_engine::changes::ChangePoller;
use gdl_engine::engine::{Engine, Intervals};
use gdl_fake_drive::{FakeDrive, TEST_ACCESS_TOKEN};
use tokio_util::sync::CancellationToken;

use crate::{DriveFs, Layout, mount};

pub(crate) const MIB: usize = 1024 * 1024;

struct TestToken;

#[async_trait::async_trait]
impl AccessTokenProvider for TestToken {
    async fn access_token(&self) -> Result<String, AuthError> {
        Ok(TEST_ACCESS_TOKEN.into())
    }

    async fn refresh_now(&self, _stale: &str) -> Result<String, AuthError> {
        Ok(TEST_ACCESS_TOKEN.into())
    }
}

fn client(fake: &FakeDrive) -> DriveClient {
    DriveClient::new(fake.base_url(), fake.upload_url(), Arc::new(TestToken)).with_backoff(
        Backoff {
            base: std::time::Duration::from_millis(1),
            max: std::time::Duration::from_millis(2),
            tries: 2,
        },
    )
}

pub(crate) struct Mounted {
    pub fake: FakeDrive,
    pub engine: Arc<Engine>,
    pub fs: DriveFs,
    pub mnt: PathBuf,
    /// O cache de miniaturas do freedesktop deste teste (`Paths::thumbnails_dir`).
    pub thumbs: PathBuf,
    _session: fuser::BackgroundSession,
    shutdown: CancellationToken,
    _dir: tempfile::TempDir,
}

impl Mounted {
    pub async fn new(fake: FakeDrive) -> Mounted {
        Mounted::with_lang(fake, "pt-BR").await
    }

    pub async fn with_lang(fake: FakeDrive, lang: &str) -> Mounted {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths::for_test(dir.path());
        let (prefs, _) = load_preferences(&paths).unwrap();
        let engine = Engine::open(&paths, &prefs, "teste@example.com", client(&fake))
            .unwrap()
            .with_intervals(Intervals {
                changes: Duration::from_millis(50),
                uploads_idle: Duration::from_millis(20),
                maintenance: Duration::from_millis(50),
                eviction: Duration::from_millis(200),
            });
        engine.ensure_indexed().await.unwrap();
        let engine = Arc::new(engine);
        let mnt = dir.path().join("montagem");
        std::fs::create_dir(&mnt).unwrap();
        let fs = DriveFs::new(
            engine.clone(),
            Layout::new(&I18n::new(lang)),
            uid(),
            gid(),
            mnt.clone(),
        );
        let session = mount(fs.clone(), &mnt).unwrap();
        Mounted {
            fake,
            engine,
            fs,
            mnt,
            thumbs: paths.thumbnails_dir.clone(),
            _session: session,
            shutdown: CancellationToken::new(),
            _dir: dir,
        }
    }

    pub fn drive(&self) -> PathBuf {
        self.mnt.join("Meu Drive")
    }

    /// Roda os laços do motor (envios, mudanças) até o fim do teste.
    pub fn run_engine(&self) {
        let (engine, shutdown) = (self.engine.clone(), self.shutdown.clone());
        tokio::spawn(async move { engine.run(shutdown).await });
    }

    pub async fn poll_changes(&self) {
        let (tx, _) = tokio::sync::broadcast::channel(16);
        let poller = ChangePoller::new(
            self.engine.store().clone(),
            Arc::new(client(&self.fake)),
            self.engine.cache().clone(),
            tx,
        );
        poller.run_once().await.unwrap();
    }
}

pub(crate) fn uid() -> u32 {
    rustix::process::getuid().as_raw()
}

fn gid() -> u32 {
    rustix::process::getgid().as_raw()
}

pub(crate) fn names(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

pub(crate) fn mode(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o777
}

pub(crate) fn xattr(path: &Path, name: &str) -> String {
    let mut buf = vec![0; 256];
    let len = rustix::fs::getxattr(path, name, &mut buf).unwrap();
    String::from_utf8(buf[..len].to_vec()).unwrap()
}

impl Drop for Mounted {
    fn drop(&mut self) {
        self.shutdown.cancel();
    }
}

/// Espera até `done` ficar verdadeiro, falhando o teste depois de 10 s. Bloqueia a thread (os
/// testes rodam no runtime de várias threads).
pub(crate) fn wait_until(what: &str, mut done: impl FnMut() -> bool) {
    let start = std::time::Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "esperando: {what}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
}
