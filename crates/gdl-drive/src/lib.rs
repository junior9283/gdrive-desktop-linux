//! Acesso à conta Google: login OAuth, guarda dos tokens e cliente da Drive API v3.

pub mod client;
pub mod model;
pub mod oauth;
pub mod retry;
pub mod throttle;
pub mod token_store;

use std::sync::{Arc, RwLock};

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

/// As raízes de certificado e a validação do TLS: as políticas `TrustedRootCertsFile`,
/// `TrustSystemRootCerts` e `DisableSSLValidation`. Sem `system_roots`, valem só as
/// `extra_roots` (o app não traz raízes próprias).
#[derive(Clone)]
pub struct Tls {
    pub extra_roots: Vec<reqwest::Certificate>,
    pub system_roots: bool,
    pub accept_invalid: bool,
}

impl Default for Tls {
    fn default() -> Tls {
        Tls {
            extra_roots: Vec::new(),
            system_roots: true,
            accept_invalid: false,
        }
    }
}

/// As raízes de um arquivo PEM (uma ou mais).
pub fn read_roots(path: &std::path::Path) -> std::io::Result<Vec<reqwest::Certificate>> {
    reqwest::Certificate::from_pem_bundle(&std::fs::read(path)?).map_err(std::io::Error::other)
}

/// A rede e o TLS de todos os clientes HTTP.
#[derive(Default)]
struct Http {
    network: Network,
    tls: Tls,
}

static HTTP: RwLock<Option<Arc<Http>>> = RwLock::new(None);

/// Vale para os clientes criados depois: muda só quando o serviço (re)inicia, como no original.
pub fn set_http(network: Network, tls: Tls) {
    *HTTP.write().unwrap_or_else(|e| e.into_inner()) = Some(Arc::new(Http { network, tls }));
}

/// Base dos clientes HTTP do app: prazo de conexão para um servidor mudo não travar nada, e a
/// rede e o TLS de `set_http`.
pub(crate) fn http_builder() -> reqwest::ClientBuilder {
    let base = || reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(10));
    let http = HTTP
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
        .unwrap_or_default();
    // `SetPreferences` recusa um endereço inválido; este só vem de edição à mão. O endereço não
    // vai para o log: pode ter usuário e senha.
    let builder = with_network(base(), &http.network).unwrap_or_else(|e| {
        tracing::warn!(erro = %e, "ProxyServer inválido; usando o proxy do sistema");
        base()
    });
    with_tls(builder, &http.tls)
}

/// Erro de um pedido HTTP fora da Drive API (`fetch_json`).
pub use reqwest::Error as HttpError;

/// GET de um JSON fora do Drive (a última release do projeto), com a rede e o TLS do app e um
/// prazo de 30 s.
pub async fn fetch_json<T: serde::de::DeserializeOwned>(url: &url::Url) -> Result<T, HttpError> {
    http_builder()
        .user_agent(concat!("gdrive-desktop-linux/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(30))
        .build()?
        .get(url.clone())
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await?
        .error_for_status()?
        .json()
        .await
}

fn with_tls(builder: reqwest::ClientBuilder, tls: &Tls) -> reqwest::ClientBuilder {
    let roots = tls.extra_roots.iter().cloned();
    let builder = match (tls.system_roots, tls.extra_roots.is_empty()) {
        (true, true) => builder,
        (true, false) => builder.tls_certs_merge(roots),
        (false, _) => builder.tls_certs_only(roots),
    };
    builder.tls_danger_accept_invalid_certs(tls.accept_invalid)
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

    /// Servidor HTTPS mínimo com certificado autoassinado para `localhost`: responde "ok" a
    /// qualquer pedido. Devolve o endereço e o certificado em PEM.
    async fn self_signed_server() -> (String, String) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        use tokio_rustls::rustls;
        let made = rcgen::generate_simple_self_signed(vec!["localhost".into()]).unwrap();
        let key = rustls::pki_types::PrivatePkcs8KeyDer::from(made.signing_key.serialize_der());
        let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
        let config = rustls::ServerConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .unwrap()
            .with_no_client_auth()
            .with_single_cert(vec![made.cert.der().clone()], key.into())
            .unwrap();
        let acceptor = tokio_rustls::TlsAcceptor::from(Arc::new(config));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            while let Ok((tcp, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                tokio::spawn(async move {
                    let Ok(mut tls) = acceptor.accept(tcp).await else {
                        return;
                    };
                    let mut buf = [0; 4096];
                    let _ = tls.read(&mut buf).await;
                    let reply =
                        "HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok";
                    let _ = tls.write_all(reply.as_bytes()).await;
                    let _ = tls.shutdown().await;
                });
            }
        });
        (format!("https://localhost:{port}/"), made.cert.pem())
    }

    async fn fetch(tls: &Tls, url: &str) -> reqwest::Result<String> {
        let client = with_tls(reqwest::Client::builder(), tls).build()?;
        client.get(url).send().await?.text().await
    }

    #[tokio::test]
    async fn trusted_root_file_reaches_a_self_signed_server() {
        let (url, pem) = self_signed_server().await;
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("raizes.pem");
        std::fs::write(&file, pem).unwrap();

        let refused = fetch(&Tls::default(), &url).await;
        let roots = read_roots(&file).unwrap();
        let merged = Tls {
            extra_roots: roots.clone(),
            ..Tls::default()
        };
        let only = Tls {
            extra_roots: roots,
            system_roots: false,
            accept_invalid: false,
        };
        let invalid = Tls {
            accept_invalid: true,
            ..Tls::default()
        };

        assert!(refused.is_err(), "sem a raiz, o certificado é recusado");
        assert_eq!(fetch(&merged, &url).await.unwrap(), "ok");
        assert_eq!(
            fetch(&only, &url).await.unwrap(),
            "ok",
            "só as raízes do arquivo"
        );
        assert_eq!(fetch(&invalid, &url).await.unwrap(), "ok");
        assert!(read_roots(&dir.path().join("falta.pem")).is_err());
    }

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
