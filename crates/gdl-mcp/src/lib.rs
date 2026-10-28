//! Servidor MCP (Model Context Protocol) do app: JSON-RPC 2.0 pela entrada e saída padrão, uma
//! mensagem por linha, feito à mão sobre o `serde_json`. Responde a `initialize`, `ping`,
//! `tools/list` e `tools/call`; as ferramentas vêm de quem implementa `Tools`.

pub mod drive;

use std::future::Future;

use serde_json::{Value, json};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncWrite, AsyncWriteExt};

/// A versão do protocolo MCP que este servidor fala.
pub const PROTOCOL_VERSION: &str = "2025-06-18";

const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;

/// Uma ferramenta oferecida em `tools/list`.
pub struct Tool {
    pub name: &'static str,
    pub description: &'static str,
    /// JSON Schema dos argumentos.
    pub input_schema: Value,
    /// Dicas para o cliente (`readOnlyHint`, `destructiveHint`…), que decide se pergunta ao
    /// usuário antes de chamar.
    pub annotations: Value,
}

/// Por que uma chamada de ferramenta não deu resultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    /// Não há ferramenta com esse nome (erro do protocolo).
    Unknown,
    /// Argumentos que não servem (erro do protocolo).
    InvalidArgs(String),
    /// A ferramenta rodou e falhou: vai para quem chamou como resultado com `isError`.
    Failed(String),
}

/// As ferramentas do servidor.
pub trait Tools {
    fn list(&self) -> Vec<Tool>;

    fn call(&self, name: &str, args: Value) -> impl Future<Output = Result<Value, ToolError>>;
}

/// Atende as mensagens de `input`, uma por linha, até a entrada acabar.
pub async fn serve(
    tools: &impl Tools,
    input: impl AsyncBufRead + Unpin,
    mut output: impl AsyncWrite + Unpin,
) -> std::io::Result<()> {
    let mut lines = input.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        if let Some(reply) = handle(tools, &line).await {
            let mut bytes = serde_json::to_vec(&reply)?;
            bytes.push(b'\n');
            output.write_all(&bytes).await?;
            output.flush().await?;
        }
    }
    Ok(())
}

/// A resposta a uma linha; `None` para notificações, que não têm resposta.
pub async fn handle(tools: &impl Tools, line: &str) -> Option<Value> {
    let Ok(mut message) = serde_json::from_str::<Value>(line) else {
        return Some(error(Value::Null, PARSE_ERROR, "Parse error"));
    };
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str);
    let (Some(method), Some("2.0")) = (method, message.get("jsonrpc").and_then(Value::as_str))
    else {
        // Uma resposta do cliente (sem `method`, com `result` ou `error`) não pede nada.
        if message.get("result").is_some() || message.get("error").is_some() {
            return None;
        }
        return Some(error(
            id.unwrap_or(Value::Null),
            INVALID_REQUEST,
            "Invalid Request",
        ));
    };
    let (id, method) = (id?, method.to_owned());
    let mut params = message
        .get_mut("params")
        .map(Value::take)
        .unwrap_or_default();
    Some(match method.as_str() {
        "initialize" => success(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": { "listChanged": false } },
                "serverInfo": { "name": "gdrive-desktop-linux", "version": env!("CARGO_PKG_VERSION") },
            }),
        ),
        "ping" => success(id, json!({})),
        "tools/list" => {
            let list: Vec<Value> = tools
                .list()
                .into_iter()
                .map(|t| {
                    json!({
                        "name": t.name,
                        "description": t.description,
                        "inputSchema": t.input_schema,
                        "annotations": t.annotations,
                    })
                })
                .collect();
            success(id, json!({ "tools": list }))
        }
        "tools/call" => {
            let args = match params.get_mut("arguments").map(Value::take) {
                Some(Value::Null) | None => json!({}),
                Some(args) => args,
            };
            let Some(name) = params.get("name").and_then(Value::as_str) else {
                return Some(error(id, INVALID_PARAMS, "Missing tool name"));
            };
            match tools.call(name, args).await {
                Err(ToolError::Unknown) => {
                    error(id, INVALID_PARAMS, &format!("Unknown tool: {name}"))
                }
                Err(ToolError::InvalidArgs(why)) => error(id, INVALID_PARAMS, &why),
                Ok(value) => success(
                    id,
                    json!({
                        "content": [{ "type": "text", "text": value.to_string() }],
                        "structuredContent": value,
                        "isError": false,
                    }),
                ),
                Err(ToolError::Failed(message)) => success(
                    id,
                    json!({ "content": [{ "type": "text", "text": message }], "isError": true }),
                ),
            }
        }
        _ => error(id, METHOD_NOT_FOUND, &format!("Method not found: {method}")),
    })
}

fn success(id: Value, result: Value) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "result": result })
}

