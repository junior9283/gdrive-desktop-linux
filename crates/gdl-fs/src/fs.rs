//! O `fuser::Filesystem` da montagem: navegação só pelo banco e leitura sob demanda pelo cache.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use fuser::{
    BackgroundSession, Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags,
    Generation, INodeNo, LockOwner, MountOption, OpenAccMode, OpenFlags, ReplyAttr, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyXattr, Request,
};
use gdl_common::i18n::I18n;
use gdl_engine::EngineError;
use gdl_engine::cache::CacheError;
use gdl_engine::engine::Engine;
use gdl_engine::gdocs::pointer_json;
use gdl_store::{Inode, Item, ItemId, ItemKind};

use crate::inodes::{Inodes, Node};
use crate::layout::{Layout, XATTRS, item_perm, xattr_value};

/// Por quanto tempo o kernel pode guardar atributos e entradas sem perguntar de novo.
const TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 4096;
/// Cota consultada no Drive vale por este tempo.
const QUOTA_TTL: Duration = Duration::from_secs(60);
/// Armazenamento ilimitado aparece como 1 PiB.
const UNLIMITED: u64 = 1 << 50;

/// A montagem do Drive. Clonar dá outro acesso ao mesmo estado.
#[derive(Clone)]
pub struct DriveFs(Arc<Inner>);

struct Inner {
    engine: Arc<Engine>,
    layout: Layout,
    inodes: Inodes,
    uid: u32,
    gid: u32,
    /// Os pedidos do FUSE chegam em threads próprias; o motor roda neste runtime.
    rt: tokio::runtime::Handle,
    mounted_at: SystemTime,
    next_handle: AtomicU64,
    /// Listagem tirada no `opendir`, servida aos pedaços pelos `readdir` seguintes.
    dirs: Mutex<HashMap<u64, Arc<Vec<Entry>>>>,
    files: Mutex<HashMap<u64, OpenFile>>,
    /// Quantas listagens de pasta foram montadas (para os testes contarem consultas).
    listings: AtomicUsize,
    quota: Mutex<Quota>,
}

/// Arquivo aberto: de onde vem o conteúdo, decidido uma vez no `open`.
enum OpenFile {
    /// Conteúdo do Drive pelo cache, com o fim da última leitura para reconhecer leitura
    /// sequencial neste handle.
    Drive {
        id: ItemId,
        last_end: Option<u64>,
    },
    /// Documento Google: o JSON do ponteiro, montado na abertura.
    Pointer(Bytes),
    Lost(File),
}

struct Entry {
    ino: u64,
    kind: FileType,
    name: OsString,
}

#[derive(Default)]
struct Quota {
    /// Total (None = ilimitado) e usado, em bytes.
    known: Option<(Option<u64>, u64)>,
    fetched: Option<Instant>,
    refreshing: bool,
}

pub(crate) fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn errno(e: impl Into<EngineError>) -> Errno {
    match e.into() {
        EngineError::NotFound | EngineError::Cache(CacheError::NotFound) => Errno::ENOENT,
        EngineError::Cache(CacheError::DiskFull) => Errno::ENOSPC,
        _ => Errno::EIO,
    }
}

fn time_of(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)
}

fn file_type(kind: &ItemKind) -> FileType {
    match kind {
        ItemKind::Folder => FileType::Directory,
        ItemKind::File | ItemKind::GoogleDoc { .. } => FileType::RegularFile,
        ItemKind::Shortcut { .. } => FileType::Symlink,
    }
}

type Res<T> = std::result::Result<T, Errno>;

impl DriveFs {
    /// Precisa ser criado dentro do runtime do tokio onde o motor roda.
    pub fn new(engine: Arc<Engine>, i18n: I18n, uid: u32, gid: u32) -> DriveFs {
        DriveFs(Arc::new(Inner {
            engine,
            layout: Layout::new(&i18n),
            inodes: Inodes::default(),
            uid,
            gid,
            rt: tokio::runtime::Handle::current(),
            mounted_at: SystemTime::now(),
            next_handle: AtomicU64::new(1),
            dirs: Mutex::default(),
            files: Mutex::default(),
            listings: AtomicUsize::new(0),
            quota: Mutex::default(),
        }))
    }

