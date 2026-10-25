//! Pesquisa rápida (nome e conteúdo), arquivos recentes e "Compartilhados comigo".

use std::collections::HashSet;

use gdl_drive::client::{SEARCH_LIMIT, SharedFile};
use gdl_store::{Inode, Item, ItemId};

use crate::engine::Engine;
use crate::{EngineError, Result};

/// Quantos arquivos recentes a pesquisa mostra antes de digitar.
pub const RECENT_LIMIT: usize = 10;

impl Engine {
    /// Os itens com `query` no nome (pelo banco) e, com rede, também no conteúdo (pela pesquisa
    /// do Drive, só os que o banco conhece, para ter caminho na montagem). Os do banco vêm
    /// primeiro. Sem rede ou com erro da API, só o banco, sem erro.
    pub async fn search(&self, query: &str) -> Result<Vec<Item>> {
        let mut found = self.store.search_names(query, SEARCH_LIMIT)?;
        if found.len() >= SEARCH_LIMIT {
            return Ok(found);
        }
        match self.client.search(query).await {
            Ok(ids) => {
                let mut seen: HashSet<ItemId> = found.iter().map(|i| i.id.clone()).collect();
                for id in ids.into_iter().map(ItemId) {
                    if !seen.insert(id.clone()) {
                        continue;
                    }
                    if let Some(item) = self.store.get(&id)?
                        && !item.trashed
                        && item.parent.is_some()
                    {
                        found.push(item);
                    }
                }
            }
            Err(e) => tracing::debug!(erro = %e, "pesquisa só pelo banco"),
        }
        found.truncate(SEARCH_LIMIT);
        Ok(found)
    }

    pub fn recent(&self) -> Result<Vec<Item>> {
        Ok(self.store.recent(RECENT_LIMIT)?)
    }

    pub async fn shared_with_me(&self) -> Result<Vec<SharedFile>> {
        Ok(self.client.shared_with_me().await?)
    }

    /// "Adicionar atalho ao Drive" de um item compartilhado: o atalho vai para a raiz do Meu
    /// Drive, com o nome do item.
    pub async fn add_shortcut(&self, target: &ItemId) -> Result<Item> {
        let remote = self.client.get(&target.0).await?;
        let root = self
            .store
            .by_inode(Inode::ROOT)?
            .ok_or(EngineError::NotIndexed)?;
        let name = gdl_common::names::to_local(&remote.name);
        self.create_shortcut(&root.id, &name, target).await
    }
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

    #[tokio::test]
    async fn add_shortcut_puts_shared_item_in_my_drive() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let shared = fake.add_shared_file("Orçamento.xlsx", "Ana Souza", b"x".to_vec());
        let (engine, _tmp) = indexed_engine(&fake).await;
        assert_eq!(engine.shared_with_me().await.unwrap()[0].owner, "Ana Souza");

        let shortcut = engine.add_shortcut(&ItemId(shared.clone())).await.unwrap();

        assert_eq!(shortcut.local_name, "Orçamento.xlsx");
        assert_eq!(shortcut.parent, Some(ItemId(fake.root_id())));
        assert_eq!(
            fake.find_live("Orçamento.xlsx").len(),
            2,
            "o original e o atalho"
        );
    }
}
