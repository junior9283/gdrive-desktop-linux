//! O que o servidor MCP pede ao serviço: o estado e o link de um caminho, a janela Compartilhar
//! com um e-mail já digitado e levar uma pasta do computador para o Meu Drive (modo "stream":
//! conferir, copiar para a montagem, conferir a cópia e trocar a pasta original por um link para
//! ela; a original vai para a lixeira do sistema, nunca é apagada, e uma falha desfaz o que já
//! foi feito).

use std::io;
use std::path::{Component, Path, PathBuf};

use gdl_folders::guard;
use gdl_folders::trash::Trash;
use gdl_ipc::{PathStatus, ServiceError};

use super::folders::refusal_text;
use super::{Daemon, live_item, web_link};

impl Daemon {
    /// O estado de `path`: na montagem, o do item (`offline`, `syncing`, `conflict` ou `online`);
    /// numa pasta do computador, o emblema dela; fora das duas, `outside` sem conta.
    pub async fn path_status(&self, path: &str) -> Result<PathStatus, ServiceError> {
        let (engine, state, item_id) = if let Ok((engine, id)) = self.item_at(path).await {
            let state = match gdl_fs::item_state(&live_item(&engine, &id.0)?) {
                "cloud" => "online",
                state => state,
            };
            (engine, state, id.0)
        } else if let Some((engine, state)) = self.folder_state(Path::new(path)).await {
            (engine, state, String::new())
        } else {
            return Ok(PathStatus {
                state: "outside".into(),
                ..Default::default()
            });
        };
        Ok(PathStatus {
            account: self.email_of(&engine).await?,
            state: state.into(),
            item_id,
        })
    }

    /// O link da web do item da montagem em `path` (sem o aviso de "link copiado" do menu).
    pub async fn web_link_of_path(&self, path: &str) -> Result<String, ServiceError> {
        let (engine, id) = self.item_at(path).await?;
        web_link(&live_item(&engine, &id.0)?)
    }

    /// Abre a janela Compartilhar do item com `email` já no campo de pessoas; não salva nada.
    pub async fn open_share(
        &self,
        account: &str,
        item_id: &str,
        email: &str,
    ) -> Result<(), ServiceError> {
        let engine = self.account_engine(account).await?;
        live_item(&engine, item_id)?;
        let account = self.email_of(&engine).await?;
        self.open_screen(format!("share:{account}/{item_id}/{email}"));
        Ok(())
    }

    /// Leva a pasta `path` para `drive_dir` (relativo ao Meu Drive da conta; vazio, a raiz) e
    /// deixa no lugar um link para a cópia. Recusa com `Refused`: um motivo de
    /// `CheckLocalFolder`, `destination-missing`, `exists` ou `no-space`. Devolve o caminho novo.
    pub async fn move_to_drive(
        &self,
        account: &str,
        path: &str,
        drive_dir: &str,
    ) -> Result<String, ServiceError> {
        let refused = |why: &str| ServiceError::Refused(why.into());
        let engine = self.account_engine(account).await?;
        let mount = self.mount_point(account).await?;
        // Um link no lugar da pasta: mover o alvo dele deixaria o link apontando para outro.
        if std::fs::symlink_metadata(path).is_ok_and(|m| m.is_symlink()) {
            return Err(refused("missing"));
        }
        // Medir a pasta e perguntar a cota ao Drive não dependem um do outro.
        let (checked, about) = tokio::join!(self.check(Path::new(path), None), engine.about());
        let (local, info) = checked.map_err(|r| refused(refusal_text(r)))?;
        let parent = inside(&mount.join(&self.layout.my_drive), drive_dir)
            .filter(|p| p.is_dir())
            .ok_or_else(|| refused("destination-missing"))?;
        let name = local.file_name().ok_or_else(|| refused("missing"))?;
        let target = parent.join(name);
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err(refused("exists"));
        }
        let about = about.map_err(super::failed)?;
        if about
            .quota_total
            .is_some_and(|total| total.saturating_sub(about.quota_used) < info.size)
        {
            return Err(refused("no-space"));
        }
        let trash = Trash::new(self.config.paths.data_home.join("Trash"));
        let done = target.clone();
        tokio::task::spawn_blocking(move || swap(&local, &done, info, &trash))
            .await
            .map_err(super::failed)?
            .map_err(super::failed)?;
        Ok(target.display().to_string())
    }
}

