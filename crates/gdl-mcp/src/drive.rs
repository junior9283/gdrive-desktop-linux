//! As ferramentas do app para clientes MCP, sobre o D-Bus do serviço (`ServiceProxy`).

use std::path::{Component, Path, PathBuf};

use gdl_ipc::{ServiceError, ServiceProxy};
use serde_json::{Map, Value, json};

use crate::{Tool, ToolError, Tools};

/// As ferramentas, falando com o serviço por `proxy`.
pub struct DriveTools {
    proxy: ServiceProxy<'static>,
    /// A pasta pessoal: só as pastas visíveis dentro dela podem ir para o Drive.
    home: PathBuf,
}

impl DriveTools {
    pub fn new(proxy: ServiceProxy<'static>, home: PathBuf) -> DriveTools {
        DriveTools { proxy, home }
    }

    async fn sync_status(&self, args: &Args) -> Result<Value, ToolError> {
        let path = args.text("path")?;
        let status = self.proxy.path_status(path).await.map_err(failed)?;
        let account_state = match status.account.as_str() {
            "" => String::new(),
            account => self.proxy.status(account).await.map_err(failed)?.state,
        };
        Ok(json!({
            "path": path,
            "state": status.state,
            "account": status.account,
            "account_state": account_state,
        }))
    }

    async fn drive_url(&self, args: &Args) -> Result<Value, ToolError> {
        let path = args.text("path")?;
        let url = self.proxy.web_link_of_path(path).await.map_err(failed)?;
        Ok(json!({ "path": path, "url": url }))
    }

    async fn tracked_folders(&self) -> Result<Value, ToolError> {
        let status = self.proxy.status("").await.map_err(failed)?;
        let mut accounts = vec![];
        for account in status.accounts {
            let mirror = self.proxy.mirror(&account.email).await.map_err(failed)?;
            accounts.push(json!({
                "account": account.email,
                "mount_point": account.mount_point,
                "mirror": mirror,
            }));
        }
        let folders: Vec<Value> = self
            .proxy
            .local_folders()
            .await
            .map_err(failed)?
            .into_iter()
            .map(|f| json!({ "account": f.account, "path": f.path, "state": f.state }))
            .collect();
        Ok(json!({ "accounts": accounts, "computer_folders": folders }))
    }

    async fn add_folder(&self, args: &Args) -> Result<Value, ToolError> {
        let path = args.text("path")?;
        let account = args.optional("account_id")?;
        if !visible_in_home(Path::new(path), &self.home) {
            return Err(ToolError::Failed(format!(
                "not-allowed: only visible folders inside {} can be added",
                self.home.display()
            )));
        }
        match args.text("sync_mode")? {
            "backup" => {
                let id = self
                    .proxy
                    .add_local_folder(account, path, false)
                    .await
                    .map_err(failed)?;
                Ok(json!({ "sync_mode": "backup", "path": path, "folder_id": id }))
            }
            "stream" => {
                let dir = args.optional("my_drive_path")?;
                let moved = self
                    .proxy
                    .move_to_drive(account, path, dir)
                    .await
                    .map_err(failed)?;
                Ok(json!({ "sync_mode": "stream", "path": moved, "link": path }))
            }
            other => Err(ToolError::InvalidArgs(format!(
                "sync_mode must be \"backup\" or \"stream\", not \"{other}\""
            ))),
        }
    }

    async fn acl_dialog(&self, args: &Args) -> Result<Value, ToolError> {
        let (email, item) = (args.text("email")?, args.text("item_id")?);
        let account = args.optional("account_id")?;
        self.proxy
            .open_share(account, item, email)
            .await
            .map_err(failed)?;
        Ok(json!({ "opened": true }))
    }
}

