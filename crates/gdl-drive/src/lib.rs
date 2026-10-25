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
    /// Este proxy para tudo (um dos `gdl_common::config::PROXY_SCHEMES`).
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
    let network = NETWORK.read().unwrap_or_else(|e| e.into_inner()).clone();
    // `SetPreferences` recusa um endereço inválido; este só vem de edição à mão. O endereço não
    // vai para o log: pode ter usuário e senha.
    with_network(builder, &network).unwrap_or_else(|e| {
        tracing::warn!(erro = %e, "ProxyServer inválido; usando o proxy do sistema");
        reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(10))
    })
}

fn with_network(
    builder: reqwest::ClientBuilder,
    network: &Network,
) -> reqwest::Result<reqwest::ClientBuilder> {
    Ok(match network {
        Network::System => builder,
        Network::Direct => builder.no_proxy(),
        Network::Proxy(url) => builder.proxy(reqwest::Proxy::all(url)?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_accepted_proxy_scheme_builds_a_client() {
        for scheme in gdl_common::config::PROXY_SCHEMES {
            let network = Network::Proxy(format!("{scheme}://usuario:senha@127.0.0.1:1080"));
            with_network(reqwest::Client::builder(), &network)
                .and_then(|b| b.build())
                .unwrap_or_else(|e| panic!("{scheme}: {e}"));
        }
    }

    /// Servidor SOCKS5 mínimo (sem senha, só CONNECT a IPv4) que conta as conexões e repassa os
    /// bytes.
    async fn socks5_server() -> (String, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let connects = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let seen = connects.clone();
        tokio::spawn(async move {
            while let Ok((mut client, _)) = listener.accept().await {
                let seen = seen.clone();
                tokio::spawn(async move {
                    let mut hello = [0u8; 2];
                    client.read_exact(&mut hello).await?;
                    let mut methods = vec![0u8; hello[1] as usize];
                    client.read_exact(&mut methods).await?;
                    client.write_all(&[5, 0]).await?;
                    let mut req = [0u8; 10];
                    client.read_exact(&mut req).await?;
                    assert_eq!(&req[..4], &[5, 1, 0, 1], "CONNECT a IPv4");
                    let ip = std::net::Ipv4Addr::new(req[4], req[5], req[6], req[7]);
                    let port = u16::from_be_bytes([req[8], req[9]]);
                    let mut target = tokio::net::TcpStream::connect((ip, port)).await?;
                    seen.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0]).await?;
                    tokio::io::copy_bidirectional(&mut client, &mut target).await?;
                    std::io::Result::Ok(())
                });
            }
        });
        (format!("socks5://{addr}"), connects)
    }

    #[tokio::test]
    async fn socks5_proxy_carries_the_requests() {
        let fake = gdl_fake_drive::FakeDrive::start().await;
        let (proxy, connects) = socks5_server().await;
        let client = with_network(reqwest::Client::builder(), &Network::Proxy(proxy))
            .unwrap()
            .build()
            .unwrap();

        let resp = client.get(fake.base_url()).send().await.unwrap();

        assert!(resp.status().as_u16() < 500, "{}", resp.status());
        assert_eq!(connects.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
