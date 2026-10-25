//! IDs reservados no Drive para criar itens: com eles, repetir uma criação cuja resposta se
//! perdeu não duplica o item (o Drive responde 409).

use gdl_drive::client::{DriveClient, DriveError};
use std::sync::Mutex;

/// Quantos IDs pedir de uma vez.
const BATCH: u32 = 100;

#[derive(Default)]
pub struct IdPool {
    ids: Mutex<Vec<String>>,
}

impl IdPool {
    /// Um ID ainda não usado; acabando o lote, pede outro (sem segurar a trava durante o
    /// pedido: sem rede, ninguém fica esperando atrás dele).
    pub async fn take(&self, client: &DriveClient) -> Result<String, DriveError> {
        if let Some(id) = self.lock().pop() {
            return Ok(id);
        }
        let mut batch = client.generate_ids(BATCH).await?;
        let id = batch
            .pop()
            .ok_or_else(|| DriveError::Protocol("generateIds sem IDs".into()))?;
        self.lock().extend(batch);
        Ok(id)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<String>> {
        self.ids.lock().unwrap_or_else(|e| e.into_inner())
    }
}
