/// End-to-end integration tests that simulate what Claude/Cursor see.
///
/// Each test:
/// 1. Spins up in-memory graph + consent gate
/// 2. Calls tool handlers directly (same path the MCP server uses)
/// 3. Asserts response structure and privacy invariants
use std::sync::Arc;

use strata::consent::ConsentGate;
use strata::graph::GraphHandle;
use strata::private_mode::SkillTag;
use strata::tools;

fn make_handles() -> (Arc<GraphHandle>, Arc<ConsentGate>) {
    let graph = Arc::new(GraphHandle::open_in_memory().unwrap());
    let consent = Arc::new(ConsentGate::open_in_memory().unwrap());
    (graph, consent)
}

// ── skills endpoint ───────────────────────────────────────────────────────────

#[tokio::test]
async fn skills_endpoint_empty_db_returns_derived_summary() {
    let (graph, consent) = make_handles();
    let result = tools::handle_skills(&graph, &consent).await.unwrap();
    assert!(result["summary"].is_string());
    assert!(result["skills"].is_array());
}

#[tokio::test]
async fn skills_endpoint_returns_derived_summary_only() {
    let (graph, consent) = make_handles();
    graph.upsert_skill(&SkillTag::new("rust")).unwrap();
    graph.upsert_skill(&SkillTag::new("async")).unwrap();

    let result = tools::handle_skills(&graph, &consent).await.unwrap();
    let json = serde_json::to_string(&result).unwrap();

    assert!(json.contains("rust"));
    assert!(!json.contains("RawSignal"));
    assert!(!json.contains("prompt"));
}

// ── ingest → skills round-trip ────────────────────────────────────────────────

#[tokio::test]
async fn ingest_then_query_skill_appears() {
    let (graph, consent) = make_handles();

    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "help me optimise this rust async function",
        "domain_hint": null,
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let result = tools::handle_skills(&graph, &consent).await.unwrap();
    let summary = result["summary"].as_str().unwrap();
    assert!(summary.contains("rust") || summary.contains("async"));
}

#[tokio::test]
async fn ingest_does_not_store_raw_content() {
    let (graph, consent) = make_handles();

    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "PRIVATE_MARKER_DO_NOT_STORE refactor my python tests",
        "domain_hint": null,
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let skills = tools::handle_skills(&graph, &consent).await.unwrap();
    let context = tools::handle_context(&graph, &consent).await.unwrap();
    let prefs = tools::handle_preferences(&graph, &consent).await.unwrap();

    for val in &[skills, context, prefs] {
        let json = serde_json::to_string(val).unwrap();
        assert!(
            !json.contains("PRIVATE_MARKER_DO_NOT_STORE"),
            "raw content leaked into response: {json}"
        );
    }
}

#[tokio::test]
async fn multiple_ingests_accumulate_skill_strength() {
    let (graph, consent) = make_handles();

    for _ in 0..3 {
        let params = serde_json::json!({
            "tool_used": "claude",
            "content": "rust programming",
            "domain_hint": null,
        });
        tools::handle_ingest(params, &graph, &consent)
            .await
            .unwrap();
    }

    let skills = graph.get_top_skills(10).unwrap();
    let rust = skills.iter().find(|s| s.tag == "rust").unwrap();
    assert!(rust.session_count >= 3);
    assert!(rust.strength >= 3.0);
}

// ── consent blocks all endpoints ──────────────────────────────────────────────

#[tokio::test]
async fn consent_revoked_blocks_all_reads() {
    let (graph, consent) = make_handles();
    graph.upsert_skill(&SkillTag::new("rust")).unwrap();
    consent.revoke(&graph).unwrap();

    let dummy_params = serde_json::json!({
        "tool_used": "claude",
        "content": "test",
        "domain_hint": null,
    });

    assert!(tools::handle_skills(&graph, &consent).await.is_err());
    assert!(tools::handle_context(&graph, &consent).await.is_err());
    assert!(tools::handle_preferences(&graph, &consent).await.is_err());
    assert!(tools::handle_ingest(dummy_params, &graph, &consent)
        .await
        .is_err());
}

