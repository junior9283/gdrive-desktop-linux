//! IDs reservados no Drive para criar itens: com eles, repetir uma criação cuja resposta se
//! perdeu não duplica o item (o Drive responde 409).

use gdl_drive::client::{DriveClient, DriveError};
use tokio::sync::Mutex;

/// Quantos IDs pedir de uma vez.
const BATCH: u32 = 100;

#[derive(Default)]
pub struct IdPool {
    ids: Mutex<Vec<String>>,
}

impl IdPool {
    /// Um ID ainda não usado; acabando o lote, pede outro.
    pub async fn take(&self, client: &DriveClient) -> Result<String, DriveError> {
        let mut ids = self.ids.lock().await;
        if ids.is_empty() {
            *ids = client.generate_ids(BATCH).await?;
        }
        ids.pop()
            .ok_or_else(|| DriveError::Protocol("generateIds sem IDs".into()))
    }
}
