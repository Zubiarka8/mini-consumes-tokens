//! Labelled retrieval cases for `detail` / `max_tokens`, shared by
//! `tests/retrieval_quality.rs` (asserts) and `examples/retrieval_eval.rs`
//! (prints the table). The project is generated from the strings below, so
//! every expected `path:line` is computed from the fixture itself and the set
//! needs no checked-in fixture tree.
//!
//! A case passes only when *every* expected evidence is in the answers and no
//! forbidden one is. Returning some result is not a pass.
//!
//! Token figures are the two heuristics kept apart everywhere else:
//! `EstimatedCounter` and `chars/4`. Neither is a tokenizer.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::time::Instant;

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

pub const FILES: &[(&str, &str)] = &[
    (
        "src/config.rs",
        "//! Configuration loading.\n\
         pub struct Config {\n    pub port: u16,\n    pub name: String,\n}\n\n\
         impl Config {\n    /// Load the configuration from disk.\n    pub fn load(path: &str) -> Result<Config, String> {\n        let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;\n        parse(&text)\n    }\n}\n\n\
         /// Parse `key=value` configuration text.\n\
         pub fn parse(text: &str) -> Result<Config, String> {\n    let mut port = 0;\n    let mut name = String::new();\n    for line in text.lines() {\n        if let Some((key, value)) = line.split_once('=') {\n            match key.trim() {\n                \"port\" => port = value.trim().parse().map_err(|_| \"bad port\".to_string())?,\n                \"name\" => name = value.trim().to_string(),\n                _ => {}\n            }\n        }\n    }\n    Ok(Config { port, name })\n}\n",
    ),
    (
        "src/http.rs",
        "//! Request handling.\n\
         pub struct Request {\n    pub path: String,\n}\n\n\
         /// Parse a raw HTTP request line.\n\
         pub fn parse(raw: &str) -> Request {\n    let path = raw.split_whitespace().nth(1).unwrap_or(\"/\").to_string();\n    Request { path }\n}\n\n\
         pub fn handle_request(raw: &str) -> String {\n    let request = parse(raw);\n    route(&request)\n}\n\n\
         fn route(request: &Request) -> String {\n    match request.path.as_str() {\n        \"/health\" => \"ok\".to_string(),\n        _ => \"not found\".to_string(),\n    }\n}\n",
    ),
    (
        "src/billing.rs",
        "//! Invoices.\n\
         pub fn calculate_tax(amount: u64) -> u64 {\n    amount * 21 / 100\n}\n\n\
         pub fn apply_discount(amount: u64, percent: u64) -> u64 {\n    amount - amount * percent / 100\n}\n\n\
         /// Create an invoice total from a net amount.\n\
         pub fn create_invoice(amount: u64, percent: u64) -> u64 {\n    let discounted = apply_discount(amount, percent);\n    let taxed = calculate_tax(discounted);\n    discounted + taxed\n}\n",
    ),
    (
        "src/retry.rs",
        "/// Retry an operation with exponential backoff.\n\
         pub fn retry_with_backoff<T>(mut op: impl FnMut() -> Result<T, String>, attempts: u32) -> Result<T, String> {\n    let mut delay = 1u64;\n    let mut last = String::new();\n    for _ in 0..attempts {\n        match op() {\n            Ok(value) => return Ok(value),\n            Err(e) => last = e,\n        }\n        delay *= 2;\n        let _ = delay;\n    }\n    Err(last)\n}\n",
    ),
    (
        "tests/billing.rs",
        "use app::billing::create_invoice;\n\n\
         fn test_create_invoice_applies_tax() {\n    let total = create_invoice(100, 0);\n    assert_eq!(total, 121);\n}\n",
    ),
    (
        "docs/adrs/001-storage.md",
        "# ADR-001: Storage engine\n\n## Context\n\nWe need an embedded store.\n\n## Decision\n\nUse SQLite with FTS5.\n\n## Alternatives considered\n\n- A key-value store. Rejected: no ranked search.\n\n## Consequences\n\nOne file on disk.\n",
    ),
    (
        "docs/adrs/002-retry-policy.md",
        "# ADR-002: Retry policy\n\n## Context\n\nCalls to the payment gateway fail transiently.\n\n## Decision\n\nRetry with exponential backoff and jitter, implemented in `retry_with_backoff`.\n\n## Alternatives considered\n\n- Fixed delay. Rejected: thundering herd.\n\n## Consequences\n\nLatency grows on persistent failure.\n",
    ),
    (
        "docs/adrs/003-config-format.md",
        "# ADR-003: Configuration format\n\n## Context\n\nOperators edit the file by hand.\n\n## Decision\n\nUse a plain key=value format, see [[001-storage]].\n\n## Alternatives considered\n\n- TOML. Rejected: one more dependency.\n- YAML. Rejected: indentation errors.\n\n## Consequences\n\nNo nested sections.\n",
    ),
    (
        "docs/notes/billing.md",
        "# Billing notes\n\nInvoices come from `create_invoice`, stored as described in [[001-storage]].\n\n## Open questions\n\nWho owns tax rounding?\n",
    ),
];