#[tokio::test]
async fn consent_paused_blocks_ingestion() {
    let (graph, consent) = make_handles();
    consent.pause().unwrap();

    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "rust async",
        "domain_hint": null,
    });
    assert!(tools::handle_ingest(params, &graph, &consent)
        .await
        .is_err());
    assert!(graph.get_top_skills(10).unwrap().is_empty());
}

#[tokio::test]
async fn consent_resume_after_pause_allows_ingestion() {
    let (graph, consent) = make_handles();
    consent.pause().unwrap();
    consent.resume().unwrap();

    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "rust code",
        "domain_hint": null,
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();
    assert!(!graph.get_top_skills(10).unwrap().is_empty());
}

// ── context endpoint ──────────────────────────────────────────────────────────

#[tokio::test]
async fn context_endpoint_reflects_recent_ingestion() {
    let (graph, consent) = make_handles();

    let params = serde_json::json!({
        "tool_used": "cursor",
        "content": "typescript react component",
        "domain_hint": null,
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let result = tools::handle_context(&graph, &consent).await.unwrap();
    assert!(result["context"].is_string());
    let ctx = result["context"].as_str().unwrap();
    assert!(!ctx.is_empty());
}

// ── preferences endpoint ──────────────────────────────────────────────────────

#[tokio::test]
async fn preferences_endpoint_returns_stored_prefs() {
    let (graph, consent) = make_handles();
    graph.set_preference("language", "en").unwrap();
    graph.set_preference("theme", "dark").unwrap();

    let result = tools::handle_preferences(&graph, &consent).await.unwrap();
    let prefs = result["preferences"].as_object().unwrap();
    assert_eq!(prefs["language"].as_str().unwrap(), "en");
    assert_eq!(prefs["theme"].as_str().unwrap(), "dark");
}

// ── AI-as-taxonomizer: pre-classified ingest ─────────────────────────────────

#[tokio::test]
async fn preclassified_ingest_stores_work_type_tag() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "",
        "work_type": "analysis",
        "domain_tags": ["food_science", "fermentation"],
        "topic_summary": "optimizing Maillard reaction in plant-based proteins",
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let skills = graph.get_top_skills(50).unwrap();
    let tags: Vec<&str> = skills.iter().map(|s| s.tag.as_str()).collect();
    assert!(
        tags.contains(&"wt:analysis"),
        "work type tag missing: {tags:?}"
    );
    assert!(
        tags.contains(&"dt:food_science"),
        "domain tag missing: {tags:?}"
    );
    assert!(
        tags.contains(&"dt:fermentation"),
        "domain tag missing: {tags:?}"
    );
}

#[tokio::test]
async fn preclassified_ingest_empty_content_is_accepted() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "cursor",
        "work_type": "research",
        "domain_tags": ["quantum_physics", "entanglement"],
    });
    let result = tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();
    assert!(result["ingested"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn skills_response_separates_work_types_and_domains() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "rust async code",
        "work_type": "creation",
        "domain_tags": ["systems_programming"],
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let result = tools::handle_skills(&graph, &consent).await.unwrap();
    assert!(result["work_types"].is_object(), "work_types missing");
    assert!(result["domains"].is_array(), "domains missing");
    assert!(result["skills"].is_array(), "skills missing");

    let work_types = result["work_types"].as_object().unwrap();
    assert!(
        work_types.contains_key("creation"),
        "creation work type missing"
    );

    let domains: Vec<&str> = result["domains"]
        .as_array()
        .unwrap()
        .iter()
        .map(|d| d["tag"].as_str().unwrap())
        .collect();
    assert!(
        domains.contains(&"systems_programming"),
        "domain tag missing"
    );
}

#[tokio::test]
async fn topic_summary_stored_in_preferences() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "",
        "work_type": "analysis",
        "domain_tags": ["climate_science"],
        "topic_summary": "analyzing CO2 absorption rates in ocean data",
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let prefs = graph.get_preferences().unwrap();
    let has_summary = prefs
        .0
        .iter()
        .any(|(k, v)| k.starts_with("topic_summary:") && v.contains("CO2"));
    assert!(has_summary, "topic summary not stored in preferences");
}