    /// Quantas listagens de pasta já foram montadas.
    pub fn listings(&self) -> usize {
        self.0.listings.load(Ordering::Relaxed)
    }
}

impl Inner {
    fn node(&self, ino: INodeNo) -> Res<Node> {
        self.inodes.node(ino.0).ok_or(Errno::ENOENT)
    }

    fn item(&self, inode: Inode) -> Res<Item> {
        self.engine
            .store()
            .by_inode(inode)
            .map_err(errno)?
            .ok_or(Errno::ENOENT)
    }

    fn item_by_id(&self, id: &ItemId) -> Res<Option<Item>> {
        self.engine.store().get(id).map_err(errno)
    }

    fn attr(&self, ino: u64, kind: FileType, size: u64, perm: u16, mtime: SystemTime) -> FileAttr {
        FileAttr {
            ino: INodeNo(ino),
            size,
            blocks: size.div_ceil(512),
            atime: mtime,
            mtime,
            ctime: mtime,
            crtime: mtime,
            kind,
            perm,
            nlink: if kind == FileType::Directory { 2 } else { 1 },
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: BLOCK_SIZE,
            flags: 0,
        }
    }

    fn virtual_dir(&self, node: &Node) -> FileAttr {
        let ino = self.inodes.ino(node);
        self.attr(ino, FileType::Directory, 0, 0o555, self.mounted_at)
    }

    fn item_attr(&self, item: &Item) -> Res<FileAttr> {
        let size = match &item.kind {
            ItemKind::Shortcut { .. } => self.link_target(item)?.len() as u64,
            _ => item.size,
        };
        let ino = self.inodes.ino(&Node::Item(item.inode));
        let kind = file_type(&item.kind);
        Ok(self.attr(ino, kind, size, item_perm(item), time_of(item.modified_ms)))
    }

    fn lost_path(&self, name: &OsStr) -> std::path::PathBuf {
        self.engine.lost_and_found().dir().join(name)
    }

    fn lost_attr(&self, name: &OsStr) -> Res<FileAttr> {
        let meta = std::fs::metadata(self.lost_path(name)).map_err(Errno::from)?;
        if !meta.is_file() {
            return Err(Errno::ENOENT);
        }
        let ino = self.inodes.ino(&Node::LostFile(name.into()));
        let mtime = meta.modified().unwrap_or(self.mounted_at);
        Ok(self.attr(ino, FileType::RegularFile, meta.size(), 0o444, mtime))
    }

    fn node_attr(&self, node: &Node) -> Res<FileAttr> {
        match node {
            Node::Item(inode) => self.item_attr(&self.item(*inode)?),
            Node::LostFile(name) => self.lost_attr(name),
            _ => Ok(self.virtual_dir(node)),
        }
    }

    /// Caminho relativo do atalho até `.shortcut-targets-by-id/<alvo>/<nome do alvo>`: sobe uma
    /// pasta por nível entre o atalho e a raiz da montagem.
    fn link_target(&self, item: &Item) -> Res<String> {
        let ItemKind::Shortcut { target_id, .. } = &item.kind else {
            return Err(Errno::EINVAL);
        };
        let name = self
            .item_by_id(target_id)?
            .map_or_else(|| target_id.0.clone(), |t| t.local_name);
        let depth = if item.trashed {
            1
        } else {
            let mut depth = 0;
            let mut parent = item.parent.clone();
            while let Some(id) = parent {
                depth += 1;
                parent = self.item_by_id(&id)?.and_then(|p| p.parent);
            }
            depth
        };
        Ok(format!(
            "{}{}/{}/{}",
            "../".repeat(depth),
            self.layout.shortcut_targets,
            target_id.0,
            name
        ))
    }

