use std::sync::Arc;

use anyhow::Context;
use tracing_subscriber::EnvFilter;

use strata::consent::ConsentGate;
use strata::graph::GraphHandle;
use strata::server::McpServer;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Logs MUST go to stderr: stdout is the JSON-RPC channel, and any stray
    // line on it corrupts the MCP stream.
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(std::io::stderr)
        .init();

    let data_dir = dirs_data_dir().unwrap_or_else(|| ".".into());
    std::fs::create_dir_all(&data_dir).context("failed to create data directory")?;
    restrict_dir_permissions(&data_dir);
    let db_path = format!("{data_dir}/strata.db");

    tracing::info!("opening graph database at {}", db_path);
    let graph = GraphHandle::open(&db_path).context("failed to open skill graph database")?;
    let graph = Arc::new(graph);

    let consent_conn = rusqlite::Connection::open(&db_path)
        .context("failed to open consent database connection")?;
    let consent = ConsentGate::new(consent_conn).context("failed to initialize consent gate")?;
    let consent = Arc::new(consent);

    let server = McpServer::new(Arc::clone(&graph), Arc::clone(&consent));

    tracing::info!("Strata MCP server starting");
    server.run().await.context("MCP server error")?;

    Ok(())
}

/// Restrict the data directory to owner-only access (Unix). Best-effort.
#[cfg(unix)]
fn restrict_dir_permissions(dir: &str) {
    use std::os::unix::fs::PermissionsExt;
    if let Err(e) = std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700)) {
        tracing::warn!("could not restrict permissions on {dir}: {e}");
    }
}

#[cfg(not(unix))]
fn restrict_dir_permissions(_dir: &str) {}

/// Return the platform-appropriate data directory path.
fn dirs_data_dir() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        std::env::var("HOME")
            .ok()
            .map(|h| format!("{h}/Library/Application Support/Strata"))
    }
    #[cfg(target_os = "linux")]
    {
        std::env::var("XDG_DATA_HOME")
            .ok()
            .or_else(|| {
                std::env::var("HOME")
                    .ok()
                    .map(|h| format!("{h}/.local/share"))
            })
            .map(|base| format!("{base}/strata"))
    }
    #[cfg(target_os = "windows")]
    {
        std::env::var("APPDATA")
            .ok()
            .map(|d| format!("{d}\\Strata"))
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        None
    }
}