/// `base` mais o caminho relativo `dir`, só com nomes comuns (nada de `..` nem absoluto).
fn inside(base: &Path, dir: &str) -> Option<PathBuf> {
    let dir = Path::new(dir.trim_matches('/'));
    dir.components()
        .all(|c| matches!(c, Component::Normal(_)))
        .then(|| base.join(dir))
}

/// Copia `local` para `target`, confere e troca `local` por um link; desfaz ao falhar.
fn swap(local: &Path, target: &Path, info: guard::FolderInfo, trash: &Trash) -> io::Result<()> {
    // Criar antes de tudo: se outra chamada já criou o destino, esta falha sem desfazer nada.
    std::fs::create_dir(target)?;
    // A cópia (inteira ou pela metade) sai do Drive: vai para a lixeira dele.
    let undo = || {
        let _ = std::fs::remove_dir_all(target);
    };
    let trashed = copy_contents(local, target)
        .and_then(|()| match guard::measure(target) {
            copy if copy == info => Ok(()),
            copy => Err(io::Error::other(format!(
                "a cópia tem {} arquivos e {} bytes; a original, {} e {}",
                copy.files, copy.size, info.files, info.size
            ))),
        })
        .and_then(|()| trash.put(local))
        .inspect_err(|_| undo())?;
    std::os::unix::fs::symlink(target, local).inspect_err(|_| {
        // Sem o link, a original volta da lixeira; só então a cópia sai.
        if std::fs::rename(&trashed, local).is_ok() {
            undo();
        }
    })
}

/// Copia as pastas e os arquivos comuns de `from` para a pasta `to`; links e arquivos especiais
/// ficam de fora, como nas pastas do computador.
pub(super) fn copy_contents(from: &Path, to: &Path) -> io::Result<()> {
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        let dest = to.join(entry.file_name());
        if kind.is_dir() {
            std::fs::create_dir(&dest)?;
            copy_contents(&entry.path(), &dest)?;
        } else if kind.is_file() {
            std::fs::copy(entry.path(), &dest)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn destination_stays_inside_my_drive() {
        let base = Path::new("/m/Meu Drive");
        assert_eq!(inside(base, ""), Some(base.to_path_buf()));
        assert_eq!(
            inside(base, "/Projetos/2026/"),
            Some(base.join("Projetos/2026"))
        );
        assert_eq!(inside(base, "../fora"), None);
        assert_eq!(inside(base, "a/./b"), Some(base.join("a/b")));
    }

    /// Outra chamada criou o destino entre a conferência e a cópia: esta falha sem apagar o que
    /// não criou.
    #[test]
    fn swap_leaves_a_target_it_did_not_create() {
        let tmp = tempfile::tempdir().unwrap();
        let (local, target) = (
            tmp.path().join("Projetos"),
            tmp.path().join("drive/Projetos"),
        );
        std::fs::create_dir_all(&local).unwrap();
        std::fs::write(local.join("a.txt"), "local").unwrap();
        std::fs::create_dir_all(&target).unwrap();
        std::fs::write(target.join("b.txt"), "da outra chamada").unwrap();
        let trash = Trash::new(tmp.path().join("Trash"));

        assert!(swap(&local, &target, guard::measure(&local), &trash).is_err());

        assert_eq!(
            std::fs::read_to_string(target.join("b.txt")).unwrap(),
            "da outra chamada"
        );
        assert!(local.join("a.txt").is_file());
    }
}
