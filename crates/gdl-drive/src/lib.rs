//! Acesso à conta Google: login OAuth, guarda dos tokens e cliente da Drive API v3.

pub mod client;
pub mod model;
pub mod oauth;
pub mod retry;
pub mod token_store;

/// Base dos clientes HTTP do app: prazo de conexão para um servidor mudo não travar nada.
pub(crate) fn http_builder() -> reqwest::ClientBuilder {
    reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(10))
}
