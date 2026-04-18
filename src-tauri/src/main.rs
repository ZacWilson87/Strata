// Prevents a console window from appearing on Windows
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod commands;

fn main() {
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
        ])
        .setup(|_app| {
            // Background MCP server task is started by the strata binary via stdio.
            // The Tauri shell connects to the graph directly for dashboard reads.
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Tauri application");
}
