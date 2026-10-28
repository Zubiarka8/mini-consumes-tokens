use std::path::PathBuf;
use std::time::Duration;

use clap::Parser;
use mct_mcp_server::readiness::{IndexState, Readiness};
use mct_mcp_server::reindex_delay::{parse_reindex_delay_arg, ReindexDelay};
use mct_mcp_server::{background, registry, server, session_compat, shutdown};
use rmcp::{transport::stdio, ServiceExt};

/// mini-consumes-tokens: MCP server exposing an AST-derived symbol graph of
/// this repository (find_symbol, find_references, find_calls, find_callers).
#[derive(Parser, Debug)]
#[command(
    after_help = "Examples:\n  --reindex-delay 30s     update the index after 30s without file changes\n  --reindex-delay 2m      custom delay (<N>s, <N>m or <N>ms)\n  --reindex-delay off     disable automatic updates"
)]
struct Args {
    /// Project root to index. Defaults to the current working directory.
    #[arg(long)]
    root: Option<PathBuf>,

    /// Milliseconds of quiet time required after the last detected file
    /// write before auto-reindexing (default 15000). 0 disables the background
    /// watcher entirely, restoring the previous startup-only behavior.
    /// Precedence: this flag, then the MCT_REINDEX_DEBOUNCE_MS environment
    /// variable, then the default. Conflicts with --reindex-delay.
    #[arg(long, conflicts_with = "reindex_delay")]
    reindex_debounce_ms: Option<u64>,

    /// Seconds between full reconciliation scans while the filesystem watcher
    /// is active (default 300; minimum 60). Invalid environment values use the default.
    #[arg(long, value_parser = clap::value_parser!(u64).range(60..))]
    reconcile_interval_seconds: Option<u64>,

    /// Quiet time without file changes before the index updates itself.
    /// Presets: 15s, 30s, 45s, 1m. Custom: <N>s, <N>m or <N>ms (N > 0), e.g.
    /// 90s, 2m, 500ms. 'off' disables automatic updates. Conflicts with
    /// --reindex-debounce-ms.
    #[arg(
        long,
        value_name = "VALUE",
        value_parser = parse_reindex_delay_arg,
        conflicts_with = "reindex_debounce_ms"
    )]
    reindex_delay: Option<ReindexDelay>,

    /// Reindex, drop every stored embedding, re-embed every symbol with the
    /// build's model (`--features semantic`), then exit without serving.
    #[arg(long)]
    rebuild_embeddings: bool,

    /// Do not regenerate the Obsidian vault in `.mct-index/obsidian/` after
    /// each reindex.
    #[arg(long)]
    no_obsidian: bool,
}

/// Default quiet time before the background watcher reindexes, in milliseconds.
const DEFAULT_REINDEX_DEBOUNCE_MS: u64 = 15_000;

/// Environment variable that sets the quiet time when the flag is absent.
const REINDEX_DEBOUNCE_ENV: &str = "MCT_REINDEX_DEBOUNCE_MS";
const RECONCILE_INTERVAL_ENV: &str = "MCT_REINDEX_RECONCILE_SECS";
const DEFAULT_RECONCILE_INTERVAL_SECS: u64 = 300;

/// Resolves the reindex debounce: flag > environment value > default. An
/// environment value that is not a non-negative integer is ignored, so a typo
/// falls back to the default instead of stopping the server.
fn resolve_reindex_debounce_ms(flag: Option<u64>, env_value: Option<&str>) -> u64 {
    flag.or_else(|| env_value.and_then(|v| v.trim().parse().ok()))
        .unwrap_or(DEFAULT_REINDEX_DEBOUNCE_MS)
}

