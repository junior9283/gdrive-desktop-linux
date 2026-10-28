//! Escrita na montagem: criar, gravar, truncar, datas, pastas, excluir para a lixeira do Drive,
//! renomear e mover, salvar por `rename` por cima e a lixeira do KIO.

use std::ffi::OsStr;
use std::fs::{File, OpenOptions};
use std::os::unix::fs::FileExt;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

use fuser::{Errno, FileAttr, RenameFlags, TimeOrNow};
use gdl_common::time::now_ms;
use gdl_engine::ops::Upload;
use gdl_store::{Inode, Item, ItemKind, LocalState};

use crate::fs::{Inner, OpenFile, Res, Writer, errno, lock, time_of};
use crate::inodes::Node;

fn name_str(name: &OsStr) -> Res<&str> {
    name.to_str().ok_or(Errno::EINVAL)
}

fn ms_of(time: TimeOrNow) -> i64 {
    match time {
        TimeOrNow::SpecificTime(t) => t
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as i64),
        TimeOrNow::Now => now_ms(),
    }
}

impl Writer {
    /// Tira o que foi gravado desde o último registro; os handles continuam abertos.
    fn take_pending(&mut self) -> Option<Writer> {
        if !self.wrote {
            return None;
        }
        let open = Writer {
            handles: self.handles,
            serial: self.serial.clone(),
            ..Writer::default()
        };
        Some(std::mem::replace(self, open))
    }
}

impl Inner {
    fn block<T>(&self, f: impl Future<Output = gdl_engine::Result<T>>) -> Res<T> {
        self.rt.block_on(f).map_err(errno)
    }

    /// Pasta do Meu Drive onde dá para criar e mover itens.
    fn writable_folder(&self, node: &Node) -> Res<Item> {
        let Node::Item(inode) = node else {
            return Err(Errno::EACCES);
        };
        let dir = self.item(*inode)?;
        match dir.kind {
            ItemKind::Folder if dir.can_edit && !dir.trashed => Ok(dir),
            ItemKind::Folder => Err(Errno::EACCES),
            _ => Err(Errno::ENOTDIR),
        }
    }

    fn child(&self, dir: &Item, name: &OsStr) -> Res<Option<Item>> {
        self.engine
            .store()
            .lookup(&dir.id, name_str(name)?)
            .map_err(errno)
    }

    fn existing_child(&self, dir: &Item, name: &OsStr) -> Res<Item> {
        self.child(dir, name)?.ok_or(Errno::ENOENT)
    }

    fn ensure_empty(&self, dir: &Item) -> Res<()> {
        match self.engine.store().has_children(&dir.id).map_err(errno)? {
            true => Err(Errno::ENOTEMPTY),
            false => Ok(()),
        }
    }

    /// Arquivo de um handle que grava direto no disco, com o inode quando é conteúdo do Drive.
    fn handle_file(&self, fh: u64) -> Option<(Arc<File>, Option<Inode>)> {
        match lock(&self.files).get(&fh)? {
            OpenFile::Write { inode, file } => Some((file.clone(), Some(*inode))),
            OpenFile::Local(file) => Some((file.clone(), None)),
            _ => None,
        }
    }

    /// Abre o conteúdo inteiro para escrita (baixando o que faltar, salvo com `O_TRUNC`).
    pub(crate) fn open_for_write(&self, item: &Item, truncate: bool) -> Res<Arc<File>> {
        if item.trashed || !item.can_edit {
            return Err(Errno::EACCES);
        }
        if !truncate {
            self.block(async { Ok(self.engine.cache().ensure_full(&item.id).await?) })?;
        }
        // Com a trava do item: um download em andamento não grava por cima do que vem agora.
        let cache = self.engine.cache();
        let file = self
            .rt
            .block_on(cache.locked(&item.id, || {
                let file = cache.open_local(&item.id, truncate)?;
                self.start_writing(item, truncate);
                Ok::<_, std::io::Error>(file)
            }))
            .map_err(Errno::from)?;
        Ok(Arc::new(file))
    }

    fn start_writing(&self, item: &Item, wrote: bool) {
        let mut writers = lock(&self.writers);
        let writer = writers.entry(item.inode).or_default();
        if writer.handles == 0 {
            // Marca a edição já na abertura: o cache não libera o conteúdo e uma mudança remota
            // durante a edição vira conflito, não sobrescreve.
            writer.was_clean = self
                .engine
                .store()
                .transition_local_state(&item.id, LocalState::Clean, LocalState::Dirty)
                .unwrap_or(false);
            writer.mark = Some(self.engine.mark_writing(item.inode));
        }
        writer.handles += 1;
        writer.wrote |= wrote;
    }

