/// MCP tool handlers — one per exposed endpoint.
///
/// All handlers return only derived data. Raw content never appears in responses.
use std::sync::Arc;

use crate::consent::{AuditEvent, ConsentError, ConsentGate};
use crate::graph::GraphHandle;
use crate::signals::{process_ingest, IngestPayload};

pub const TOOL_SKILLS: &str = "strata_skills";
pub const TOOL_CONTEXT: &str = "strata_context";
pub const TOOL_PREFERENCES: &str = "strata_preferences";
pub const TOOL_INGEST: &str = "strata_ingest";

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
///
/// Response is categorized into three buckets:
/// - `skills`: technology/concept tags (no prefix)
/// - `work_types`: aggregated counts of `wt:` prefixed tags
/// - `domains`: `dt:` prefixed domain tags provided by AI tools
pub async fn handle_skills(
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;
    consent.record(AuditEvent::SkillQueried)?;
    let summary = graph.get_skill_summary()?;
    let all_skills = graph.get_top_skills(100)?;

    let mut skills = Vec::new();
    let mut work_types: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut domains = Vec::new();

    for node in all_skills {
        if let Some(wt) = node.tag.strip_prefix("wt:") {
            *work_types.entry(wt.to_string()).or_insert(0.0) += node.strength;
        } else if let Some(dt) = node.tag.strip_prefix("dt:") {
            domains.push(serde_json::json!({
                "tag": dt,
                "strength": node.strength,
                "session_count": node.session_count,
            }));
        } else {
            skills.push(node);
        }
    }

    Ok(serde_json::json!({
        "summary": summary.as_str(),
        "skills": skills,
        "work_types": work_types,
        "domains": domains,
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

    // Store topic summary if provided (max 50 retained, keyed by timestamp).
    // Key includes optional conversation_id so work units from the same chat cluster together.
    if let Some(ref summary) = signal.topic_summary {
        let conv_suffix = signal
            .conversation_id
            .as_deref()
            .map(|id| format!(":{}", id))
            .unwrap_or_default();
        let key = format!(
            "topic_summary:{}{}",
            signal.timestamp.timestamp_millis(),
            conv_suffix
        );
        graph.set_preference(&key, summary)?;
        evict_old_topic_summaries(graph, 50)?;
    }

    consent.record(AuditEvent::SkillIngested { count: tag_count })?;

    Ok(serde_json::json!({
        "ingested": tag_count,
        "tool": signal.tool_used,
    }))
}

/// Evict oldest topic summary preferences beyond `max_count`.
fn evict_old_topic_summaries(
    graph: &Arc<GraphHandle>,
    max_count: usize,
) -> Result<(), crate::graph::queries::GraphError> {
    let prefs = graph.get_preferences()?;
    let mut summary_keys: Vec<String> = prefs
        .0
        .keys()
        .filter(|k| k.starts_with("topic_summary:"))
        .cloned()
        .collect();

    if summary_keys.len() > max_count {
        // Keys include timestamp millis — sort ascending to find oldest.
        summary_keys.sort();
        let excess = summary_keys.len() - max_count;
        for key in summary_keys.into_iter().take(excess) {
            graph.delete_preference(&key)?;
        }
    }
    Ok(())
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
