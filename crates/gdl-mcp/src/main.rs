//! `gdrive-desktop-linux-mcp`: o servidor MCP do app, pela entrada e saída padrão. Fala com o
//! serviço pelo D-Bus da sessão (abrindo-o, se preciso).

use gdl_ipc::ServiceProxy;
use gdl_mcp::drive::DriveTools;
use tokio::io::BufReader;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let conn = zbus::Connection::session().await?;
    // Logo depois de instalar, o barramento ainda pode não saber abrir o serviço.
    let _ = gdl_ipc::ensure_activatable(&conn).await;
    let tools = DriveTools::new(ServiceProxy::new(&conn).await?);
    let stdin = BufReader::new(tokio::io::stdin());
    gdl_mcp::serve(&tools, stdin, tokio::io::stdout()).await?;
    Ok(())
}
