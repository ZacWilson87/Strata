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

    let db_path = strata::paths::prepare_db_path().context("failed to prepare data directory")?;

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
