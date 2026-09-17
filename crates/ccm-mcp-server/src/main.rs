use std::path::PathBuf;

use ccm_mcp_server::{registry, server};
use clap::Parser;
use rmcp::{ServiceExt, transport::stdio};

/// mini-consumes-tokens: MCP server exposing an AST-derived symbol graph of
/// this repository (find_symbol, find_references, find_calls, find_callers).
#[derive(Parser, Debug)]
struct Args {
    /// Project root to index. Defaults to the current working directory.
    #[arg(long)]
    root: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        // MCP over stdio reserves stdout for the JSON-RPC stream — all
        // logging must go to stderr or it corrupts the protocol.
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let args = Args::parse();
    let root = match args.root {
        Some(root) => root,
        None => std::env::current_dir()?,
    };
    let db_path = root.join(".ccm-index").join("index.sqlite3");

    let language_registry = registry::build_registry();
    let mut index = ccm_index::Index::open(&root, &db_path, ccm_index::ExcludeSet::default())?;

    tracing::info!(root = %root.display(), "starting initial reindex");
    let report = index.reindex(&language_registry, false)?;
    tracing::info!(
        parsed = report.files_parsed,
        unchanged = report.files_unchanged,
        removed = report.files_removed,
        symbols = report.symbols_written,
        issues = report.issues.len(),
        "initial reindex complete"
    );

    let service = server::CcmServer::new(index, language_registry)
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {e:?}"))?;

    service.waiting().await?;
    Ok(())
}
