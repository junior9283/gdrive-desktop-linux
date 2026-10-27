//! Um par em ação: lê o computador, o Drive (pelo banco que o motor da conta mantém em dia) e o
//! último estado sincronizado, decide cada caminho (`decide`) e aplica.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io::{self, Read};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, UNIX_EPOCH};

use gdl_common::names::{fit, split_ext, to_remote};
use gdl_common::temp_files::is_local_only;
use gdl_drive::client::{DriveClient, RemoteFile, SessionSlot};
use gdl_drive::model::Change;
use gdl_store::folders::{Pair, PairState, Synced};
use gdl_store::{Item, ItemId, ItemKind, Store};
use md5::{Digest, Md5};
use rustix::fs::{CWD, RenameFlags, renameat_with};
use tokio::io::AsyncWriteExt;

use crate::decide::{Action, Base, Side, decide};
use crate::guard;
use crate::trash::Trash;

#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    #[error(transparent)]
    Store(#[from] gdl_store::StoreError),
    #[error(transparent)]
    Engine(#[from] gdl_engine::EngineError),
    #[error(transparent)]
    Drive(#[from] gdl_drive::client::DriveError),
    #[error("erro de arquivo local: {0}")]
    Io(#[from] std::io::Error),
    #[error("a pasta de cima ainda não existe no Drive")]
    NoParent,
    #[error("o caminho passa por um link simbólico")]
    Symlink,
}

pub type Result<T> = std::result::Result<T, SyncError>;

/// Uma comparação para quando nada mais muda (um conflito ou um renomear pede outra passada).
const MAX_PASSES: usize = 3;
/// O download chega num temporário ao lado (e oculto) e só então toma o lugar do arquivo.
const PART_SUFFIX: &str = ".gdl-part";

/// O que uma comparação fez.
#[derive(Debug, Default)]
pub struct Report {
    /// Ações aplicadas (sem contar "nada a fazer").
    pub applied: usize,
    /// Caminhos que falharam na última passada, com o erro; os outros seguem.
    pub failed: Vec<(String, String)>,
}

pub struct FolderSync {
    store: Arc<Store>,
    client: Arc<DriveClient>,
    pair: Pair,
    trash: Trash,
    /// A palavra do sufixo de conflito, no idioma do sistema.
    conflict_word: String,
    /// Quantas exclusões o usuário confirmou: a próxima comparação aplica até esse tanto.
    deletes_confirmed: AtomicU64,
}

impl FolderSync {
    pub fn new(
        store: Arc<Store>,
        client: Arc<DriveClient>,
        pair: Pair,
        trash: Trash,
        conflict_word: String,
    ) -> FolderSync {
        FolderSync {
            store,
            client,
            pair,
            trash,
            conflict_word,
            deletes_confirmed: AtomicU64::new(0),
        }
    }

    pub fn pair(&self) -> &Pair {
        &self.pair
    }

    pub(crate) fn store(&self) -> &Store {
        &self.store
    }

    /// O usuário confirmou as exclusões que a comparação segurou (`PairState::ConfirmDeletes`).
    pub fn confirm_deletes(&self) -> Result<()> {
        if let PairState::ConfirmDeletes(n) = self.state()? {
            self.deletes_confirmed.store(n, Ordering::Relaxed);
            self.set_state(PairState::Active)?;
        }
        Ok(())
    }

    fn state(&self) -> Result<PairState> {
        let pairs = self.store.pairs()?;
        let this = pairs.into_iter().find(|p| p.id == self.pair.id);
        Ok(this.map_or(PairState::Active, |p| p.state))
    }

    /// Grava o estado só quando ele muda.
    fn set_state(&self, state: PairState) -> Result<()> {
        if self.state()? != state {
            self.store.set_pair_state(self.pair.id, state)?;
        }
        Ok(())
    }

    /// Compara os dois lados e aplica o que mudou. Com a pasta do computador inacessível, o par
    /// para (`PairState::Stopped`) sem tocar em nada: nunca é tratado como "tudo excluído".
    pub async fn reconcile(&self) -> Result<Report> {
        if let Some(reason) = guard::root_problem(&self.pair.local) {
            tracing::warn!(pair = self.pair.id, ?reason, "pasta do par inacessível");
            self.set_state(PairState::Stopped(reason))?;
            return Ok(Report::default());
        }
        let mut report = Report::default();
        for _ in 0..MAX_PASSES {
            report.failed.clear();
            if !self.pass(&mut report).await? {
                break;
            }
        }
        Ok(report)
    }

    /// Uma passada; verdadeiro se outra é necessária.
    async fn pass(&self, report: &mut Report) -> Result<bool> {
        let mut v = self.view().await?;
        // Renomear ou mover num lado vira o mesmo no outro (sem novo envio nem download).
        if self.follow_local_renames(&v).await? | self.follow_remote_renames(&v)? {
            return Ok(true);
        }
        let paths: BTreeSet<String> = v
            .locals
            .keys()
            .chain(v.remotes.keys())
            .chain(v.bases.keys())
            .cloned()
            .collect();
        let mut plan = vec![];
        for rel in paths {
            match self.plan(&v, &rel).await {
                Ok(Action::Nothing) => {}
                Ok(action) => plan.push((rel, action)),
                Err(e) => self.failed(report, rel, e),
            }
        }
        let is_delete = |a: &Action| matches!(a, Action::TrashLocal | Action::TrashRemote);
        let deletes = plan.iter().filter(|(_, a)| is_delete(a)).count();
        // A confirmação vale para o tanto que o usuário viu, e só uma vez.
        let confirmed = self.deletes_confirmed.swap(0, Ordering::Relaxed);
        let held = guard::is_mass_delete(deletes, v.bases.len()) && deletes as u64 > confirmed;
        if held {
            // O resto segue; as exclusões esperam o usuário.
            plan.retain(|(_, a)| !is_delete(a));
        }
        self.set_state(match held {
            true => PairState::ConfirmDeletes(deletes as u64),
            false => PairState::Active,
        })?;
        for (rel, action) in plan {
            if v.gone.as_deref().is_some_and(|g| inside(&rel, g)) {
                continue;
            }
            match self.apply(&mut v, &rel, action).await {
                Ok(()) => report.applied += 1,
                Err(e) => self.failed(report, rel, e),
            }
        }
        Ok(v.again)
    }

    fn failed(&self, report: &mut Report, rel: String, e: SyncError) {
        tracing::warn!(pair = self.pair.id, rel, "falha ao sincronizar: {e}");
        report.failed.push((rel, e.to_string()));
    }

    async fn view(&self) -> Result<View> {
        let root = self.pair.local.clone();
        let locals = tokio::task::spawn_blocking(move || scan(&root))
            .await
            .map_err(io::Error::other)??;
        let remotes: BTreeMap<String, Item> = self
            .store
            .descendants(&self.pair.remote_id)?
            .into_iter()
            .filter(|(rel, item)| {
                matches!(item.kind, ItemKind::File | ItemKind::Folder) && !ignored(last(rel))
            })
            .collect();
        let root = (String::new(), self.pair.remote_id.0.clone());
        let dirs = remotes
            .iter()
            .filter(|(_, item)| item.kind == ItemKind::Folder)
            .map(|(rel, item)| (rel.clone(), item.id.0.clone()))
            .chain([root])
            .collect();
        let bases = self
            .store
            .synced_of(self.pair.id)?
            .into_iter()
            .map(|s| (s.rel_path.clone(), s))
            .collect();
        Ok(View {
            locals,
            remotes,
            bases,
            dirs,
            gone: None,
            again: false,
        })
    }

    /// No computador, o caminho novo sem base tem o inode de uma base cujo caminho sumiu: o mesmo
    /// renomear (ou mover) é feito no Drive.
    async fn follow_local_renames(&self, v: &View) -> Result<bool> {
        let vanished: HashMap<u64, &Synced> = v
            .bases
            .values()
            .filter(|b| !v.locals.contains_key(&b.rel_path))
            .map(|b| (b.inode, b))
            .collect();
        let mut moved = false;
        for (rel, local) in &v.locals {
            let Some(base) = vanished.get(&local.inode).filter(|b| local.matches(b)) else {
                continue;
            };
            let Some(item) = v
                .remotes
                .get(&base.rel_path)
                .filter(|i| i.id == base.item_id)
            else {
                continue;
            };
            if v.bases.contains_key(rel) || v.remotes.contains_key(rel) {
                continue;
            }
            let Ok(new_parent) = v.parent_id(rel) else {
                continue;
            };
            let name = to_remote(last(rel));
            let old_parent = item.parent.as_ref().map(|p| p.0.as_str());
            let other_folder = old_parent != Some(new_parent);
            let renamed = self
                .client
                .rename_move(
                    &item.id.0,
                    (name != item.remote_name).then_some(name.as_str()),
                    other_folder.then_some(new_parent),
                    old_parent.filter(|_| other_folder),
                )
                .await;
            match renamed {
                Ok(r) => {
                    self.remember(r)?;
                    self.store
                        .rename_synced(self.pair.id, &base.rel_path, rel)?;
                    moved = true;
                }
                // Sem o renomear, a passada envia o novo e manda o antigo para a lixeira.
                Err(e) => tracing::warn!(pair = self.pair.id, rel, "renomear no Drive: {e}"),
            }
        }
        Ok(moved)
    }

    /// No Drive, o ID de uma base está noutro caminho: o mesmo renomear é feito aqui.
    fn follow_remote_renames(&self, v: &View) -> Result<bool> {
        let mut moved = false;
        let remote_paths: HashMap<&ItemId, &String> = v
            .remotes
            .iter()
            .map(|(rel, item)| (&item.id, rel))
            .collect();
        for base in v.bases.values() {
            let Some(&to) = remote_paths.get(&base.item_id) else {
                continue;
            };
            let parent_here = to
                .rsplit_once('/')
                .is_none_or(|(p, _)| v.locals.get(p).is_some_and(|l| l.is_dir));
            let free = !v.bases.contains_key(to) && !v.locals.contains_key(to);
            let unchanged = v
                .locals
                .get(&base.rel_path)
                .is_some_and(|l| l.matches(base));
            if *to == base.rel_path
                || !(parent_here && free && unchanged)
                || self.beneath(to).is_err()
                || self.beneath(&base.rel_path).is_err()
            {
                continue;
            }
            let renamed = renameat_with(
                CWD,
                self.path(&base.rel_path),
                CWD,
                self.path(to),
                RenameFlags::NOREPLACE,
            );
            match renamed {
                Ok(()) => {
                    self.store.rename_synced(self.pair.id, &base.rel_path, to)?;
                    moved = true;
                }
                Err(e) => tracing::warn!(pair = self.pair.id, to, "renomear aqui: {e}"),
            }
        }
        Ok(moved)
    }

    /// O que fazer com um caminho.
    async fn plan(&self, v: &View, rel: &str) -> Result<Action> {
        let local = v.locals.get(rel).copied();
        let remote = v.remotes.get(rel);
        let base = v.bases.get(rel);
        // O md5 local só é lido quando a decisão depende dele (ver `Side::File`).
        let need_md5 = local.is_some_and(|l| {
            !l.is_dir
                && match base {
                    Some(b) => !l.matches(b),
                    None => remote.is_some_and(|r| r.kind == ItemKind::File),
                }
        });
        let md5 = match need_md5 {
            true => Some(self.md5(rel).await?),
            false => None,
        };
        let local_side = local.map_or(Side::Absent, |l| l.side(md5));
        let remote_side = remote.map_or(Side::Absent, remote_side);
        let base_side = base.map(Base::from);
        let action = decide(
            &local_side,
            &remote_side,
            base_side.as_ref(),
            self.pair.direction,
        );
        Ok(v.keep_dirs(rel, action))
    }

    /// Aplica `action` a um caminho.
    async fn apply(&self, v: &mut View, rel: &str, action: Action) -> Result<()> {
        let local = v.locals.get(rel).copied();
        let remote = v.remotes.get(rel).cloned();
        let path = self.path(rel);
        if matches!(
            action,
            Action::Download | Action::CreateLocalDir | Action::TrashLocal | Action::Conflict
        ) {
            self.beneath(rel)?;
        }
        match action {
            Action::Nothing => {}
            Action::UpdateBase => {
                let (l, r) = (local.expect("lado local"), remote.expect("lado do Drive"));
                self.put(rel, &r.id.0, l, r.md5)?;
            }
            Action::ForgetBase => self.forget(v, rel)?,
            Action::Upload => {
                let existing = remote.as_ref().map(|r| r.id.0.as_str());
                let (r, sent) = self.upload(v, rel, existing, to_remote(last(rel))).await?;
                self.put(rel, &r.id, sent, r.md5)?;
            }
            Action::UploadAsConflict => {
                let name = self.conflict_name(&to_remote(last(rel)), 0);
                let (_, sent) = self.upload(v, rel, None, name).await?;
                // A versão do Drive fica no caminho; a base guarda o computador como está.
                let kept = remote.expect("lado do Drive");
                self.put(rel, &kept.id.0, sent, kept.md5)?;
            }
            Action::Download => {
                let r = remote.expect("lado do Drive");
                match self.download(&path, &r, local).await? {
                    Some(l) => self.put(rel, &r.id.0, l, r.md5)?,
                    // O arquivo mudou durante o download: a próxima passada decide de novo.
                    None => v.again = true,
                }
            }
            Action::CreateRemoteDir => {
                let parent = v.parent_id(rel)?.to_owned();
                let r = self
                    .client
                    .create_folder(&parent, &to_remote(last(rel)), None)
                    .await?;
                let id = r.id.clone();
                v.dirs.insert(rel.into(), id.clone());
                self.remember(r)?;
                self.put(rel, &id, local.expect("lado local"), None)?;
            }
            Action::CreateLocalDir => {
                std::fs::create_dir(&path)?;
                let made = Local::existing(&path)?;
                self.put(rel, &remote.expect("lado do Drive").id.0, made, None)?;
            }
            Action::TrashRemote => {
                let r = self
                    .client
                    .set_trashed(&remote.expect("lado do Drive").id.0, true)
                    .await?;
                self.remember(r)?;
                self.forget(v, rel)?;
            }
            Action::TrashLocal => {
                if Local::stat(&path)? == local {
                    self.trash.put(&path)?;
                    self.forget(v, rel)?;
                } else {
                    v.again = true;
                }
            }
            Action::Conflict => {
                self.set_aside(&path)?;
                v.again = true;
            }
        }
        Ok(())
    }

    async fn upload(
        &self,
        v: &View,
        rel: &str,
        existing: Option<&str>,
        name: String,
    ) -> Result<(RemoteFile, Local)> {
        let path = self.path(rel);
        let sent = Local::existing(&path)?;
        let session = KvSession {
            store: &self.store,
            key: format!("folders:{}:{rel}", self.pair.id),
            stamp: format!("{}:{}", sent.size, sent.mtime_ns),
        };
        let modified_ms = sent.mtime_ns / 1_000_000;
        let r = match existing {
            Some(id) => {
                self.client
                    .upload_update(id, &path, modified_ms, &session)
                    .await?
            }
            None => {
                self.client
                    .upload_new(v.parent_id(rel)?, &name, &path, modified_ms, &session, None)
                    .await?
            }
        };
        self.remember(r.clone())?;
        Ok((r, sent))
    }

    /// Baixa `item` para `path` se o computador ainda está como em `expected`; devolve o
    /// arquivo novo, ou `None` se ele mudou nesse meio-tempo.
    async fn download(
        &self,
        path: &Path,
        item: &Item,
        expected: Option<Local>,
    ) -> Result<Option<Local>> {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let part = path.with_file_name(format!(".{name}{PART_SUFFIX}"));
        let result = async {
            // Um temporário largado (ou plantado como link) sai antes; o novo é criado sem seguir
            // links e sem abrir nada que já exista.
            match std::fs::remove_file(&part) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e.into()),
                _ => {}
            }
            let mut file = tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
                .open(&part)
                .await?;
            if item.remote_size > 0 {
                let mut body = self
                    .client
                    .download_stream(&item.id.0, 0, item.remote_size)
                    .await?;
                while let Some(piece) = body.chunk().await? {
                    file.write_all(&piece).await?;
                }
            }
            file.sync_all().await?;
            let modified = UNIX_EPOCH + Duration::from_millis(item.modified_ms.max(0) as u64);
            file.into_std().await.set_modified(modified)?;
            if Local::stat(path)? != expected {
                std::fs::remove_file(&part)?;
                return Ok(None);
            }
            std::fs::rename(&part, path)?;
            Ok(Local::stat(path)?)
        }
        .await;
        if result.is_err() {
            let _ = std::fs::remove_file(&part);
        }
        result
    }

    /// Tira a versão do computador do caminho, com o sufixo de conflito, sem sobrescrever nada.
    fn set_aside(&self, path: &Path) -> Result<()> {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let mut n = 0;
        loop {
            let aside = path.with_file_name(self.conflict_name(&name, n));
            match renameat_with(CWD, path, CWD, &aside, RenameFlags::NOREPLACE) {
                Err(rustix::io::Errno::EXIST) => n += 1,
                done => return Ok(done.map_err(io::Error::from)?),
            }
        }
    }

    /// `nome (conflito AAAA-MM-DD HHhMM).ext`, com um número a partir do segundo.
    fn conflict_name(&self, name: &str, n: u32) -> String {
        let (stem, ext) = split_ext(name);
        let stamp = chrono::Local::now().format("%Y-%m-%d %Hh%M");
        let word = &self.conflict_word;
        let suffix = match n {
            0 => format!(" ({word} {stamp}){ext}"),
            n => format!(" ({word} {stamp} {}){ext}", n + 1),
        };
        fit(stem, &suffix)
    }

    async fn md5(&self, rel: &str) -> Result<String> {
        let path = self.path(rel);
        let digest = tokio::task::spawn_blocking(move || -> io::Result<String> {
            let mut file = std::fs::File::open(path)?;
            let (mut hasher, mut buf) = (Md5::new(), vec![0; 256 * 1024]);
            loop {
                match file.read(&mut buf)? {
                    0 => break,
                    n => hasher.update(&buf[..n]),
                }
            }
            Ok(hasher
                .finalize()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect())
        })
        .await
        .map_err(io::Error::other)??;
        Ok(digest)
    }

    /// Grava no banco o item como o Drive o devolveu, pelo mesmo caminho das mudanças lidas.
    fn remember(&self, r: RemoteFile) -> Result<()> {
        gdl_engine::changes::apply_changes(&self.store, vec![Change::Upsert(Box::new(r))])?;
        Ok(())
    }

    fn put(&self, rel: &str, id: &str, l: Local, md5: Option<String>) -> Result<()> {
        let synced = Synced {
            rel_path: rel.into(),
            item_id: ItemId(id.into()),
            is_dir: l.is_dir,
            size: l.size,
            mtime_ns: l.mtime_ns,
            inode: l.inode,
            md5,
        };
        self.store.put_synced(self.pair.id, &[synced])?;
        Ok(())
    }

    /// Esquece o caminho (e o que há abaixo, que a passada pula).
    fn forget(&self, v: &mut View, rel: &str) -> Result<()> {
        self.store.forget_synced(self.pair.id, rel)?;
        v.gone = Some(rel.into());
        Ok(())
    }

    /// Nenhuma pasta entre a raiz do par e `rel` é link simbólico: o que se escreve ali não sai
    /// do par (a varredura não segue links, então o Drive pode ter um caminho "através" de um).
    fn beneath(&self, rel: &str) -> Result<()> {
        let mut dir = self.pair.local.clone();
        let parent = rel.rsplit_once('/').map_or("", |(parent, _)| parent);
        for part in parent.split('/').filter(|p| !p.is_empty()) {
            dir.push(part);
            if !std::fs::symlink_metadata(&dir)?.is_dir() {
                return Err(SyncError::Symlink);
            }
        }
        Ok(())
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.pair.local.join(rel)
    }
}

/// Os três lados de uma passada, mais o que ela já fez.
struct View {
    locals: BTreeMap<String, Local>,
    remotes: BTreeMap<String, Item>,
    bases: BTreeMap<String, Synced>,
    /// IDs das pastas do Drive pelo caminho (`""` é a do par), inclusive as criadas agora.
    dirs: HashMap<String, String>,
    /// O último caminho resolvido por inteiro (pasta na lixeira, base esquecida): os caminhos
    /// vêm em ordem, então o que está abaixo dele vem logo em seguida.
    gone: Option<String>,
    again: bool,
}

impl View {
    fn parent_id(&self, rel: &str) -> Result<&str> {
        let parent = rel.rsplit_once('/').map_or("", |(parent, _)| parent);
        self.dirs
            .get(parent)
            .map(String::as_str)
            .ok_or(SyncError::NoParent)
    }

    /// Uma pasta apagada de um lado não leva junto o que é novo do outro: ela é recriada.
    fn keep_dirs(&self, rel: &str, action: Action) -> Action {
        match action {
            Action::TrashLocal
                if self.locals.get(rel).is_some_and(|l| l.is_dir)
                    && under(&self.locals, rel)
                        .any(|(p, l)| self.bases.get(p).is_none_or(|b| !l.matches(b))) =>
            {
                Action::CreateRemoteDir
            }
            Action::TrashRemote
                if self
                    .remotes
                    .get(rel)
                    .is_some_and(|r| r.kind == ItemKind::Folder)
                    && under(&self.remotes, rel).any(|(p, r)| {
                        self.bases
                            .get(p)
                            .is_none_or(|b| r.kind == ItemKind::File && r.md5 != b.md5)
                    }) =>
            {
                Action::CreateLocalDir
            }
            other => other,
        }
    }
}

/// Um arquivo ou pasta do computador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Local {
    is_dir: bool,
    size: u64,
    mtime_ns: i64,
    inode: u64,
}

