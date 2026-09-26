use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use mct_mcp_server::{background, registry, server, session_compat};
use rmcp::{transport::stdio, ServiceExt};

/// mini-consumes-tokens: MCP server exposing an AST-derived symbol graph of
/// this repository (find_symbol, find_references, find_calls, find_callers).
#[derive(Parser, Debug)]
struct Args {
    /// Project root to index. Defaults to the current working directory.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Milliseconds of quiet time required after the last detected file
    /// write before auto-reindexing. 0 disables the background watcher
    /// entirely, restoring the previous startup-only behavior.
    #[arg(long, default_value_t = 1500)]
    reindex_debounce_ms: u64,
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
    let db_path = root.join(".mct-index").join("index.sqlite3");

    let language_registry = registry::build_registry();
    let exclude = mct_index::ExcludeSet::new(&mct_index::read_ignore_file(&root));
    let mut index = mct_index::Index::open(&root, &db_path, exclude.clone())?;

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

    let watch_root = index.root().to_path_buf();
    let server = server::MctServer::new(index, language_registry);

    // Kept alive for the process's lifetime: dropping it would stop the
    // watch. `None` means the watcher is disabled (--reindex-debounce-ms 0).
    let _watcher = if args.reindex_debounce_ms > 0 {
        match background::spawn_watcher(
            server.index_handle(),
            server.registry_handle(),
            watch_root,
            exclude,
            Duration::from_millis(args.reindex_debounce_ms),
        ) {
            Ok(watcher) => Some(watcher),
            Err(err) => {
                tracing::warn!(error = %err, "failed to start auto-reindex watcher; continuing without it");
                None
            }
        }
    } else {
        None
    };

    let service = session_compat::SessionCompat::new(server)
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {e:?}"))?;

    service.waiting().await?;
    Ok(())
}
