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
    /// entirely, restoring the previous startup-only behavior. Precedence:
    /// this flag, then the MCT_REINDEX_DEBOUNCE_MS environment variable,
    /// then the default of 15000.
    #[arg(long)]
    reindex_debounce_ms: Option<u64>,
}

/// Default quiet time before the background watcher reindexes, in milliseconds.
const DEFAULT_REINDEX_DEBOUNCE_MS: u64 = 15_000;

/// Environment variable that sets the quiet time when the flag is absent.
const REINDEX_DEBOUNCE_ENV: &str = "MCT_REINDEX_DEBOUNCE_MS";

/// Resolves the reindex debounce: flag > environment value > default. An
/// environment value that is not a non-negative integer is ignored, so a typo
/// falls back to the default instead of stopping the server.
fn resolve_reindex_debounce_ms(flag: Option<u64>, env_value: Option<&str>) -> u64 {
    flag.or_else(|| env_value.and_then(|v| v.trim().parse().ok()))
        .unwrap_or(DEFAULT_REINDEX_DEBOUNCE_MS)
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
    // One set of rules shared by the index and the watcher (clones share
    // it), reloaded when `.mctignore`/`.gitignore` change — no restart needed.
    let exclude = mct_index::ExcludeSet::for_project(&root);
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
    let reindex_debounce_ms = resolve_reindex_debounce_ms(
        args.reindex_debounce_ms,
        std::env::var(REINDEX_DEBOUNCE_ENV).ok().as_deref(),
    );
    let _watcher = if reindex_debounce_ms > 0 {
        match background::spawn_watcher(
            server.index_handle(),
            server.registry_handle(),
            watch_root,
            exclude,
            Duration::from_millis(reindex_debounce_ms),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_15_seconds() {
        assert_eq!(resolve_reindex_debounce_ms(None, None), 15_000);
    }

    #[test]
    fn env_overrides_default() {
        assert_eq!(resolve_reindex_debounce_ms(None, Some("5000")), 5_000);
        assert_eq!(resolve_reindex_debounce_ms(None, Some(" 60000 ")), 60_000);
    }

    #[test]
    fn flag_overrides_env_and_default() {
        assert_eq!(
            resolve_reindex_debounce_ms(Some(2_000), Some("5000")),
            2_000
        );
        assert_eq!(resolve_reindex_debounce_ms(Some(2_000), None), 2_000);
    }

    #[test]
    fn zero_from_flag_or_env_disables_watcher() {
        assert_eq!(resolve_reindex_debounce_ms(Some(0), Some("5000")), 0);
        assert_eq!(resolve_reindex_debounce_ms(None, Some("0")), 0);
    }

    #[test]
    fn invalid_env_falls_back_to_default() {
        assert_eq!(resolve_reindex_debounce_ms(None, Some("fast")), 15_000);
        assert_eq!(resolve_reindex_debounce_ms(None, Some("-1")), 15_000);
        assert_eq!(resolve_reindex_debounce_ms(None, Some("")), 15_000);
    }
}
