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

// ── JSON-RPC routing ──────────────────────────────────────────────────────────

#[tokio::test]
async fn unknown_method_returns_method_not_found() {
    use strata::server::router::{dispatch, JsonRpcRequest};

    let (graph, consent) = make_handles();

    let req = JsonRpcRequest {
        jsonrpc: "2.0".into(),
        id: serde_json::json!(1),
        method: "strata/does_not_exist".into(),
        params: None,
    };
    let resp = dispatch(req, &graph, &consent).await;
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, -32601);
}
