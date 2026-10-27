//! Drives compartilhados: lista, índice, liga/desliga e criação.

use gdl_drive::client::{Corpus, DriveClient};
use gdl_drive::model::SharedDrive;
use gdl_store::{Inode, Item, ItemId, ItemKind, LocalState, PinState, Store, StoredDrive};

use crate::engine::Engine;
use crate::indexer::index_corpus;
use crate::{EngineError, Result};

/// Um Drive compartilhado da conta, para a tela de gerenciar e para o "Adicionar atalho".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SharedDriveInfo {
    pub id: String,
    pub name: String,
    /// Ligado: indexado e visível na montagem.
    pub enabled: bool,
    /// Falso para quem só vê ou comenta: não dá para criar nada dentro dele.
    pub can_add_children: bool,
}

/// A pasta que é a raiz do Drive: sem pai, com o ID do próprio Drive.
fn root_item(drive: &SharedDrive) -> Item {
    Item {
        id: ItemId(drive.id.clone()),
        inode: Inode::UNASSIGNED,
        parent: None,
        remote_name: drive.name.clone(),
        local_name: gdl_common::names::to_local(&drive.name),
        kind: ItemKind::Folder,
        size: 0,
        remote_size: 0,
        md5: None,
        head_revision: None,
        base_revision: None,
        base_md5: None,
        modified_ms: 0,
        created_ms: 0,
        trashed: false,
        can_edit: drive.can_add_children,
        local_state: LocalState::Clean,
        pin: PinState::None,
    }
}

/// Liga o Drive compartilhado e grava a raiz e todos os itens dele. Ligado antes de listar: uma
/// mudança que chegue durante a listagem já é aplicada, em vez de descartada. Com a falha no
/// meio, desfaz tudo (o Drive volta a como estava, sem o que entrou) e a próxima vez recomeça.
async fn index_drive(
    store: &Store,
    client: &DriveClient,
    drive: &SharedDrive,
    email: &str,
) -> Result<()> {
    let id = ItemId(drive.id.clone());
    let was_known = store.shared_drives()?.iter().any(|k| k.id == id);
    store.put_shared_drive(&StoredDrive {
        id: id.clone(),
        name: drive.name.clone(),
        enabled: true,
    })?;
    store.set_drive_enabled(&id, true)?;
    let indexed = async {
        store.upsert_item(&root_item(drive))?;
        index_corpus(
            store,
            client,
            &Corpus::Drive(drive.id.clone()),
            email,
            id.clone(),
        )
        .await
    };
    if let Err(e) = indexed.await {
        store.forget_drive_items(&id)?;
        if was_known {
            store.set_drive_enabled(&id, false)?;
        } else {
            store.delete_shared_drive(&id)?;
        }
        return Err(e);
    }
    Ok(())
}

/// Consulta a lista de Drives da conta: os que o banco ainda não conhecia entram ligados e
/// indexados, e os outros só têm o nome atualizado. Devolve a lista, por nome.
pub(crate) async fn sync_drive_list(
    store: &Store,
    client: &DriveClient,
    email: &str,
) -> Result<Vec<SharedDrive>> {
    let mut drives = client.drives().await?;
    drives.sort_by(|a, b| a.name.cmp(&b.name).then_with(|| a.id.cmp(&b.id)));
    let known = store.shared_drives()?;
    for drive in &drives {
        match known.iter().find(|k| k.id.0 == drive.id) {
            Some(known) if known.name == drive.name => {}
            Some(known) => store.put_shared_drive(&StoredDrive {
                name: drive.name.clone(),
                ..known.clone()
            })?,
            None => index_drive(store, client, drive, email).await?,
        }
    }
    Ok(drives)
}

/// `name`, ou `name (2)`, `name (3)`… o primeiro que nenhum de `taken` usa.
fn unique_name(name: &str, taken: &[String]) -> String {
    let free = |candidate: &str| !taken.iter().any(|t| t == candidate);
    if free(name) {
        return name.into();
    }
    (2..)
        .map(|n| format!("{name} ({n})"))
        .find(|candidate| free(candidate))
        .expect("a sequência é infinita")
}

