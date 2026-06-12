/// MCP tool handlers — one per exposed endpoint.
///
/// All handlers return only derived data. Raw content never appears in responses.
use std::sync::Arc;

use crate::consent::{AuditEvent, ConsentError, ConsentGate};
use crate::graph::{topic_summary_key, GraphHandle, TOPIC_SUMMARY_PREFIX, USER_PREF_PREFIX};
use crate::signals::{process_ingest, sanitize_preference_key, IngestPayload};

pub const TOOL_SKILLS: &str = "strata_skills";
pub const TOOL_CONTEXT: &str = "strata_context";
pub const TOOL_PREFERENCES: &str = "strata_preferences";
pub const TOOL_INGEST: &str = "strata_ingest";
pub const TOOL_SET_PREFERENCE: &str = "strata_set_preference";

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

/// Server-side cap on topic_summary length. Oversized strings are truncated, not rejected.
const MAX_TOPIC_SUMMARY_CHARS: usize = 500;

/// Handle `strata/skills` — returns the user's top skill tags as a derived summary.
///
/// Response is categorized into four buckets:
/// - `skills`: technology/concept tags (no prefix)
/// - `work_types`: aggregated counts of `wt:` prefixed tags
/// - `domains`: `dt:` prefixed domain tags provided by AI tools
/// - `tool_usage`: aggregated counts of `tool:` prefixed tags (which tools were used)
pub async fn handle_skills(
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;
    consent.record(AuditEvent::SkillQueried)?;
    let summary = graph.get_skill_summary()?;
    let all_skills = graph.get_top_skills(100)?;
    let recent_strengths = graph.get_recent_strengths()?;

    let mut skills = Vec::new();
    let mut work_types: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    let mut domains = Vec::new();
    let mut tool_usage: std::collections::HashMap<String, f64> = std::collections::HashMap::new();

    for node in all_skills {
        if let Some(wt) = node.tag.strip_prefix("wt:") {
            *work_types.entry(wt.to_string()).or_insert(0.0) += node.strength;
        } else if let Some(dt) = node.tag.strip_prefix("dt:") {
            domains.push(serde_json::json!({
                "tag": dt,
                "strength": node.strength,
                "session_count": node.session_count,
            }));
        } else if let Some(tool) = node.tag.strip_prefix("tool:") {
            *tool_usage.entry(tool.to_string()).or_insert(0.0) += node.strength;
        } else {
            let recent = recent_strengths.get(&node.tag).copied().unwrap_or(0.0);
            skills.push(serde_json::json!({
                "id": node.id,
                "tag": node.tag,
                "strength": node.strength,
                "recent_strength": recent,
                "last_seen": node.last_seen,
                "session_count": node.session_count,
            }));
        }
    }

    Ok(serde_json::json!({
        "summary": summary.as_str(),
        "skills": skills,
        "work_types": work_types,
        "domains": domains,
        "tool_usage": tool_usage,
    }))
}

/// How many top skills / domains / topics the context briefing includes.
/// Kept small on purpose — this payload is read at session start, so every
/// entry costs the user tokens in their AI tool.
const CONTEXT_TOP_SKILLS: usize = 8;
const CONTEXT_TOP_DOMAINS: usize = 5;
const CONTEXT_RECENT_TOPICS: usize = 5;
const CONTEXT_MAX_INSIGHTS: usize = 2;

