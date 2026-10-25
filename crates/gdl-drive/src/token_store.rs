//! Guarda das credenciais e dos tokens de cada conta. Nada vai para arquivo: em produção fica no
//! Secret Service (chaveiro do KDE/GNOME), nos testes fica na memória.

use std::collections::HashMap;
use std::fmt;
use std::sync::Mutex;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

const APP: &str = "gdrive-desktop-linux";

/// Client ID e segredo do app no Google Cloud, informados pelo próprio usuário.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientCredentials {
    pub client_id: String,
    pub client_secret: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at_ms: i64,
    pub email: String,
}

// O `Debug` nunca mostra segredos, para não vazarem em logs.
impl fmt::Debug for ClientCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClientCredentials")
            .field("client_id", &self.client_id)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TokenSet")
            .field("email", &self.email)
            .field("expires_at_ms", &self.expires_at_ms)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TokenStoreError {
    #[error("erro no chaveiro do sistema: {0}")]
    Keyring(Box<oo7::Error>),
    #[error("credenciais guardadas ilegíveis: {0}")]
    Corrupt(#[from] serde_json::Error),
}

impl From<oo7::Error> for TokenStoreError {
    fn from(e: oo7::Error) -> Self {
        TokenStoreError::Keyring(Box::new(e))
    }
}

pub type Result<T> = std::result::Result<T, TokenStoreError>;

#[async_trait]
pub trait TokenStore: Send + Sync {
    async fn save(&self, account: &str, creds: &ClientCredentials, tokens: &TokenSet)
    -> Result<()>;
    async fn load(&self, account: &str) -> Result<Option<(ClientCredentials, TokenSet)>>;
    async fn delete(&self, account: &str) -> Result<()>;
}

/// O que fica guardado no chaveiro, como um único segredo JSON por conta.
#[derive(Deserialize)]
struct Stored {
    creds: ClientCredentials,
    tokens: TokenSet,
}

/// O mesmo formato de `Stored`, por referência, para gravar sem copiar os segredos.
#[derive(Serialize)]
struct StoredRef<'a> {
    creds: &'a ClientCredentials,
    tokens: &'a TokenSet,
}

/// Chaveiro do sistema via Secret Service (D-Bus).
pub struct SecretServiceStore {
    keyring: oo7::Keyring,
}

impl SecretServiceStore {
    pub async fn new() -> Result<SecretServiceStore> {
        Ok(SecretServiceStore {
            keyring: oo7::Keyring::new().await?,
        })
    }

    fn attributes(account: &str) -> HashMap<&str, &str> {
        HashMap::from([("application", APP), ("account", account)])
    }
}

#[async_trait]
impl TokenStore for SecretServiceStore {
    async fn save(
        &self,
        account: &str,
        creds: &ClientCredentials,
        tokens: &TokenSet,
    ) -> Result<()> {
        let secret = serde_json::to_vec(&StoredRef { creds, tokens })?;
        let label = format!("Google Drive para Linux ({account})");
        self.keyring
            .create_item(&label, &Self::attributes(account), secret, true)
            .await?;
        Ok(())
    }

    async fn load(&self, account: &str) -> Result<Option<(ClientCredentials, TokenSet)>> {
        let Some(item) = self
            .keyring
            .search_items(&Self::attributes(account))
            .await?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let stored: Stored = serde_json::from_slice(&item.secret().await?)?;
        Ok(Some((stored.creds, stored.tokens)))
    }

    async fn delete(&self, account: &str) -> Result<()> {
        self.keyring.delete(&Self::attributes(account)).await?;
        Ok(())
    }
}

/// Guarda em memória, para testes.
#[derive(Default)]
pub struct MemoryStore {
    accounts: Mutex<HashMap<String, (ClientCredentials, TokenSet)>>,
}

impl MemoryStore {
    fn accounts(
        &self,
    ) -> std::sync::MutexGuard<'_, HashMap<String, (ClientCredentials, TokenSet)>> {
        self.accounts.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[async_trait]
impl TokenStore for MemoryStore {
    async fn save(
        &self,
        account: &str,
        creds: &ClientCredentials,
        tokens: &TokenSet,
    ) -> Result<()> {
        self.accounts()
            .insert(account.into(), (creds.clone(), tokens.clone()));
        Ok(())
    }

    async fn load(&self, account: &str) -> Result<Option<(ClientCredentials, TokenSet)>> {
        Ok(self.accounts().get(account).cloned())
    }

    async fn delete(&self, account: &str) -> Result<()> {
        self.accounts().remove(account);
        Ok(())
    }
}