#[tokio::test]
async fn structural_fallback_detects_work_type_from_content() {
    let (graph, consent) = make_handles();
    // No work_type provided — should detect "debugging" from content
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "there is an error in my code that is not working and I need to fix it",
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let skills = graph.get_top_skills(50).unwrap();
    let tags: Vec<&str> = skills.iter().map(|s| s.tag.as_str()).collect();
    assert!(
        tags.contains(&"wt:debugging"),
        "structural fallback should detect debugging: {tags:?}"
    );
}

#[tokio::test]
async fn topic_summary_never_leaks_to_skills_response() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "",
        "work_type": "research",
        "topic_summary": "PRIVATE_SUMMARY_MARKER investigating proprietary formula",
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let skills = tools::handle_skills(&graph, &consent).await.unwrap();
    let json = serde_json::to_string(&skills).unwrap();
    assert!(
        !json.contains("PRIVATE_SUMMARY_MARKER"),
        "topic summary leaked into skills response"
    );
}

// ── topic_summary validation ─────────────────────────────────────────────────

#[tokio::test]
async fn topic_summary_truncated_at_500_chars() {
    let (graph, consent) = make_handles();
    let long_summary = "x".repeat(600);
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "",
        "work_type": "analysis",
        "topic_summary": long_summary,
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let prefs = graph.get_preferences().unwrap();
    let stored = prefs
        .0
        .values()
        .find(|v| v.starts_with('x'))
        .expect("topic_summary should be stored");
    assert!(
        stored.chars().count() <= 500,
        "topic_summary was not truncated: {} chars",
        stored.chars().count()
    );
}

// ── tool usage tracking ───────────────────────────────────────────────────────

#[tokio::test]
async fn ingest_stores_tool_tag() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "cursor",
        "content": "",
        "work_type": "creation",
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let skills = tools::handle_skills(&graph, &consent).await.unwrap();
    let tool_usage = skills["tool_usage"].as_object().unwrap();
    assert!(
        tool_usage.contains_key("cursor"),
        "tool_usage should contain 'cursor'"
    );
}

// ── audit log read interface ──────────────────────────────────────────────────

#[tokio::test]
async fn audit_log_reflects_ingestion() {
    // ConsentGate and GraphHandle both write to the same strata.db in production;
    // in-memory DBs are separate, so this test requires a shared file.
    let tmp = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    let path = tmp.to_str().unwrap();
    let graph = Arc::new(GraphHandle::open(path).unwrap());
    let consent_conn = rusqlite::Connection::open(path).unwrap();
    let consent = Arc::new(strata::consent::ConsentGate::new(consent_conn).unwrap());

    let params = serde_json::json!({"tool_used": "claude", "content": "", "work_type": "review"});
    tools::handle_ingest(params.clone(), &graph, &consent)
        .await
        .unwrap();
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let log = graph.get_audit_log(20).unwrap();
    let ingested_count = log.iter().filter(|e| e.event == "skill_ingested").count();
    assert!(
        ingested_count >= 2,
        "should have at least 2 skill_ingested events"
    );
}

// ── skill history ─────────────────────────────────────────────────────────────

#[tokio::test]
async fn skill_history_returns_recent_snapshots() {
    let (graph, _consent) = make_handles();
    // Upsert some skills so there's data in the last 8 weeks.
    graph.upsert_skill(&SkillTag::new("rust")).unwrap();
    graph.upsert_skill(&SkillTag::new("async")).unwrap();

    let history = graph.get_skill_history(8).unwrap();
    // At minimum the current week should appear.
    assert!(
        !history.is_empty(),
        "skill history should be non-empty after upserts"
    );
    assert!(
        history
            .iter()
            .any(|s| s.top_tags.contains(&"rust".to_string())),
        "rust should appear in skill history"
    );
}

// ── privacy invariants (Phase 0 hardening) ───────────────────────────────────

