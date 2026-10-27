//! Indexação inicial: grava no banco a árvore inteira do Meu Drive e dos Drives compartilhados.

use std::collections::BTreeSet;

use gdl_common::names;
use gdl_drive::client::{Corpus, DriveClient, FOLDER_MIME, RemoteFile, SHORTCUT_MIME};
use gdl_store::{Inode, Item, ItemId, ItemKind, LocalState, PinState, Store};

use crate::drives::sync_drive_list;
use crate::gdocs::{pointer_extension, pointer_json};
use crate::{EngineError, Result};

/// Chaves do `kv` do banco.
pub(crate) const START_PAGE_TOKEN: &str = "start_page_token";
pub(crate) const ACCOUNT_EMAIL: &str = "account_email";
pub(crate) const ROOT_ID: &str = "root_id";

/// Item do banco a partir dos metadados do Drive, já sincronizado (revisão-base = atual). O nome
/// local é provisório: `recompute_local_names` resolve extensão de ponteiro e repetidos.
pub(crate) fn item_from_remote(r: RemoteFile, email: &str) -> Item {
    let (kind, size) = match r.mime_type.as_str() {
        FOLDER_MIME => (ItemKind::Folder, 0),
        SHORTCUT_MIME => {
            let (target_id, target_mime) = r.shortcut_target.unwrap_or_default();
            let target_id = ItemId(target_id);
            (
                ItemKind::Shortcut {
                    target_id,
                    target_mime,
                },
                0,
            )
        }
        mime if pointer_extension(mime).is_some() => {
            let size = pointer_json(&r.id, mime, email).len() as u64;
            (ItemKind::GoogleDoc { mime: mime.into() }, size)
        }
        _ => (ItemKind::File, r.size.unwrap_or(0)),
    };
    Item {
        local_name: names::to_local(&r.name),
        id: ItemId(r.id),
        inode: Inode::UNASSIGNED,
        parent: r.parents.into_iter().next().map(ItemId),
        remote_name: r.name,
        kind,
        size,
        remote_size: size,
        base_revision: r.head_revision.clone(),
        base_md5: r.md5.clone(),
        md5: r.md5,
        head_revision: r.head_revision,
        modified_ms: r.modified_ms,
        created_ms: r.created_ms,
        trashed: r.trashed,
        can_edit: r.can_edit,
        local_state: LocalState::Clean,
        pin: PinState::None,
    }
}

/// Grava os itens de `corpus`, que ficam sob a raiz `root` (já gravada), e refaz os nomes locais.
pub(crate) async fn index_corpus(
    store: &Store,
    client: &DriveClient,
    corpus: &Corpus,
    email: &str,
    root: ItemId,
) -> Result<()> {
    let mut parents = BTreeSet::from([root]);
    client
        .list_all(corpus, |page| {
            let items: Vec<Item> = page
                .into_iter()
                .map(|r| item_from_remote(r, email))
                .collect();
            parents.extend(items.iter().filter_map(|i| i.parent.clone()));
            store.upsert_items(&items).map_err(EngineError::from)
        })
        .await?;
    store.recompute_local_names(&parents)?;
    Ok(())
}

/// Grava a raiz e todos os itens do Meu Drive e dos Drives compartilhados num banco vazio.
pub async fn initial_index(store: &Store, client: &DriveClient) -> Result<()> {
    // O token vem antes da listagem: o que mudar durante ela aparece na primeira consulta.
    let token = client.start_page_token().await?;
    let about = client.about().await?;
    let mut root = item_from_remote(client.get("root").await?, &about.email);
    root.inode = Inode::ROOT;
    root.parent = None;
    store.upsert_item(&root)?;
    let root_id = root.id.0.clone();

    index_corpus(store, client, &Corpus::MyDrive, &about.email, root.id).await?;
    sync_drive_list(store, client, &about.email).await?;
    store.kv_set(ACCOUNT_EMAIL, &about.email)?;
    store.kv_set(ROOT_ID, &root_id)?;
    store.kv_set(START_PAGE_TOKEN, &token)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::Env;

    #[tokio::test]
    async fn initial_index_builds_tree_and_names() {
        let env = Env::new().await;
        let root = env.fake.root_id();
        let a = env.fake.add_folder(&root, "A");
        let b = env.fake.add_folder(&a, "B");
        let slash = env.fake.add_file(&b, "x/y.txt", b"1".to_vec());
        let first = env.fake.add_file(&a, "dup.txt", b"22".to_vec());
        let second = env.fake.add_file(&a, "dup.txt", b"333".to_vec());

        initial_index(&env.store, &env.client).await.unwrap();

        let store = &env.store;
        assert_eq!(env.item(&root).inode, Inode::ROOT);
        let found = |parent: &str, name: &str| {
            store
                .lookup(&ItemId(parent.into()), name)
                .unwrap()
                .map(|i| i.id.0)
        };
        assert_eq!(found(&root, "A"), Some(a.clone()));
        assert_eq!(found(&a, "B"), Some(b.clone()));
        assert_eq!(found(&b, "x∕y.txt"), Some(slash.clone()));
        assert_eq!(found(&a, "dup.txt"), Some(first.clone()));
        assert_eq!(found(&a, "dup (1).txt"), Some(second));

        let f = env.item(&first);
        assert_eq!(f.size, 2);
        assert!(f.head_revision.is_some());
        assert_eq!(f.base_revision, f.head_revision);
        assert_eq!(f.base_md5, f.md5);
        assert!(store.kv_get(START_PAGE_TOKEN).unwrap().is_some());
    }

    #[tokio::test]
    async fn gdoc_gets_pointer_name_and_size() {
        let env = Env::new().await;
        let mime = "application/vnd.google-apps.spreadsheet";
        let doc = env.fake.add_gdoc(&env.fake.root_id(), "Plano", mime);

        initial_index(&env.store, &env.client).await.unwrap();

        let item = env
            .store
            .lookup(&env.root(), "Plano.gsheet")
            .unwrap()
            .unwrap();
        assert_eq!(item.id.0, doc);
        assert_eq!(item.remote_name, "Plano");
        assert_eq!(
            item.size,
            pointer_json(&doc, mime, &env.fake.email()).len() as u64
        );
    }

    #[tokio::test]
    async fn changes_during_initial_listing_are_not_lost() {
        let env = Env::new().await;
        let root = env.fake.root_id();
        let ids: Vec<String> = (0..1500)
            .map(|n| env.fake.add_file(&root, &format!("f{n}"), vec![1]))
            .collect();
        // O simulado lista em ordem de ID: o menor sai na primeira página, antes da edição.
        let early = ids.iter().min().unwrap();
        env.fake
            .modify_remote_after_list_pages(early, b"novo".to_vec(), 1);

        initial_index(&env.store, &env.client).await.unwrap();
        assert_eq!(
            env.item(early).size,
            1,
            "a primeira página veio antes da edição"
        );

        let (poller, _) = env.poller();
        assert!(poller.run_once().await.unwrap() >= 1);
        let item = env.item(early);
        assert_eq!(item.size, 4);
        assert_eq!(
            item.head_revision.as_deref(),
            env.fake.meta(early)["headRevisionId"].as_str()
        );
    }
}
