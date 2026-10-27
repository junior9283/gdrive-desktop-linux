//! Pesquisa rápida (nome e conteúdo), arquivos recentes e "Compartilhados comigo".

use std::collections::HashSet;

use gdl_drive::client::{SEARCH_LIMIT, SharedFile};
use gdl_store::{Inode, Item, ItemId};

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
        self.keep_in_my_drive(&mut found)?;
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
                        && self.in_my_drive(&item)?
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
        self.keep_in_my_drive(&mut items)?;
        Ok(items)
    }

    /// Tira o que não está dentro do Meu Drive (ver `in_my_drive`).
    fn keep_in_my_drive(&self, items: &mut Vec<Item>) -> Result<()> {
        let mut inside = Vec::with_capacity(items.len());
        for item in items.drain(..) {
            if self.in_my_drive(&item)? {
                inside.push(item);
            }
        }
        *items = inside;
        Ok(())
    }

    /// O item desce da raiz do Meu Drive (não é a raiz nem conteúdo de pastas compartilhadas
    /// por outras pessoas nem de Drives compartilhados, que a montagem ainda não mostra: tarefa
    /// 9 da 0.8), ou seja, tem caminho na montagem.
    fn in_my_drive(&self, item: &Item) -> Result<bool> {
        Ok(item.inode != Inode::ROOT && self.store.my_drive_path_of(&item.id)?.is_some())
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