    /// Atributos de `name` dentro de `parent`, com uma consulta só para itens do banco.
    fn lookup(&self, parent: &Node, name: &OsStr) -> Res<FileAttr> {
        let store = self.engine.store();
        let item = match parent {
            Node::MountRoot => {
                let node = self.layout.lookup_root(name).ok_or(Errno::ENOENT)?;
                return self.node_attr(&node);
            }
            Node::LostFound => return self.lost_attr(name),
            Node::ShortcutTargets => {
                let id = ItemId(name.to_str().ok_or(Errno::ENOENT)?.into());
                self.item_by_id(&id)?.ok_or(Errno::ENOENT)?;
                return Ok(self.virtual_dir(&Node::TargetDir(id)));
            }
            Node::Trash => store
                .lookup_trashed(name.to_str().ok_or(Errno::ENOENT)?)
                .map_err(errno)?,
            Node::TargetDir(id) => self
                .item_by_id(id)?
                .filter(|t| OsStr::new(&t.local_name) == name),
            Node::Item(inode) => {
                let dir = self.item(*inode)?;
                store
                    .lookup(&dir.id, name.to_str().ok_or(Errno::ENOENT)?)
                    .map_err(errno)?
            }
            Node::LostFile(_) => return Err(Errno::ENOTDIR),
        };
        self.item_attr(&item.ok_or(Errno::ENOENT)?)
    }

    fn trashed(&self) -> Res<Vec<Item>> {
        self.engine.store().trashed().map_err(errno)
    }

    fn item_entry(&self, item: Item) -> Entry {
        Entry {
            ino: self.inodes.ino(&Node::Item(item.inode)),
            kind: file_type(&item.kind),
            name: item.local_name.into(),
        }
    }

    /// Conteúdo da pasta, só pelo banco (nada é baixado).
    fn list(&self, node: &Node) -> Res<Vec<Entry>> {
        self.listings.fetch_add(1, Ordering::Relaxed);
        // "." e ".." ocupam as posições 0 e 1 (o kernel não usa o inode deles).
        let ino = self.inodes.ino(node);
        let mut entries: Vec<Entry> = [".", ".."]
            .map(|name| Entry {
                ino,
                kind: FileType::Directory,
                name: name.into(),
            })
            .into();
        entries.extend(self.list_children(node)?);
        Ok(entries)
    }

    fn list_children(&self, node: &Node) -> Res<Vec<Entry>> {
        Ok(match node {
            Node::MountRoot => self
                .layout
                .root_entries()
                .into_iter()
                .map(|(name, node)| Entry {
                    ino: self.inodes.ino(&node),
                    kind: FileType::Directory,
                    name: name.into(),
                })
                .collect(),
            Node::Trash => self
                .trashed()?
                .into_iter()
                .map(|i| self.item_entry(i))
                .collect(),
            Node::LostFound => self
                .engine
                .lost_and_found()
                .list()
                .into_iter()
                .filter_map(|path| {
                    let name = path.file_name()?.to_owned();
                    Some(Entry {
                        ino: self.inodes.ino(&Node::LostFile(name.clone())),
                        kind: FileType::RegularFile,
                        name,
                    })
                })
                .collect(),
            // Os alvos aparecem só quando um atalho aponta para eles.
            Node::ShortcutTargets => vec![],
            Node::TargetDir(id) => self
                .item_by_id(id)?
                .into_iter()
                .map(|i| self.item_entry(i))
                .collect(),
            Node::Item(inode) => {
                let dir = self.item(*inode)?;
                if dir.kind != ItemKind::Folder {
                    return Err(Errno::ENOTDIR);
                }
                let children = self.engine.store().children(&dir.id).map_err(errno)?;
                children.into_iter().map(|i| self.item_entry(i)).collect()
            }
            Node::LostFile(_) => return Err(Errno::ENOTDIR),
        })
    }

    fn handle(&self) -> u64 {
        self.next_handle.fetch_add(1, Ordering::Relaxed)
    }