impl Local {
    fn of(m: &std::fs::Metadata) -> Local {
        Local {
            is_dir: m.is_dir(),
            size: if m.is_dir() { 0 } else { m.len() },
            mtime_ns: m.mtime() * 1_000_000_000 + m.mtime_nsec(),
            inode: m.ino(),
        }
    }

    fn existing(path: &Path) -> io::Result<Local> {
        Ok(Local::of(&std::fs::symlink_metadata(path)?))
    }

    fn stat(path: &Path) -> io::Result<Option<Local>> {
        match std::fs::symlink_metadata(path) {
            Ok(m) => Ok(Some(Local::of(&m))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e),
        }
    }

    fn side(&self, md5: Option<String>) -> Side {
        match self.is_dir {
            true => Side::Dir,
            false => Side::File {
                size: self.size,
                mtime_ns: self.mtime_ns,
                md5,
            },
        }
    }

    /// Continua como na base (pasta é pasta; arquivo com o mesmo tamanho e data).
    fn matches(&self, b: &Synced) -> bool {
        self.is_dir == b.is_dir
            && (self.is_dir || (self.size, self.mtime_ns) == (b.size, b.mtime_ns))
    }
}

/// O upload retomável guardado no banco: sobrevive a uma queda e só vale para o arquivo como
/// estava (tamanho e data) quando a sessão começou.
struct KvSession<'a> {
    store: &'a Store,
    key: String,
    stamp: String,
}