pub fn write_project() -> PathBuf {
    let files: Vec<(&str, String)> = FILES.iter().map(|(p, t)| (*p, (*t).to_string())).collect();
    write_files(&files)
}

pub fn write_files(files: &[(&str, String)]) -> PathBuf {
    // Tests of one binary run on parallel threads, and the clock ticks in
    // microseconds on some systems: a counter keeps two projects apart.
    static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let dir = std::env::temp_dir().join(format!(
        "mct-retrieval-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::remove_dir_all(&dir).ok();
    for (path, text) in files {
        let at = dir.join(path);
        std::fs::create_dir_all(at.parent().unwrap()).unwrap();
        std::fs::write(at, text).unwrap();
    }
    dir.canonicalize().unwrap()
}

pub async fn server_at(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

/// 1-based line of the first line of `path` containing `marker`.
pub fn line_of(path: &str, marker: &str) -> u32 {
    let text = FILES.iter().find(|(p, _)| *p == path).unwrap().1;
    let at = text
        .lines()
        .position(|l| l.contains(marker))
        .unwrap_or_else(|| panic!("`{marker}` not in {path}"));
    at as u32 + 1
}

#[derive(Clone, Debug)]
pub enum Need {
    /// The definition at this line is in the answers (`path:L<line>`).
    At(&'static str, u32),
    /// This text is in the answers.
    Text(&'static str),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Kind {
    #[default]
    Search,
    Pack,
}

/// What the task is after. Declared per case from the task's wording, before
/// any level is run, never derived from which level succeeded.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Intent {
    /// Where is X defined / which file or section.
    #[default]
    Locate,
    /// Callers, callees, tests, links.
    Relations,
    /// The text of a function body.
    Source,
    /// The text of a Markdown section.
    DocText,
    /// Many candidates, pick one.
    Browse,
    /// There is nothing to find.
    Nothing,
}

/// Cases tuned on (`Dev`) and cases only measured (`Eval`). Assigned by a
/// fixed rule (alternating inside each category), never by outcome.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Split {
    #[default]
    Dev,
    Eval,
}

#[derive(Default)]
pub struct Case {
    pub id: &'static str,
    pub category: &'static str,
    pub kind: Kind,
    pub tool: &'static str,
    pub args: Value,
    pub needs: Vec<Need>,
    /// Answers must not contain any of these.
    pub forbid: Vec<String>,
    /// Definitions that count as relevant rows (precision of a search).
    pub relevant: Vec<(&'static str, u32)>,
    /// The right answer is "nothing found".
    pub expect_none: bool,
    pub intent: Intent,
    pub split: Split,
    /// A budget the task itself imposes (`max_tokens` on every call of every
    /// strategy that can pass one).
    pub task_budget: Option<usize>,
    /// The right answer is an explained refusal: nothing fits, and it says so.
    pub expect_signal: bool,
}

pub fn cases() -> Vec<Case> {
    let at = |p: &'static str, m: &str| Need::At(p, line_of(p, m));
    vec![
        Case {
            id: "locate-function",
            category: "function location",
            kind: Kind::Search,
            tool: "search_symbols_like",
            args: json!({"query": "calculate tax"}),
            needs: vec![at("src/billing.rs", "pub fn calculate_tax")],
            forbid: vec![],
            relevant: vec![(
                "src/billing.rs",
                line_of("src/billing.rs", "pub fn calculate_tax"),
            )],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "locate-function-hybrid",
            category: "function location",
            kind: Kind::Search,
            tool: "hybrid_search_like",
            args: json!({"query": "retry backoff", "alpha": 0.0}),
            needs: vec![at("src/retry.rs", "pub fn retry_with_backoff")],
            forbid: vec![],
            relevant: vec![(
                "src/retry.rs",
                line_of("src/retry.rs", "pub fn retry_with_backoff"),
            )],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "ambiguous-symbol-search",
            category: "ambiguous symbols",
            kind: Kind::Search,
            tool: "search_symbols_like",
            args: json!({"query": "parse"}),
            needs: vec![
                at("src/config.rs", "pub fn parse"),
                at("src/http.rs", "pub fn parse"),
            ],
            forbid: vec![],
            relevant: vec![
                ("src/config.rs", line_of("src/config.rs", "pub fn parse")),
                ("src/http.rs", line_of("src/http.rs", "pub fn parse")),
            ],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "ambiguous-symbol-pack",
            category: "ambiguous symbols",
            kind: Kind::Pack,
            tool: "build_context_pack",
            args: json!({"symbol": "parse", "path": "src/http.rs"}),
            needs: vec![
                at("src/http.rs", "pub fn parse"),
                Need::Text("handle_request"),
            ],
            // The other `parse` must not be presented as the definition.
            forbid: vec![format!(
                "src/config.rs:L{}-L",
                line_of("src/config.rs", "pub fn parse")
            )],
            relevant: vec![],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "references",
            category: "references",
            kind: Kind::Pack,
            tool: "build_context_pack",
            args: json!({"symbol": "create_invoice"}),
            needs: vec![Need::Text("test_create_invoice_applies_tax")],
            forbid: vec![],
            relevant: vec![],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "dependencies",
            category: "dependencies",
            kind: Kind::Pack,
            tool: "build_context_pack",
            args: json!({"symbol": "create_invoice"}),
            needs: vec![Need::Text("calculate_tax"), Need::Text("apply_discount")],
            forbid: vec![],
            relevant: vec![],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "needs-the-body",
            category: "function location",
            kind: Kind::Pack,
            tool: "build_context_pack",
            args: json!({"symbol": "create_invoice"}),
            // Only a level that shows source can satisfy this one.
            needs: vec![Need::Text("let taxed = calculate_tax(discounted);")],
            forbid: vec![],
            relevant: vec![],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "markdown-section",
            category: "markdown sections",
            kind: Kind::Pack,
            tool: "build_context_pack",
            args: json!({"symbol": "Decision", "path": "docs/adrs/002-retry-policy.md"}),
            needs: vec![
                at("docs/adrs/002-retry-policy.md", "## Decision"),
                Need::Text("jitter"),
            ],
            forbid: vec!["Use SQLite with FTS5".to_string()],
            relevant: vec![],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "adr-decision-lookup",
            category: "ADR decisions",
            kind: Kind::Search,
            tool: "search_symbols_like",
            args: json!({"query": "alternatives considered"}),
            // Three ADRs share the heading; the labelled one is the last.
            needs: vec![at(
                "docs/adrs/003-config-format.md",
                "## Alternatives considered",
            )],
            forbid: vec![],
            relevant: vec![(
                "docs/adrs/003-config-format.md",
                line_of(
                    "docs/adrs/003-config-format.md",
                    "## Alternatives considered",
                ),
            )],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "mixed-code-and-docs",
            category: "mixed code and documentation",
            kind: Kind::Search,
            tool: "search_symbols_like",
            args: json!({"query": "retry"}),
            needs: vec![
                at("src/retry.rs", "pub fn retry_with_backoff"),
                at("docs/adrs/002-retry-policy.md", "# ADR-002"),
            ],
            forbid: vec![],
            relevant: vec![
                (
                    "src/retry.rs",
                    line_of("src/retry.rs", "pub fn retry_with_backoff"),
                ),
                (
                    "docs/adrs/002-retry-policy.md",
                    line_of("docs/adrs/002-retry-policy.md", "# ADR-002"),
                ),
            ],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "wikilink-dependency",
            category: "dependencies",
            kind: Kind::Pack,
            tool: "build_context_pack",
            args: json!({"symbol": "Billing notes", "path": "docs/notes/billing.md"}),
            needs: vec![Need::Text("001-storage")],
            forbid: vec![],
            relevant: vec![],
            expect_none: false,
            ..Default::default()
        },
        Case {
            id: "no-evidence",
            category: "insufficient evidence",
            kind: Kind::Search,
            tool: "search_symbols_like",
            args: json!({"query": "zzqxv unobtainium"}),
            needs: vec![],
            forbid: vec![],
            relevant: vec![],
            expect_none: true,
            ..Default::default()
        },
    ]
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Strategy {
    /// The tool as it was, no new arguments.
    Default,
    Overview,
    Summary,
    /// `detail=detailed` inside an estimated budget, one call.
    DetailedBudget,
    /// `detail=overview` inside a small budget, following `next: offset=`.
    OverviewBudgetFollow,
    /// Overview first, then `detailed` for the same target (packs).
    OverviewThenDetailed,
    /// `detail=detailed`, no budget, as the task's own budget allows.
    DetailedDirect,
    /// `overview`, then `detailed` only if the evidence is still missing.
    OverviewEscalate,
    /// `detailed` inside the class budget, following `next: offset=`.
    DetailedBudgetFollow,
    /// Level chosen from what the server could see (tool, query, path), then
    /// `detailed` if the evidence is still missing.
    AutoRules,
    /// Level chosen from the declared intent of the task, then `detailed` if
    /// the evidence is still missing. Needs a caller that knows its intent.
    IntentRules,
}

impl Strategy {
    pub const ALL: [Strategy; 6] = [
        Strategy::Default,
        Strategy::Overview,
        Strategy::Summary,
        Strategy::DetailedBudget,
        Strategy::OverviewBudgetFollow,
        Strategy::OverviewThenDetailed,
    ];

    /// The strategies of the extended set, baseline first.
    pub const EXTENDED: [Strategy; 7] = [
        Strategy::Default,
        Strategy::DetailedDirect,
        Strategy::Summary,
        Strategy::OverviewEscalate,
        Strategy::DetailedBudgetFollow,
        Strategy::AutoRules,
        Strategy::IntentRules,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Strategy::DetailedDirect => "detailed, direct",
            Strategy::OverviewEscalate => "overview, escalate if missing",
            Strategy::DetailedBudgetFollow => "detailed + budget, follow next",
            Strategy::AutoRules => "auto (query-only rules)",
            Strategy::IntentRules => "intent rules (caller knows its intent)",
            Strategy::Default => "default",
            Strategy::Overview => "overview",
            Strategy::Summary => "summary",
            Strategy::DetailedBudget => "detailed + budget",
            Strategy::OverviewBudgetFollow => "overview + small budget, follow next",
            Strategy::OverviewThenDetailed => "overview then detailed",
        }
    }

    fn applies(self, case: &Case) -> bool {
        match self {
            Strategy::OverviewThenDetailed => case.kind == Kind::Pack,
            // Pages only exist for searches; a pack is a single response.
            Strategy::OverviewBudgetFollow => case.kind == Kind::Search,
            _ => true,
        }
    }
}

/// Budget for single-call strategies and for each page of the follow one.
pub fn budget(kind: Kind, strategy: Strategy) -> usize {
    match (kind, strategy) {
        (Kind::Pack, _) => 350,
        (Kind::Search, Strategy::OverviewBudgetFollow) => 90,
        (Kind::Search, _) => 200,
    }
}

#[derive(Debug, Clone)]
pub struct Outcome {
    pub case: &'static str,
    pub strategy: Strategy,
    pub calls: usize,
    pub estimated: usize,
    pub chars4: usize,
    pub micros: u128,
    /// Every need present, nothing forbidden (or "nothing" for `expect_none`).
    pub satisfied: bool,
    /// Needs present / needs total.
    pub recall: f64,
    /// Relevant rows / rows returned, for searches with labelled rows.
    pub precision: Option<f64>,
    /// The answers carry some marker saying they are lowered, cut, or lower
    /// level than the maximum.
    pub signalled: bool,
    /// Evidence missing although no answer said anything was left out.
    pub silent_miss: bool,
    /// The raw responses, in call order.
    pub answers: Vec<String>,
}

fn text_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|b| b.as_text())
        .map(|t| t.text.clone())
        .collect::<Vec<_>>()
        .join("\n")
}

fn merge(base: &Value, extra: Value, format: &str) -> Value {
    let mut args = base.clone();
    args["format"] = json!(format);
    if let (Some(a), Some(e)) = (args.as_object_mut(), extra.as_object()) {
        a.extend(e.clone());
    }
    args
}

/// What the answer itself tells a caller to change: rows omitted for want of
/// `limit`, or source cut at `source_lines`. `None` when it says neither.
fn obey_notice(case: &Case, text: &str) -> Option<Value> {
    if text.contains("omitted") && text.contains("higher `limit`") {
        let total: usize = text.split_whitespace().next()?.parse().ok()?;
        return Some(json!({"limit": total}));
    }
    if text.contains("raise `source_lines`") && case.args.get("source_lines").is_none() {
        return Some(json!({"source_lines": 200}));
    }
    None
}

fn next_offset(text: &str) -> Option<usize> {
    text.split("next: offset=")
        .nth(1)
        .and_then(|r| r.split(['\n', ';']).next())
        .and_then(|n| n.trim().parse().ok())
}

/// One request shape: a level and an estimated budget (either may be absent).
#[derive(Clone, Copy, Debug)]
pub struct Step {
    pub detail: Option<&'static str>,
    pub max_tokens: Option<usize>,
}

/// Level for a call decided from what a server could see before running it:
/// the tool, the query or symbol text, the path. Rule set 1; see the doc in
/// `benchmarks/adaptive-detail.md` for how it was tuned.
pub fn auto_step(case: &Case) -> Step {
    let doc_path = case.args["path"]
        .as_str()
        .is_some_and(|p| p.ends_with(".md"));
    let detail = match case.kind {
        Kind::Pack if doc_path => "detailed",
        _ => "overview",
    };
    Step {
        detail: Some(detail),
        max_tokens: None,
    }
}

/// Level for a call decided from what the *task* wants, which only the caller
/// knows. An upper bound for any server-side rule.
pub fn intent_step(case: &Case) -> Step {
    let detail = match case.intent {
        Intent::Source | Intent::DocText => "detailed",
        _ => "overview",
    };
    Step {
        detail: Some(detail),
        max_tokens: None,
    }
}

/// One step, then pages while the answer says `partial` and the evidence is
/// still missing (only when a budget cuts pages). The task's own budget wins.
async fn drive(
    server: &MctServer,
    case: &Case,
    format: &str,
    step: Step,
    answers: &mut Vec<String>,
) {
    let max = case.task_budget.or(step.max_tokens);
    let mut offset = 0usize;
    for _ in 0..30 {
        let mut extra = json!({"cache": false});
        if let Some(detail) = step.detail {
            extra["detail"] = json!(detail);
        }
        if let Some(max) = max {
            extra["max_tokens"] = json!(max);
        }
        if offset > 0 {
            extra["offset"] = json!(offset);
        }
        let result = server
            .call_read_only_tool(
                case.tool,
                merge(&case.args, extra, format).as_object().cloned(),
            )
            .await
            .unwrap();
        let text = text_of(&result);
        let next = next_offset(&text);
        let notice = obey_notice(case, &text);
        answers.push(text);
        if satisfied(case, &answers.join("\n")) {
            break;
        }
        // Obey the page's own continuation, budget or not.
        if let Some(n) = next.filter(|n| *n > offset) {
            offset = n;
            continue;
        }
        if let (Some(mut again), 0) = (notice, offset) {
            // The page's own "pass a higher `limit`" / "raise `source_lines`".
            again["cache"] = json!(false);
            if let Some(detail) = step.detail {
                again["detail"] = json!(detail);
            }
            let result = server
                .call_read_only_tool(
                    case.tool,
                    merge(&case.args, again, format).as_object().cloned(),
                )
                .await
                .unwrap();
            answers.push(text_of(&result));
        }
        break;
    }
}

/// `drive`, then the same target at `detailed` if the evidence is missing.
async fn drive_escalating(
    server: &MctServer,
    case: &Case,
    format: &str,
    first: Step,
    answers: &mut Vec<String>,
) {
    drive(server, case, format, first, answers).await;
    if !satisfied(case, &answers.join("\n")) && first.detail != Some("detailed") {
        let detailed = Step {
            detail: Some("detailed"),
            max_tokens: first.max_tokens,
        };
        drive(server, case, format, detailed, answers).await;
    }
}

const SIGNALS: [&str; 5] = [
    "Budget:",
    "omitted",
    "No row fits",
    "none fit",
    "more line(s)",
];

pub async fn run(
    server: &MctServer,
    case: &Case,
    strategy: Strategy,
    format: &str,
) -> Option<Outcome> {
    if !strategy.applies(case) {
        return None;
    }
    let max = budget(case.kind, strategy);
    let mut answers: Vec<String> = Vec::new();
    let start = Instant::now();
    let call = |args: Value| {
        let tool = case.tool;
        async move {
            let result = server
                .call_read_only_tool(tool, args.as_object().cloned())
                .await
                .unwrap();
            text_of(&result)
        }
    };
    let step =
        |detail: Option<&'static str>, max_tokens: Option<usize>| Step { detail, max_tokens };
    match strategy {
        Strategy::Default => {
            let text = call(merge(&case.args, json!({"cache": false}), format)).await;
            let again = obey_notice(case, &text).filter(|_| !satisfied(case, &text));
            answers.push(text);
            if let Some(mut extra) = again {
                // The notice the unchanged tool prints; the cost of obeying it.
                extra["cache"] = json!(false);
                answers.push(call(merge(&case.args, extra, format)).await);
            }
        }
        Strategy::Overview => {
            drive(
                server,
                case,
                format,
                step(Some("overview"), None),
                &mut answers,
            )
            .await
        }
        Strategy::Summary => {
            drive(
                server,
                case,
                format,
                step(Some("summary"), None),
                &mut answers,
            )
            .await
        }
        Strategy::DetailedDirect => {
            drive(
                server,
                case,
                format,
                step(Some("detailed"), None),
                &mut answers,
            )
            .await
        }
        Strategy::OverviewEscalate => {
            drive_escalating(
                server,
                case,
                format,
                step(Some("overview"), None),
                &mut answers,
            )
            .await
        }
        Strategy::DetailedBudgetFollow => {
            drive(
                server,
                case,
                format,
                step(Some("detailed"), Some(max)),
                &mut answers,
            )
            .await
        }
        Strategy::AutoRules => {
            drive_escalating(server, case, format, auto_step(case), &mut answers).await
        }
        Strategy::IntentRules => {
            drive_escalating(server, case, format, intent_step(case), &mut answers).await
        }
        Strategy::DetailedBudget => answers.push(
            call(merge(
                &case.args,
                json!({"detail": "detailed", "max_tokens": max}),
                format,
            ))
            .await,
        ),
        Strategy::OverviewThenDetailed => {
            answers.push(call(merge(&case.args, json!({"detail": "overview"}), format)).await);
            answers.push(call(merge(&case.args, json!({"detail": "detailed"}), format)).await);
        }
        Strategy::OverviewBudgetFollow => {
            let mut offset = 0usize;
            for _ in 0..30 {
                let text = call(merge(
                    &case.args,
                    json!({"detail": "overview", "max_tokens": max, "offset": offset}),
                    format,
                ))
                .await;
                let next = next_offset(&text);
                let all = answers.join("\n") + &text;
                answers.push(text);
                if satisfied(case, &all) || next.is_none() {
                    break;
                }
                match next {
                    Some(n) if n > offset => offset = n,
                    _ => break,
                }
            }
        }
    }
    let micros = start.elapsed().as_micros();
    let all = answers.join("\n");
    let ok = satisfied(case, &all);
    let signalled = answers
        .iter()
        .any(|a| SIGNALS.iter().any(|s| a.contains(s)));
    Some(Outcome {
        case: case.id,
        strategy,
        calls: answers.len(),
        estimated: answers
            .iter()
            .map(|a| EstimatedCounter.count(a).value)
            .sum(),
        chars4: answers.iter().map(|a| a.chars().count().div_ceil(4)).sum(),
        micros,
        satisfied: ok,
        recall: recall(case, &all),
        precision: precision(case, &answers),
        signalled,
        silent_miss: !ok && !signalled && !case.expect_none,
        answers,
    })
}

fn has(need: &Need, text: &str) -> bool {
    match need {
        // `path:L12` must not match `path:L123-...`.
        Need::At(path, line) => {
            let needle = format!("{path}:L{line}");
            text.match_indices(&needle).any(|(at, _)| {
                !text[at + needle.len()..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_digit())
            })
        }
        Need::Text(t) => text.contains(t),
    }
}

fn recall(case: &Case, all: &str) -> f64 {
    if case.needs.is_empty() {
        return 1.0;
    }
    case.needs.iter().filter(|n| has(n, all)).count() as f64 / case.needs.len() as f64
}

fn satisfied(case: &Case, all: &str) -> bool {
    if case.expect_signal {
        return all.contains("none fit `max_tokens`") && !all.contains(":L");
    }
    if case.expect_none {
        // Nothing found, and the answer says so instead of inventing rows.
        return (all.contains("No symbol matches") || all.contains("No symbol named"))
            && !all.contains(":L");
    }
    case.needs.iter().all(|n| has(n, all)) && !case.forbid.iter().any(|f| all.contains(f.as_str()))
}

/// Relevant rows over rows returned, counting each distinct `path:Lline` once.
fn precision(case: &Case, answers: &[String]) -> Option<f64> {
    if case.relevant.is_empty() {
        return None;
    }
    let mut rows = std::collections::BTreeSet::new();
    for answer in answers {
        for line in answer.lines() {
            let head = line.split_whitespace().next().unwrap_or("");
            if let Some((path, rest)) = head.split_once(":L") {
                let start = rest.split('-').next().unwrap_or("");
                if start.parse::<u32>().is_ok() && line.contains(" [") {
                    rows.insert(format!("{path}:L{start}"));
                }
            }
        }
    }
    if rows.is_empty() {
        return Some(0.0);
    }
    let relevant = rows
        .iter()
        .filter(|row| {
            case.relevant
                .iter()
                .any(|(p, l)| **row == format!("{p}:L{l}"))
        })
        .count();
    Some(relevant as f64 / rows.len() as f64)
}

pub async fn run_all(server: &MctServer) -> Vec<Outcome> {
    run_all_in(server, "text").await
}

/// Same cases in `format` (`text` or `toon`). The substring checks assume the
/// `path:Lline` shape of `text`; other formats are for token measurements.
pub async fn run_all_in(server: &MctServer, format: &str) -> Vec<Outcome> {
    let mut out = Vec::new();
    for case in cases() {
        for strategy in Strategy::ALL {
            if let Some(o) = run(server, &case, strategy, format).await {
                out.push(o);
            }
        }
    }
    out
}

/// Every strategy of `strategies` over `cases`, `repeat` times each. Tokens
/// and answers are deterministic; `micros` is the median of the repeats.
pub async fn run_cases(
    server: &MctServer,
    cases: &[Case],
    strategies: &[Strategy],
    format: &str,
    repeat: usize,
) -> Vec<Outcome> {
    let mut out = Vec::new();
    for case in cases {
        for &strategy in strategies {
            let mut runs = Vec::new();
            for _ in 0..repeat.max(1) {
                if let Some(o) = run(server, case, strategy, format).await {
                    runs.push(o);
                }
            }
            if runs.is_empty() {
                continue;
            }
            let mut micros: Vec<u128> = runs.iter().map(|o| o.micros).collect();
            micros.sort_unstable();
            let mut first = runs.swap_remove(0);
            first.micros = micros[micros.len() / 2];
            out.push(first);
        }
    }
    out
}
