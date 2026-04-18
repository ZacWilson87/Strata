/// MCP tool handlers — one per exposed endpoint.
///
/// All handlers return only derived data. Raw content never appears in responses.
use std::sync::Arc;

use crate::consent::{AuditEvent, ConsentError, ConsentGate};
use crate::graph::GraphHandle;
use crate::signals::{process_ingest, IngestPayload};

pub const TOOL_SKILLS: &str = "strata/skills";
pub const TOOL_CONTEXT: &str = "strata/context";
pub const TOOL_PREFERENCES: &str = "strata/preferences";
pub const TOOL_INGEST: &str = "strata/ingest";

/// Error type for tool handler failures.
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("consent error: {0}")]
    Consent(#[from] ConsentError),

    #[error("graph error: {0}")]
    Graph(#[from] crate::graph::queries::GraphError),

    #[error("invalid request: {0}")]
    BadRequest(String),

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Handle `strata/skills` — returns the user's top skill tags as a derived summary.
pub async fn handle_skills(
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;
    consent.record(AuditEvent::SkillQueried)?;
    let summary = graph.get_skill_summary()?;
    let skills = graph.get_top_skills(20)?;
    Ok(serde_json::json!({
        "summary": summary.as_str(),
        "skills": skills,
    }))
}

/// Handle `strata/context` — returns the current session personalization context.
pub async fn handle_context(
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;
    let context = graph.get_context_summary()?;
    Ok(serde_json::json!({
        "context": context.as_str(),
    }))
}

/// Handle `strata/preferences` — returns the user's stored workflow preferences.
pub async fn handle_preferences(
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;
    consent.record(AuditEvent::PreferencesQueried)?;
    let prefs = graph.get_preferences()?;
    Ok(serde_json::json!({ "preferences": prefs.0 }))
}

/// Handle `strata/ingest` — receives raw signals from AI clients, processes them, discards raw content.
pub async fn handle_ingest(
    params: serde_json::Value,
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;

    let payload: IngestPayload =
        serde_json::from_value(params).map_err(|e| ToolError::BadRequest(e.to_string()))?;

    // Raw content is consumed inside process_ingest — it cannot leak out.
    let signal = process_ingest(payload);
    let tag_count = signal.skill_tags.len();

    for tag in &signal.skill_tags {
        graph.upsert_skill(tag)?;
    }

    // Record co-occurrences for pairs of tags in this signal.
    let tags: Vec<_> = signal.skill_tags.clone();
    for i in 0..tags.len() {
        for j in (i + 1)..tags.len() {
            graph.record_co_occurrence(&tags[i], &tags[j])?;
        }
    }

    consent.record(AuditEvent::SkillIngested { count: tag_count })?;

    Ok(serde_json::json!({
        "ingested": tag_count,
        "tool": signal.tool_used,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::consent::ConsentGate;
    use crate::graph::GraphHandle;
    use crate::private_mode::SkillTag;

    fn make_handles() -> (Arc<GraphHandle>, Arc<ConsentGate>) {
        let graph = Arc::new(GraphHandle::open_in_memory().unwrap());
        let consent = Arc::new(ConsentGate::open_in_memory().unwrap());
        (graph, consent)
    }

    #[tokio::test]
    async fn skills_returns_no_raw_content() {
        let (graph, consent) = make_handles();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        let result = handle_skills(&graph, &consent).await.unwrap();
        let json = serde_json::to_string(&result).unwrap();
        // Must not contain any raw content markers
        assert!(!json.contains("prompt"));
        assert!(!json.contains("RawSignal"));
        assert!(json.contains("rust"));
    }

    #[tokio::test]
    async fn context_returns_derived_summary() {
        let (graph, consent) = make_handles();
        let result = handle_context(&graph, &consent).await.unwrap();
        assert!(result.get("context").is_some());
    }

    #[tokio::test]
    async fn preferences_returns_preferences_object() {
        let (graph, consent) = make_handles();
        graph.set_preference("theme", "dark").unwrap();
        let result = handle_preferences(&graph, &consent).await.unwrap();
        let prefs = result["preferences"].as_object().unwrap();
        assert_eq!(prefs["theme"].as_str().unwrap(), "dark");
    }

    #[tokio::test]
    async fn ingest_stores_skill_tags() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "write a rust async function",
            "domain_hint": null,
        });
        handle_ingest(params, &graph, &consent).await.unwrap();
        let skills = graph.get_top_skills(10).unwrap();
        let tags: Vec<&str> = skills.iter().map(|s| s.tag.as_str()).collect();
        assert!(tags.contains(&"rust"));
        assert!(tags.contains(&"async"));
    }

    #[tokio::test]
    async fn ingest_response_has_no_raw_content() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "PRIVATE_CONTENT_MARKER write some python",
            "domain_hint": null,
        });
        let result = handle_ingest(params, &graph, &consent).await.unwrap();
        let json = serde_json::to_string(&result).unwrap();
        assert!(!json.contains("PRIVATE_CONTENT_MARKER"));
    }

    #[tokio::test]
    async fn paused_consent_blocks_all_handlers() {
        let (graph, consent) = make_handles();
        consent.pause().unwrap();
        assert!(handle_skills(&graph, &consent).await.is_err());
        assert!(handle_context(&graph, &consent).await.is_err());
        assert!(handle_preferences(&graph, &consent).await.is_err());
        let params = serde_json::json!({"tool_used":"claude","content":"x","domain_hint":null});
        assert!(handle_ingest(params, &graph, &consent).await.is_err());
    }

    #[tokio::test]
    async fn ingest_bad_payload_returns_bad_request() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({ "wrong_field": 42 });
        let err = handle_ingest(params, &graph, &consent).await.unwrap_err();
        assert!(matches!(err, ToolError::BadRequest(_)));
    }
}