impl Engine {
    /// Os Drives compartilhados da conta, direto do Drive (um que ainda não era conhecido entra
    /// ligado e indexado), com o estado de ligado de cada um.
    pub async fn shared_drives(&self) -> Result<Vec<SharedDriveInfo>> {
        let email = self.account_email()?;
        let drives = sync_drive_list(&self.store, &self.client, &email).await?;
        self.poller.forget_searched_drives();
        self.infos(drives)
    }

    fn infos(&self, drives: Vec<SharedDrive>) -> Result<Vec<SharedDriveInfo>> {
        let known = self.store.shared_drives()?;
        Ok(drives
            .into_iter()
            .map(|d| SharedDriveInfo {
                enabled: known.iter().any(|k| k.enabled && k.id.0 == d.id),
                id: d.id,
                name: d.name,
                can_add_children: d.can_add_children,
            })
            .collect())
    }

    /// Liga exatamente os Drives de `enabled`: os que passam a ligados são indexados e os que
    /// passam a desligados são esquecidos (itens, cache, fixados). Com envio pendente em algum
    /// dos que seriam desligados, recusa e nada muda. IDs desconhecidos são ignorados.
    pub async fn set_drives_enabled(&self, enabled: &[String]) -> Result<()> {
        let known = self.store.shared_drives()?;
        let wanted = |d: &StoredDrive| enabled.contains(&d.id.0);
        let to_disable: Vec<&StoredDrive> =
            known.iter().filter(|d| d.enabled && !wanted(d)).collect();
        for drive in &to_disable {
            if self.store.drive_has_pending(&drive.id)? {
                return Err(EngineError::PendingUploads);
            }
        }
        let to_enable: Vec<&StoredDrive> =
            known.iter().filter(|d| !d.enabled && wanted(d)).collect();
        if !to_enable.is_empty() {
            let email = self.account_email()?;
            let remote = self.client.drives().await?;
            for drive in to_enable {
                if let Some(drive) = remote.iter().find(|r| r.id == drive.id.0) {
                    index_drive(&self.store, &self.client, drive, &email).await?;
                }
            }
        }
        for drive in to_disable {
            self.store.set_drive_enabled(&drive.id, false)?;
            for id in self.store.forget_drive_items(&drive.id)? {
                self.cache.discard(&id).await?;
            }
        }
        Ok(())
    }

    /// Cria um Drive compartilhado, que já entra ligado e indexado. Se o nome já é de outro
    /// Drive da conta, ganha o sufixo " (2)", " (3)"…
    pub async fn create_shared_drive(&self, name: &str) -> Result<SharedDriveInfo> {
        let email = self.account_email()?;
        let taken: Vec<String> = sync_drive_list(&self.store, &self.client, &email)
            .await?
            .into_iter()
            .map(|d| d.name)
            .collect();
        // Um pedido novo a cada chamada: o mesmo `requestId` devolveria o Drive de antes.
        let request_id = format!("{:032x}", rand::random::<u128>());
        let drive = self
            .client
            .create_drive(&unique_name(name, &taken), &request_id)
            .await?;
        index_drive(&self.store, &self.client, &drive, &email).await?;
        Ok(self.infos(vec![drive])?.remove(0))
    }
}

#[cfg(test)]
mod tests {
    use super::index_drive;
    use gdl_drive::model::SharedDrive;
    use gdl_fake_drive::{FakeAccount, FakeDrive, Fault};
    use gdl_store::{DrivePath, DriveRoot, Inode, ItemId, LocalState};

    use crate::EngineError;
    use crate::engine::Engine;
    use crate::testutil::indexed_engine;
    use crate::uploads::{commit_local_write, create_local_item};

    /// Ana cria o Drive compartilhado "Equipe" (com `Planos/plano.txt`) e dá acesso à conta do
    /// Drive simulado, que também tem um `meu.txt` no Meu Drive.
    struct Team {
        ana: FakeAccount,
        drive: String,
        folder: String,
        file: String,
    }

    /// Drive simulado com a equipe e o motor já indexado.
    async fn setup() -> (FakeDrive, Team, Engine, tempfile::TempDir) {
        let fake = FakeDrive::start().await;
        let team = team(&fake);
        let (engine, tmp) = indexed_engine(&fake).await;
        (fake, team, engine, tmp)
    }