    fn mark_written(&self, inode: Inode) {
        if let Some(writer) = lock(&self.writers).get_mut(&inode) {
            writer.wrote = true;
            // Gravar depois de escolher a data volta a data para agora, como no Linux.
            writer.mtime = None;
        }
    }

    /// `close`: registra o que foi gravado até aqui, para quem vem depois do `close` (ler o
    /// arquivo, ver o atalho do ponteiro copiado) não depender do `release`, que o kernel manda
    /// sem esperar. Cópias do descritor (`fork`) também mandam `flush`, antes e ao mesmo tempo.
    pub(crate) fn flush(&self, fh: u64) -> Res<()> {
        self.commit_pending(fh, Upload::Soon)
    }

    /// `fsync` explícito: o envio vai para a fila na hora.
    pub(crate) fn fsync(&self, fh: u64) -> Res<()> {
        let Some((file, _)) = self.handle_file(fh) else {
            return Ok(());
        };
        file.sync_all().map_err(Errno::from)?;
        self.commit_pending(fh, Upload::Now)
    }

    /// Trava de registro do item: um por vez, e quem chega espera o que está em andamento.
    fn serial(&self, inode: Inode) -> Option<Arc<std::sync::Mutex<()>>> {
        lock(&self.writers).get(&inode).map(|w| w.serial.clone())
    }

    fn commit_pending(&self, fh: u64, upload: Upload) -> Res<()> {
        let Some((_, Some(inode))) = self.handle_file(fh) else {
            return Ok(());
        };
        let Some(serial) = self.serial(inode) else {
            return Ok(());
        };
        let _one = lock(&serial);
        let pending = lock(&self.writers)
            .get_mut(&inode)
            .and_then(Writer::take_pending);
        match pending {
            Some(pending) => self.commit(inode, &pending, upload),
            None => Ok(()),
        }
    }

    /// Último fechamento: registra a edição e agenda o envio (ou desfaz a marca, se nada foi
    /// gravado).
    pub(crate) fn finish_writing(&self, inode: Inode) {
        let Some(serial) = self.serial(inode) else {
            return;
        };
        let _one = lock(&serial);
        let writer = {
            let mut writers = lock(&self.writers);
            let Some(mut writer) = writers.remove(&inode) else {
                return;
            };
            writer.handles -= 1;
            if writer.handles > 0 {
                writers.insert(inode, writer);
                return;
            }
            writer
        };
        if let Err(e) = self.commit(inode, &writer, Upload::Soon) {
            tracing::warn!(erro = ?e, "falha ao registrar a gravação");
        }
    }

    fn commit(&self, inode: Inode, writer: &Writer, upload: Upload) -> Res<()> {
        if !writer.wrote && !writer.was_clean {
            return Ok(());
        }
        let item = match self.item(inode) {
            // Já virou atalho (ou foi para a lixeira) num registro anterior.
            Err(Errno::ENOENT) => return Ok(()),
            item => item?,
        };
        if !writer.wrote {
            self.engine
                .store()
                .transition_local_state(&item.id, LocalState::Dirty, LocalState::Clean)
                .map_err(errno)?;
            return Ok(());
        }
        let path = self.engine.cache().local_path(&item.id);
        let size = std::fs::metadata(path).map_err(Errno::from)?.len();
        let modified = writer.mtime.unwrap_or_else(now_ms);
        self.block(self.engine.finish_write(&item.id, size, modified, upload))
    }

    pub(crate) fn create(&self, parent: &Node, name: &OsStr) -> Res<(FileAttr, OpenFile)> {
        if let Node::KioInfo = parent {
            let node = Node::KioInfoFile(name.into());
            let path = self.kio.info_path(name);
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(path)
                .map_err(Errno::from)?;
            return Ok((self.node_attr(&node)?, OpenFile::Local(Arc::new(file))));
        }
        let dir = self.writable_folder(parent)?;
        let item = self
            .engine
            .create_file(&dir.id, name_str(name)?)
            .map_err(errno)?;
        // Aberto com `O_TRUNC`, conta como gravado: um arquivo novo sobe mesmo vazio (`touch`).
        let file = self.open_for_write(&item, true)?;
        let attr = self.item_attr(&item)?;
        Ok((
            attr,
            OpenFile::Write {
                inode: item.inode,
                file,
            },
        ))
    }

