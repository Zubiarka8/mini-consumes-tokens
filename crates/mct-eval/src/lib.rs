//! Continuous quality evaluation for the MCP tools (issue #21).
//!
//! A fixed suite of tool calls (`suite.json`) runs in-process against a
//! versioned fixture through [`MctServer::call_read_only_tool`] — the same
//! dispatcher `batch` uses, so each call sees exactly what an MCP client
//! would. Every call is scored on four metrics:
//!
//! - **accuracy**: the fraction of the case's `expect` strings found in the
//!   output, zeroed when any `absent` string shows up (a wrong hit);
//! - **MRR**: for `ranked` cases, the reciprocal rank of the first `expect`
//!   string among the output's hit lines;
//! - **success**: the call returned a non-error result;
//! - **latency** (p50/p95 over `iterations` timed calls) and **tokens**
//!   (the JSON-RPC request + response as it crosses the transport, with the
//!   same approximate tokenizer as `mct-mcp-server/tests/batch.rs`).
//!
//! [`compare`] checks a run against `baseline.json`. Accuracy, MRR, success
//! and tokens are deterministic on a fixture, so they are gated tightly;
//! latency depends on the machine and build profile, so it is gated only by
//! an absolute per-call budget and otherwise reported for trend.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use rmcp::handler::server::wrapper::Parameters;
use rmcp::model::CallToolResult;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};

/// Tolerance for comparing scores read back from JSON.
const SCORE_EPSILON: f64 = 1e-9;

/// `suite.json` next to this crate's manifest.
pub fn default_suite_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("suite.json")
}

/// `baseline.json` next to this crate's manifest.
pub fn default_baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("baseline.json")
}

/// A named set of cases run against one fixture.
#[derive(Debug, Deserialize)]
pub struct Suite {
    pub name: String,
    /// Fixture root, relative to the suite file's directory.
    pub fixture: String,
    pub cases: Vec<Case>,
    /// Set by [`Suite::load`]: `fixture` resolved against the suite file.
    #[serde(skip)]
    pub fixture_root: PathBuf,
}

/// One tool call and what a correct answer must (and must not) contain.
#[derive(Debug, Deserialize)]
pub struct Case {
    pub name: String,
    pub category: String,
    pub tool: String,
    #[serde(default)]
    pub args: Option<Map<String, Value>>,
    /// Strings a correct answer contains, e.g. `backend/ledger.go:10`.
    #[serde(default)]
    pub expect: Vec<String>,
    /// Strings a correct answer must not contain (a wrong hit).
    #[serde(default)]
    pub absent: Vec<String>,
    /// Score the rank of `expect[0]` among the output's hit lines.
    #[serde(default)]
    pub ranked: bool,
}

impl Suite {
    pub fn load(path: &Path) -> Result<Self> {
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let mut suite: Suite =
            serde_json::from_str(&source).with_context(|| format!("parsing {}", path.display()))?;
        let dir = path.parent().unwrap_or_else(|| Path::new("."));
        suite.fixture_root = dir.join(&suite.fixture);
        Ok(suite)
    }
}

/// One case's outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseResult {
    pub name: String,
    pub category: String,
    pub tool: String,
    pub success: bool,
    /// Accuracy in `0.0..=1.0`: expected strings found, 0 on a wrong hit.
    pub score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reciprocal_rank: Option<f64>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub missing: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub unexpected: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub tokens: usize,
    pub p50_ms: f64,
    pub p95_ms: f64,
    /// The tool's text output, kept in the JSON report so a regression can
    /// be diagnosed from the CI artifact alone.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub output: String,
}

/// Aggregates over every case of a run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Summary {
    pub cases: usize,
    pub accuracy: f64,
    pub mrr: f64,
    pub success_rate: f64,
    pub tokens: usize,
    /// The `tools/list` payload every session pays for up front.
    pub catalog_tokens: usize,
    pub index_build_ms: f64,
    pub p50_ms: f64,
    pub p95_ms: f64,
    pub max_ms: f64,
}

/// A full run: every case plus the summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub suite: String,
    pub iterations: usize,
    pub summary: Summary,
    pub cases: Vec<CaseResult>,
}

