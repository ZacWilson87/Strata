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
pub async fn get_consent_status(
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
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
