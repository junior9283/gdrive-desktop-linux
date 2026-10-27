//! Pesquisa rápida (nome e conteúdo), arquivos recentes e "Compartilhados comigo".

use std::collections::HashSet;

use gdl_drive::client::{SEARCH_LIMIT, SharedFile};
use gdl_store::{Inode, Item, ItemId, ItemKind};

use crate::engine::Engine;
use crate::{EngineError, Result};

/// O máximo que a pesquisa rápida espera a API (que acha também pelo conteúdo); depois, só os
/// nomes do banco. Sem rede, o cliente tentaria de novo por vários segundos.
const SEARCH_API_WAIT: std::time::Duration = std::time::Duration::from_millis(2500);

/// Quantos arquivos recentes a pesquisa mostra antes de digitar.
pub const RECENT_LIMIT: usize = 10;

impl Engine {
    /// Os itens com `query` no nome (pelo banco) e, com rede, também no conteúdo (pela pesquisa
    /// do Drive, só os que o banco conhece, para ter caminho na montagem). Os do banco vêm
    /// primeiro. Sem rede ou com erro da API, só o banco, sem erro.
    pub async fn search(&self, query: &str) -> Result<Vec<Item>> {
        let mut found = self.store.search_names(query, SEARCH_LIMIT)?;
        self.keep_mounted(&mut found)?;
        if found.len() >= SEARCH_LIMIT {
            return Ok(found);
        }
        match tokio::time::timeout(SEARCH_API_WAIT, self.client.search(query)).await {
            Ok(Ok(ids)) => {
                let mut seen: HashSet<ItemId> = found.iter().map(|i| i.id.clone()).collect();
                for id in ids.into_iter().map(ItemId) {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    if let Some(item) = self.store.get(&id)?
                        && !item.trashed
                        && self.is_mounted(&item)?
                    {
                        found.push(item);
                    }
                }
            }
            Ok(Err(e)) => tracing::debug!(erro = %e, "pesquisa só pelo banco"),
            Err(_) => tracing::debug!("a API demorou; pesquisa só pelo banco"),
        }
        found.truncate(SEARCH_LIMIT);
        Ok(found)
    }

    pub fn recent(&self) -> Result<Vec<Item>> {
        let mut items = self.store.recent(RECENT_LIMIT)?;
        self.keep_mounted(&mut items)?;
        Ok(items)
    }

    /// Tira o que não aparece na montagem (ver `is_mounted`).
    fn keep_mounted(&self, items: &mut Vec<Item>) -> Result<()> {
        let mut inside = Vec::with_capacity(items.len());
        for item in items.drain(..) {
            if self.is_mounted(&item)? {
                inside.push(item);
            }
        }
        *items = inside;
        Ok(())
    }

    /// O item desce da raiz do Meu Drive ou de um Drive compartilhado ligado (não é uma raiz
    /// nem conteúdo de pastas compartilhadas por outras pessoas), ou seja, tem caminho na
    /// montagem.
    fn is_mounted(&self, item: &Item) -> Result<bool> {
        Ok(item.parent.is_some() && self.store.path_of(&item.id)?.is_some())
    }

    pub async fn shared_with_me(&self) -> Result<Vec<SharedFile>> {
        Ok(self.client.shared_with_me().await?)
    }

    /// "Adicionar atalho ao Drive": o atalho para `target`, com o nome dele, vai para `folder` (uma
    /// pasta da montagem onde a conta pode criar) ou, sem ela, para a raiz do Meu Drive.
    pub async fn add_shortcut(&self, target: &ItemId, folder: Option<&ItemId>) -> Result<Item> {
        let folder = match folder {
            Some(id) => self
                .store
                .get(id)?
                .filter(|f| self.takes_shortcuts(f).unwrap_or(false))
                .ok_or(EngineError::NotFound)?,
            None => self.my_drive_root()?,
        };
        let remote = self.client.get(&target.0).await?;
        let name = gdl_common::names::to_local(&remote.name);
        self.create_shortcut(&folder.id, &name, target).await
    }

