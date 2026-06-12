// Prevents a console window from appearing on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;
mod integrations;

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
            commands::get_growth,
            commands::get_topic_summaries,
            commands::get_insights,
            commands::dismiss_insight,
            commands::set_user_preference,
            commands::delete_user_preference,
            commands::scan_transcripts,
            commands::run_backfill,
            commands::get_integrations,
            commands::install_integration,
        ])
        .setup(|app| {
            let db_path = strata::paths::prepare_db_path()
                .context("failed to prepare data directory")
                .map_err(|e| e.to_string())?;

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