fn error(id: Value, code: i64, message: &str) -> Value {
    json!({ "jsonrpc": "2.0", "id": id, "error": { "code": code, "message": message } })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Uma ferramenta que devolve os argumentos e outra que sempre falha.
    struct Echo;

    impl Tools for Echo {
        fn list(&self) -> Vec<Tool> {
            vec![Tool {
                name: "echo",
                description: "Returns its arguments.",
                input_schema: json!({ "type": "object" }),
                annotations: json!({ "readOnlyHint": true }),
            }]
        }

        async fn call(&self, name: &str, args: Value) -> Result<Value, ToolError> {
            match name {
                "echo" if args.get("bad").is_some() => Err(ToolError::InvalidArgs("bad".into())),
                "echo" => Ok(args),
                "broken" => Err(ToolError::Failed("não deu".into())),
                _ => Err(ToolError::Unknown),
            }
        }
    }

    async fn ask(line: &str) -> Option<Value> {
        handle(&Echo, line).await
    }

    #[tokio::test]
    async fn initialize_and_list_tools() {
        let init = ask(r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"t","version":"1"}}}"#)
            .await
            .unwrap();
        assert_eq!(init["id"], 1);
        assert_eq!(init["result"]["protocolVersion"], PROTOCOL_VERSION);
        assert!(init["result"]["capabilities"]["tools"].is_object());
        assert_eq!(
            init["result"]["serverInfo"]["version"],
            env!("CARGO_PKG_VERSION")
        );
        assert_eq!(
            ask(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).await,
            None
        );

        let list = ask(r#"{"jsonrpc":"2.0","id":"a","method":"tools/list"}"#)
            .await
            .unwrap();
        assert_eq!(list["id"], "a");
        assert_eq!(list["result"]["tools"][0]["name"], "echo");
        assert_eq!(list["result"]["tools"][0]["inputSchema"]["type"], "object");
        assert_eq!(
            list["result"]["tools"][0]["annotations"]["readOnlyHint"],
            true
        );

        let ping = ask(r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#)
            .await
            .unwrap();
        assert_eq!(ping["result"], json!({}));

        let call = ask(r#"{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"echo","arguments":{"x":1}}}"#)
            .await
            .unwrap();
        assert_eq!(call["result"]["isError"], false);
        assert_eq!(call["result"]["structuredContent"], json!({"x": 1}));
        assert_eq!(call["result"]["content"][0]["text"], r#"{"x":1}"#);

        let failed =
            ask(r#"{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"broken"}}"#)
                .await
                .unwrap();
        assert_eq!(
            failed["result"]["isError"], true,
            "falha da ferramenta não é erro do protocolo"
        );
        assert_eq!(failed["result"]["content"][0]["text"], "não deu");
    }

    #[tokio::test]
    async fn unknown_method_is_an_error() {
        let reply = ask(r#"{"jsonrpc":"2.0","id":7,"method":"resources/list"}"#)
            .await
            .unwrap();
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["error"]["code"], METHOD_NOT_FOUND);
        let tool =
            ask(r#"{"jsonrpc":"2.0","id":8,"method":"tools/call","params":{"name":"nada"}}"#)
                .await
                .unwrap();
        assert_eq!(tool["error"]["code"], INVALID_PARAMS);
        let bad = ask(r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"echo","arguments":{"bad":1}}}"#)
            .await
            .unwrap();
        assert_eq!(bad["error"]["code"], INVALID_PARAMS);
        let nameless = ask(r#"{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{}}"#)
            .await
            .unwrap();
        assert_eq!(nameless["error"]["code"], INVALID_PARAMS);
        // Notificação desconhecida: sem resposta.
        assert_eq!(
            ask(r#"{"jsonrpc":"2.0","method":"notifications/cancelled"}"#).await,
            None
        );
    }

    #[tokio::test]
    async fn malformed_line_is_an_error() {
        let reply = ask("{não é json").await.unwrap();
        assert_eq!(reply["id"], Value::Null);
        assert_eq!(reply["error"]["code"], PARSE_ERROR);
        for bad in [
            r#"{"id":1,"method":"ping"}"#,
            r#"[{"jsonrpc":"2.0","id":1,"method":"ping"}]"#,
            "42",
        ] {
            let reply = ask(bad).await.unwrap();
            assert_eq!(reply["error"]["code"], INVALID_REQUEST, "{bad}");
        }
        assert_eq!(ask(r#"{"id":1,"method":"ping"}"#).await.unwrap()["id"], 1);
        // A resposta do cliente a um pedido nosso não gera nada.
        assert_eq!(ask(r#"{"jsonrpc":"2.0","id":5,"result":{}}"#).await, None);
    }

    #[tokio::test]
    async fn serve_answers_line_by_line() {
        let input = concat!(
            r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
            "\n\n",
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            "\n",
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
            "\n"
        );
        let mut output = Vec::new();
        serve(&Echo, input.as_bytes(), &mut output).await.unwrap();
        let lines: Vec<Value> = String::from_utf8(output)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(
            (lines[0]["id"].clone(), lines[1]["id"].clone()),
            (json!(1), json!(2))
        );
    }
}
