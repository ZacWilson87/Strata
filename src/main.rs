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

    // Subcommand dispatch: bare `strata` runs the MCP server (what AI clients
    // spawn); `strata hook session-end` is invoked by Claude Code's SessionEnd
    // hook with the hook event JSON on stdin; `strata backfill` runs a
    // headless transcript import (same path as the dashboard's Setup page).
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        None => run_mcp_server().await,
        Some("hook") => {
            run_session_end_hook(args.get(2).map(String::as_str));
            // Hooks must never fail the client session: outcomes (including
            // errors) are logged to stderr and the process exits 0.
            Ok(())
        }
        Some("backfill") => run_backfill(),
        Some(other) => {
            anyhow::bail!("unknown subcommand: {other} (expected none, `hook`, or `backfill`)")
        }
    }
}

/// Handle `strata backfill`: import local transcripts through the privacy
/// pipeline and print the report as JSON on stdout.
fn run_backfill() -> anyhow::Result<()> {
    let (graph, consent) = open_handles()?;
    let root = strata::backfill::default_transcripts_root()
        .ok_or_else(|| anyhow::anyhow!("could not resolve home directory"))?;
    let report = strata::backfill::run(&root, &graph, &consent)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

/// Open the shared database and serve MCP over stdio.
async fn run_mcp_server() -> anyhow::Result<()> {
    let (graph, consent) = open_handles()?;
    let server = McpServer::new(Arc::clone(&graph), Arc::clone(&consent));
    tracing::info!("Strata MCP server starting");
    server.run().await.context("MCP server error")?;
    Ok(())
}

/// Handle `strata hook session-end`: read the hook event from stdin and
/// ingest the finished session's transcript through the privacy pipeline.
fn run_session_end_hook(event_name: Option<&str>) {
    if event_name != Some("session-end") {
        tracing::error!("unknown hook event: {event_name:?} (expected `session-end`)");
        return;
    }
    let raw_event = match std::io::read_to_string(std::io::stdin()) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("could not read hook event from stdin: {e}");
            return;
        }
    };
    let handles = open_handles();
    let (graph, consent) = match &handles {
        Ok((g, c)) => (g, c),
        Err(e) => {
            tracing::error!("could not open Strata database: {e}");
            return;
        }
    };
    match strata::backfill::ingest_hook_event(graph, consent, &raw_event) {
        Ok(outcome) => tracing::info!("session-end hook: {outcome:?}"),
        Err(e) => tracing::warn!("session-end hook skipped: {e}"),
    }
}

/// Open the graph and consent gate over the shared database file.
fn open_handles() -> anyhow::Result<(Arc<GraphHandle>, Arc<ConsentGate>)> {
    let db_path = strata::paths::prepare_db_path().context("failed to prepare data directory")?;
    tracing::info!("opening graph database at {}", db_path);
    let graph = GraphHandle::open(&db_path).context("failed to open skill graph database")?;
    let consent_conn = rusqlite::Connection::open(&db_path)
        .context("failed to open consent database connection")?;
    let consent = ConsentGate::new(consent_conn).context("failed to initialize consent gate")?;
    Ok((Arc::new(graph), Arc::new(consent)))
}
