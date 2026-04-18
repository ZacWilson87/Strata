/// JSON-RPC 2.0 request/response types and method dispatch.
use std::sync::Arc;

use crate::consent::ConsentGate;
use crate::graph::GraphHandle;
use crate::tools;

/// Errors that can occur in the server layer.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    #[error("I/O error: {0}")]
    Io(#[from] tokio::io::Error),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// A JSON-RPC 2.0 request.
#[derive(Debug, serde::Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

/// A JSON-RPC 2.0 response.
#[derive(Debug, serde::Serialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// A JSON-RPC 2.0 error object.
#[derive(Debug, serde::Serialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
}

impl JsonRpcResponse {
    pub fn ok(id: serde_json::Value, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: serde_json::Value, code: i32, message: String) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(JsonRpcError { code, message }),
        }
    }

    pub fn parse_error(detail: String) -> Self {
        Self::error(
            serde_json::Value::Null,
            -32700,
            format!("Parse error: {detail}"),
        )
    }
}

/// Dispatch a JSON-RPC request to the appropriate tool handler.
pub async fn dispatch(
    req: JsonRpcRequest,
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> JsonRpcResponse {
    let id = req.id.clone();
    let params = req
        .params
        .unwrap_or(serde_json::Value::Object(Default::default()));

    match req.method.as_str() {
        tools::TOOL_SKILLS => match tools::handle_skills(graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        tools::TOOL_CONTEXT => match tools::handle_context(graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        tools::TOOL_PREFERENCES => match tools::handle_preferences(graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        tools::TOOL_INGEST => match tools::handle_ingest(params, graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, result),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        _ => JsonRpcResponse::error(id, -32601, format!("Method not found: {}", req.method)),
    }
}
