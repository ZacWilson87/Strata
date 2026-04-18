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

/// A JSON-RPC 2.0 request (or notification — notifications have no `id`).
#[derive(Debug, serde::Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: serde_json::Value,
    pub method: String,
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    /// Returns true if this is a notification (no id = no response expected).
    pub fn is_notification(&self) -> bool {
        self.id.is_null()
    }
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
/// Returns `None` for notifications (no response should be sent).
pub async fn dispatch(
    req: JsonRpcRequest,
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Option<JsonRpcResponse> {
    // Notifications have no id — process but don't respond.
    if req.is_notification() {
        return None;
    }

    let id = req.id.clone();
    let params = req
        .params
        .unwrap_or(serde_json::Value::Object(Default::default()));

    let response = match req.method.as_str() {
        // Standard MCP lifecycle methods.
        "initialize" => JsonRpcResponse::ok(
            id,
            serde_json::json!({
                "protocolVersion": "2024-11-05",
                "capabilities": { "tools": {} },
                "serverInfo": { "name": "strata", "version": "0.1.0" }
            }),
        ),
        "tools/list" => JsonRpcResponse::ok(
            id,
            serde_json::json!({
                "tools": [
                    {
                        "name": tools::TOOL_SKILLS,
                        "description": "Returns your derived skill summary — ranked skills and strength scores. Never includes raw content.",
                        "inputSchema": { "type": "object", "properties": {} }
                    },
                    {
                        "name": tools::TOOL_CONTEXT,
                        "description": "Returns current session personalization context based on recent skill activity.",
                        "inputSchema": { "type": "object", "properties": {} }
                    },
                    {
                        "name": tools::TOOL_PREFERENCES,
                        "description": "Returns stored workflow preferences.",
                        "inputSchema": { "type": "object", "properties": {} }
                    },
                    {
                        "name": tools::TOOL_INGEST,
                        "description": "Ingests a workflow signal. Raw content is processed in-memory and discarded; only skill tags are persisted.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "tool_used": { "type": "string", "description": "Name of the AI tool used (e.g. 'claude', 'cursor')" },
                                "content": { "type": "string", "description": "Raw signal content — never stored, consumed in-memory only" },
                                "domain_hint": { "type": ["string", "null"], "description": "Optional domain hint to guide skill extraction" }
                            },
                            "required": ["tool_used", "content"]
                        }
                    }
                ]
            }),
        ),
        // Standard MCP tool invocation.
        "tools/call" => {
            let name = params
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));

            dispatch_tool(&name, args, id, graph, consent).await
        }
        // Legacy direct-method calls (kept for backwards compat with manual testing).
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
    };

    Some(response)
}

async fn dispatch_tool(
    name: &str,
    args: serde_json::Value,
    id: serde_json::Value,
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> JsonRpcResponse {
    match name {
        tools::TOOL_SKILLS => match tools::handle_skills(graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, serde_json::json!({ "content": [{ "type": "text", "text": result.to_string() }] })),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        tools::TOOL_CONTEXT => match tools::handle_context(graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, serde_json::json!({ "content": [{ "type": "text", "text": result.to_string() }] })),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        tools::TOOL_PREFERENCES => match tools::handle_preferences(graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, serde_json::json!({ "content": [{ "type": "text", "text": result.to_string() }] })),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        tools::TOOL_INGEST => match tools::handle_ingest(args, graph, consent).await {
            Ok(result) => JsonRpcResponse::ok(id, serde_json::json!({ "content": [{ "type": "text", "text": result.to_string() }] })),
            Err(e) => JsonRpcResponse::error(id, -32000, e.to_string()),
        },
        _ => JsonRpcResponse::error(id, -32601, format!("Unknown tool: {name}")),
    }
}