    pub(crate) fn write(&self, fh: u64, offset: u64, data: &[u8]) -> Res<u32> {
        let (file, inode) = self.handle_file(fh).ok_or(Errno::EBADF)?;
        file.write_all_at(data, offset).map_err(|e| {
            self.engine.cache().note_write_error(&e, now_ms());
            Errno::from(e)
        })?;
        if let Some(inode) = inode {
            self.mark_written(inode);
        }
        Ok(data.len() as u32)
    }

    pub(crate) fn setattr(
        &self,
        node: &Node,
        fh: Option<u64>,
        size: Option<u64>,
        mtime: Option<TimeOrNow>,
    ) -> Res<FileAttr> {
        if let Some(size) = size {
            self.truncate(node, fh, size)?;
        }
        if let Some(mtime) = mtime {
            self.set_mtime(node, ms_of(mtime))?;
        }
        self.node_attr(node)
    }

    /// `truncate` é uma edição: pelo handle aberto ou abrindo e fechando na hora.
    fn truncate(&self, node: &Node, fh: Option<u64>, size: u64) -> Res<()> {
        if let Some((file, inode)) = fh.and_then(|fh| self.handle_file(fh)) {
            file.set_len(size).map_err(Errno::from)?;
            if let Some(inode) = inode {
                self.mark_written(inode);
            }
            return Ok(());
        }
        if let Some(path) = self.writable_local(node)? {
            let file = OpenOptions::new()
                .write(true)
                .open(path)
                .map_err(Errno::from)?;
            return file.set_len(size).map_err(Errno::from);
        }
        let Node::Item(inode) = node else {
            return Err(Errno::EISDIR);
        };
        let item = self.item(*inode)?;
        match item.kind {
            ItemKind::File => {}
            ItemKind::GoogleDoc { .. } => return Err(Errno::EPERM),
            _ => return Err(Errno::EISDIR),
        }
        let file = self.open_for_write(&item, size == 0)?;
        let result = file.set_len(size).map_err(Errno::from);
        self.mark_written(item.inode);
        self.finish_writing(item.inode);
        result
    }

    fn set_mtime(&self, node: &Node, ms: i64) -> Res<()> {
        if let Some(path) = self.writable_local(node)? {
            return File::options()
                .write(true)
                .open(path)
                .and_then(|f| f.set_modified(time_of(ms)))
                .map_err(Errno::from);
        }
        let Node::Item(inode) = node else {
            return Ok(());
        };
        let item = self.item(*inode)?;
        if let ItemKind::GoogleDoc { .. } = item.kind {
            return Err(Errno::EPERM);
        }
        if let Some(writer) = lock(&self.writers).get_mut(inode) {
            writer.mtime = Some(ms);
        }
        self.block(self.engine.set_modified(&item.id, ms))?;
        Ok(())
    }

    pub(crate) fn mkdir(&self, parent: &Node, name: &OsStr) -> Res<FileAttr> {
        if let Node::KioTrash = parent {
            return Err(Errno::EEXIST);
        }
        let dir = self.writable_folder(parent)?;
        let item = self.block(self.engine.create_folder(&dir.id, name_str(name)?))?;
        self.item_attr(&item)
    }

    /// Esvaziar a lixeira do Dolphin: só a entrada local some; o item continua na lixeira do
    /// Drive. Dentro de uma pasta que está lá, o conteúdo vai junto com ela e não muda. Devolve
    /// se a remoção era na lixeira (e já foi tratada).
    fn forget_from_kio_trash(&self, parent: &Node, name: &OsStr) -> Res<bool> {
        let found = match parent {
            Node::KioFiles => {
                let item = self.kio_item(name)?;
                if let Some(item) = &item {
                    self.block(self.engine.empty_from_trash(&item.id))?;
                }
                self.kio.forget(name).is_some() || item.is_some()
            }
            Node::Item(inode) => {
                let dir = self.item(*inode)?;
                if !dir.trashed {
                    return Ok(false);
                }
                self.child(&dir, name)?.is_some()
            }
            _ => return Ok(false),
        };
        if found { Ok(true) } else { Err(Errno::ENOENT) }
    }

    pub(crate) fn unlink(&self, parent: &Node, name: &OsStr) -> Res<()> {
        if self.forget_from_kio_trash(parent, name)? {
            return Ok(());
        }
        match parent {
            Node::KioInfo => std::fs::remove_file(self.kio.info_path(name)).map_err(Errno::from),
            _ => {
                let dir = self.writable_folder(parent)?;
                let item = self.existing_child(&dir, name)?;
                if item.kind == ItemKind::Folder {
                    return Err(Errno::EISDIR);
                }
                self.block(self.engine.trash(&item.id))
            }
        }
    }