/// Builds the fixture's index in memory and runs every case `iterations`
/// times (after one untimed warm-up call).
pub async fn run(suite: &Suite, iterations: usize) -> Result<Report> {
    let iterations = iterations.max(1);
    let start = Instant::now();
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&suite.fixture_root, ExcludeSet::default())
        .with_context(|| format!("opening {}", suite.fixture_root.display()))?;
    index
        .reindex(&registry, false)
        .context("indexing the fixture")?;
    let index_build = start.elapsed();
    let server = MctServer::new(index, registry);
    let catalog = serde_json::to_string(&server.tool_catalog())?;

    let mut cases = Vec::with_capacity(suite.cases.len());
    let mut all_timings = Vec::new();
    for case in &suite.cases {
        let outcome = call(&server, &case.tool, case.args.clone()).await;
        let mut timings = Vec::with_capacity(iterations);
        for _ in 0..iterations {
            let start = Instant::now();
            // The outcome scored is the warm-up's; the timed calls only time.
            let _ = call(&server, &case.tool, case.args.clone()).await;
            timings.push(start.elapsed());
        }
        all_timings.extend_from_slice(&timings);
        cases.push(score_case(case, &outcome, &timings));
    }

    let ranked: Vec<f64> = cases.iter().filter_map(|c| c.reciprocal_rank).collect();
    let summary = Summary {
        cases: cases.len(),
        accuracy: mean(cases.iter().map(|c| c.score)),
        mrr: mean(ranked.iter().copied()),
        success_rate: mean(cases.iter().map(|c| if c.success { 1.0 } else { 0.0 })),
        tokens: cases.iter().map(|c| c.tokens).sum(),
        catalog_tokens: approx_tokens(&catalog),
        index_build_ms: millis(index_build),
        p50_ms: percentile(&all_timings, 0.50),
        p95_ms: percentile(&all_timings, 0.95),
        max_ms: percentile(&all_timings, 1.0),
    };
    Ok(Report {
        suite: suite.name.clone(),
        iterations,
        summary,
        cases,
    })
}

/// Calls `tool` by name. `batch` isn't reachable through
/// [`MctServer::call_read_only_tool`] (a batch can't nest), so it is called
/// directly.
async fn call(
    server: &MctServer,
    tool: &str,
    args: Option<Map<String, Value>>,
) -> Result<CallToolResult, rmcp::ErrorData> {
    if tool != "batch" {
        return server.call_read_only_tool(tool, args).await;
    }
    let args = serde_json::from_value(Value::Object(args.unwrap_or_default())).map_err(|err| {
        rmcp::ErrorData::invalid_params(format!("invalid args for `batch`: {err}"), None)
    })?;
    server.batch(Parameters(args)).await
}

fn score_case(
    case: &Case,
    outcome: &Result<CallToolResult, rmcp::ErrorData>,
    timings: &[Duration],
) -> CaseResult {
    let (success, text, response, error) = match outcome {
        Ok(result) => {
            let text = result_text(result);
            let failed = result.is_error == Some(true);
            let response = serde_json::to_value(result).unwrap_or(Value::Null);
            let error = failed.then(|| text.clone());
            (!failed, text, json!({ "result": response }), error)
        }
        Err(err) => {
            let response = serde_json::to_value(err).unwrap_or(Value::Null);
            (
                false,
                String::new(),
                json!({ "error": response }),
                Some(err.message.to_string()),
            )
        }
    };

    let (score, missing, unexpected) = accuracy(&text, &case.expect, &case.absent);
    let score = if success { score } else { 0.0 };
    let reciprocal_rank = case.ranked.then(|| {
        case.expect
            .first()
            .and_then(|want| hit_rank(&text, want))
            .map_or(0.0, |rank| 1.0 / rank as f64)
    });
    let tokens = approx_tokens(&exchange(&case.tool, case.args.as_ref(), response));
    let output = match (&error, text.is_empty()) {
        (Some(error), true) => error.clone(),
        _ => text,
    };
    CaseResult {
        name: case.name.clone(),
        category: case.category.clone(),
        tool: case.tool.clone(),
        success,
        score,
        reciprocal_rank,
        missing,
        unexpected,
        error,
        tokens,
        p50_ms: percentile(timings, 0.50),
        p95_ms: percentile(timings, 0.95),
        output,
    }
}

