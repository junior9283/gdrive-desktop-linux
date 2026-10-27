//! Compartilhar e "Gerenciar versões": o acesso de outras pessoas a um item e as versões
//! guardadas do conteúdo. Tudo vai direto ao Drive (sem internet, não há o que mostrar).

use std::io::Write;

use bytes::Bytes;
use gdl_common::time::now_ms;
use gdl_drive::model::{Grantee, Permission, Revision};
use gdl_store::{Item, ItemId, LocalState};

use crate::engine::Engine;
use crate::ops::{Upload, never_uploaded};
use crate::{EngineError, Result};

impl Engine {
    /// O item já no Drive (os que ainda não subiram não têm acesso nem versões).
    fn uploaded(&self, id: &ItemId) -> Result<Item> {
        let item = self.item(id)?;
        if never_uploaded(&item) {
            return Err(EngineError::NotUploaded);
        }
        Ok(item)
    }

    pub async fn permissions(&self, id: &ItemId) -> Result<Vec<Permission>> {
        self.uploaded(id)?;
        Ok(self.client.permissions(&id.0).await?)
    }

    /// Dá acesso ao item; `notify` manda o aviso por e-mail (com a mensagem, se houver).
    pub async fn share(
        &self,
        id: &ItemId,
        grantee: &Grantee,
        role: &str,
        notify: Option<&str>,
    ) -> Result<Permission> {
        self.uploaded(id)?;
        Ok(self
            .client
            .add_permission(&id.0, grantee, role, notify)
            .await?)
    }

    pub async fn set_share_role(&self, id: &ItemId, permission: &str, role: &str) -> Result<()> {
        self.uploaded(id)?;
        self.client
            .set_permission_role(&id.0, permission, role)
            .await?;
        Ok(())
    }

    pub async fn unshare(&self, id: &ItemId, permission: &str) -> Result<()> {
        self.uploaded(id)?;
        Ok(self.client.remove_permission(&id.0, permission).await?)
    }

    /// Troca o acesso geral (de domínio ou de qualquer pessoa com o link): `None` deixa o item
    /// restrito. Com o mesmo tipo, só muda o papel; com outro, tira o anterior e dá o novo.
    pub async fn set_general_access(
        &self,
        id: &ItemId,
        grantee: Option<&Grantee>,
        role: &str,
    ) -> Result<()> {
        let kind = grantee.map(Grantee::kind);
        let perms = self.permissions(id).await?;
        // Tira todos os de outro tipo antes de mexer no que fica (nenhum sobra por engano).
        let mut same = None;
        for old in perms.iter().filter(|p| p.is_general()) {
            match Some(old.kind.as_str()) == kind {
                true => same = Some(&old.id),
                false => self.unshare(id, &old.id).await?,
            }
        }
        match (same, grantee) {
            (Some(same), _) => self.set_share_role(id, same, role).await,
            (None, Some(grantee)) => self.share(id, grantee, role, None).await.map(drop),
            (None, None) => Ok(()),
        }
    }

    /// As versões do conteúdo, da mais nova para a mais antiga (a primeira é a atual).
    pub async fn revisions(&self, id: &ItemId) -> Result<Vec<Revision>> {
        self.uploaded(id)?;
        let mut list = self.client.revisions(&id.0).await?;
        list.reverse();
        Ok(list)
    }

    pub async fn download_revision(&self, id: &ItemId, revision: &str) -> Result<Bytes> {
        self.uploaded(id)?;
        Ok(self.client.download_revision(&id.0, revision).await?)
    }

    pub async fn keep_revision(&self, id: &ItemId, revision: &str, keep: bool) -> Result<()> {
        self.uploaded(id)?;
        self.client.set_keep_forever(&id.0, revision, keep).await?;
        Ok(())
    }