    pub(crate) fn rmdir(&self, parent: &Node, name: &OsStr) -> Res<()> {
        if self.forget_from_kio_trash(parent, name)? {
            return Ok(());
        }
        let dir = self.writable_folder(parent)?;
        let item = self.existing_child(&dir, name)?;
        if item.kind != ItemKind::Folder {
            return Err(Errno::ENOTDIR);
        }
        self.ensure_empty(&item)?;
        self.block(self.engine.trash(&item.id))
    }

    pub(crate) fn rename(
        &self,
        from: &Node,
        name: &OsStr,
        to: &Node,
        new_name: &OsStr,
        flags: RenameFlags,
    ) -> Res<()> {
        if flags.intersects(RenameFlags::RENAME_EXCHANGE | RenameFlags::RENAME_WHITEOUT) {
            return Err(Errno::EINVAL);
        }
        let no_replace = flags.contains(RenameFlags::RENAME_NOREPLACE);
        match (from, to) {
            (Node::Item(_), Node::Item(_)) => {
                self.rename_in_drive(from, name, to, new_name, no_replace)
            }
            // Dolphin mandando para a lixeira.
            (Node::Item(_), Node::KioFiles) => {
                let dir = self.writable_folder(from)?;
                let item = self.existing_child(&dir, name)?;
                self.block(self.engine.trash(&item.id))?;
                self.kio.remember(new_name, item.id);
                Ok(())
            }
            // Restaurar da lixeira do Dolphin.
            (Node::KioFiles, Node::Item(_)) => {
                let item = self.kio_item(name)?.ok_or(Errno::ENOENT)?;
                let dir = self.writable_folder(to)?;
                let restored = self.block(self.engine.untrash(&item.id))?;
                let (new_name, parent) = (name_str(new_name)?, Some(&dir.id));
                if restored.parent.as_ref() != parent || restored.local_name != new_name {
                    self.block(self.engine.move_item(&item.id, &dir.id, new_name))?;
                }
                self.kio.forget(name);
                Ok(())
            }
            (Node::KioFiles, Node::KioFiles) => {
                let item = self.kio_item(name)?.ok_or(Errno::ENOENT)?;
                self.kio.forget(name);
                self.kio.remember(new_name, item.id);
                Ok(())
            }
            (Node::KioInfo, Node::KioInfo) => {
                std::fs::rename(self.kio.info_path(name), self.kio.info_path(new_name))
                    .map_err(Errno::from)
            }
            // Tirar uma cópia guardada da pasta dela é um arquivo novo no Drive.
            (Node::Kept(folder), Node::Item(_)) => {
                let dir = self.writable_folder(to)?;
                let path = self.kept_path(*folder, name);
                self.block(
                    self.engine
                        .restore_lost(&path, &dir.id, name_str(new_name)?),
                )?;
                Ok(())
            }
            _ => Err(Errno::EXDEV),
        }
    }

    fn rename_in_drive(
        &self,
        from: &Node,
        name: &OsStr,
        to: &Node,
        new_name: &OsStr,
        no_replace: bool,
    ) -> Res<()> {
        let src_dir = self.writable_folder(from)?;
        let dst_dir = if from == to {
            src_dir.clone()
        } else {
            self.writable_folder(to)?
        };
        let src = self.existing_child(&src_dir, name)?;
        let new_name = name_str(new_name)?;
        let Some(dst) = self.child(&dst_dir, OsStr::new(new_name))? else {
            self.block(self.engine.move_item(&src.id, &dst_dir.id, new_name))?;
            return Ok(());
        };
        if dst.id == src.id {
            return Ok(());
        }
        if no_replace {
            return Err(Errno::EEXIST);
        }
        match (&src.kind, &dst.kind) {
            // Salvar por cima: o destino mantém o ID do Drive e recebe o conteúdo.
            (ItemKind::File, ItemKind::File) => {
                return self.block(self.engine.save_over(&src.id, &dst.id));
            }
            (ItemKind::Folder, ItemKind::Folder) => self.ensure_empty(&dst)?,
            (ItemKind::Folder, _) => return Err(Errno::ENOTDIR),
            (_, ItemKind::Folder) => return Err(Errno::EISDIR),
            _ => {}
        }
        // Substituir por outro item: o antigo vai para a lixeira do Drive.
        self.block(async {
            self.engine.trash(&dst.id).await?;
            self.engine
                .move_item(&src.id, &dst_dir.id, new_name)
                .await?;
            Ok(())
        })
    }
}
