//! Um processo só: o primeiro fica com o nome `UI_BUS_NAME` e recebe os pedidos de tela
//! (`Ui.Open`); os seguintes passam o pedido para ele e saem.

use gdl_ipc::{UI_BUS_NAME, UI_OBJECT_PATH, UiProxy, claim_name};
use tokio::sync::mpsc;
use zbus::Connection;

/// Os pedidos de tela (`tela[:argumento]`) dos outros processos.
pub type Requests = mpsc::UnboundedReceiver<String>;

pub enum SingleInstance {
    /// Este processo é a janela: chegam aqui as telas que os outros pedirem.
    Primary(Requests),
    /// Já havia uma janela, que recebeu o pedido.
    Secondary,
}

struct UiObject {
    screens: mpsc::UnboundedSender<String>,
}

#[zbus::interface(name = "io.github.junior9283.GDriveDesktopLinux1.Ui")]
impl UiObject {
    fn open(&self, screen: String) {
        let _ = self.screens.send(screen);
    }
}

impl SingleInstance {
    /// Fica com o nome ou, se outro processo já o tem, passa a ele o pedido `request`.
    pub async fn claim(conn: &Connection, request: &str) -> zbus::Result<SingleInstance> {
        let (screens, requests) = mpsc::unbounded_channel();
        // O objeto vem antes do nome: quem achar o nome já encontra quem responda.
        let server = conn.object_server();
        server.at(UI_OBJECT_PATH, UiObject { screens }).await?;
        if claim_name(conn, UI_BUS_NAME).await? {
            return Ok(SingleInstance::Primary(requests));
        }
        server.remove::<UiObject, _>(UI_OBJECT_PATH).await?;
        UiProxy::new(conn).await?.open(request).await?;
        Ok(SingleInstance::Secondary)
    }
}

#[cfg(test)]
mod tests {
    use gdl_ipc::testing::PrivateBus;

    use super::*;

    #[tokio::test]
    async fn second_instance_forwards_open() {
        let bus = PrivateBus::start();
        let (first, second) = (bus.connect().await, bus.connect().await);

        let SingleInstance::Primary(mut requests) =
            SingleInstance::claim(&first, "home").await.unwrap()
        else {
            panic!("o primeiro processo devia ficar com o nome");
        };
        let again = SingleInstance::claim(&second, "search").await.unwrap();

        assert!(matches!(again, SingleInstance::Secondary));
        assert_eq!(requests.recv().await.as_deref(), Some("search"));
    }
}