    fn team(fake: &FakeDrive) -> Team {
        let ana = fake.add_account("ana@example.com");
        let drive = ana.add_shared_drive("Equipe");
        fake.share_drive(&drive, &fake.email());
        let folder = ana.add_folder(&drive, "Planos");
        let file = ana.add_file(&folder, "plano.txt", b"plano".to_vec());
        fake.add_file(&fake.root_id(), "meu.txt", b"meu".to_vec());
        Team {
            ana,
            drive,
            folder,
            file,
        }
    }

    fn root_of(engine: &crate::engine::Engine) -> ItemId {
        engine.store().by_inode(Inode::ROOT).unwrap().unwrap().id
    }

    fn id(s: &str) -> ItemId {
        ItemId(s.into())
    }

    #[tokio::test]
    async fn shared_drives_are_indexed_beside_my_drive() {
        let (_fake, team, engine, _tmp) = setup().await;
        let store = engine.store();

        let drives = engine.shared_drives().await.unwrap();
        assert_eq!(drives.len(), 1);
        assert_eq!(
            (drives[0].id.as_str(), drives[0].name.as_str()),
            (team.drive.as_str(), "Equipe")
        );
        assert!(drives[0].enabled && drives[0].can_add_children);

        let root = store.get(&id(&team.drive)).unwrap().unwrap();
        assert_eq!(root.parent, None);
        assert_ne!(root.inode, Inode::ROOT);
        assert_eq!(
            store.path_of(&id(&team.file)).unwrap(),
            Some(DrivePath {
                root: DriveRoot::Shared {
                    id: id(&team.drive),
                    name: "Equipe".into()
                },
                path: "Planos/plano.txt".into()
            })
        );
        let mine = store.lookup(&root_of(&engine), "meu.txt").unwrap().unwrap();
        assert_eq!(
            store.path_of(&mine.id).unwrap().unwrap().root,
            DriveRoot::MyDrive
        );
    }

    #[tokio::test]
    async fn disabling_a_drive_with_pending_uploads_is_refused() {
        let (_fake, team, engine, _tmp) = setup().await;
        let store = engine.store();
        let local = create_local_item(
            store,
            &id(&team.folder),
            "novo.txt",
            gdl_store::ItemKind::File,
        )
        .unwrap();
        commit_local_write(store, &local.id, 1, 1).unwrap();
        assert_eq!(
            store.get(&local.id).unwrap().unwrap().local_state,
            LocalState::Dirty
        );

        let err = engine.set_drives_enabled(&[]).await.unwrap_err();

        assert!(matches!(err, EngineError::PendingUploads), "{err}");
        assert!(engine.shared_drives().await.unwrap()[0].enabled);
        assert!(store.get(&id(&team.file)).unwrap().is_some(), "nada mudou");
    }

    #[tokio::test]
    async fn disabling_a_drive_forgets_it_and_its_offline_items() {
        let (_fake, team, engine, _tmp) = setup().await;
        let store = engine.store();
        engine.pins().pin(&id(&team.folder)).unwrap();
        assert_eq!(store.count_top_pinned().unwrap(), 1);

        engine.set_drives_enabled(&[]).await.unwrap();

        assert!(!engine.shared_drives().await.unwrap()[0].enabled);
        for gone in [&team.drive, &team.folder, &team.file] {
            assert!(store.get(&id(gone)).unwrap().is_none(), "{gone}");
        }
        assert_eq!(
            store.count_top_pinned().unwrap(),
            0,
            "deixou de ser off-line"
        );
        assert!(
            store
                .lookup(&root_of(&engine), "meu.txt")
                .unwrap()
                .is_some()
        );

        engine
            .set_drives_enabled(std::slice::from_ref(&team.drive))
            .await
            .unwrap();
        assert!(
            store.get(&id(&team.file)).unwrap().is_some(),
            "ligado de novo, indexado"
        );
    }

