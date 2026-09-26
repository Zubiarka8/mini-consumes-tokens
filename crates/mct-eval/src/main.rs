//! `mct-eval`: runs the quality suite, prints the report and exits non-zero
//! on a regression against the baseline. See the crate docs in `lib.rs`.

use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::Parser;
use mct_eval::{compare, render_markdown, Baseline, Suite, Thresholds};

#[derive(Parser)]
#[command(about = "Evaluate the MCP tools' accuracy, success rate, latency and token cost")]
struct Cli {
    /// Suite of cases to run. Defaults to this crate's `suite.json`.
    #[arg(long)]
    suite: Option<PathBuf>,
    /// Baseline to compare against. Defaults to this crate's `baseline.json`.
    #[arg(long)]
    baseline: Option<PathBuf>,
    /// Timed calls per case, after one untimed warm-up.
    #[arg(long, default_value_t = 5)]
    iterations: usize,
    /// Per-case p95 latency budget, in milliseconds.
    #[arg(long, default_value_t = Thresholds::default().latency_budget_ms)]
    latency_budget_ms: f64,
    /// Write the full report as JSON here.
    #[arg(long)]
    json: Option<PathBuf>,
    /// Write the report as Markdown here (e.g. for a CI job summary).
    #[arg(long)]
    markdown: Option<PathBuf>,
    /// Also print every case's tool output to stderr.
    #[arg(long)]
    verbose: bool,
    /// Overwrite the baseline with this run instead of comparing against it.
    #[arg(long)]
    write_baseline: bool,
}

#[tokio::main]
async fn main() -> ExitCode {
    match run(Cli::parse()).await {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(err) => {
            eprintln!("mct-eval: {err:#}");
            ExitCode::from(2)
        }
    }
}

/// `Ok(false)` when the run regressed against the baseline.
async fn run(cli: Cli) -> Result<bool> {
    let suite_path = cli.suite.unwrap_or_else(mct_eval::default_suite_path);
    let baseline_path = cli.baseline.unwrap_or_else(mct_eval::default_baseline_path);
    let suite = Suite::load(&suite_path)?;
    let report = mct_eval::run(&suite, cli.iterations).await?;
    if cli.verbose {
        for case in &report.cases {
            eprintln!("--- {} (`{}`)\n{}", case.name, case.tool, case.output);
        }
    }

    let comparison = if cli.write_baseline {
        Baseline::from_report(&report).save(&baseline_path)?;
        eprintln!("mct-eval: wrote {}", baseline_path.display());
        None
    } else {
        let baseline = Baseline::load(&baseline_path)?;
        let thresholds = Thresholds {
            latency_budget_ms: cli.latency_budget_ms,
            ..Thresholds::default()
        };
        Some(compare(&report, &baseline, &thresholds))
    };

    let markdown = render_markdown(&report, comparison.as_ref());
    println!("{markdown}");
    if let Some(path) = &cli.markdown {
        std::fs::write(path, &markdown).with_context(|| format!("writing {}", path.display()))?;
    }
    if let Some(path) = &cli.json {
        let json = serde_json::to_string_pretty(&report)?;
        std::fs::write(path, json).with_context(|| format!("writing {}", path.display()))?;
    }
    Ok(comparison.is_none_or(|cmp| cmp.regressions.is_empty()))
}
