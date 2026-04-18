/// MCP server — JSON-RPC 2.0 over stdio.
///
/// Reads newline-delimited JSON requests from stdin, dispatches to tool handlers,
/// writes responses to stdout. This is the standard MCP transport used by
/// Claude Desktop and Cursor.
pub mod router;

use std::sync::Arc;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

use crate::consent::ConsentGate;
use crate::graph::GraphHandle;

pub use router::{JsonRpcError, JsonRpcRequest, JsonRpcResponse, ServerError};

/// The MCP server instance.
pub struct McpServer {
    pub graph: Arc<GraphHandle>,
    pub consent: Arc<ConsentGate>,
}

impl McpServer {
    pub fn new(graph: Arc<GraphHandle>, consent: Arc<ConsentGate>) -> Self {
        Self { graph, consent }
    }

    /// Run the server loop: read from stdin, write to stdout.
    pub async fn run(self) -> Result<(), ServerError> {
        let stdin = tokio::io::stdin();
        let mut stdout = tokio::io::stdout();
        let mut lines = BufReader::new(stdin).lines();

        tracing::info!("MCP server listening on stdio");

        while let Some(line) = lines.next_line().await? {
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }

            let response = match serde_json::from_str::<JsonRpcRequest>(&line) {
                Ok(req) => router::dispatch(req, &self.graph, &self.consent).await,
                Err(e) => JsonRpcResponse::parse_error(e.to_string()),
            };

            let mut json = serde_json::to_string(&response)?;
            json.push('\n');
            stdout.write_all(json.as_bytes()).await?;
            stdout.flush().await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tools::{TOOL_CONTEXT, TOOL_INGEST, TOOL_PREFERENCES, TOOL_SKILLS};

    fn make_server() -> McpServer {
        let graph = Arc::new(GraphHandle::open_in_memory().unwrap());
        let consent = Arc::new(ConsentGate::open_in_memory().unwrap());
        McpServer::new(graph, consent)
    }

    fn req(method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::Number(1.into()),
            method: method.into(),
            params: Some(params),
        }
    }

    #[tokio::test]
    async fn unknown_method_returns_error() {
        let server = make_server();
        let r = router::dispatch(
            req("nonexistent_method", serde_json::json!({})),
            &server.graph,
            &server.consent,
        )
        .await;
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn skills_method_returns_result() {
        let server = make_server();
        let r = router::dispatch(
            req(TOOL_SKILLS, serde_json::json!({})),
            &server.graph,
            &server.consent,
        )
        .await;
        assert!(r.error.is_none());
        assert!(r.result.is_some());
    }

    #[tokio::test]
    async fn context_method_returns_result() {
        let server = make_server();
        let r = router::dispatch(
            req(TOOL_CONTEXT, serde_json::json!({})),
            &server.graph,
            &server.consent,
        )
        .await;
        assert!(r.error.is_none());
    }

    #[tokio::test]
    async fn preferences_method_returns_result() {
        let server = make_server();
        let r = router::dispatch(
            req(TOOL_PREFERENCES, serde_json::json!({})),
            &server.graph,
            &server.consent,
        )
        .await;
        assert!(r.error.is_none());
    }

    #[tokio::test]
    async fn ingest_method_accepts_payload() {
        let server = make_server();
        let r = router::dispatch(
            req(
                TOOL_INGEST,
                serde_json::json!({ "tool_used": "claude", "content": "rust async code", "domain_hint": null }),
            ),
            &server.graph,
            &server.consent,
        )
        .await;
        assert!(r.error.is_none());
    }
}