/// Handle `strata/context` — a structured session-start briefing.
///
/// Assembles what the AI tool actually needs to start warm: recency-weighted
/// top skills, active domains and work mix from the last 30 days, the latest
/// topic summaries, stored user preferences, and current workflow insights.
/// The `context` field is a compact human-readable rendering of the same data
/// (kept for backwards compatibility and for models that prefer prose).
pub async fn handle_context(
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;
    consent.record(AuditEvent::ContextQueried)?;

    // Recency-weighted strengths, split by tag namespace.
    let strengths = graph.get_recent_strengths()?;
    let mut skills: Vec<(&str, f64)> = Vec::new();
    let mut domains: Vec<(&str, f64)> = Vec::new();
    for (tag, strength) in &strengths {
        if *strength <= 0.0 {
            continue;
        }
        if let Some(dt) = tag.strip_prefix("dt:") {
            domains.push((dt, *strength));
        } else if !tag.contains(':') {
            skills.push((tag, *strength));
        }
    }
    let by_strength = |a: &(&str, f64), b: &(&str, f64)| {
        b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal)
    };
    skills.sort_by(by_strength);
    domains.sort_by(by_strength);
    skills.truncate(CONTEXT_TOP_SKILLS);
    domains.truncate(CONTEXT_TOP_DOMAINS);

    // Work mix over the last 30 days of session signals.
    let mut work_mix: std::collections::BTreeMap<String, i64> = std::collections::BTreeMap::new();
    for row in graph.get_session_signals_since(30)? {
        if let Some(wt) = row.work_type {
            *work_mix.entry(wt).or_insert(0) += 1;
        }
    }

    let recent_topics: Vec<String> = graph
        .get_topic_summaries()?
        .into_iter()
        .take(CONTEXT_RECENT_TOPICS)
        .map(|t| t.summary)
        .collect();

    let preferences = user_preferences_map(graph)?;

    let insights: Vec<String> = graph
        .get_insights()?
        .into_iter()
        .take(CONTEXT_MAX_INSIGHTS)
        .map(|i| format!("{} — {}", i.title, i.evidence))
        .collect();

    // Compact prose rendering of the same data.
    let round1 = |v: f64| (v * 10.0).round() / 10.0;
    let mut lines: Vec<String> = Vec::new();
    if !skills.is_empty() {
        let list: Vec<String> = skills
            .iter()
            .map(|(t, s)| format!("{t} ({})", round1(*s)))
            .collect();
        lines.push(format!(
            "Top skills (recency-weighted): {}",
            list.join(", ")
        ));
    }
    if !domains.is_empty() {
        let list: Vec<String> = domains.iter().map(|(t, _)| t.to_string()).collect();
        lines.push(format!("Active domains: {}", list.join(", ")));
    }
    if !work_mix.is_empty() {
        let list: Vec<String> = work_mix.iter().map(|(k, v)| format!("{k} {v}")).collect();
        lines.push(format!("Work mix (30d): {}", list.join(", ")));
    }
    if !recent_topics.is_empty() {
        lines.push(format!("Recent topics: {}", recent_topics.join("; ")));
    }
    if !preferences.is_empty() {
        let list: Vec<String> = preferences
            .iter()
            .map(|(k, v)| format!("{k}: {v}"))
            .collect();
        lines.push(format!(
            "User preferences (follow these): {}",
            list.join("; ")
        ));
    }
    if !insights.is_empty() {
        lines.push(format!("Workflow watch-outs: {}", insights.join(" | ")));
    }
    let context = if lines.is_empty() {
        "No context available yet.".to_string()
    } else {
        lines.join("\n")
    };

    Ok(serde_json::json!({
        "context": context,
        "skills": skills
            .iter()
            .map(|(t, s)| serde_json::json!({ "tag": t, "recent_strength": round1(*s) }))
            .collect::<Vec<_>>(),
        "domains": domains.iter().map(|(t, _)| *t).collect::<Vec<_>>(),
        "work_mix_30d": work_mix,
        "recent_topics": recent_topics,
        "preferences": preferences,
        "insights": insights,
    }))
}

/// Handle `strata/preferences` — returns the user's stored workflow
/// preferences (the `pref:` namespace, keys stripped). Strata's internal
/// preference storage (topic summaries, insight dismissals) is not exposed.
pub async fn handle_preferences(
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;
    consent.record(AuditEvent::PreferencesQueried)?;
    Ok(serde_json::json!({ "preferences": user_preferences_map(graph)? }))
}