fn resolve_reconcile_interval_seconds(flag: Option<u64>, env_value: Option<&str>) -> u64 {
    flag.or_else(|| {
        env_value
            .and_then(|v| v.trim().parse::<u64>().ok())
            .filter(|value| *value >= 60)
    })
    .unwrap_or(DEFAULT_RECONCILE_INTERVAL_SECS)
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
    let publish_guard = mct_obsidian::PublishGuard::default();
    if !args.no_obsidian {
        mct_obsidian::register_guarded(&mut index, publish_guard.clone());
    }

    if args.rebuild_embeddings {
        log_initial_reindex(index.reindex(&language_registry, false)?);
        let model = mct_mcp_server::embedder::SemanticModel::default();
        let embedder = model
            .get(index.root())
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        index.clear_embeddings()?;
        let embedded = index.refresh_embeddings(embedder)?;
        tracing::info!(embedded, model = embedder.model_id(), "embeddings rebuilt");
        return Ok(());
    }

    let watch_root = index.root().to_path_buf();
    let server = server::MctServer::new(index, language_registry);

    // Kept alive for the process's lifetime: dropping it would stop the
    // watch. `None` means the watcher is disabled (--reindex-debounce-ms 0
    // or --reindex-delay off). --reindex-delay wins; otherwise the debounce
    // is resolved from the flag, the environment and the 15 s default.
    let debounce = match args.reindex_delay {
        Some(ReindexDelay(delay)) => delay,
        None => match resolve_reindex_debounce_ms(
            args.reindex_debounce_ms,
            std::env::var(REINDEX_DEBOUNCE_ENV).ok().as_deref(),
        ) {
            0 => None,
            ms => Some(Duration::from_millis(ms)),
        },
    };
    let reconcile_interval = Duration::from_secs(resolve_reconcile_interval_seconds(
        args.reconcile_interval_seconds,
        std::env::var(RECONCILE_INTERVAL_ENV).ok().as_deref(),
    ));

    // `initialize` must be answered within the client's start-up timeout, which
    // a large first index exceeds. Serve first and index in the background;
    // tool calls report the state until the index is complete (see
    // `mct_mcp_server::readiness`).
    let readiness = server.readiness();
    readiness.begin_indexing();
    let startup = tokio::spawn(startup_index(
        server.index_handle(),
        server.registry_handle(),
        readiness.clone(),
        WatcherSettings {
            watch_root,
            exclude,
            debounce,
            reconcile_interval,
        },
    ));

    let service = session_compat::SessionCompat::new(server)
        .serve(stdio())
        .await
        .inspect_err(|e| tracing::error!("serving error: {e:?}"))?;

    service.waiting().await?;
    if matches!(readiness.state(), IndexState::Indexing { .. }) {
        // Waiting for the blocking indexing would keep the process alive long
        // after the client left. SQLite rolls the unfinished transaction back;
        // a vault publication already under way is let finish so its writer
        // lock is released (see `mct_mcp_server::shutdown`).
        tracing::warn!("client disconnected during startup indexing; exiting");
        shutdown::exit_when_not_publishing(&publish_guard, || std::process::exit(0));
    }
    startup.abort();
    Ok(())
}

struct WatcherSettings {
    watch_root: PathBuf,
    exclude: mct_index::ExcludeSet,
    debounce: Option<Duration>,
    reconcile_interval: Duration,
}

fn log_initial_reindex(report: mct_index::ReindexReport) {
    for warning in &report.warnings {
        tracing::warn!(%warning, "post-reindex hook failed");
    }
    tracing::info!(
        parsed = report.files_parsed,
        unchanged = report.files_unchanged,
        removed = report.files_removed,
        symbols = report.symbols_written,
        issues = report.issues.len(),
        "initial reindex complete"
    );
}

/// Runs the initial reindex off the async runtime, publishes the outcome on
/// `readiness`, and once the index is ready (after a failed start: when `reindex`
/// recovers it) starts the auto-reindex watcher (after the
/// initial index, which should not pay for symbol-change tracking). The
/// watcher lives as long as this task, i.e. the process.
async fn startup_index(
    index: std::sync::Arc<tokio::sync::Mutex<mct_index::Index>>,
    registry: mct_core::LanguageRegistry,
    readiness: Readiness,
    watcher: WatcherSettings,
) {
    tracing::info!("starting initial reindex");
    let outcome = tokio::task::spawn_blocking({
        let index = index.clone();
        let registry = registry.clone();
        move || index.blocking_lock().reindex(&registry, false)
    })
    .await;
    match outcome {
        Ok(Ok(report)) => {
            log_initial_reindex(report);
            readiness.mark_ready();
        }
        Ok(Err(err)) => {
            tracing::error!(error = %err, "initial reindex failed; tools report it until `reindex` succeeds");
            readiness.mark_failed(err.to_string());
            // The watcher starts once the `reindex` tool has recovered the index.
            readiness.until_ready().await;
        }
        Err(join) => {
            tracing::error!(error = %join, "initial reindex task failed");
            readiness.mark_failed(format!("indexing task failed: {join}"));
            readiness.until_ready().await;
        }
    }

    let Some(debounce) = watcher.debounce else {
        return;
    };
    let _watcher = match background::spawn_watcher_with_reconciliation(
        index,
        registry,
        watcher.watch_root,
        watcher.exclude,
        debounce,
        watcher.reconcile_interval,
    ) {
        Ok(watcher) => watcher,
        Err(err) => {
            tracing::warn!(error = %err, "failed to start auto-reindex watcher; continuing without it");
            return;
        }
    };
    // Dropping the watcher would stop it: keep it for the process's lifetime.
    std::future::pending::<()>().await;
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

    #[test]
    fn reconciliation_interval_uses_flag_then_valid_environment_then_default() {
        assert_eq!(resolve_reconcile_interval_seconds(None, None), 300);
        assert_eq!(resolve_reconcile_interval_seconds(None, Some("600")), 600);
        assert_eq!(
            resolve_reconcile_interval_seconds(Some(120), Some("600")),
            120
        );
        assert_eq!(resolve_reconcile_interval_seconds(None, Some("59")), 300);
        assert_eq!(resolve_reconcile_interval_seconds(None, Some("bad")), 300);
    }
}