    /// As pastas de "Adicionar atalho": sem `parent`, o Meu Drive (sem nome: quem mostra dá o
    /// dele) e os Drives compartilhados ligados que aceitam itens novos; com ele, as subpastas.
    pub fn shortcut_folders(&self, parent: Option<&ItemId>) -> Result<Vec<FolderChoice>> {
        let folders = match parent {
            None => {
                let my_drive = self.my_drive_root()?;
                let drives = self.store.enabled_drive_roots()?.into_iter();
                std::iter::once((None, my_drive))
                    .chain(
                        drives
                            .filter(|(_, root)| root.can_edit)
                            .map(|(name, root)| (Some(name), root)),
                    )
                    .collect()
            }
            Some(parent) => self
                .store
                .children(parent)?
                .into_iter()
                .filter(|f| f.kind == ItemKind::Folder)
                .map(|f| (Some(f.local_name.clone()), f))
                .collect::<Vec<_>>(),
        };
        folders
            .into_iter()
            .map(|(name, folder)| {
                Ok(FolderChoice {
                    has_children: self.store.has_subfolders(&folder.id)?,
                    id: folder.id,
                    name,
                })
            })
            .collect()
    }

    fn my_drive_root(&self) -> Result<Item> {
        self.store
            .by_inode(Inode::ROOT)?
            .ok_or(EngineError::NotIndexed)
    }

    /// Pasta da montagem, fora da lixeira, onde a conta pode criar.
    fn takes_shortcuts(&self, folder: &Item) -> Result<bool> {
        Ok(folder.kind == ItemKind::Folder
            && !folder.trashed
            && folder.can_edit
            && self.store.path_of(&folder.id)?.is_some())
    }
}

/// Uma pasta de destino de "Adicionar atalho"; `name` `None` é o Meu Drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderChoice {
    pub id: ItemId,
    pub name: Option<String>,
    pub has_children: bool,
}

#[cfg(test)]
mod tests {
    use gdl_fake_drive::Fault;

    use super::*;
    use crate::testutil::indexed_engine;

    #[tokio::test]
    async fn search_finds_content_matches_online_and_names_offline() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let root = fake.root_id();
        let pasta = fake.add_folder(&root, "Contábil");
        fake.add_file(&pasta, "relatorio.txt", b"faturamento de outubro".to_vec());
        fake.add_file(&root, "Relatório final.txt", b"resumo".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        let names =
            |items: Vec<Item>| -> Vec<String> { items.into_iter().map(|i| i.local_name).collect() };

        assert_eq!(
            names(engine.search("faturamento").await.unwrap()),
            ["relatorio.txt"]
        );
        assert_eq!(
            names(engine.search("relat").await.unwrap()),
            ["Relatório final.txt", "relatorio.txt"],
            "nome pelo banco, sem repetir o que a API também achou"
        );

        fake.inject(Fault::Offline);
        assert!(engine.search("faturamento").await.unwrap().is_empty());
        assert_eq!(
            names(engine.search("RELATORIO").await.unwrap()),
            ["Relatório final.txt", "relatorio.txt"]
        );
    }