impl SessionSlot for KvSession<'_> {
    fn load(&self) -> Option<String> {
        let saved = self.store.kv_get(&self.key).ok()??;
        let (stamp, url) = saved.split_once('|')?;
        (stamp == self.stamp).then(|| url.to_owned())
    }

    fn store(&self, url: Option<&str>) {
        let value = url.map_or_else(String::new, |u| format!("{}|{u}", self.stamp));
        if let Err(e) = self.store.kv_set(&self.key, &value) {
            tracing::warn!(key = self.key, "sessão de envio não guardada: {e}");
        }
    }
}

/// Arquivos e pastas sob `root` (sem atalhos simbólicos nem arquivos especiais), pelo caminho
/// relativo. Uma pasta ilegível falha a varredura: ausente, ela iria para a lixeira do Drive.
fn scan(root: &Path) -> io::Result<BTreeMap<String, Local>> {
    let mut out = BTreeMap::new();
    let walker = walkdir::WalkDir::new(root)
        .min_depth(1)
        .into_iter()
        .filter_entry(|e| e.file_name().to_str().is_some_and(|n| !ignored(n)));
    for entry in walker {
        let entry = entry?;
        let kind = entry.file_type();
        if !kind.is_dir() && !kind.is_file() {
            continue;
        }
        let Some(rel) = entry.path().strip_prefix(root).ok().and_then(Path::to_str) else {
            continue;
        };
        out.insert(rel.to_owned(), Local::of(&entry.metadata()?));
    }
    Ok(out)
}