    fn open_file(&self, node: &Node) -> Res<OpenFile> {
        match node {
            Node::Item(inode) => {
                let item = self.item(*inode)?;
                match &item.kind {
                    ItemKind::File => Ok(OpenFile::Drive {
                        id: item.id,
                        last_end: None,
                    }),
                    ItemKind::GoogleDoc { mime } => {
                        let email = self.engine.account_email().map_err(errno)?;
                        Ok(OpenFile::Pointer(
                            pointer_json(&item.id.0, mime, &email).into(),
                        ))
                    }
                    ItemKind::Folder => Err(Errno::EISDIR),
                    ItemKind::Shortcut { .. } => Err(Errno::EINVAL),
                }
            }
            Node::LostFile(name) => Ok(OpenFile::Lost(
                File::open(self.lost_path(name)).map_err(Errno::from)?,
            )),
            _ => Err(Errno::EISDIR),
        }
    }

    fn read(&self, fh: u64, offset: u64, size: u32) -> Res<Bytes> {
        let end = offset.saturating_add(size.into());
        // Decide com o mapa travado; a leitura em si acontece fora da trava.
        let (id, sequential) = match lock(&self.files).get_mut(&fh).ok_or(Errno::EBADF)? {
            OpenFile::Drive { id, last_end } => {
                let sequential = last_end.replace(end) == Some(offset);
                (id.clone(), sequential)
            }
            OpenFile::Pointer(json) => return Ok(slice(json, offset, size)),
            OpenFile::Lost(file) => {
                let mut buf = vec![0; size as usize];
                let n = file.read_at(&mut buf, offset).map_err(Errno::from)?;
                buf.truncate(n);
                return Ok(buf.into());
            }
        };
        let cache = self.engine.cache();
        self.rt
            .block_on(cache.read_hinted(&id, offset, size, Some(sequential)))
            .map_err(errno)
    }

    /// Cota em blocos de 4096 bytes. Nunca espera a rede: responde com o último valor conhecido
    /// e atualiza em segundo plano quando ele envelhece.
    fn quota_blocks(self: &Arc<Self>) -> (u64, u64) {
        let mut quota = lock(&self.quota);
        let stale = quota.fetched.is_none_or(|t| t.elapsed() > QUOTA_TTL);
        if stale && !quota.refreshing {
            quota.refreshing = true;
            let inner = self.clone();
            self.rt.spawn(async move {
                let about = inner.engine.about().await;
                let mut quota = lock(&inner.quota);
                quota.refreshing = false;
                quota.fetched = Some(Instant::now());
                match about {
                    Ok(a) => quota.known = Some((a.quota_total, a.quota_used)),
                    Err(e) => tracing::debug!(erro = %e, "cota indisponível"),
                }
            });
        }
        let (total, used) = quota.known.unwrap_or((None, 0));
        let total = total.unwrap_or(used + UNLIMITED);
        let block = u64::from(BLOCK_SIZE);
        (total / block, total.saturating_sub(used) / block)
    }
}

fn slice(data: &Bytes, offset: u64, size: u32) -> Bytes {
    let start = usize::try_from(offset)
        .unwrap_or(usize::MAX)
        .min(data.len());
    let end = start.saturating_add(size as usize).min(data.len());
    data.slice(start..end)
}

impl Filesystem for DriveFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        match self.0.node(parent).and_then(|p| self.0.lookup(&p, name)) {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(e) => reply.error(e),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        match self.0.node(ino).and_then(|n| self.0.node_attr(&n)) {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(e) => reply.error(e),
        }
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {
        let target = self.0.node(ino).and_then(|node| match node {
            Node::Item(inode) => self.0.link_target(&self.0.item(inode)?),
            _ => Err(Errno::EINVAL),
        });
        match target {
            Ok(target) => reply.data(target.as_bytes()),
            Err(e) => reply.error(e),
        }
    }

    fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        // A gravação chega com a T12; até lá a montagem só lê.
        if flags.acc_mode() != OpenAccMode::O_RDONLY {
            return reply.error(Errno::EROFS);
        }
        match self.0.node(ino).and_then(|n| self.0.open_file(&n)) {
            Ok(file) => {
                let fh = self.0.handle();
                lock(&self.0.files).insert(fh, file);
                reply.opened(FileHandle(fh), FopenFlags::empty());
            }
            Err(e) => reply.error(e),
        }
    }

    fn read(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        size: u32,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyData,
    ) {
        match self.0.read(fh.0, offset, size) {
            Ok(data) => reply.data(&data),
            Err(e) => reply.error(e),
        }
    }

    fn release(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        _flush: bool,
        reply: ReplyEmpty,
    ) {
        lock(&self.0.files).remove(&fh.0);
        reply.ok();
    }

    fn opendir(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        match self.0.node(ino).and_then(|n| self.0.list(&n)) {
            Ok(entries) => {
                let fh = self.0.handle();
                lock(&self.0.dirs).insert(fh, Arc::new(entries));
                reply.opened(FileHandle(fh), FopenFlags::empty());
            }
            Err(e) => reply.error(e),
        }
    }

    fn readdir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let Some(entries) = lock(&self.0.dirs).get(&fh.0).cloned() else {
            return reply.error(Errno::EBADF);
        };
        // O kernel pede a partir da posição seguinte à última recebida.
        let start = usize::try_from(offset).unwrap_or(usize::MAX);
        for (i, entry) in entries.iter().enumerate().skip(start) {
            if reply.add(INodeNo(entry.ino), i as u64 + 1, entry.kind, &entry.name) {
                break;
            }
        }
        reply.ok();
    }

    fn releasedir(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        lock(&self.0.dirs).remove(&fh.0);
        reply.ok();
    }

    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        let (blocks, free) = self.0.quota_blocks();
        reply.statfs(blocks, free, free, 0, 0, BLOCK_SIZE, 255, BLOCK_SIZE);
    }

    fn getxattr(&self, _req: &Request, ino: INodeNo, name: &OsStr, size: u32, reply: ReplyXattr) {
        let value = self.0.node(ino).and_then(|node| match node {
            Node::Item(inode) => xattr_value(&self.0.item(inode)?, name).ok_or(Errno::NO_XATTR),
            _ => Err(Errno::NO_XATTR),
        });
        match value {
            Ok(value) => reply_xattr(reply, value.as_bytes(), size),
            Err(e) => reply.error(e),
        }
    }

    fn listxattr(&self, _req: &Request, ino: INodeNo, size: u32, reply: ReplyXattr) {
        let names = match self.0.node(ino) {
            Ok(Node::Item(_)) => XATTRS
                .iter()
                .flat_map(|(n, _)| [n.as_bytes(), b"\0"])
                .flatten()
                .copied()
                .collect(),
            Ok(_) => vec![],
            Err(e) => return reply.error(e),
        };
        reply_xattr(reply, &names, size);
    }
}

/// Com `size` 0 o kernel só quer saber o tamanho; menor que o valor é `ERANGE`.
fn reply_xattr(reply: ReplyXattr, value: &[u8], size: u32) {
    let len = value.len() as u32;
    if size == 0 {
        reply.size(len);
    } else if len > size {
        reply.error(Errno::ERANGE);
    } else {
        reply.data(value);
    }
}

/// Monta `fs` em `at`. Desmonta quando a sessão devolvida é descartada.
pub fn mount(fs: DriveFs, at: &Path) -> std::io::Result<BackgroundSession> {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get().min(8));
    let mut config = Config::default();
    config.mount_options = vec![
        MountOption::FSName("gdrive-desktop-linux".into()),
        MountOption::Subtype("gdrive".into()),
        MountOption::DefaultPermissions,
        MountOption::NoExec,
        MountOption::NoSuid,
        MountOption::NoDev,
    ];
    config.n_threads = Some(threads);
    fuser::spawn_mount(fs, at, &config)
}