/// User preferences as a sorted key → value map with the namespace stripped.
fn user_preferences_map(
    graph: &GraphHandle,
) -> Result<std::collections::BTreeMap<String, String>, ToolError> {
    Ok(graph
        .get_preferences_with_prefix(USER_PREF_PREFIX)?
        .into_iter()
        .filter_map(|(k, v)| k.strip_prefix(USER_PREF_PREFIX).map(|k| (k.to_string(), v)))
        .collect())
}

/// Server-side caps for the preference write path.
const MAX_PREFERENCE_VALUE_CHARS: usize = 500;
const MAX_USER_PREFERENCES: usize = 100;

#[derive(serde::Deserialize)]
struct SetPreferencePayload {
    key: String,
    #[serde(default)]
    value: String,
}

/// Handle `strata/set_preference` — store (or clear) a durable user workflow
/// preference. This is the cross-tool memory write path: a preference stated
/// once in any connected AI tool is served to every other tool via
/// `strata_preferences` and the `strata_context` briefing.
///
/// An empty `value` clears the preference. Keys are validated (lowercase
/// `[a-z0-9_.-]`, ≤64 chars — colons rejected so clients cannot write into
/// internal namespaces); values are truncated at 500 chars; at most 100
/// preferences are kept.
pub async fn handle_set_preference(
    params: serde_json::Value,
    graph: &Arc<GraphHandle>,
    consent: &Arc<ConsentGate>,
) -> Result<serde_json::Value, ToolError> {
    consent.check()?;

    let payload: SetPreferencePayload =
        serde_json::from_value(params).map_err(|e| ToolError::BadRequest(e.to_string()))?;
    let key = sanitize_preference_key(&payload.key).ok_or_else(|| {
        ToolError::BadRequest(
            "invalid preference key: use lowercase letters, digits, '_', '-', '.' (max 64 chars)"
                .into(),
        )
    })?;
    let stored_key = format!("{USER_PREF_PREFIX}{key}");

    let value = payload.value.trim();
    if value.is_empty() {
        graph.delete_preference(&stored_key)?;
        consent.record(AuditEvent::PreferenceSet {
            key: key.clone(),
            cleared: true,
        })?;
        return Ok(serde_json::json!({ "key": key, "status": "cleared" }));
    }

    let value: String = value.chars().take(MAX_PREFERENCE_VALUE_CHARS).collect();
    let existing = graph.get_preferences_with_prefix(USER_PREF_PREFIX)?;
    let is_new = !existing.iter().any(|(k, _)| k == &stored_key);
    if is_new && existing.len() >= MAX_USER_PREFERENCES {
        return Err(ToolError::BadRequest(format!(
            "preference limit reached ({MAX_USER_PREFERENCES}) — clear one before adding more"
        )));
    }

    graph.set_preference(&stored_key, &value)?;
    consent.record(AuditEvent::PreferenceSet {
        key: key.clone(),
        cleared: false,
    })?;
    Ok(serde_json::json!({ "key": key, "status": "stored" }))
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
    let tags = &signal.skill_tags;
    for i in 0..tags.len() {
        for j in (i + 1)..tags.len() {
            graph.record_co_occurrence(&tags[i], &tags[j])?;
        }
    }

    // Store topic summary if provided (max 50 retained, keyed by timestamp).
    // Key includes optional conversation_id so work units from the same chat cluster together.
    // Truncate at MAX_TOPIC_SUMMARY_CHARS to enforce a server-side length cap.
    if let Some(ref raw_summary) = signal.topic_summary {
        let summary: String = if raw_summary.chars().count() > MAX_TOPIC_SUMMARY_CHARS {
            raw_summary.chars().take(MAX_TOPIC_SUMMARY_CHARS).collect()
        } else {
            raw_summary.clone()
        };
        let key = topic_summary_key(
            signal.timestamp.timestamp_millis(),
            signal.conversation_id.as_deref(),
        );
        graph.set_preference(&key, &summary)?;
        evict_old_topic_summaries(graph, 50)?;
    }

    // Record a session-signal row for the insights engine when the AI tool
    // reported any derived workflow signals (ADR 0005).
    if !signal.friction_signals.is_empty()
        || !signal.features_used.is_empty()
        || signal.outcome.is_some()
    {
        let row = crate::graph::SessionSignalRow {
            day: signal.timestamp.date_naive().to_string(),
            tool: signal.tool_used.clone(),
            work_type: signal
                .skill_tags
                .iter()
                .find_map(|t| t.as_str().strip_prefix("wt:").map(str::to_string)),
            domains: signal
                .skill_tags
                .iter()
                .filter_map(|t| t.as_str().strip_prefix("dt:").map(str::to_string))
                .collect(),
            friction: signal.friction_signals.clone(),
            features: signal.features_used.clone(),
            outcome: signal.outcome.clone(),
        };
        graph.record_session_signal(&row)?;
    }

    consent.record(AuditEvent::SkillIngested {
        count: tag_count,
        tool: signal.tool_used.clone(),
    })?;

    Ok(serde_json::json!({
        "ingested": tag_count,
        "tool": signal.tool_used,
    }))
}

