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
                        "description": "Records a completed work unit. Call once per discrete task — when you finish a feature, resolve a bug, complete a research query, or when the topic shifts significantly. Multiple calls per conversation are expected and correct. Raw content is never stored.",
                        "inputSchema": {
                            "type": "object",
                            "properties": {
                                "tool_used": { "type": "string", "description": "Name of the AI tool used (e.g. 'claude', 'cursor')" },
                                "content": { "type": "string", "description": "Raw signal content — never stored. May be empty when pre-classified fields are provided." },
                                "domain_hint": { "type": ["string", "null"], "description": "Optional domain hint to guide skill extraction" },
                                "work_type": { "type": ["string", "null"], "description": "Work type pre-classified by the AI tool. One of: research, analysis, creation, debugging, review, planning" },
                                "domain_tags": { "type": ["array", "null"], "items": { "type": "string" }, "description": "Domain tags pre-classified by the AI tool (e.g. ['food_science', 'fermentation']). Universal — any domain." },
                                "topic_summary": { "type": ["string", "null"], "maxLength": 500, "description": "One-sentence derived summary from the AI tool. No PII, no raw content. Truncated server-side at 500 chars. Max 50 retained." },
                                "conversation_id": { "type": ["string", "null"], "description": "Optional stable identifier for the conversation. Groups multiple work units from the same chat." },
                                "friction_signals": { "type": ["array", "null"], "items": { "type": "string", "enum": ["repeated_context", "many_corrections", "restarted_approach", "manual_repetition", "context_lost"] }, "description": "Derived friction observed in this work unit: repeated_context (user had to re-explain project/context), many_corrections (output corrected 3+ times), restarted_approach (an approach was abandoned and redone), manual_repetition (user manually repeated a mechanical task the tool could do), context_lost (the session lost earlier context). Report only clear cases." },
                                "features_used": { "type": ["array", "null"], "items": { "type": "string" }, "description": "Tool capabilities this work unit exercised (e.g. plan_mode, subagents, hooks, code_review). Lowercase snake_case names." },
                                "outcome": { "type": ["string", "null"], "enum": ["resolved", "partial", "unresolved", null], "description": "How the work unit ended: resolved (goal achieved), partial (progress but incomplete), unresolved (abandoned or blocked)." }
                            },
                            "required": ["tool_used"]
                        }
                    }
                ]
            }),
        ),
        // Standard MCP tool invocation — wraps the result in a content envelope.
        "tools/call" => {
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let args = params
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(Default::default()));

            match call_tool(name, args, graph, consent).await {
                Ok(result) => JsonRpcResponse::ok(
                    id,
                    serde_json::json!({ "content": [{ "type": "text", "text": result.to_string() }] }),
                ),
                Err(e) => JsonRpcResponse::error(id, e.code, e.message),
            }
        }
        // Legacy direct-method calls (kept for backwards compat with manual
        // testing) — return the raw result without the MCP envelope.
        tools::TOOL_SKILLS | tools::TOOL_CONTEXT | tools::TOOL_PREFERENCES | tools::TOOL_INGEST => {
            match call_tool(&req.method, params, graph, consent).await {
                Ok(result) => JsonRpcResponse::ok(id, result),
                Err(e) => JsonRpcResponse::error(id, e.code, e.message),
            }
        }
        _ => JsonRpcResponse::error(id, -32601, format!("Method not found: {}", req.method)),
    };

    Some(response)
}

