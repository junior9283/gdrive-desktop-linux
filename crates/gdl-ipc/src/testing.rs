//! Barramentos para os testes da bandeja e da janela (feature `testing`).

use std::io::BufRead;

use zbus::connection::Builder;
use zbus::{Connection, Guid};

/// Uma conexão ponto a ponto: (lado do servidor, lado do cliente).
pub async fn p2p_pair() -> (Connection, Connection) {
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

/// Um barramento de sessão só do teste (`dbus-daemon`), sem ativação de serviços: há donos de
/// nome, mas ninguém responde pelo serviço, pelo portal ou pela janela. Termina ao sair de cena.
pub struct PrivateBus {
    daemon: std::process::Child,
    address: String,
    _dir: tempfile::TempDir,
}

impl PrivateBus {
    pub fn start() -> PrivateBus {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("bus.conf");
        std::fs::write(
            &config,
            format!(
                "<busconfig><type>session</type><listen>unix:dir={}</listen>\
                 <policy context=\"default\"><allow send_destination=\"*\"/>\
                 <allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>",
                dir.path().display()
            ),
        )
        .unwrap();
        let mut daemon = std::process::Command::new("dbus-daemon")
            .arg(format!("--config-file={}", config.display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .expect("dbus-daemon");
        let mut address = String::new();
        std::io::BufReader::new(daemon.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        PrivateBus {
            daemon,
            address: address.trim().to_string(),
            _dir: dir,
        }
    }

    /// Uma conexão nova neste barramento.
    pub async fn connect(&self) -> Connection {
        Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
    }
}