/// Evict oldest topic summary preferences beyond `max_count`.
fn evict_old_topic_summaries(
    graph: &GraphHandle,
    max_count: usize,
) -> Result<(), crate::graph::queries::GraphError> {
    let mut summary_keys: Vec<String> = graph
        .get_preferences_with_prefix(TOPIC_SUMMARY_PREFIX)?
        .into_iter()
        .map(|(key, _)| key)
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
        assert_eq!(
            result["context"].as_str().unwrap(),
            "No context available yet."
        );
    }

    #[tokio::test]
    async fn context_briefing_includes_skills_preferences_and_topics() {
        let (graph, consent) = make_handles();
        graph.upsert_skill(&SkillTag::new("rust")).unwrap();
        graph
            .upsert_skill(&SkillTag::new("dt:mcp-protocol"))
            .unwrap();
        graph
            .set_preference("pref:commit_style", "no emojis in commit messages")
            .unwrap();
        graph
            .set_preference("topic_summary:1700000000000", "built the ingest pipeline")
            .unwrap();

        let result = handle_context(&graph, &consent).await.unwrap();
        let context = result["context"].as_str().unwrap();
        assert!(context.contains("rust"), "skills missing: {context}");
        assert!(
            context.contains("mcp-protocol"),
            "domains missing: {context}"
        );
        assert!(
            context.contains("no emojis in commit messages"),
            "preferences missing: {context}"
        );
        assert!(
            context.contains("built the ingest pipeline"),
            "topics missing: {context}"
        );

        // Structured fields mirror the prose.
        assert_eq!(result["domains"][0].as_str(), Some("mcp-protocol"));
        assert_eq!(
            result["preferences"]["commit_style"].as_str(),
            Some("no emojis in commit messages")
        );
        // Internal namespaces never leak as preference keys.
        assert!(result["preferences"]
            .get("topic_summary:1700000000000")
            .is_none());
    }

    #[tokio::test]
    async fn context_excludes_tool_and_worktype_tags_from_skills() {
        let (graph, consent) = make_handles();
        graph
            .upsert_skill(&SkillTag::new("tool:claude-code"))
            .unwrap();
        graph.upsert_skill(&SkillTag::new("wt:debugging")).unwrap();
        let result = handle_context(&graph, &consent).await.unwrap();
        assert!(result["skills"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn preferences_returns_only_user_namespace_stripped() {
        let (graph, consent) = make_handles();
        graph.set_preference("pref:theme", "dark").unwrap();
        graph.set_preference("topic_summary:1", "internal").unwrap();
        graph.set_preference("insight_dismissed:x", "ts").unwrap();
        let result = handle_preferences(&graph, &consent).await.unwrap();
        let prefs = result["preferences"].as_object().unwrap();
        assert_eq!(prefs["theme"].as_str().unwrap(), "dark");
        assert_eq!(prefs.len(), 1, "internal keys must not be exposed");
    }

    #[tokio::test]
    async fn set_preference_roundtrips_through_preferences_and_context() {
        let (graph, consent) = make_handles();
        let result = handle_set_preference(
            serde_json::json!({ "key": "Commit_Style", "value": "no emojis" }),
            &graph,
            &consent,
        )
        .await
        .unwrap();
        assert_eq!(result["status"].as_str(), Some("stored"));
        assert_eq!(
            result["key"].as_str(),
            Some("commit_style"),
            "key normalised"
        );

        let prefs = handle_preferences(&graph, &consent).await.unwrap();
        assert_eq!(
            prefs["preferences"]["commit_style"].as_str(),
            Some("no emojis")
        );
        let context = handle_context(&graph, &consent).await.unwrap();
        assert!(context["context"].as_str().unwrap().contains("no emojis"));
    }

    #[tokio::test]
    async fn set_preference_empty_value_clears() {
        let (graph, consent) = make_handles();
        for value in ["keep it brief", ""] {
            handle_set_preference(
                serde_json::json!({ "key": "verbosity", "value": value }),
                &graph,
                &consent,
            )
            .await
            .unwrap();
        }
        let prefs = handle_preferences(&graph, &consent).await.unwrap();
        assert!(prefs["preferences"].as_object().unwrap().is_empty());
    }

    #[tokio::test]
    async fn set_preference_rejects_namespace_forgery_and_bad_keys() {
        let (graph, consent) = make_handles();
        for key in ["topic_summary:1", "pref:nested", "has spaces", "", "x:y"] {
            let err = handle_set_preference(
                serde_json::json!({ "key": key, "value": "v" }),
                &graph,
                &consent,
            )
            .await
            .unwrap_err();
            assert!(
                matches!(err, ToolError::BadRequest(_)),
                "key {key:?} must be rejected"
            );
        }
        // Nothing was written under any namespace.
        assert!(graph.get_preferences().unwrap().0.is_empty());
    }

    #[tokio::test]
    async fn set_preference_caps_value_and_count() {
        let (graph, consent) = make_handles();
        // Oversized value is truncated, not rejected.
        handle_set_preference(
            serde_json::json!({ "key": "long", "value": "x".repeat(2000) }),
            &graph,
            &consent,
        )
        .await
        .unwrap();
        let prefs = handle_preferences(&graph, &consent).await.unwrap();
        assert_eq!(
            prefs["preferences"]["long"]
                .as_str()
                .unwrap()
                .chars()
                .count(),
            500
        );

        // 101st distinct key is rejected; updating an existing key still works.
        for i in 1..100 {
            handle_set_preference(
                serde_json::json!({ "key": format!("k{i}"), "value": "v" }),
                &graph,
                &consent,
            )
            .await
            .unwrap();
        }
        let err = handle_set_preference(
            serde_json::json!({ "key": "overflow", "value": "v" }),
            &graph,
            &consent,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::BadRequest(_)));
        handle_set_preference(
            serde_json::json!({ "key": "long", "value": "updated" }),
            &graph,
            &consent,
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn set_preference_blocked_when_consent_paused() {
        let (graph, consent) = make_handles();
        consent.pause().unwrap();
        let err = handle_set_preference(
            serde_json::json!({ "key": "style", "value": "v" }),
            &graph,
            &consent,
        )
        .await
        .unwrap_err();
        assert!(matches!(err, ToolError::Consent(_)));
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

    #[tokio::test]
    async fn ingest_records_co_occurrences_between_tags() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "rust async database",
        });
        handle_ingest(params, &graph, &consent).await.unwrap();
        let skills = graph.get_top_skills(10).unwrap();
        // At least rust, async, sql-adjacent tags should be stored.
        assert!(
            skills.len() >= 2,
            "expected multiple skill tags from multi-keyword content"
        );
    }

    #[tokio::test]
    async fn ingest_with_all_optional_fields() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "",
            "work_type": "analysis",
            "domain_tags": ["food_science", "fermentation"],
            "topic_summary": "analyzing fermentation kinetics",
            "conversation_id": "conv-xyz",
        });
        let result = handle_ingest(params, &graph, &consent).await.unwrap();
        assert!(result["ingested"].as_u64().unwrap() > 0);

        let skills = graph.get_top_skills(100).unwrap();
        let tags: Vec<&str> = skills.iter().map(|s| s.tag.as_str()).collect();
        assert!(tags.contains(&"wt:analysis"), "work type tag missing");
        assert!(tags.contains(&"dt:food_science"), "domain tag missing");
        assert!(tags.contains(&"dt:fermentation"), "domain tag missing");

        let prefs = graph.get_preferences().unwrap();
        let has_summary = prefs
            .0
            .iter()
            .any(|(k, v)| k.starts_with("topic_summary:") && v.contains("fermentation"));
        assert!(has_summary, "topic summary should be stored in preferences");
    }

    #[tokio::test]
    async fn ingest_domain_tags_stored_with_dt_prefix() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({
            "tool_used": "cursor",
            "content": "",
            "domain_tags": ["medicine", "pharmacology"],
        });
        handle_ingest(params, &graph, &consent).await.unwrap();
        let skills = graph.get_top_skills(100).unwrap();
        let tags: Vec<&str> = skills.iter().map(|s| s.tag.as_str()).collect();
        assert!(tags.contains(&"dt:medicine"));
        assert!(tags.contains(&"dt:pharmacology"));
    }

    #[tokio::test]
    async fn ingest_topic_summary_stored_in_preferences() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "",
            "topic_summary": "refactoring async error handling",
        });
        handle_ingest(params, &graph, &consent).await.unwrap();
        let prefs = graph.get_preferences().unwrap();
        let stored = prefs
            .0
            .values()
            .any(|v| v == "refactoring async error handling");
        assert!(stored, "topic summary should appear in preferences");
    }

    #[tokio::test]
    async fn evict_old_topic_summaries_keeps_max_50() {
        let (graph, consent) = make_handles();
        // Pre-populate 50 topic_summary preferences directly (simulating prior ingests).
        for i in 0..50u64 {
            graph
                .set_preference(&format!("topic_summary:{i:016}"), &format!("summary {i}"))
                .unwrap();
        }
        // One more ingest with a topic_summary should trigger eviction down to 50.
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "",
            "topic_summary": "the 51st summary",
        });
        handle_ingest(params, &graph, &consent).await.unwrap();

        let prefs = graph.get_preferences().unwrap();
        let count = prefs
            .0
            .keys()
            .filter(|k| k.starts_with("topic_summary:"))
            .count();
        assert_eq!(count, 50, "should retain exactly 50 topic summaries");
    }

    #[tokio::test]
    async fn skills_response_separates_work_types_and_domains() {
        let (graph, consent) = make_handles();
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "rust code",
            "work_type": "debugging",
            "domain_tags": ["embedded_systems"],
        });
        handle_ingest(params, &graph, &consent).await.unwrap();

        let result = handle_skills(&graph, &consent).await.unwrap();
        let skills_arr = result["skills"].as_array().unwrap();
        let work_types = result["work_types"].as_object().unwrap();
        let domains = result["domains"].as_array().unwrap();

        // "rust" belongs in skills (no prefix)
        assert!(skills_arr.iter().any(|s| s["tag"].as_str() == Some("rust")));
        // "wt:debugging" should appear in work_types keyed as "debugging"
        assert!(
            work_types.contains_key("debugging"),
            "work_types missing 'debugging'"
        );
        // "dt:embedded_systems" should appear in domains keyed as "embedded_systems"
        assert!(
            domains
                .iter()
                .any(|d| d["tag"].as_str() == Some("embedded_systems")),
            "domains missing 'embedded_systems'"
        );
    }
}