    /// A rede que não responde (sem cair) não segura a pesquisa rápida: valem os nomes do banco.
    #[tokio::test]
    async fn search_does_not_wait_long_for_a_stuck_network() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        fake.add_file(&fake.root_id(), "contrato.pdf", b"x".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        fake.inject(Fault::StallConnectionAfterBytes(0));

        let started = std::time::Instant::now();
        let found = engine.search("contrato").await.unwrap();

        assert_eq!(found.len(), 1);
        // Sem o prazo, o cliente esperava a rede por um minuto.
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "{:?}",
            started.elapsed()
        );
    }

    #[tokio::test]
    async fn search_and_recent_skip_items_outside_my_drive() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let projeto = fake.add_shared_folder("Projeto", "Ana Souza");
        let shared = fake.add_file(&projeto, "orcamento.xlsx", b"fora".to_vec());
        fake.add_file(&fake.root_id(), "orcamento meu.xlsx", b"dentro".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        engine
            .store()
            .touch_access(&ItemId(shared), gdl_common::time::now_ms())
            .unwrap();

        let found: Vec<String> = engine
            .search("orcamento")
            .await
            .unwrap()
            .into_iter()
            .map(|i| i.local_name)
            .collect();

        assert_eq!(
            found,
            ["orcamento meu.xlsx"],
            "nem pelo nome nem pelo conteúdo"
        );
        assert!(engine.recent().unwrap().is_empty());
    }

    #[tokio::test]
    async fn search_and_recent_include_enabled_shared_drives_but_not_their_roots() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let ana = fake.add_account("ana@example.com");
        let drive = ana.add_shared_drive("Orcamento da equipe");
        fake.share_drive(&drive, &fake.email());
        let file = ana.add_file(&drive, "orcamento.xlsx", b"x".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        engine
            .store()
            .touch_access(&ItemId(file), gdl_common::time::now_ms())
            .unwrap();
        let names = |items: Vec<gdl_store::Item>| -> Vec<String> {
            items.into_iter().map(|i| i.local_name).collect()
        };

        assert_eq!(
            names(engine.search("orcamento").await.unwrap()),
            ["orcamento.xlsx"]
        );
        assert_eq!(names(engine.recent().unwrap()), ["orcamento.xlsx"]);
        engine.set_drives_enabled(&[]).await.unwrap();
        assert!(engine.search("orcamento").await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn add_shortcut_puts_shared_item_in_my_drive() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let shared = fake.add_shared_file("Orçamento.xlsx", "Ana Souza", b"x".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        assert_eq!(engine.shared_with_me().await.unwrap()[0].owner, "Ana Souza");

        let shortcut = engine
            .add_shortcut(&ItemId(shared.clone()), None)
            .await
            .unwrap();

        assert_eq!(shortcut.local_name, "Orçamento.xlsx");
        assert_eq!(shortcut.parent, Some(ItemId(fake.root_id())));
        assert_eq!(
            fake.find_live("Orçamento.xlsx").len(),
            2,
            "o original e o atalho"
        );
    }

    #[tokio::test]
    async fn shortcut_goes_to_the_chosen_folder() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let shared = fake.add_shared_file("Orçamento.xlsx", "Ana Souza", b"x".to_vec());
        let plans = fake.add_folder(&fake.root_id(), "Planos");
        let notes = fake.add_file(&fake.root_id(), "notas.txt", b"n".to_vec());
        let locked = fake.add_folder(&fake.root_id(), "Fechada");
        fake.make_read_only(&locked);
        let (engine, _tmp) = indexed_engine(&fake).await;

        let shortcut = engine
            .add_shortcut(&ItemId(shared), Some(&ItemId(plans.clone())))
            .await
            .unwrap();

        assert_eq!(shortcut.parent, Some(ItemId(plans)));
        // Só pasta é destino.
        let refused = engine
            .add_shortcut(&ItemId(notes.clone()), Some(&ItemId(notes.clone())))
            .await
            .unwrap_err();
        assert!(matches!(refused, EngineError::NotFound), "{refused}");
        // Nem pasta onde a conta não pode criar.
        let refused = engine
            .add_shortcut(&ItemId(notes), Some(&ItemId(locked)))
            .await
            .unwrap_err();
        assert!(matches!(refused, EngineError::NotFound), "{refused}");
    }

    #[tokio::test]
    async fn shortcut_folders_are_my_drive_and_the_drives_that_take_new_items() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let plans = fake.add_folder(&fake.root_id(), "Planos");
        fake.add_folder(&plans, "2026");
        let empty = fake.add_folder(&fake.root_id(), "Vazia");
        fake.add_file(&empty, "só arquivo.txt", b"a".to_vec());
        fake.add_file(&fake.root_id(), "notas.txt", b"n".to_vec());
        let ana = fake.add_account("ana@example.com");
        let team = ana.add_shared_drive("Equipe");
        let read_only = ana.add_shared_drive("Arquivo");
        for drive in [&team, &read_only] {
            fake.share_drive(drive, &fake.email());
        }
        fake.set_drive_can_add_children(&read_only, false);
        let (engine, _tmp) = indexed_engine(&fake).await;

        let roots = engine.shortcut_folders(None).unwrap();
        let named: Vec<_> = roots.iter().map(|f| f.name.as_deref()).collect();
        assert_eq!(named, [None, Some("Equipe")], "Meu Drive primeiro");
        assert_eq!(roots[0].id, ItemId(fake.root_id()));

        let inside = engine.shortcut_folders(Some(&roots[0].id)).unwrap();
        assert_eq!(
            inside,
            [
                FolderChoice {
                    id: ItemId(plans),
                    name: Some("Planos".into()),
                    has_children: true,
                },
                FolderChoice {
                    id: ItemId(empty),
                    name: Some("Vazia".into()),
                    has_children: false,
                },
            ],
            "só pastas; a seta só onde há subpasta"
        );
    }
}