impl Tools for DriveTools {
    fn list(&self) -> Vec<Tool> {
        let path = json!({ "type": "string", "description": "Absolute local path." });
        let account = json!({
            "type": "string",
            "description": "Email of the account; defaults to the first one.",
        });
        vec![
            Tool {
                name: "get_sync_status",
                description: "Sync state of a local path: offline (available offline), syncing, \
                    conflict, online (only in the cloud) or outside (not in Drive nor in a synced \
                    folder), plus the state of its account.",
                input_schema: schema(json!({ "path": path }), &["path"]),
            },
            Tool {
                name: "get_drive_url",
                description: "Google Drive web link of a file or folder in the Drive mount.",
                input_schema: schema(json!({ "path": path }), &["path"]),
            },
            Tool {
                name: "list_tracked_folders",
                description: "Drive mount point and mirror folder of each account, and the \
                    computer folders backed up to Drive.",
                input_schema: schema(json!({}), &[]),
            },
            Tool {
                name: "add_folder_to_sync",
                description: "Sync a visible folder inside the home folder (hidden folders and \
                    anything outside home are refused). \"backup\" keeps it in place and backs it up \
                    to Computers in Drive. \"stream\" moves it into My Drive (my_drive_path, \
                    default the root) and leaves a symbolic link in its place; the original goes \
                    to the system trash.",
                input_schema: schema(
                    json!({
                        "path": path,
                        "sync_mode": { "type": "string", "enum": ["backup", "stream"] },
                        "account_id": account,
                        "my_drive_path": {
                            "type": "string",
                            "description": "Folder inside My Drive, relative (stream only).",
                        },
                    }),
                    &["path", "sync_mode"],
                ),
            },
            Tool {
                name: "open_acl_dialog",
                description: "Open the app's Share window for a Drive item with the email \
                    already typed in. Nothing is shared until the user confirms.",
                input_schema: schema(
                    json!({
                        "email": { "type": "string" },
                        "item_id": { "type": "string", "description": "Drive item ID." },
                        "account_id": account,
                    }),
                    &["email", "item_id"],
                ),
            },
        ]
    }

    async fn call(&self, name: &str, args: Value) -> Result<Value, ToolError> {
        let args = Args::new(args)?;
        match name {
            "get_sync_status" => self.sync_status(&args).await,
            "get_drive_url" => self.drive_url(&args).await,
            "list_tracked_folders" => self.tracked_folders().await,
            "add_folder_to_sync" => self.add_folder(&args).await,
            "open_acl_dialog" => self.acl_dialog(&args).await,
            _ => Err(ToolError::Unknown),
        }
    }
}

/// `path` (seguindo links) é uma pasta dentro de `home`, sem nenhum nome oculto no caminho: um
/// assistente, ou um texto que o engane, não leva ao Drive `~/.ssh`, `~/.gnupg`, `~/.config` nem
/// nada fora da pasta pessoal.
fn visible_in_home(path: &Path, home: &Path) -> bool {
    let (Ok(path), Ok(home)) = (path.canonicalize(), home.canonicalize()) else {
        return false;
    };
    path.strip_prefix(&home).is_ok_and(|inside| {
        inside.components().next().is_some()
            && inside.components().all(|c| match c {
                Component::Normal(name) => !name.to_string_lossy().starts_with('.'),
                _ => false,
            })
    })
}

fn schema(properties: Value, required: &[&str]) -> Value {
    json!({ "type": "object", "properties": properties, "required": required })
}

/// O erro do serviço como falha da ferramenta; argumentos recusados pelo serviço são do
/// protocolo.
fn failed(e: ServiceError) -> ToolError {
    match e {
        ServiceError::InvalidArgs(m) => ToolError::InvalidArgs(m),
        e => ToolError::Failed(format!("{}: {}", e.short_name(), e.into_message())),
    }
}

/// Os argumentos de uma chamada.
struct Args(Map<String, Value>);

impl Args {
    fn new(args: Value) -> Result<Args, ToolError> {
        match args {
            Value::Object(map) => Ok(Args(map)),
            _ => Err(ToolError::InvalidArgs("arguments must be an object".into())),
        }
    }

    /// O texto obrigatório `key`.
    fn text(&self, key: &str) -> Result<&str, ToolError> {
        match self.optional(key)? {
            "" => Err(ToolError::InvalidArgs(format!("missing \"{key}\""))),
            text => Ok(text),
        }
    }

    /// O texto opcional `key`; vazio se não veio.
    fn optional(&self, key: &str) -> Result<&str, ToolError> {
        match self.0.get(key) {
            None | Some(Value::Null) => Ok(""),
            Some(Value::String(text)) => Ok(text),
            Some(_) => Err(ToolError::InvalidArgs(format!(
                "\"{key}\" must be a string"
            ))),
        }
    }
}
