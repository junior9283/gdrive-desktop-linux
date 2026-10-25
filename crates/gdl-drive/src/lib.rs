//! Acesso à conta Google: login OAuth, guarda dos tokens e cliente da Drive API v3.

pub mod client;
pub mod model;
pub mod oauth;
pub mod retry;
pub mod throttle;
pub mod token_store;

use std::sync::RwLock;

/// Como os clientes HTTP saem para a internet (`DirectConnection`, `ProxyServer`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Network {
    /// O proxy do sistema (variáveis `HTTPS_PROXY`, `NO_PROXY`…).
    #[default]
    System,
    /// Sem proxy nenhum.
    Direct,
    /// Este proxy para tudo (`http://`, `https://` ou `socks5://`).
    Proxy(String),
}

static NETWORK: RwLock<Network> = RwLock::new(Network::System);

/// Vale para os clientes criados depois: muda só quando o serviço (re)inicia, como no original.
pub fn set_network(network: Network) {
    *NETWORK.write().unwrap_or_else(|e| e.into_inner()) = network;
}

/// Base dos clientes HTTP do app: prazo de conexão para um servidor mudo não travar nada, e a
/// rede escolhida (`set_network`).
pub(crate) fn http_builder() -> reqwest::ClientBuilder {
    let builder = reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(10));
    match &*NETWORK.read().unwrap_or_else(|e| e.into_inner()) {
        Network::System => builder,
        Network::Direct => builder.no_proxy(),
        Network::Proxy(url) => match reqwest::Proxy::all(url) {
            Ok(proxy) => builder.proxy(proxy),
            Err(e) => {
                tracing::warn!(erro = %e, proxy = %url, "proxy inválido; usando o do sistema");
                builder
            }
        },
    }
}