/// Fraction of `expect` found in `text` (1.0 when there is nothing to
/// expect), zeroed when any `absent` string is present; plus the misses and
/// the wrong hits, for the report.
pub fn accuracy(
    text: &str,
    expect: &[String],
    absent: &[String],
) -> (f64, Vec<String>, Vec<String>) {
    let missing: Vec<String> = expect
        .iter()
        .filter(|w| !text.contains(w.as_str()))
        .cloned()
        .collect();
    let unexpected: Vec<String> = absent
        .iter()
        .filter(|w| text.contains(w.as_str()))
        .cloned()
        .collect();
    let score = if !unexpected.is_empty() {
        0.0
    } else if expect.is_empty() {
        1.0
    } else {
        (expect.len() - missing.len()) as f64 / expect.len() as f64
    };
    (score, missing, unexpected)
}

/// 1-based rank of the first hit line containing `want`. Hit lines are the
/// non-indented lines that open with a `path:line` location
/// (`a/b.rs:12:5 …`, `a/b.rs:L3-L9 …`), so header/notice lines and indented
/// snippets don't count.
pub fn hit_rank(text: &str, want: &str) -> Option<usize> {
    text.lines()
        .filter(|line| is_hit_line(line))
        .position(|line| line.contains(want))
        .map(|i| i + 1)
}

fn is_hit_line(line: &str) -> bool {
    !line.starts_with(char::is_whitespace)
        && line
            .split_whitespace()
            .next()
            .and_then(|first| first.split_once(':'))
            .is_some_and(|(path, rest)| {
                !path.is_empty() && rest.starts_with(|c: char| c.is_ascii_digit() || c == 'L')
            })
}

/// Same heuristic as `mct-mcp-server/tests/batch.rs::approx_tokens`: a run of
/// alphanumerics/`_` is one token, every other non-whitespace char is one.
pub fn approx_tokens(s: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_word = false;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            if !in_word {
                tokens += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !c.is_whitespace() {
                tokens += 1;
            }
        }
    }
    tokens
}

/// One `tools/call` round trip as it crosses the stdio transport.
fn exchange(tool: &str, args: Option<&Map<String, Value>>, response: Value) -> String {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "tools/call",
        "params": { "name": tool, "arguments": args.cloned().unwrap_or_default() },
    });
    let mut response = response;
    if let Value::Object(map) = &mut response {
        map.insert("jsonrpc".into(), json!("2.0"));
        map.insert("id".into(), json!(1));
    }
    format!("{request}\n{response}\n")
}

fn result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| block.as_text())
        .map(|text| text.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (sum, n) = values.fold((0.0, 0usize), |(sum, n), v| (sum + v, n + 1));
    if n == 0 {
        0.0
    } else {
        sum / n as f64
    }
}