    /// Volta o arquivo para uma versão anterior: o conteúdo dela vira uma edição local, que sobe
    /// pela fila de envio como versão nova (a atual continua no histórico). Recusa se houver
    /// edição local ainda não enviada, para não perdê-la.
    pub async fn restore_revision(&self, id: &ItemId, revision: &str) -> Result<()> {
        let item = self.uploaded(id)?;
        if !item.can_edit {
            return Err(gdl_drive::client::DriveError::PermissionDenied.into());
        }
        let content = self.client.download_revision(&id.0, revision).await?;
        // Com a trava do item: nenhum download grava por cima do conteúdo restaurado.
        self.cache
            .locked(id, || -> Result<()> {
                let clean =
                    self.store
                        .transition_local_state(id, LocalState::Clean, LocalState::Dirty)?;
                if !clean {
                    return Err(EngineError::LocalEdits);
                }
                self.cache.open_local(id, true)?.write_all(&content)?;
                self.commit_write(id, content.len() as u64, now_ms(), Upload::Now)
                    .map(drop)
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use gdl_fake_drive::FakeDrive;

    use super::*;
    use crate::testutil::indexed_engine;

    #[tokio::test]
    async fn sharing_goes_to_the_drive() {
        let fake = FakeDrive::start().await;
        let id = ItemId(fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec()));
        let (engine, _tmp) = indexed_engine(&fake).await;

        let who = Grantee::Person("ana@example.com".into());
        let added = engine.share(&id, &who, "reader", None).await.unwrap();
        engine
            .set_share_role(&id, &added.id, "writer")
            .await
            .unwrap();

        let roles: Vec<_> = engine
            .permissions(&id)
            .await
            .unwrap()
            .into_iter()
            .map(|p| p.role)
            .collect();
        assert_eq!(roles, ["owner", "writer"]);
        engine.unshare(&id, &added.id).await.unwrap();
        assert_eq!(fake.permissions(&id.0).len(), 1);
    }

    #[tokio::test]
    async fn general_access_is_replaced_or_removed() {
        let fake = FakeDrive::start().await;
        let id = ItemId(fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec()));
        let (engine, _tmp) = indexed_engine(&fake).await;
        let general = || async {
            let perms = engine.permissions(&id).await.unwrap();
            perms
                .into_iter()
                .filter(|p| p.is_general())
                .map(|p| (p.kind, p.role))
                .collect::<Vec<_>>()
        };

        let domain = Grantee::Domain("example.com".into());
        engine
            .set_general_access(&id, Some(&domain), "reader")
            .await
            .unwrap();
        engine
            .set_general_access(&id, Some(&domain), "writer")
            .await
            .unwrap();
        assert_eq!(general().await, [("domain".into(), "writer".into())]);

        engine
            .set_general_access(&id, Some(&Grantee::Anyone), "reader")
            .await
            .unwrap();
        assert_eq!(general().await, [("anyone".into(), "reader".into())]);

        engine.set_general_access(&id, None, "").await.unwrap();
        assert!(general().await.is_empty());
    }

    #[tokio::test]
    async fn changing_general_access_removes_every_other_kind() {
        let fake = FakeDrive::start().await;
        let id = ItemId(fake.add_file(&fake.root_id(), "a.txt", b"a".to_vec()));
        let (engine, _tmp) = indexed_engine(&fake).await;
        // O Drive aceita os dois acessos gerais ao mesmo tempo.
        engine
            .share(&id, &Grantee::Anyone, "reader", None)
            .await
            .unwrap();
        let domain = Grantee::Domain("example.com".into());
        engine.share(&id, &domain, "reader", None).await.unwrap();

        engine
            .set_general_access(&id, Some(&Grantee::Anyone), "writer")
            .await
            .unwrap();

        let general: Vec<_> = fake
            .permissions(&id.0)
            .into_iter()
            .filter(|p| matches!(p["type"].as_str(), Some("anyone" | "domain")))
            .map(|p| (p["type"].clone(), p["role"].clone()))
            .collect();
        assert_eq!(general, [("anyone".into(), "writer".into())]);
    }

    #[tokio::test]
    async fn revisions_come_newest_first() {
        let fake = FakeDrive::start().await;
        let id = ItemId(fake.add_file(&fake.root_id(), "a.txt", b"um".to_vec()));
        fake.modify_remote(&id.0, b"dois".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;

        let list = engine.revisions(&id).await.unwrap();
        let sizes: Vec<_> = list.iter().map(|r| r.size).collect();
        assert_eq!(sizes, [Some(4), Some(2)]);
        let old = engine.download_revision(&id, &list[1].id).await.unwrap();
        assert_eq!(old.as_ref(), b"um");
        engine.keep_revision(&id, &list[1].id, true).await.unwrap();
        assert!(engine.revisions(&id).await.unwrap()[1].keep_forever);
    }

    #[tokio::test]
    async fn restoring_a_version_queues_its_content_as_an_edit() {
        let fake = FakeDrive::start().await;
        let id = ItemId(fake.add_file(&fake.root_id(), "a.txt", b"um".to_vec()));
        fake.modify_remote(&id.0, b"dois".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        let old = engine.revisions(&id).await.unwrap()[1].id.clone();

        engine.restore_revision(&id, &old).await.unwrap();

        let item = engine.item(&id).unwrap();
        assert_eq!(item.local_state, LocalState::Dirty);
        assert_eq!(item.size, 2);
        assert_eq!(std::fs::read(engine.cache.local_path(&id)).unwrap(), b"um");
        assert_eq!(engine.summary().unwrap().pending_uploads, 1);
    }

    #[tokio::test]
    async fn restoring_never_overwrites_a_local_edit() {
        let fake = FakeDrive::start().await;
        let id = ItemId(fake.add_file(&fake.root_id(), "a.txt", b"um".to_vec()));
        fake.modify_remote(&id.0, b"dois".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        let old = engine.revisions(&id).await.unwrap()[1].id.clone();
        let path = engine.cache.local_path(&id);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"minha edicao").unwrap();
        crate::uploads::commit_local_write(&engine.store, &id, 12, now_ms()).unwrap();

        let res = engine.restore_revision(&id, &old).await;

        assert!(matches!(res, Err(EngineError::LocalEdits)), "{res:?}");
        assert_eq!(std::fs::read(&path).unwrap(), b"minha edicao");
    }

    #[tokio::test]
    async fn local_only_files_have_no_versions() {
        let fake = FakeDrive::start().await;
        let (engine, _tmp) = indexed_engine(&fake).await;
        let root = ItemId(fake.root_id());
        let item = engine.create_file(&root, "novo.txt").unwrap();
        let res = engine.revisions(&item.id).await;
        assert!(matches!(res, Err(EngineError::NotUploaded)), "{res:?}");
    }
}
