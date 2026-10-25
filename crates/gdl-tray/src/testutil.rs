//! Apoio aos testes da bandeja.

use zbus::connection::Builder;
use zbus::{Connection, Guid};

/// Uma conexão ponto a ponto: (lado do servidor, lado do cliente).
pub(crate) async fn p2p_pair() -> (Connection, Connection) {
    let (a, b) = tokio::net::UnixStream::pair().unwrap();
    let server = Builder::unix_stream(a)
        .server(Guid::generate())
        .unwrap()
        .p2p()
        .build();
    let client = Builder::unix_stream(b).p2p().build();
    let (server, client) = tokio::join!(server, client);
    (server.unwrap(), client.unwrap())
}