    #[tokio::test]
    async fn new_drive_names_are_unique() {
        let fake = FakeDrive::start().await;
        let (engine, _tmp) = indexed_engine(&fake).await;

        let first = engine.create_shared_drive("Projetos").await.unwrap();
        let second = engine.create_shared_drive("Projetos").await.unwrap();
        let third = engine.create_shared_drive("Projetos").await.unwrap();

        let names: Vec<_> = [&first, &second, &third].map(|d| d.name.as_str()).into();
        assert_eq!(names, ["Projetos", "Projetos (2)", "Projetos (3)"]);
        for drive in [&first, &second, &third] {
            assert!(drive.enabled);
            assert!(engine.store().get(&id(&drive.id)).unwrap().is_some());
        }
        assert_eq!(engine.shared_drives().await.unwrap().len(), 3);
    }

    #[tokio::test]
    async fn changes_in_a_shared_drive_reach_the_index() {
        let (fake, team, engine, _tmp) = setup().await;

        fake.modify_remote(&team.file, b"plano novo".to_vec());
        let added = team.ana.add_file(&team.folder, "novo.txt", b"1".to_vec());
        engine.poller.run_once().await.unwrap();

        let store = engine.store();
        assert_eq!(store.get(&id(&team.file)).unwrap().unwrap().size, 10);
        assert_eq!(
            store.path_of(&id(&added)).unwrap().unwrap().path,
            "Planos/novo.txt"
        );
    }

    #[tokio::test]
    async fn new_drives_in_the_changes_join_enabled() {
        let fake = FakeDrive::start().await;
        let (engine, _tmp) = indexed_engine(&fake).await;
        let ana = fake.add_account("ana@example.com");
        let drive = ana.add_shared_drive("Novo");
        fake.share_drive(&drive, &fake.email());
        let old = ana.add_file(&drive, "antigo.txt", b"1".to_vec());
        engine.poller.run_once().await.unwrap();

        let drives = engine.shared_drives().await.unwrap();
        assert_eq!((drives[0].name.as_str(), drives[0].enabled), ("Novo", true));
        assert!(engine.store().get(&id(&old)).unwrap().is_some());
    }

    #[tokio::test]
    async fn disabled_drive_has_no_path_and_its_changes_are_ignored() {
        let (fake, team, engine, _tmp) = setup().await;
        engine.set_drives_enabled(&[]).await.unwrap();

        let added = team.ana.add_file(&team.folder, "novo.txt", b"1".to_vec());
        fake.modify_remote(&team.file, b"outro".to_vec());
        engine.poller.run_once().await.unwrap();

        let store = engine.store();
        assert_eq!(store.path_of(&id(&team.file)).unwrap(), None);
        assert!(store.get(&id(&added)).unwrap().is_none());
        assert!(store.get(&id(&team.file)).unwrap().is_none());
        assert!(
            !engine.shared_drives().await.unwrap()[0].enabled,
            "segue desligado"
        );
    }

    #[tokio::test]
    async fn failed_indexing_leaves_the_drive_as_it_was() {
        let (fake, team, engine, _tmp) = setup().await;
        engine.set_drives_enabled(&[]).await.unwrap();
        let store = engine.store();
        let remote = SharedDrive {
            id: team.drive.clone(),
            name: "Equipe".into(),
            can_add_children: true,
        };

        fake.inject(Fault::Status {
            code: 500,
            times: 10,
        });
        let err = index_drive(store, &engine.client, &remote, "a@b.c").await;

        assert!(err.is_err());
        let known = store.shared_drives().unwrap();
        assert_eq!(known.len(), 1);
        assert!(!known[0].enabled, "segue desligado");
        assert!(store.get(&id(&team.drive)).unwrap().is_none(), "sem a raiz");
    }

    #[tokio::test]
    async fn failed_indexing_of_a_new_drive_forgets_it_so_it_is_retried() {
        let (fake, team, engine, _tmp) = setup().await;
        let store = engine.store();
        store.delete_shared_drive(&id(&team.drive)).unwrap();
        store.forget_drive_items(&id(&team.drive)).unwrap();

        fake.inject(Fault::Status {
            code: 500,
            times: 10,
        });
        assert!(engine.shared_drives().await.is_err());
        fake.clear_faults();

        assert!(store.shared_drives().unwrap().is_empty());
        assert!(engine.shared_drives().await.unwrap()[0].enabled);
        assert!(store.get(&id(&team.file)).unwrap().is_some());
    }
}
