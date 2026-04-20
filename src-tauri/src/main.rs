// Prevents a console window from appearing on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

use std::sync::Arc;

use anyhow::Context;
use commands::AppState;
use strata::consent::ConsentGate;
use strata::graph::GraphHandle;
use tauri::Manager;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_skills,
            commands::get_context,
            commands::get_preferences,
            commands::ingest_signal,
            commands::get_consent_status,
            commands::pause_consent,
            commands::resume_consent,
            commands::revoke_consent,
            commands::get_audit_log,
            commands::get_skill_history,
        ])
        .setup(|app| {
            let data_dir = data_dir(app).unwrap_or_else(|| ".".into());
            std::fs::create_dir_all(&data_dir)
                .context("failed to create data directory")
                .map_err(|e| e.to_string())?;
            let db_path = format!("{data_dir}/strata.db");

            let graph = GraphHandle::open(&db_path)
                .context("failed to open skill graph database")
                .map_err(|e| e.to_string())?;
            let consent_conn = rusqlite::Connection::open(&db_path)
                .context("failed to open consent database connection")
                .map_err(|e| e.to_string())?;
            let consent = ConsentGate::new(consent_conn)
                .context("failed to initialize consent gate")
                .map_err(|e| e.to_string())?;

            app.manage(AppState {
                graph: Arc::new(graph),
                consent: Arc::new(consent),
            });

            tracing::info!("Strata desktop app started, graph at {db_path}");
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}

fn data_dir(_app: &tauri::App) -> Option<String> {
    // Must match the path used by the MCP server binary (src/main.rs dirs_data_dir()).
    // Both processes share the same SQLite file — diverging paths produce split databases.
    #[cfg(target_os = "macos")]
    {
        dirs::home_dir()
            .map(|h| format!("{}/Library/Application Support/Strata", h.display()))
    }
    #[cfg(target_os = "linux")]
    {
        dirs::data_local_dir().map(|d| format!("{}/strata", d.display()))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::data_dir().map(|d| format!("{}\\Strata", d.display()))
    }
}