/// Invoke a tool handler by name and return its raw result value.
async fn call_tool(
    name: &str,
    args: serde_json::Value,
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, JsonRpcError> {
    let result = match name {
        tools::TOOL_SKILLS => tools::handle_skills(graph, consent).await,
        tools::TOOL_CONTEXT => tools::handle_context(graph, consent).await,
        tools::TOOL_PREFERENCES => tools::handle_preferences(graph, consent).await,
        tools::TOOL_INGEST => tools::handle_ingest(args, graph, consent).await,
        _ => {
            return Err(JsonRpcError {
                code: -32601,
                message: format!("Unknown tool: {name}"),
            })
        }
    };
    result.map_err(|e| JsonRpcError {
        code: -32000,
        message: e.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::ConsentGate;
    use crate::graph::GraphHandle;
    use crate::tools::{TOOL_CONTEXT, TOOL_INGEST, TOOL_PREFERENCES, TOOL_SKILLS};
    use std::sync::Arc;

    fn make_handles() -> (Arc<GraphHandle>, Arc<ConsentGate>) {
        let graph = Arc::new(GraphHandle::open_in_memory().unwrap());
        let consent = Arc::new(ConsentGate::open_in_memory().unwrap());
        (graph, consent)
    }

    fn req(id: serde_json::Value, method: &str, params: serde_json::Value) -> JsonRpcRequest {
        JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id,
            method: method.into(),
            params: Some(params),
        }
    }

    #[test]
    fn is_notification_true_when_id_is_null() {
        let r = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::Null,
            method: "notifications/initialized".into(),
            params: None,
        };
        assert!(r.is_notification());
    }

    #[test]
    fn is_notification_false_when_id_is_number() {
        let r = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::Number(1.into()),
            method: "tools/list".into(),
            params: None,
        };
        assert!(!r.is_notification());
    }

    #[tokio::test]
    async fn notification_dispatch_returns_none() {
        let (graph, consent) = make_handles();
        let notif = JsonRpcRequest {
            jsonrpc: "2.0".into(),
            id: serde_json::Value::Null,
            method: "notifications/initialized".into(),
            params: None,
        };
        let result = dispatch(notif, &graph, &consent).await;
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn initialize_method_returns_protocol_version() {
        let (graph, consent) = make_handles();
        let r = dispatch(
            req(serde_json::json!(1), "initialize", serde_json::json!({})),
            &graph,
            &consent,
        )
        .await
        .unwrap();
        assert!(r.error.is_none());
        let result = r.result.unwrap();
        assert_eq!(result["protocolVersion"].as_str(), Some("2024-11-05"));
        assert!(result["capabilities"].is_object());
        assert_eq!(result["serverInfo"]["name"].as_str(), Some("strata"));
    }

    #[tokio::test]
    async fn tools_list_returns_all_four_tools() {
        let (graph, consent) = make_handles();
        let r = dispatch(
            req(serde_json::json!(1), "tools/list", serde_json::json!({})),
            &graph,
            &consent,
        )
        .await
        .unwrap();
        assert!(r.error.is_none());
        let tools = r.result.unwrap()["tools"].as_array().unwrap().clone();
        let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
        assert!(names.contains(&TOOL_SKILLS));
        assert!(names.contains(&TOOL_CONTEXT));
        assert!(names.contains(&TOOL_PREFERENCES));
        assert!(names.contains(&TOOL_INGEST));
        assert_eq!(names.len(), 4);
    }

    #[tokio::test]
    async fn tools_call_routes_to_skills() {
        let (graph, consent) = make_handles();
        let r = dispatch(
            req(
                serde_json::json!(42),
                "tools/call",
                serde_json::json!({ "name": TOOL_SKILLS, "arguments": {} }),
            ),
            &graph,
            &consent,
        )
        .await
        .unwrap();
        assert!(r.error.is_none());
        // tools/call wraps result in MCP content envelope
        let content = &r.result.unwrap()["content"];
        assert!(content.is_array());
    }

    #[tokio::test]
    async fn tools_call_unknown_tool_returns_error() {
        let (graph, consent) = make_handles();
        let r = dispatch(
            req(
                serde_json::json!(1),
                "tools/call",
                serde_json::json!({ "name": "nonexistent_tool", "arguments": {} }),
            ),
            &graph,
            &consent,
        )
        .await
        .unwrap();
        assert!(r.error.is_some());
        assert_eq!(r.error.unwrap().code, -32601);
    }

    #[tokio::test]
    async fn response_id_matches_request_id() {
        let (graph, consent) = make_handles();
        let r = dispatch(
            req(serde_json::json!(99), TOOL_SKILLS, serde_json::json!({})),
            &graph,
            &consent,
        )
        .await
        .unwrap();
        assert_eq!(r.id, serde_json::json!(99));
    }
}