/// Nearest-rank percentile in milliseconds; 0 for no samples.
fn percentile(samples: &[Duration], p: f64) -> f64 {
    let mut sorted = samples.to_vec();
    sorted.sort();
    let rank = ((p * sorted.len() as f64).ceil() as usize).clamp(1, sorted.len().max(1));
    sorted.get(rank - 1).copied().map_or(0.0, millis)
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

/// The deterministic part of a run, checked in as `baseline.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Baseline {
    pub suite: String,
    pub catalog_tokens: usize,
    pub cases: BTreeMap<String, CaseBaseline>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CaseBaseline {
    pub score: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reciprocal_rank: Option<f64>,
    pub tokens: usize,
}

impl Baseline {
    pub fn from_report(report: &Report) -> Self {
        Self {
            suite: report.suite.clone(),
            catalog_tokens: report.summary.catalog_tokens,
            cases: report
                .cases
                .iter()
                .map(|c| {
                    let case = CaseBaseline {
                        score: c.score,
                        reciprocal_rank: c.reciprocal_rank,
                        tokens: c.tokens,
                    };
                    (c.name.clone(), case)
                })
                .collect(),
        }
    }

    pub fn load(path: &Path) -> Result<Self> {
        let source =
            std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        serde_json::from_str(&source).with_context(|| format!("parsing {}", path.display()))
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let json = serde_json::to_string_pretty(self)?;
        std::fs::write(path, format!("{json}\n"))
            .with_context(|| format!("writing {}", path.display()))
    }
}

/// How much a run may drift from the baseline before it counts as a
/// regression.
#[derive(Debug, Clone, Copy)]
pub struct Thresholds {
    /// Allowed relative token growth per case and for the catalog.
    pub token_growth: f64,
    /// Absolute token slack on top of `token_growth`, so a tiny response
    /// gaining a word isn't a regression.
    pub token_slack: usize,
    /// Per-case p95 latency budget. Absolute rather than relative to the
    /// baseline: timings vary by machine and build profile.
    pub latency_budget_ms: f64,
}

impl Default for Thresholds {
    fn default() -> Self {
        Self {
            token_growth: 0.10,
            token_slack: 8,
            latency_budget_ms: 500.0,
        }
    }
}

/// What changed between a run and the baseline.
#[derive(Debug, Default)]
pub struct Comparison {
    /// Fail the gate.
    pub regressions: Vec<String>,
    /// Better than the baseline — refresh it to lock the gain in.
    pub improvements: Vec<String>,
    /// Informational (e.g. a case with no baseline yet).
    pub notes: Vec<String>,
}

pub fn compare(report: &Report, baseline: &Baseline, thresholds: &Thresholds) -> Comparison {
    let mut cmp = Comparison::default();
    let grew = |now: usize, before: usize| {
        now as f64 > before as f64 * (1.0 + thresholds.token_growth) + thresholds.token_slack as f64
    };

    if grew(report.summary.catalog_tokens, baseline.catalog_tokens) {
        cmp.regressions.push(format!(
            "tool catalog: ~{} → ~{} tokens",
            baseline.catalog_tokens, report.summary.catalog_tokens
        ));
    } else if report.summary.catalog_tokens < baseline.catalog_tokens {
        cmp.improvements.push(format!(
            "tool catalog: ~{} → ~{} tokens",
            baseline.catalog_tokens, report.summary.catalog_tokens
        ));
    }

    for case in &report.cases {
        let name = &case.name;
        if !case.success {
            let why = case.error.as_deref().unwrap_or("error result");
            cmp.regressions.push(format!("{name}: call failed: {why}"));
        }
        if case.p95_ms > thresholds.latency_budget_ms {
            cmp.regressions.push(format!(
                "{name}: p95 {:.1} ms over the {:.0} ms budget",
                case.p95_ms, thresholds.latency_budget_ms
            ));
        }
        let Some(before) = baseline.cases.get(name) else {
            cmp.notes.push(format!("{name}: new case, no baseline yet"));
            continue;
        };
        if case.score + SCORE_EPSILON < before.score {
            let mut detail = Vec::new();
            if !case.missing.is_empty() {
                detail.push(format!("missing {}", case.missing.join(", ")));
            }
            if !case.unexpected.is_empty() {
                detail.push(format!("wrong hit {}", case.unexpected.join(", ")));
            }
            cmp.regressions.push(format!(
                "{name}: accuracy {:.2} → {:.2} ({})",
                before.score,
                case.score,
                detail.join("; ")
            ));
        } else if case.score > before.score + SCORE_EPSILON {
            cmp.improvements.push(format!(
                "{name}: accuracy {:.2} → {:.2}",
                before.score, case.score
            ));
        }
        if let (Some(now), Some(then)) = (case.reciprocal_rank, before.reciprocal_rank) {
            if now + SCORE_EPSILON < then {
                cmp.regressions
                    .push(format!("{name}: reciprocal rank {then:.2} → {now:.2}"));
            } else if now > then + SCORE_EPSILON {
                cmp.improvements
                    .push(format!("{name}: reciprocal rank {then:.2} → {now:.2}"));
            }
        }
        if grew(case.tokens, before.tokens) {
            cmp.regressions.push(format!(
                "{name}: ~{} → ~{} tokens",
                before.tokens, case.tokens
            ));
        } else if (case.tokens as f64) < before.tokens as f64 * (1.0 - thresholds.token_growth) {
            cmp.improvements.push(format!(
                "{name}: ~{} → ~{} tokens",
                before.tokens, case.tokens
            ));
        }
    }

    for name in baseline.cases.keys() {
        if !report.cases.iter().any(|c| &c.name == name) {
            cmp.regressions.push(format!(
                "{name}: in the baseline but no longer in the suite"
            ));
        }
    }
    cmp
}

/// The report as Markdown, for a CI job summary or the monthly report.
pub fn render_markdown(report: &Report, comparison: Option<&Comparison>) -> String {
    let s = &report.summary;
    let mut out = String::new();
    let _ = writeln!(out, "# Quality evaluation — `{}`\n", report.suite);
    let _ = writeln!(
        out,
        "{} cases, {} timed iteration(s) each.\n",
        s.cases, report.iterations
    );
    let _ = writeln!(out, "| Metric | Value |\n|---|---|");
    let _ = writeln!(out, "| Accuracy | {:.3} |", s.accuracy);
    let _ = writeln!(out, "| MRR (ranked cases) | {:.3} |", s.mrr);
    let _ = writeln!(out, "| Success rate | {:.1}% |", s.success_rate * 100.0);
    let _ = writeln!(out, "| Response tokens (all cases) | ~{} |", s.tokens);
    let _ = writeln!(out, "| Tool catalog tokens | ~{} |", s.catalog_tokens);
    let _ = writeln!(
        out,
        "| Latency p50 / p95 / max | {:.2} / {:.2} / {:.2} ms |",
        s.p50_ms, s.p95_ms, s.max_ms
    );
    let _ = writeln!(out, "| Index build | {:.1} ms |", s.index_build_ms);

    if let Some(cmp) = comparison {
        let _ = writeln!(out, "\n## Against the baseline\n");
        if cmp.regressions.is_empty() {
            let _ = writeln!(out, "No regressions.");
        } else {
            let _ = writeln!(out, "**{} regression(s):**\n", cmp.regressions.len());
            for line in &cmp.regressions {
                let _ = writeln!(out, "- {line}");
            }
        }
        if !cmp.improvements.is_empty() {
            let _ = writeln!(
                out,
                "\nImprovements (refresh the baseline to lock them in):\n"
            );
            for line in &cmp.improvements {
                let _ = writeln!(out, "- {line}");
            }
        }
        for line in &cmp.notes {
            let _ = writeln!(out, "\n- {line}");
        }
    }

    let mut categories: BTreeMap<&str, Vec<&CaseResult>> = BTreeMap::new();
    for case in &report.cases {
        categories
            .entry(case.category.as_str())
            .or_default()
            .push(case);
    }
    let _ = writeln!(out, "\n## By category\n");
    let _ = writeln!(
        out,
        "| Category | Cases | Accuracy | Tokens | Worst p95 (ms) |\n|---|---|---|---|---|"
    );
    for (category, cases) in &categories {
        let worst = cases.iter().map(|c| c.p95_ms).fold(0.0, f64::max);
        let _ = writeln!(
            out,
            "| {category} | {} | {:.3} | ~{} | {worst:.2} |",
            cases.len(),
            mean(cases.iter().map(|c| c.score)),
            cases.iter().map(|c| c.tokens).sum::<usize>(),
        );
    }

    let _ = writeln!(out, "\n## Cases\n");
    let _ = writeln!(out, "| Case | Tool | OK | Accuracy | RR | Tokens | p50 / p95 (ms) |\n|---|---|---|---|---|---|---|");
    for c in &report.cases {
        let rr = c
            .reciprocal_rank
            .map_or_else(|| "—".to_string(), |rr| format!("{rr:.2}"));
        let _ = writeln!(
            out,
            "| {} | `{}` | {} | {:.2} | {rr} | ~{} | {:.2} / {:.2} |",
            c.name,
            c.tool,
            if c.success { "✅" } else { "❌" },
            c.score,
            c.tokens,
            c.p50_ms,
            c.p95_ms,
        );
    }
    out
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;

    fn strings(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn case(name: &str, score: f64, rr: Option<f64>, tokens: usize) -> CaseResult {
        CaseResult {
            name: name.into(),
            category: "c".into(),
            tool: "find_symbol".into(),
            success: true,
            score,
            reciprocal_rank: rr,
            missing: Vec::new(),
            unexpected: Vec::new(),
            error: None,
            tokens,
            p50_ms: 1.0,
            p95_ms: 1.0,
            output: String::new(),
        }
    }

    fn report(cases: Vec<CaseResult>) -> Report {
        Report {
            suite: "s".into(),
            iterations: 1,
            summary: Summary {
                cases: cases.len(),
                accuracy: 1.0,
                mrr: 1.0,
                success_rate: 1.0,
                tokens: 0,
                catalog_tokens: 100,
                index_build_ms: 0.0,
                p50_ms: 1.0,
                p95_ms: 1.0,
                max_ms: 1.0,
            },
            cases,
        }
    }

    #[test]
    fn accuracy_counts_expected_hits_and_zeroes_on_a_wrong_one() {
        let text = "2 hits:\na.rs:1 foo\nb.rs:2 bar\n";
        let (score, missing, _) = accuracy(text, &strings(&["a.rs:1", "c.rs:3"]), &[]);
        assert_eq!(score, 0.5);
        assert_eq!(missing, strings(&["c.rs:3"]));
        let (score, _, unexpected) = accuracy(text, &strings(&["a.rs:1"]), &strings(&["bar"]));
        assert_eq!(score, 0.0);
        assert_eq!(unexpected, strings(&["bar"]));
    }

    #[test]
    fn hit_rank_counts_only_location_lines() {
        let text = "hybrid: alpha 0, foo only\n2 match(es) for `foo`:\na.rs:L1-L3 first\n    snippet mentioning foo\n\nb.rs:9:1 foo\n";
        assert_eq!(hit_rank(text, "foo"), Some(2));
        assert_eq!(hit_rank(text, "a.rs:L1"), Some(1));
        assert_eq!(hit_rank(text, "nowhere"), None);
    }

    #[test]
    fn percentile_is_nearest_rank() {
        let samples: Vec<Duration> = (1..=20).map(Duration::from_millis).collect();
        assert_eq!(percentile(&samples, 0.50), 10.0);
        assert_eq!(percentile(&samples, 0.95), 19.0);
        assert_eq!(percentile(&samples, 1.0), 20.0);
        assert_eq!(percentile(&[], 0.95), 0.0);
    }

    #[test]
    fn an_unchanged_run_matches_its_own_baseline() {
        let run = report(vec![case("a", 1.0, Some(1.0), 50)]);
        let cmp = compare(&run, &Baseline::from_report(&run), &Thresholds::default());
        assert!(cmp.regressions.is_empty(), "{:?}", cmp.regressions);
        assert!(cmp.improvements.is_empty(), "{:?}", cmp.improvements);
    }

    #[test]
    fn accuracy_rank_token_and_failure_drops_are_regressions() {
        let before = report(vec![
            case("accuracy", 1.0, None, 50),
            case("rank", 1.0, Some(1.0), 50),
            case("tokens", 1.0, None, 100),
            case("fails", 1.0, None, 50),
            case("removed", 1.0, None, 50),
        ]);
        let baseline = Baseline::from_report(&before);
        let mut failing = case("fails", 0.0, None, 50);
        failing.success = false;
        let mut slow = case("slow", 1.0, None, 50);
        slow.p95_ms = 10_000.0;
        let after = report(vec![
            case("accuracy", 0.5, None, 50),
            case("rank", 1.0, Some(0.5), 50),
            case("tokens", 1.0, None, 130),
            failing,
            slow,
        ]);
        let cmp = compare(&after, &baseline, &Thresholds::default());
        for name in ["accuracy", "rank", "tokens", "fails", "removed", "slow"] {
            assert!(
                cmp.regressions.iter().any(|r| r.starts_with(name)),
                "`{name}` not flagged: {:?}",
                cmp.regressions
            );
        }
        assert!(cmp.notes.iter().any(|n| n.starts_with("slow")));
    }

    #[test]
    fn small_token_growth_within_tolerance_is_not_a_regression() {
        let before = report(vec![case("a", 1.0, None, 100)]);
        let baseline = Baseline::from_report(&before);
        let after = report(vec![case("a", 1.0, None, 115)]);
        assert!(compare(&after, &baseline, &Thresholds::default())
            .regressions
            .is_empty());
    }
}
