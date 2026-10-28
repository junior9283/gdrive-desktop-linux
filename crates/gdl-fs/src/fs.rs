//! O `fuser::Filesystem` da montagem: navegação só pelo banco e leitura sob demanda pelo cache.
//! A escrita fica em `write.rs`.

use std::collections::HashMap;
use std::ffi::{OsStr, OsString};
use std::fs::File;
use std::os::unix::fs::{FileExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bytes::Bytes;
use fuser::{
    BackgroundSession, Config, Errno, FileAttr, FileHandle, FileType, Filesystem, FopenFlags,
    Generation, INodeNo, LockOwner, MountOption, OpenAccMode, OpenFlags, RenameFlags, ReplyAttr,
    ReplyCreate, ReplyData, ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs,
    ReplyWrite, ReplyXattr, Request, TimeOrNow, WriteFlags,
};
use gdl_drive::client::DriveError;
use gdl_engine::EngineError;
use gdl_engine::cache::CacheError;
use gdl_engine::engine::Engine;
use gdl_engine::gdocs::pointer_json;
use gdl_engine::kept::KeptFolder;
use gdl_store::{Inode, Item, ItemId, ItemKind};

use crate::inodes::{Inodes, Node};
use crate::layout::{Layout, XATTRS, item_perm, xattr_value};
use crate::trash::KioTrash;

/// Por quanto tempo o kernel pode guardar atributos e entradas sem perguntar de novo.
pub(crate) const TTL: Duration = Duration::from_secs(1);
const BLOCK_SIZE: u32 = 4096;
/// Cota consultada no Drive vale por este tempo.
const QUOTA_TTL: Duration = Duration::from_secs(60);
/// Armazenamento ilimitado aparece como 1 PiB.
const UNLIMITED: u64 = 1 << 50;
/// Quanto o `opendir` espera a semeadura das miniaturas; o resto segue em segundo plano.
pub const THUMB_WAIT: Duration = Duration::from_millis(1500);
/// Quantos `opendir` podem esperar miniaturas ao mesmo tempo; os demais listam na hora, para
/// sobrar thread do FUSE às outras operações.
const MAX_THUMB_WAITERS: usize = 4;
/// Indexadores de arquivos (início do `comm`, que o kernel corta em 15 bytes): percorrem pastas
/// sem ninguém olhando, e semear para eles buscaria no Google as miniaturas do Drive inteiro.
const INDEXERS: [&str; 6] = [
    "baloo_file",
    "tracker-miner",
    "localsearch",
    "updatedb",
    "plocate",
    "mlocate",
];

/// Programas que percorrem pastas sem usar o cache de miniaturas (`comm` exato, ao contrário dos
/// `INDEXERS`, cujos nomes têm sufixos: `cp` não pode pegar `cpp`): buscas, cópias, backups,
/// controle de versão e IDEs (que mostram imagens pelo arquivo, não pelo cache). Esperar e semear
/// cada pasta para eles só deixava a varredura lenta; a pasta é semeada quando um gerenciador de
/// arquivos a abre. Um programa de fora da lista só fica como antes: correto, mais lento.
const WALKERS: &[&str] = &[
    "find",
    "fd",
    "fdfind",
    "rg",
    "grep",
    "du",
    "ncdu",
    "dust",
    "tree",
    "ls",
    "stat",
    "file",
    "cp",
    "mv",
    "rsync",
    "tar",
    "zip",
    "7z",
    "7za",
    "rclone",
    "restic",
    "borg",
    "kopia",
    "duplicity",
    "deja-dup",
    "rdiff-backup",
    "git",
    "code",
    "codium",
    "fsnotifier",
];

/// O `comm` (de `/proc/<pid>/comm`) é de um indexador (`INDEXERS`)?
fn is_indexer(comm: &str) -> bool {
    INDEXERS.iter().any(|name| comm.starts_with(name))
}

/// O `comm` é de um programa que percorre pastas (`WALKERS`)?
fn is_walker(comm: &str) -> bool {
    WALKERS.contains(&comm.trim_end())
}

/// Quem pediu (`pid` do FUSE) não usa miniaturas (indexador ou varredura)? Sem `/proc` legível
/// (processo que já saiu), usa.
fn skips_thumbnails(pid: u32) -> bool {
    skips_thumbnails_in(Path::new("/proc"), pid)
}

/// Nome (`comm`) do processo da thread `tid`, lido de `proc_root` (o `/proc`). O `comm` da
/// própria thread é o nome dela ("Thread (pooled)" no Baloo); o do processo vem do `Tgid`.
fn process_name(proc_root: &Path, tid: u32) -> Option<String> {
    let status = std::fs::read_to_string(proc_root.join(tid.to_string()).join("status")).ok()?;
    let tgid: u32 = status
        .lines()
        .find_map(|line| line.strip_prefix("Tgid:"))?
        .trim()
        .parse()
        .ok()?;
    std::fs::read_to_string(proc_root.join(tgid.to_string()).join("comm")).ok()
}

/// O FUSE informa o ID da thread que chamou, não o do processo: decide pelo nome do processo e,
/// de quebra, pelo da thread. Erro de leitura é "não": a abertura segue normal.
fn skips_thumbnails_in(proc_root: &Path, tid: u32) -> bool {
    let skips = |comm: &str| is_indexer(comm) || is_walker(comm);
    let thread = std::fs::read_to_string(proc_root.join(tid.to_string()).join("comm"));
    thread.is_ok_and(|comm| skips(&comm)) || process_name(proc_root, tid).is_some_and(|c| skips(&c))
}

/// Threads do FUSE: fixas, porque ficam paradas em E/S (e na espera das miniaturas), não na CPU.
const FUSE_THREADS: usize = 8;

/// A montagem do Drive. Clonar dá outro acesso ao mesmo estado.
#[derive(Clone)]
pub struct DriveFs(Arc<Inner>);

pub(crate) struct Inner {
    pub(crate) engine: Arc<Engine>,
    layout: Layout,
    /// Onde a montagem está, para o caminho (e a URI) das miniaturas.
    mount_point: PathBuf,
    inodes: Inodes,
    uid: u32,
    gid: u32,
    /// Os pedidos do FUSE chegam em threads próprias; o motor roda neste runtime.
    pub(crate) rt: tokio::runtime::Handle,
    mounted_at: SystemTime,
    next_handle: AtomicU64,
    /// Listagem tirada no `opendir`, servida aos pedaços pelos `readdir` seguintes.
    dirs: Mutex<HashMap<u64, Arc<Vec<Entry>>>>,
    pub(crate) files: Mutex<HashMap<u64, OpenFile>>,
    /// Itens abertos para escrita, pelo inode (o ID muda quando o temporário sobe).
    pub(crate) writers: Mutex<HashMap<Inode, Writer>>,
    pub(crate) kio: KioTrash,
    /// Vagas para `opendir` esperar miniaturas (`MAX_THUMB_WAITERS`).
    thumb_waiters: tokio::sync::Semaphore,
    /// Quantas listagens de pasta foram montadas (para os testes contarem consultas).
    listings: AtomicUsize,
    quota: Mutex<Quota>,
}

/// Arquivo aberto: de onde vem o conteúdo, decidido uma vez no `open`.
pub(crate) enum OpenFile {
    /// Conteúdo do Drive pelo cache, com o fim da última leitura para reconhecer leitura
    /// sequencial neste handle. O ID é o último conhecido do inode: o temporário de um item novo
    /// vira o do Drive quando o envio termina, com o arquivo ainda aberto.
    Drive {
        inode: Inode,
        id: ItemId,
        last_end: Option<u64>,
    },
    /// Documento Google: o JSON do ponteiro, montado na abertura.
    Pointer(Bytes),
    /// Arquivo que só existe neste computador (cópias guardadas, `.trashinfo` do KIO).
    Local(Arc<File>),
    /// Item do Drive aberto para escrita: o conteúdo inteiro está no arquivo local do cache.
    Write { inode: Inode, file: Arc<File> },
}

/// Quem está escrevendo num item.
#[derive(Default)]
pub(crate) struct Writer {
    pub handles: usize,
    pub wrote: bool,
    /// A abertura marcou o item como editado (estava sincronizado).
    pub was_clean: bool,
    /// Data escolhida pelo app com o arquivo aberto (`cp -p`); vale no fechamento.
    pub mtime: Option<i64>,
    /// Um registro da gravação por vez: `flush` de cópias do descritor (`fork`) chegam juntos.
    pub serial: Arc<Mutex<()>>,
    /// Enquanto houver descritor aberto, o cancelamento do envio é recusado.
    pub mark: Option<gdl_engine::uploads::WritingMark>,
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

pub(crate) fn errno(e: impl Into<EngineError>) -> Errno {
    match e.into() {
        EngineError::NotFound | EngineError::Cache(CacheError::NotFound) => Errno::ENOENT,
        EngineError::Cache(CacheError::DiskFull) => Errno::ENOSPC,
        EngineError::Drive(DriveError::PermissionDenied) => Errno::EACCES,
        // `EXDEV` faria o gerenciador de arquivos copiar e apagar a pasta.
        EngineError::FolderCrossDrive => Errno::EPERM,
        _ => Errno::EIO,
    }
}

pub(crate) fn time_of(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(ms.max(0) as u64)
}

fn file_type(kind: &ItemKind) -> FileType {
    match kind {
        ItemKind::Folder => FileType::Directory,
        ItemKind::File | ItemKind::GoogleDoc { .. } => FileType::RegularFile,
        ItemKind::Shortcut { .. } => FileType::Symlink,
    }
}

pub(crate) type Res<T> = std::result::Result<T, Errno>;

impl DriveFs {
    /// Precisa ser criado dentro do runtime do tokio onde o motor roda.
    pub fn new(
        engine: Arc<Engine>,
        layout: Layout,
        uid: u32,
        gid: u32,
        mount_point: PathBuf,
    ) -> DriveFs {
        DriveFs(Arc::new(Inner {
            layout,
            mount_point,
            inodes: Inodes::default(),
            kio: KioTrash::new(uid, engine.data_dir()),
            uid,
            gid,
            rt: tokio::runtime::Handle::current(),
            mounted_at: SystemTime::now(),
            next_handle: AtomicU64::new(1),
            dirs: Mutex::default(),
            files: Mutex::default(),
            writers: Mutex::default(),
            listings: AtomicUsize::new(0),
            thumb_waiters: tokio::sync::Semaphore::new(MAX_THUMB_WAITERS),
            quota: Mutex::default(),
            engine,
        }))
    }

    /// Quantas listagens de pasta já foram montadas.
    pub fn listings(&self) -> usize {
        self.0.listings.load(Ordering::Relaxed)
    }
}

impl Inner {
    pub(crate) fn node(&self, ino: INodeNo) -> Res<Node> {
        self.inodes.node(ino.0).ok_or(Errno::ENOENT)
    }

    pub(crate) fn item(&self, inode: Inode) -> Res<Item> {
        self.engine
            .store()
            .by_inode(inode)
            .map_err(errno)?
            .ok_or(Errno::ENOENT)
    }

    pub(crate) fn item_by_id(&self, id: &ItemId) -> Res<Option<Item>> {
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

    /// Pastas virtuais são somente leitura, salvo a raiz (para `rename` para fora do Meu Drive
    /// dar `EXDEV`, não `EACCES`; criar nela é recusado pelo app) e a lixeira do KIO, que o KIO
    /// exige `0o700`.
    fn virtual_dir(&self, node: &Node) -> FileAttr {
        let perm = match node {
            Node::MountRoot => 0o755,
            Node::KioTrash | Node::KioFiles | Node::KioInfo => 0o700,
            _ => 0o555,
        };
        let ino = self.inodes.ino(node);
        self.attr(ino, FileType::Directory, 0, perm, self.mounted_at)
    }

    pub(crate) fn item_attr(&self, item: &Item) -> Res<FileAttr> {
        let size = match &item.kind {
            ItemKind::Shortcut { .. } => self.link_target(item)?.len() as u64,
            // Aberto para escrita: o banco só sabe o tamanho no fechamento, e o kernel usa este
            // valor para posicionar o `O_APPEND`.
            ItemKind::File if lock(&self.writers).contains_key(&item.inode) => {
                std::fs::metadata(self.engine.cache().local_path(&item.id))
                    .map_or(item.size, |m| m.len())
            }
            _ => item.size,
        };
        let ino = self.inodes.ino(&Node::Item(item.inode));
        let kind = file_type(&item.kind);
        Ok(self.attr(ino, kind, size, item_perm(item), time_of(item.modified_ms)))
    }

    pub(crate) fn kept_path(&self, folder: KeptFolder, name: &OsStr) -> PathBuf {
        self.engine.kept().get(folder).dir().join(name)
    }

    /// Caminho de um nó que é arquivo só local, com o modo dele.
    pub(crate) fn local_file(&self, node: &Node) -> Option<(PathBuf, u16)> {
        match node {
            Node::KeptFile(folder, name) => Some((self.kept_path(*folder, name), 0o444)),
            Node::KioInfoFile(name) => Some((self.kio.info_path(name), 0o600)),
            _ => None,
        }
    }

    /// Arquivo local que pode ser alterado; `EROFS` se for só leitura.
    pub(crate) fn writable_local(&self, node: &Node) -> Res<Option<PathBuf>> {
        match self.local_file(node) {
            Some((_, perm)) if perm & 0o200 == 0 => Err(Errno::EROFS),
            local => Ok(local.map(|(path, _)| path)),
        }
    }

    fn local_attr(&self, node: &Node) -> Res<FileAttr> {
        let (path, perm) = self.local_file(node).ok_or(Errno::ENOENT)?;
        let meta = std::fs::metadata(path).map_err(Errno::from)?;
        if !meta.is_file() {
            return Err(Errno::ENOENT);
        }
        let mtime = meta.modified().unwrap_or(self.mounted_at);
        let ino = self.inodes.ino(node);
        Ok(self.attr(ino, FileType::RegularFile, meta.size(), perm, mtime))
    }

    pub(crate) fn node_attr(&self, node: &Node) -> Res<FileAttr> {
        match node {
            Node::Item(inode) => self.item_attr(&self.item(*inode)?),
            Node::KeptFile(..) | Node::KioInfoFile(_) => self.local_attr(node),
            _ => Ok(self.virtual_dir(node)),
        }
    }

    /// Item em `files/` da lixeira do KIO com este nome.
    pub(crate) fn kio_item(&self, name: &OsStr) -> Res<Option<Item>> {
        if let Some(id) = self.kio.remembered(name) {
            return Ok(self.item_by_id(&id)?.filter(|i| i.trashed));
        }
        if !self.kio.has_info(name) {
            return Ok(None);
        }
        let name = name.to_str().ok_or(Errno::ENOENT)?;
        self.engine.store().lookup_trashed(name).map_err(errno)
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
            // A pasta de apoio não é um nível: "Computadores" fica na raiz da montagem.
            let support = self.engine.store().computers_folder().map_err(errno)?;
            let mut depth = 0;
            let mut parent = item.parent.clone();
            while let Some(id) = parent {
                depth += usize::from(support.as_ref() != Some(&id));
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
            Node::MountRoot if name == OsStr::new(&self.kio.name) => {
                return Ok(self.virtual_dir(&Node::KioTrash));
            }
            Node::MountRoot if name == OsStr::new(&self.layout.computers) => self.computers()?,
            Node::MountRoot => {
                let node = self.layout.lookup_root(name).ok_or(Errno::ENOENT)?;
                // Só "Meu Drive" depende do modo: o banco não é lido para as outras.
                if node == Node::Item(Inode::ROOT) && self.mirrored()? {
                    return Err(Errno::ENOENT);
                }
                return self.node_attr(&node);
            }
            Node::Kept(folder) => return self.local_attr(&Node::KeptFile(*folder, name.into())),
            Node::KioTrash => {
                let node = match name.to_str() {
                    Some("files") => Node::KioFiles,
                    Some("info") => Node::KioInfo,
                    _ => return Err(Errno::ENOENT),
                };
                return Ok(self.virtual_dir(&node));
            }
            Node::KioFiles => self.kio_item(name)?,
            Node::KioInfo => return self.local_attr(&Node::KioInfoFile(name.into())),
            Node::ShortcutTargets => {
                let id = ItemId(name.to_str().ok_or(Errno::ENOENT)?.into());
                self.item_by_id(&id)?.ok_or(Errno::ENOENT)?;
                return Ok(self.virtual_dir(&Node::TargetDir(id)));
            }
            Node::SharedDrives => {
                Layout::shared_drive_root(store, name.to_str().ok_or(Errno::ENOENT)?)
                    .map_err(errno)?
            }
            Node::Trash => store
                .lookup_trashed(name.to_str().ok_or(Errno::ENOENT)?)
                .map_err(errno)?,
            Node::TargetDir(id) => self
                .item_by_id(id)?
                .filter(|t| OsStr::new(&t.local_name) == name),
            Node::Item(inode) => {
                let dir = self.item(*inode)?;
                let found = store
                    .lookup(&dir.id, name.to_str().ok_or(Errno::ENOENT)?)
                    .map_err(errno)?;
                let support = store.computers_folder().map_err(errno)?;
                found.filter(|i| support.as_ref() != Some(&i.id))
            }
            Node::KeptFile(..) | Node::KioInfoFile(_) => return Err(Errno::ENOTDIR),
        };
        self.item_attr(&item.ok_or(Errno::ENOENT)?)
    }

    /// A pasta de apoio "Computadores", quando conhecida, viva e com algum computador dentro.
    fn computers(&self) -> Res<Option<Item>> {
        let store = self.engine.store();
        let Some(id) = store.computers_folder().map_err(errno)? else {
            return Ok(None);
        };
        let item = self.item_by_id(&id)?.filter(|i| !i.trashed);
        match item {
            Some(item) if store.has_children(&id).map_err(errno)? => Ok(Some(item)),
            _ => Ok(None),
        }
    }

    /// A conta está no modo Espelho: o Meu Drive fica numa pasta do computador.
    fn mirrored(&self) -> Res<bool> {
        Ok(self.engine.store().mirror().map_err(errno)?.is_some())
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
                .root_entries(self.mirrored()?)
                .into_iter()
                .map(|(name, node)| Entry {
                    ino: self.inodes.ino(&node),
                    kind: FileType::Directory,
                    name: name.into(),
                })
                .chain(self.computers()?.map(|c| Entry {
                    name: self.layout.computers.clone().into(),
                    ..self.item_entry(c)
                }))
                .collect(),
            Node::SharedDrives => self
                .engine
                .store()
                .enabled_drive_roots()
                .map_err(errno)?
                .into_iter()
                .map(|(name, root)| Entry {
                    name: name.into(),
                    ..self.item_entry(root)
                })
                .collect(),
            Node::Trash => self
                .trashed()?
                .into_iter()
                .map(|i| self.item_entry(i))
                .collect(),
            Node::Kept(folder) => self.local_entries(self.engine.kept().get(*folder).list(), |n| {
                Node::KeptFile(*folder, n)
            }),
            Node::KioTrash => [("files", Node::KioFiles), ("info", Node::KioInfo)]
                .map(|(name, node)| Entry {
                    ino: self.inodes.ino(&node),
                    kind: FileType::Directory,
                    name: name.into(),
                })
                .into(),
            Node::KioFiles => {
                let mut entries = vec![];
                for name in self.kio.candidate_names() {
                    if let Some(item) = self.kio_item(&name)? {
                        entries.push(Entry {
                            name,
                            ..self.item_entry(item)
                        });
                    }
                }
                entries
            }
            Node::KioInfo => {
                let paths = std::fs::read_dir(self.kio.info_dir())
                    .map_err(Errno::from)?
                    .filter_map(|e| Some(e.ok()?.path()))
                    .collect();
                self.local_entries(paths, Node::KioInfoFile)
            }
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
                let store = self.engine.store();
                let children = store.children(&dir.id).map_err(errno)?;
                // A pasta de apoio aparece na raiz da montagem, não dentro do Meu Drive.
                let support = store.computers_folder().map_err(errno)?;
                children
                    .into_iter()
                    .filter(|i| support.as_ref() != Some(&i.id))
                    .map(|i| self.item_entry(i))
                    .collect()
            }
            Node::KeptFile(..) | Node::KioInfoFile(_) => return Err(Errno::ENOTDIR),
        })
    }

    fn local_entries(&self, paths: Vec<PathBuf>, node: impl Fn(OsString) -> Node) -> Vec<Entry> {
        paths
            .into_iter()
            .filter_map(|path| {
                let name = path.file_name()?.to_owned();
                Some(Entry {
                    ino: self.inodes.ino(&node(name.clone())),
                    kind: FileType::RegularFile,
                    name,
                })
            })
            .collect()
    }

    /// A pasta do Meu Drive ou de um Drive compartilhado em `node` e o caminho dela na montagem.
    /// Pastas virtuais, da lixeira ou fora dessas raízes ficam de fora.
    fn drive_folder(&self, node: &Node) -> Option<(ItemId, PathBuf)> {
        let Node::Item(inode) = node else {
            return None;
        };
        let dir = self
            .item(*inode)
            .inspect_err(|e| tracing::debug!(erro = ?e, "pasta sem item; sem miniaturas"))
            .ok()?;
        if dir.kind != ItemKind::Folder || dir.trashed {
            return None;
        }
        let at = self
            .engine
            .store()
            .path_of(&dir.id)
            .inspect_err(|e| tracing::debug!(erro = %e, "caminho da pasta; sem miniaturas"))
            .ok()??;
        Some((dir.id, self.layout.mounted(&self.mount_point, &at)))
    }

    /// Semeia as miniaturas de uma pasta do Meu Drive ou de um drive (`drive_folder`) e espera até
    /// `THUMB_WAIT`; o resto segue em segundo plano. Sem rede ou sem vaga entre os que esperam,
    /// só dispara; com a pasta já em semeadura (reaberta antes de terminar), nem espera.
    fn wait_thumbnails(&self, node: &Node) {
        let Some((id, path)) = self.drive_folder(node) else {
            return;
        };
        let waiter = if self.engine.is_offline() {
            None
        } else {
            self.thumb_waiters.try_acquire().ok()
        };
        // A semeadura cria tarefas: precisa do contexto do runtime, não da thread do FUSE.
        let Some(_waiter) = waiter else {
            let _rt = self.rt.enter();
            drop(self.engine.seed_thumbnails(&id, path));
            return;
        };
        self.rt.block_on(async {
            if let Some(seeding) = self.engine.seed_thumbnails(&id, path) {
                let _ = tokio::time::timeout(THUMB_WAIT, seeding).await;
            }
        });
    }

    pub(crate) fn handle(&self) -> u64 {
        self.next_handle.fetch_add(1, Ordering::Relaxed)
    }

    fn open_file(&self, node: &Node, flags: OpenFlags) -> Res<OpenFile> {
        let write = flags.acc_mode() != OpenAccMode::O_RDONLY;
        let truncate = flags.0 & libc::O_TRUNC != 0;
        let local = match write {
            true => self.writable_local(node)?,
            false => self.local_file(node).map(|(path, _)| path),
        };
        if let Some(path) = local {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(write)
                .truncate(write && truncate)
                .open(path)
                .map_err(Errno::from)?;
            return Ok(OpenFile::Local(Arc::new(file)));
        }
        let Node::Item(inode) = node else {
            return Err(Errno::EISDIR);
        };
        let item = self.item(*inode)?;
        match &item.kind {
            ItemKind::File if write => Ok(OpenFile::Write {
                inode: item.inode,
                file: self.open_for_write(&item, truncate)?,
            }),
            // Com alguém escrevendo, o conteúdo atual é o do arquivo local.
            ItemKind::File if lock(&self.writers).contains_key(&item.inode) => {
                let path = self.engine.cache().local_path(&item.id);
                Ok(OpenFile::Local(Arc::new(
                    File::open(path).map_err(Errno::from)?,
                )))
            }
            ItemKind::File => Ok(OpenFile::Drive {
                inode: item.inode,
                id: item.id,
                last_end: None,
            }),
            // Ponteiro de Documento Google não se edita.
            ItemKind::GoogleDoc { .. } if write => Err(Errno::EPERM),
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

    fn read(&self, fh: u64, offset: u64, size: u32) -> Res<Bytes> {
        let end = offset.saturating_add(size.into());
        // Decide com o mapa travado; a leitura em si acontece fora da trava.
        let (inode, id, sequential) = match lock(&self.files).get_mut(&fh).ok_or(Errno::EBADF)? {
            OpenFile::Drive {
                inode,
                id,
                last_end,
            } => {
                let sequential = last_end.replace(end) == Some(offset);
                (*inode, id.clone(), sequential)
            }
            OpenFile::Pointer(json) => return Ok(slice(json, offset, size)),
            OpenFile::Local(file) | OpenFile::Write { file, .. } => {
                let mut buf = vec![0; size as usize];
                let n = file.read_at(&mut buf, offset).map_err(Errno::from)?;
                buf.truncate(n);
                return Ok(buf.into());
            }
        };
        let cache = self.engine.cache();
        let read = |id: &ItemId| {
            self.rt
                .block_on(cache.read_hinted(id, offset, size, Some(sequential)))
        };
        let result = read(&id).or_else(|e| match self.item(inode) {
            // O envio terminou e o conteúdo mudou de nome: lê pelo ID novo, que o handle guarda.
            Ok(now) if now.id != id => {
                if let Some(OpenFile::Drive { id, .. }) = lock(&self.files).get_mut(&fh) {
                    *id = now.id.clone();
                }
                read(&now.id)
            }
            _ => Err(e),
        });
        result.map_err(errno)
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
        match self.0.node(ino).and_then(|n| self.0.open_file(&n, flags)) {
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
        let file = lock(&self.0.files).remove(&fh.0);
        if let Some(OpenFile::Write { inode, .. }) = file {
            self.0.finish_writing(inode);
        }
        reply.ok();
    }

    fn create(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        _flags: i32,
        reply: ReplyCreate,
    ) {
        match self.0.node(parent).and_then(|p| self.0.create(&p, name)) {
            Ok((attr, file)) => {
                let fh = self.0.handle();
                lock(&self.0.files).insert(fh, file);
                reply.created(
                    &TTL,
                    &attr,
                    Generation(0),
                    FileHandle(fh),
                    FopenFlags::empty(),
                );
            }
            Err(e) => reply.error(e),
        }
    }

    fn write(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        offset: u64,
        data: &[u8],
        _write_flags: WriteFlags,
        _flags: OpenFlags,
        _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        match self.0.write(fh.0, offset, data) {
            Ok(n) => reply.written(n),
            Err(e) => reply.error(e),
        }
    }

    fn flush(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        match self.0.flush(fh.0) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }

    fn fsync(
        &self,
        _req: &Request,
        _ino: INodeNo,
        fh: FileHandle,
        _datasync: bool,
        reply: ReplyEmpty,
    ) {
        match self.0.fsync(fh.0) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }

    fn setattr(
        &self,
        _req: &Request,
        ino: INodeNo,
        _mode: Option<u32>,
        _uid: Option<u32>,
        _gid: Option<u32>,
        size: Option<u64>,
        _atime: Option<TimeOrNow>,
        mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>,
        fh: Option<FileHandle>,
        _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>,
        _bkuptime: Option<SystemTime>,
        _flags: Option<fuser::BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        // `chmod` e `chown` são ignorados sem erro: o Drive não tem permissões Unix.
        let result = self
            .0
            .node(ino)
            .and_then(|node| self.0.setattr(&node, fh.map(|f| f.0), size, mtime));
        match result {
            Ok(attr) => reply.attr(&TTL, &attr),
            Err(e) => reply.error(e),
        }
    }

    fn mkdir(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        _mode: u32,
        _umask: u32,
        reply: ReplyEntry,
    ) {
        match self.0.node(parent).and_then(|p| self.0.mkdir(&p, name)) {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(e) => reply.error(e),
        }
    }

    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        match self.0.node(parent).and_then(|p| self.0.unlink(&p, name)) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }

    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        match self.0.node(parent).and_then(|p| self.0.rmdir(&p, name)) {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }

    fn rename(
        &self,
        _req: &Request,
        parent: INodeNo,
        name: &OsStr,
        newparent: INodeNo,
        newname: &OsStr,
        flags: RenameFlags,
        reply: ReplyEmpty,
    ) {
        let result = self.0.node(parent).and_then(|from| {
            let to = self.0.node(newparent)?;
            self.0.rename(&from, name, &to, newname, flags)
        });
        match result {
            Ok(()) => reply.ok(),
            Err(e) => reply.error(e),
        }
    }

    fn opendir(&self, req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        let listed = self.0.node(ino).and_then(|n| {
            if !skips_thumbnails(req.pid()) {
                self.0.wait_thumbnails(&n);
            }
            self.0.list(&n)
        });
        match listed {
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
    let mut config = Config::default();
    config.mount_options = vec![
        MountOption::FSName("gdrive-desktop-linux".into()),
        MountOption::Subtype("gdrive".into()),
        MountOption::DefaultPermissions,
        MountOption::NoExec,
        MountOption::NoSuid,
        MountOption::NoDev,
    ];
    config.n_threads = Some(FUSE_THREADS);
    fuser::spawn_mount(fs, at, &config)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{is_indexer, is_walker, process_name, skips_thumbnails_in};

    /// `/proc` falso: a thread `tid` (com o nome `thread`) do processo `tgid` (com o nome `name`).
    fn fake_proc(root: &Path, tid: u32, thread: &str, tgid: u32, name: &str) {
        let task = root.join(tid.to_string());
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("comm"), format!("{thread}\n")).unwrap();
        let status = format!(
            "Name:\t{thread}\nUmask:\t0022\nState:\tS (sleeping)\nTgid:\t{tgid}\nNgid:\t0\nPid:\t{tid}\n"
        );
        std::fs::write(task.join("status"), status).unwrap();
        let process = root.join(tgid.to_string());
        std::fs::create_dir_all(&process).unwrap();
        std::fs::write(process.join("comm"), format!("{name}\n")).unwrap();
    }

    /// Bug: o FUSE informa o ID da thread que chamou, e o `comm` dela é o nome da thread
    /// ("Thread (pooled)" no Baloo, "pool-tracker-mi" no Tracker): o indexador passava.
    #[test]
    fn indexer_is_recognized_by_the_process_of_the_thread() {
        let proc_root = tempfile::tempdir().unwrap();
        fake_proc(
            proc_root.path(),
            4321,
            "Thread (pooled)",
            4000,
            "baloo_file",
        );
        fake_proc(
            proc_root.path(),
            5321,
            "pool-tracker-mi",
            5000,
            "tracker-miner-f",
        );
        fake_proc(proc_root.path(), 6321, "pool-dolphin", 6000, "dolphin");
        assert_eq!(
            process_name(proc_root.path(), 4321)
                .as_deref()
                .map(str::trim),
            Some("baloo_file")
        );
        assert!(skips_thumbnails_in(proc_root.path(), 4321));
        assert!(skips_thumbnails_in(proc_root.path(), 5321));
        assert!(!skips_thumbnails_in(proc_root.path(), 6321));
    }

    /// Sem `/proc` legível, ou com `status` sem `Tgid`, não é indexador (segue normal).
    #[test]
    fn unreadable_proc_is_not_an_indexer() {
        let proc_root = tempfile::tempdir().unwrap();
        assert!(!skips_thumbnails_in(proc_root.path(), 1));
        assert_eq!(process_name(proc_root.path(), 1), None);
        let task = proc_root.path().join("7");
        std::fs::create_dir_all(&task).unwrap();
        std::fs::write(task.join("comm"), "Thread (pooled)\n").unwrap();
        std::fs::write(task.join("status"), "Name:\tThread (pooled)\n").unwrap();
        assert_eq!(process_name(proc_root.path(), 7), None);
        assert!(!skips_thumbnails_in(proc_root.path(), 7));
    }

    /// No `/proc` de verdade: pelo ID de uma thread com nome próprio sai o nome do processo.
    #[test]
    fn real_thread_id_gives_the_process_name() {
        let expected = std::fs::read_to_string("/proc/self/comm").unwrap();
        let (name, thread) = std::thread::Builder::new()
            .name("Thread (pooled)".into())
            .spawn(|| {
                // `/proc/thread-self` aponta para `<tgid>/task/<tid>`.
                let link = std::fs::read_link("/proc/thread-self").unwrap();
                let tid: u32 = link.file_name().unwrap().to_str().unwrap().parse().unwrap();
                let thread = std::fs::read_to_string(format!("/proc/{tid}/comm")).unwrap();
                (process_name(Path::new("/proc"), tid), thread)
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(thread.trim(), "Thread (pooled)");
        assert_eq!(name.as_deref(), Some(expected.as_str()));
    }

    #[test]
    fn indexers_are_recognized_by_comm() {
        // O `comm` vem de `/proc/<pid>/comm`: com quebra de linha e cortado em 15 bytes.
        for comm in [
            "baloo_file\n",
            "baloo_file_extr\n",
            "tracker-miner-f\n",
            "localsearch-3\n",
            "updatedb\n",
            "plocate-build\n",
            "mlocate\n",
        ] {
            assert!(is_indexer(comm), "{comm:?}");
        }
        for comm in [
            "dolphin\n",
            "ls\n",
            "nautilus",
            "kioworker\n",
            "",
            "baloo\n",
        ] {
            assert!(!is_indexer(comm), "{comm:?}");
        }
    }

    /// Varreduras (find, rsync, backup, IDE) não mostram miniaturas: uma subárvore de 75 pastas
    /// levava 112 s esperando e semeando cada pasta. Pelo nome exato do processo: `cp` não é
    /// `cpp`, e `find` não é `findmnt`.
    #[test]
    fn walkers_skip_thumbnails_by_exact_name() {
        for comm in [
            "find\n", "rsync\n", "rg\n", "du\n", "git\n", "restic\n", "code\n",
        ] {
            assert!(is_walker(comm), "{comm:?}");
        }
        for comm in [
            "findmnt\n",
            "cpp\n",
            "dolphin\n",
            "nautilus\n",
            "kioworker\n",
            "",
        ] {
            assert!(!is_walker(comm), "{comm:?}");
        }
        let proc_root = tempfile::tempdir().unwrap();
        fake_proc(proc_root.path(), 7321, "find", 7000, "find");
        fake_proc(proc_root.path(), 8321, "pool-dolphin", 8000, "dolphin");
        assert!(skips_thumbnails_in(proc_root.path(), 7321));
        assert!(!skips_thumbnails_in(proc_root.path(), 8321));
    }
}