/// Nomes que ficam só no computador: travas e temporários, os downloads em curso e lixeiras.
fn ignored(name: &str) -> bool {
    is_local_only(name) || name.ends_with(PART_SUFFIX) || name.starts_with(".Trash-")
}

impl From<&Synced> for Base {
    fn from(b: &Synced) -> Base {
        Base {
            is_dir: b.is_dir,
            size: b.size,
            mtime_ns: b.mtime_ns,
            md5: b.md5.clone(),
        }
    }
}

fn remote_side(item: &Item) -> Side {
    match item.kind {
        ItemKind::Folder => Side::Dir,
        _ => Side::File {
            size: item.remote_size,
            mtime_ns: item.modified_ms * 1_000_000,
            md5: item.md5.clone(),
        },
    }
}

fn last(rel: &str) -> &str {
    rel.rsplit_once('/').map_or(rel, |(_, name)| name)
}

/// `rel` é `dir` ou está abaixo dele.
fn inside(rel: &str, dir: &str) -> bool {
    rel.strip_prefix(dir)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

/// O que está abaixo de `dir` num mapa por caminho (as chaves abaixo dele são contíguas).
fn under<'a, V>(
    map: &'a BTreeMap<String, V>,
    dir: &str,
) -> impl Iterator<Item = (&'a String, &'a V)> {
    let prefix = format!("{dir}/");
    map.range(prefix.clone()..)
        .take_while(move |(p, _)| p.starts_with(&prefix))
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;

    use gdl_drive::oauth::{AccessTokenProvider, AuthError};
    use gdl_drive::retry::Backoff;
    use gdl_fake_drive::{FakeDrive, Fault, TEST_ACCESS_TOKEN};
    use gdl_store::ItemId;
    use gdl_store::folders::{Direction, PairKind};

    use super::*;

    const MIB: usize = 1024 * 1024;

    struct Token;

    #[async_trait::async_trait]
    impl AccessTokenProvider for Token {
        async fn access_token(&self) -> std::result::Result<String, AuthError> {
            Ok(TEST_ACCESS_TOKEN.into())
        }

        async fn refresh_now(&self, _stale: &str) -> std::result::Result<String, AuthError> {
            Ok(TEST_ACCESS_TOKEN.into())
        }
    }

    fn client(fake: &FakeDrive, tries: u32) -> Arc<DriveClient> {
        Arc::new(
            DriveClient::new(fake.base_url(), fake.upload_url(), Arc::new(Token)).with_backoff(
                Backoff {
                    base: Duration::from_millis(1),
                    max: Duration::from_millis(4),
                    tries,
                },
            ),
        )
    }

    /// Um Drive simulado com a pasta "Pasta", o banco indexado e uma pasta local ligada a ela.
    struct World {
        fake: FakeDrive,
        _tmp: tempfile::TempDir,
        store: Arc<Store>,
        local: PathBuf,
        trash_dir: PathBuf,
        folder: String,
        pair: Pair,
    }

    impl World {
        async fn new() -> World {
            let fake = FakeDrive::start().await;
            let folder = fake.add_folder(&fake.root_id(), "Pasta");
            let tmp = tempfile::tempdir().unwrap();
            let store = Arc::new(Store::open(&tmp.path().join("db.sqlite")).unwrap());
            gdl_engine::indexer::initial_index(&store, &client(&fake, 8))
                .await
                .unwrap();
            let local = tmp.path().join("Local");
            std::fs::create_dir(&local).unwrap();
            let id = store
                .add_pair(
                    &local,
                    &ItemId(folder.clone()),
                    PairKind::Folder,
                    Direction::TwoWay,
                )
                .unwrap();
            let pair = store
                .pairs()
                .unwrap()
                .into_iter()
                .find(|p| p.id == id)
                .unwrap();
            World {
                trash_dir: tmp.path().join("Trash"),
                fake,
                _tmp: tmp,
                store,
                local,
                folder,
                pair,
            }
        }

        fn sync_with(&self, tries: u32) -> FolderSync {
            FolderSync::new(
                self.store.clone(),
                client(&self.fake, tries),
                self.pair.clone(),
                Trash::new(self.trash_dir.clone()),
                "conflito".into(),
            )
        }

        /// Traz as mudanças do Drive para o banco, como o motor da conta faz.
        async fn refresh(&self) {
            let key = gdl_engine::indexer::START_PAGE_TOKEN;
            let token = self.store.kv_get(key).unwrap().unwrap();
            let (changes, next) = client(&self.fake, 8).changes(&token).await.unwrap();
            gdl_engine::changes::apply_changes(&self.store, changes).unwrap();
            self.store.kv_set(key, &next).unwrap();
        }

        async fn reconcile(&self) -> Report {
            self.refresh().await;
            let report = self.sync_with(8).reconcile().await.unwrap();
            assert!(report.failed.is_empty(), "{:?}", report.failed);
            report
        }

        fn write(&self, rel: &str, bytes: &[u8]) {
            let path = self.local.join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, bytes).unwrap();
        }

        fn read(&self, rel: &str) -> Vec<u8> {
            std::fs::read(self.local.join(rel)).unwrap()
        }

        /// O arquivo vivo `name` no Drive (há de ser um só).
        fn remote(&self, name: &str) -> String {
            let found = self.fake.find_live(name);
            assert_eq!(found.len(), 1, "{name} no Drive: {found:?}");
            found[0].clone()
        }

        fn local_names(&self) -> Vec<String> {
            let mut names: Vec<_> = std::fs::read_dir(&self.local)
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
    }

    #[tokio::test]
    async fn first_meeting_links_identical_files() {
        let w = World::new().await;
        w.fake.add_file(&w.folder, "igual.txt", b"mesmo".to_vec());
        w.fake
            .add_file(&w.folder, "da-nuvem.txt", b"nuvem".to_vec());
        w.write("igual.txt", b"mesmo");
        w.write("sub/local.txt", b"local");

        w.reconcile().await;

        w.remote("igual.txt");
        assert_eq!(w.read("da-nuvem.txt"), b"nuvem");
        let up = w.remote("local.txt");
        assert_eq!(w.fake.content(&up), b"local");
        assert_eq!(
            w.fake.meta(&up)["parents"][0],
            w.fake.meta(&w.remote("sub"))["id"]
        );
        let synced: Vec<_> = w
            .store
            .synced_of(w.pair.id)
            .unwrap()
            .into_iter()
            .map(|s| s.rel_path)
            .collect();
        assert_eq!(
            synced,
            ["da-nuvem.txt", "igual.txt", "sub", "sub/local.txt"]
        );
        assert_eq!(
            w.reconcile().await.applied,
            0,
            "a segunda passada não tem nada a fazer"
        );
    }

    #[tokio::test]
    async fn local_edit_goes_up() {
        let w = World::new().await;
        w.write("a.txt", b"um");
        w.reconcile().await;
        let id = w.remote("a.txt");

        w.write("a.txt", b"dois, maior");
        w.reconcile().await;

        assert_eq!(
            w.remote("a.txt"),
            id,
            "a mesma versão no Drive, com conteúdo novo"
        );
        assert_eq!(w.fake.content(&id), b"dois, maior");
    }

    #[tokio::test]
    async fn remote_edit_comes_down() {
        let w = World::new().await;
        let id = w.fake.add_file(&w.folder, "a.txt", b"um".to_vec());
        w.reconcile().await;

        w.fake.modify_remote(&id, b"da web".to_vec());
        w.reconcile().await;

        assert_eq!(w.read("a.txt"), b"da web");
        assert_eq!(w.local_names(), ["a.txt"], "sem temporário largado");
    }

    #[tokio::test]
    async fn both_edited_keeps_both_versions_everywhere() {
        let w = World::new().await;
        let id = w.fake.add_file(&w.folder, "a.txt", b"um".to_vec());
        w.reconcile().await;

        w.fake.modify_remote(&id, b"da web".to_vec());
        w.write("a.txt", b"do computador");
        w.reconcile().await;

        let names = w.local_names();
        assert_eq!(names.len(), 2, "{names:?}");
        let copy = names.iter().find(|n| n.contains("(conflito ")).unwrap();
        assert!(
            copy.starts_with("a (conflito ") && copy.ends_with(").txt"),
            "{copy}"
        );
        assert_eq!(w.read("a.txt"), b"da web");
        assert_eq!(w.read(copy), b"do computador");
        assert_eq!(w.fake.content(&w.remote(copy)), b"do computador");
        assert_eq!(w.fake.content(&id), b"da web");
        assert_eq!(w.reconcile().await.applied, 0);
    }

    #[tokio::test]
    async fn local_delete_goes_to_drive_trash() {
        let w = World::new().await;
        w.write("a.txt", b"um");
        w.write("pasta/b.txt", b"dois");
        w.reconcile().await;
        let (a, folder) = (w.remote("a.txt"), w.remote("pasta"));

        std::fs::remove_file(w.local.join("a.txt")).unwrap();
        std::fs::remove_dir_all(w.local.join("pasta")).unwrap();
        w.reconcile().await;

        assert_eq!(w.fake.meta(&a)["trashed"], true);
        assert_eq!(w.fake.meta(&folder)["trashed"], true);
        assert!(w.store.synced_of(w.pair.id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn remote_delete_goes_to_system_trash() {
        let w = World::new().await;
        let id = w.fake.add_file(&w.folder, "a.txt", b"guardado".to_vec());
        w.reconcile().await;

        w.fake.trash_remote(&id);
        w.reconcile().await;

        assert!(w.local_names().is_empty());
        assert_eq!(
            std::fs::read(w.trash_dir.join("files/a.txt")).unwrap(),
            b"guardado"
        );
        assert!(w.store.synced_of(w.pair.id).unwrap().is_empty());
    }

    #[tokio::test]
    async fn local_rename_is_a_move_in_drive() {
        let w = World::new().await;
        w.write("a.txt", b"um");
        w.write("sub/x", b"");
        w.reconcile().await;
        let id = w.remote("a.txt");

        std::fs::rename(w.local.join("a.txt"), w.local.join("sub/novo.txt")).unwrap();
        w.reconcile().await;

        assert_eq!(w.remote("novo.txt"), id, "o mesmo arquivo, sem novo envio");
        assert_eq!(
            w.fake.meta(&id)["parents"][0],
            w.fake.meta(&w.remote("sub"))["id"]
        );
        assert!(w.fake.find_live("a.txt").is_empty());
        assert_eq!(w.fake.meta(&id)["trashed"], false);
    }

    #[tokio::test]
    async fn remote_move_is_a_rename_here() {
        use std::os::unix::fs::MetadataExt;
        let w = World::new().await;
        let id = w.fake.add_file(&w.folder, "a.txt", b"um".to_vec());
        let sub = w.fake.add_folder(&w.folder, "sub");
        w.reconcile().await;
        let inode = std::fs::metadata(w.local.join("a.txt")).unwrap().ino();

        w.fake.move_remote(&id, &sub);
        w.reconcile().await;

        let moved = std::fs::metadata(w.local.join("sub/a.txt")).unwrap();
        assert_eq!(moved.ino(), inode, "o mesmo arquivo, sem baixar de novo");
        assert_eq!(w.local_names(), ["sub"]);
        assert!(!w.trash_dir.exists(), "nada foi para a lixeira");
    }

    #[tokio::test]
    async fn folder_deleted_on_one_side_keeps_what_is_new_on_the_other() {
        let w = World::new().await;
        w.write("pasta/velho.txt", b"velho");
        w.reconcile().await;
        let folder = w.remote("pasta");

        w.fake.trash_remote(&folder);
        w.write("pasta/novo.txt", b"novo");
        w.reconcile().await;

        assert_eq!(w.fake.content(&w.remote("novo.txt")), b"novo");
        assert_ne!(w.remote("pasta"), folder, "a pasta foi recriada no Drive");
        assert!(
            !w.local.join("pasta/velho.txt").exists(),
            "o que o Drive apagou sai daqui"
        );
        assert_eq!(
            std::fs::read(w.trash_dir.join("files/velho.txt")).unwrap(),
            b"velho"
        );
    }

    #[tokio::test]
    async fn nothing_is_written_through_a_symlinked_folder() {
        let w = World::new().await;
        let sub = w.fake.add_folder(&w.folder, "sub");
        w.fake.add_file(&sub, "x.txt", b"nuvem".to_vec());
        let outside = w._tmp.path().join("fora");
        std::fs::create_dir(&outside).unwrap();
        std::os::unix::fs::symlink(&outside, w.local.join("sub")).unwrap();
        // E um temporário de download plantado como link para fora.
        w.fake.add_file(&w.folder, "y.txt", b"nuvem".to_vec());
        std::os::unix::fs::symlink(outside.join("alvo"), w.local.join(".y.txt.gdl-part")).unwrap();

        w.refresh().await;
        let report = w.sync_with(8).reconcile().await.unwrap();

        assert!(!outside.join("x.txt").exists(), "nada escrito fora do par");
        assert!(
            !outside.join("alvo").exists(),
            "o temporário não segue link"
        );
        assert!(
            report.failed.iter().any(|(rel, _)| rel == "sub/x.txt"),
            "{report:?}"
        );
        assert_eq!(w.read("y.txt"), b"nuvem");
    }

    fn state(w: &World) -> PairState {
        let pairs = w.store.pairs().unwrap();
        pairs.into_iter().find(|p| p.id == w.pair.id).unwrap().state
    }

    #[tokio::test]
    async fn missing_root_stops_the_pair_without_trashing() {
        let w = World::new().await;
        w.write("a.txt", b"um");
        w.reconcile().await;
        let id = w.remote("a.txt");

        std::fs::rename(&w.local, w._tmp.path().join("Renomeada")).unwrap();
        w.reconcile().await;

        assert_eq!(
            state(&w),
            PairState::Stopped(gdl_store::folders::StopReason::Missing)
        );
        assert_eq!(
            w.fake.meta(&id)["trashed"],
            false,
            "nada foi para a lixeira do Drive"
        );
        assert_eq!(
            w.store.synced_of(w.pair.id).unwrap().len(),
            1,
            "a base fica"
        );

        std::fs::rename(w._tmp.path().join("Renomeada"), &w.local).unwrap();
        w.reconcile().await;
        assert_eq!(state(&w), PairState::Active, "a pasta voltou, o par volta");
    }

    #[tokio::test]
    async fn mass_delete_asks_first() {
        let w = World::new().await;
        for n in 0..25 {
            w.write(&format!("f{n}.txt"), b"x");
        }
        w.write("fica.txt", b"y");
        w.reconcile().await;
        for n in 0..25 {
            std::fs::remove_file(w.local.join(format!("f{n}.txt"))).unwrap();
        }
        w.write("novo.txt", b"z");

        w.reconcile().await;
        assert_eq!(state(&w), PairState::ConfirmDeletes(25));
        assert_eq!(
            w.fake.find_live("f0.txt").len(),
            1,
            "nada excluído antes de confirmar"
        );
        w.remote("novo.txt");

        let sync = w.sync_with(8);
        sync.confirm_deletes().unwrap();
        w.refresh().await;
        sync.reconcile().await.unwrap();
        assert!(w.fake.find_live("f0.txt").is_empty());
        assert_eq!(state(&w), PairState::Active);
        w.remote("fica.txt");
    }

    /// Espera `done` virar verdade (até 10 s).
    async fn eventually(mut done: impl FnMut() -> bool) {
        for _ in 0..200 {
            if done() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("não aconteceu a tempo");
    }

    fn quick() -> crate::watch::Timing {
        crate::watch::Timing {
            quiet: Duration::from_millis(300),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn file_still_being_written_waits_until_stable() {
        use std::io::Write;
        let w = World::new().await;
        let (_events, rx) = tokio::sync::broadcast::channel(16);
        let shutdown = tokio_util::sync::CancellationToken::new();
        let sync = Arc::new(w.sync_with(8));
        let running = tokio::spawn({
            let (sync, shutdown) = (sync.clone(), shutdown.clone());
            async move { sync.run(rx, quick(), shutdown).await }
        });

        let mut file = std::fs::File::create(w.local.join("gravando.txt")).unwrap();
        for _ in 0..10 {
            file.write_all(&[b'x'; 1000]).unwrap();
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        drop(file);

        eventually(|| w.fake.find_live("gravando.txt").len() == 1).await;
        assert_eq!(w.fake.content(&w.remote("gravando.txt")).len(), 10_000);
        assert_eq!(
            w.fake.requests("/upload/drive/v3/files"),
            1,
            "subiu uma vez, inteiro"
        );
        shutdown.cancel();
        running.await.unwrap();
    }

    #[tokio::test]
    async fn a_change_in_the_drive_is_brought_when_the_engine_says_so() {
        let w = World::new().await;
        let id = w.fake.add_file(&w.folder, "a.txt", b"um".to_vec());
        w.reconcile().await;
        let (events, rx) = tokio::sync::broadcast::channel(16);
        let shutdown = tokio_util::sync::CancellationToken::new();
        let sync = Arc::new(w.sync_with(8));
        let running = tokio::spawn({
            let (sync, shutdown) = (sync.clone(), shutdown.clone());
            async move { sync.run(rx, quick(), shutdown).await }
        });
        tokio::time::sleep(Duration::from_millis(500)).await;

        w.fake.modify_remote(&id, b"da web".to_vec());
        w.refresh().await;
        events
            .send(gdl_engine::events::EngineEvent::ItemsChanged {
                ids: vec![ItemId(id)],
            })
            .unwrap();

        eventually(|| std::fs::read(w.local.join("a.txt")).unwrap() == b"da web").await;
        shutdown.cancel();
        running.await.unwrap();
    }

    #[tokio::test]
    async fn big_file_upload_resumes() {
        let w = World::new().await;
        let data: Vec<u8> = (0..9 * MIB).map(|i| (i % 251) as u8).collect();
        w.write("grande.bin", &data);
        w.refresh().await;
        w.fake
            .inject(Fault::DropConnectionAfterBytes(2 * MIB as u64));

        let report = w.sync_with(1).reconcile().await.unwrap();
        assert_eq!(report.failed.len(), 1, "{report:?}");

        w.reconcile().await;
        assert_eq!(w.fake.content(&w.remote("grande.bin")), data);
        assert_eq!(
            w.fake
                .requests("/upload/drive/v3/files?uploadType=resumable"),
            1,
            "a segunda tentativa continuou a mesma sessão"
        );
    }
}