/// Invariant: a pause made by one process (dashboard) must immediately block
/// ingestion in another process (MCP server) sharing the same database.
#[tokio::test]
async fn pause_from_dashboard_blocks_running_mcp_server() {
    let tmp = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    let path = tmp.to_str().unwrap();

    let graph = Arc::new(GraphHandle::open(path).unwrap());
    let server_gate =
        Arc::new(ConsentGate::new(rusqlite::Connection::open(path).unwrap()).unwrap());
    let dashboard_gate = ConsentGate::new(rusqlite::Connection::open(path).unwrap()).unwrap();

    let params = serde_json::json!({"tool_used": "claude", "content": "rust code"});
    tools::handle_ingest(params.clone(), &graph, &server_gate)
        .await
        .unwrap();

    // Dashboard pauses — the "server" gate must block WITHOUT a restart.
    dashboard_gate.pause().unwrap();
    assert!(
        tools::handle_ingest(params, &graph, &server_gate)
            .await
            .is_err(),
        "pause from another process must block ingestion immediately"
    );
}

/// Invariant: revocation deletes ALL collected data, including topic summaries
/// (stored in preferences) — the most sensitive data Strata holds.
#[tokio::test]
async fn revoke_wipes_topic_summaries_and_all_data() {
    let tmp = tempfile::NamedTempFile::new().unwrap().into_temp_path();
    let path = tmp.to_str().unwrap();
    let graph = Arc::new(GraphHandle::open(path).unwrap());
    let consent = Arc::new(ConsentGate::new(rusqlite::Connection::open(path).unwrap()).unwrap());

    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "rust async",
        "work_type": "creation",
        "domain_tags": ["systems_programming"],
        "topic_summary": "SENSITIVE_WORK_DESCRIPTION building auth flow",
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    consent.revoke(&graph).unwrap();

    assert!(graph.get_top_skills(100).unwrap().is_empty());
    assert!(
        graph.get_preferences().unwrap().0.is_empty(),
        "topic summaries must not survive revocation"
    );
    assert!(graph.get_skill_history(8).unwrap().is_empty());
}

/// Invariant: one ingest counts each skill exactly once, regardless of how
/// many other tags it co-occurs with.
#[tokio::test]
async fn single_ingest_counts_each_skill_once() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "rust async sql database python testing",
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    for node in graph.get_top_skills(100).unwrap() {
        assert_eq!(
            node.session_count, 1,
            "'{}' was counted {} times in a single ingest",
            node.tag, node.session_count
        );
    }
}

/// Invariant: clients cannot forge reserved namespace prefixes through any
/// user-supplied field.
#[tokio::test]
async fn forged_prefixes_rejected_end_to_end() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({
        "tool_used": "claude",
        "content": "",
        "domain_hint": "wt:debugging",
        "domain_tags": ["tool:fake", "wt:research"],
    });
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let result = tools::handle_skills(&graph, &consent).await.unwrap();
    let work_types = result["work_types"].as_object().unwrap();
    assert!(
        work_types.is_empty(),
        "forged work types leaked: {work_types:?}"
    );
    let tool_usage = result["tool_usage"].as_object().unwrap();
    assert!(
        !tool_usage.contains_key("fake"),
        "forged tool tag leaked: {tool_usage:?}"
    );
}

/// Skills response includes a recency-weighted strength alongside lifetime strength.
#[tokio::test]
async fn skills_response_includes_recent_strength() {
    let (graph, consent) = make_handles();
    let params = serde_json::json!({"tool_used": "claude", "content": "rust code"});
    tools::handle_ingest(params, &graph, &consent)
        .await
        .unwrap();

    let result = tools::handle_skills(&graph, &consent).await.unwrap();
    let rust = result["skills"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["tag"] == "rust")
        .expect("rust skill present");
    let recent = rust["recent_strength"].as_f64().unwrap();
    assert!(
        recent > 0.9,
        "today's activity should have ~full weight: {recent}"
    );
}

// ── JSON-RPC routing ──────────────────────────────────────────────────────────

#[tokio::test]
async fn unknown_method_returns_method_not_found() {
    use strata::server::router::{dispatch, JsonRpcRequest};

    let (graph, consent) = make_handles();

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: serde_json::json!(1),
        method: "strata_does_not_exist".into(),
        params: None,
    };
    let resp = dispatch(req, &graph, &consent).await.unwrap();
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32601);
}
