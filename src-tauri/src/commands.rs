/// Tauri IPC command handlers — bridge between the React frontend and the Rust graph.
///
/// All commands return only derived data. Raw content is never exposed to the frontend.
use std::sync::Arc;

use strata::consent::ConsentGate;
use strata::graph::GraphHandle;
use strata::tools;

/// Shared app state passed to Tauri commands.
pub struct AppState {
    pub graph: Arc<GraphHandle>,
    pub consent: Arc<ConsentGate>,
}

#[tauri::command]
pub async fn get_skills(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    tools::handle_skills(&state.graph, &state.consent)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_context(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    tools::handle_context(&state.graph, &state.consent)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_preferences(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    tools::handle_preferences(&state.graph, &state.consent)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn ingest_signal(
    params: serde_json::Value,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    tools::handle_ingest(params, &state.graph, &state.consent)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_consent_status(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state
        .consent
        .status()
        .map(|s| s.to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn pause_consent(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.consent.pause().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn resume_consent(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.consent.resume().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn revoke_consent(state: tauri::State<'_, AppState>) -> Result<(), String> {
    state
        .consent
        .revoke(&state.graph)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_audit_log(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let entries = state.graph.get_audit_log(50).map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "entries": entries }))
}

#[tauri::command]
pub async fn get_skill_history(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let snapshots = state
        .graph
        .get_skill_history(8)
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "weeks": snapshots }))
}

#[tauri::command]
pub async fn get_growth(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let skills = state
        .graph
        .get_skills_with_velocity(30)
        .map_err(|e| e.to_string())?;
    let recent_strengths = state
        .graph
        .get_recent_strengths()
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "skills": skills, "recent_strengths": recent_strengths }))
}

#[tauri::command]
pub async fn get_insights(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let insights = state.graph.get_insights().map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "insights": insights }))
}

#[tauri::command]
pub async fn dismiss_insight(id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    // Id validation is enforced once, in the graph layer (`GraphHandle::dismiss_insight`).
    state.graph.dismiss_insight(&id).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_topic_summaries(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let summaries = state
        .graph
        .get_topic_summaries()
        .map_err(|e| e.to_string())?;
    Ok(serde_json::json!({ "summaries": summaries }))
}

/// Aggregate session-mechanics summary over the last 90 days (ADR 0008).
/// Pure numbers computed from locally-derived metrics — no content.
#[tauri::command]
pub async fn get_session_mechanics(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    const WINDOW_DAYS: i64 = 90;
    let rows = state
        .graph
        .get_session_metrics_since(WINDOW_DAYS)
        .map_err(|e| e.to_string())?;

    let n = rows.len();
    let avg = |f: &dyn Fn(&strata::graph::SessionMetricsRow) -> f64| -> f64 {
        if n == 0 {
            0.0
        } else {
            (rows.iter().map(f).sum::<f64>() / n as f64 * 10.0).round() / 10.0
        }
    };
    let mut durations: Vec<f64> = rows.iter().map(|r| r.duration_min).collect();
    durations.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let median_duration = if n == 0 { 0.0 } else { durations[n / 2] };
    let interrupted = rows.iter().filter(|r| r.interruptions > 0).count();
    let tool_calls: i64 = rows.iter().map(|r| r.tool_calls).sum();
    let tool_errors: i64 = rows.iter().map(|r| r.tool_errors).sum();

    Ok(serde_json::json!({
        "window_days": WINDOW_DAYS,
        "sessions": n,
        "avg_prompts": avg(&|r| r.prompts as f64),
        "median_duration_min": median_duration,
        "interrupted_sessions": interrupted,
        "tool_calls": tool_calls,
        "tool_errors": tool_errors,
        "avg_first_prompt_chars": avg(&|r| r.first_prompt_chars as f64),
    }))
}

/// Store (or update) a user workflow preference. Same validated write path
/// AI clients use via the `strata_set_preference` MCP tool.
#[tauri::command]
pub async fn set_user_preference(
    key: String,
    value: String,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    tools::handle_set_preference(
        serde_json::json!({ "key": key, "value": value }),
        &state.graph,
        &state.consent,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Clear a user workflow preference (write path with an empty value).
#[tauri::command]
pub async fn delete_user_preference(
    key: String,
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    tools::handle_set_preference(
        serde_json::json!({ "key": key, "value": "" }),
        &state.graph,
        &state.consent,
    )
    .await
    .map_err(|e| e.to_string())
}

/// Scan `~/.claude/projects` for importable transcripts. Read-only: counts and
/// a date range from file metadata, no transcript content is opened.
#[tauri::command]
pub async fn scan_transcripts(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    state.consent.check().map_err(|e| e.to_string())?;
    let root = strata::backfill::default_transcripts_root()
        .ok_or_else(|| "could not resolve home directory".to_string())?;
    let report = strata::backfill::scan(&root, &state.graph).map_err(|e| e.to_string())?;
    serde_json::to_value(report).map_err(|e| e.to_string())
}

/// Import all not-yet-ingested local transcripts through the privacy pipeline.
/// Parsing happens off the main thread; only derived tags are persisted.
#[tauri::command]
pub async fn run_backfill(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let graph = Arc::clone(&state.graph);
    let consent = Arc::clone(&state.consent);
    let report = tokio::task::spawn_blocking(move || {
        let root = strata::backfill::default_transcripts_root()
            .ok_or_else(|| "could not resolve home directory".to_string())?;
        strata::backfill::run(&root, &graph, &consent).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| e.to_string())??;
    serde_json::to_value(report).map_err(|e| e.to_string())
}

/// Read-only status of AI-client integrations (MCP configs + session hook).
#[tauri::command]
pub async fn get_integrations() -> Result<serde_json::Value, String> {
    let statuses = crate::integrations::status_all()?;
    Ok(serde_json::json!({ "integrations": statuses }))
}

/// Wire Strata into one AI client's local config. User-initiated from the
/// Setup page; returns the refreshed status list.
#[tauri::command]
pub async fn install_integration(id: String) -> Result<serde_json::Value, String> {
    let statuses = crate::integrations::install(&id)?;
    Ok(serde_json::json!({ "integrations": statuses }))
}
