//! The extended labelled set: a larger generated corpus and 50+ cases meant to
//! be hard in the ways real repositories are: names defined several times,
//! many hits for one word, a function longer than the pack's source cut,
//! headings shared by many documents, evidence that exists nowhere, and
//! budgets too small for the answer.
//!
//! Nothing here was written to make a strategy look good. Evidence is a
//! `path:line` computed from the generated text (never from a tool's output)
//! or an exact string of the corpus. Cases the unchanged tools cannot answer
//! stay in the set; that is a finding, not a bug of the set.
//!
//! Dev/eval: inside each category the cases alternate `Dev`, `Eval`, `Dev`, ...
//! in the order written below. Rules for the adaptive simulation are tuned on
//! `Dev` only; `Eval` is measured once afterwards.

#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::{json, Value};

use super::retrieval::{Case, Intent, Kind, Need, Split, FILES};

pub type Corpus = Vec<(&'static str, String)>;

const HANDLERS: [(&str, &str); 6] = [
    ("src/handlers/users.rs", "users"),
    ("src/handlers/orders.rs", "orders"),
    ("src/handlers/payments.rs", "payments"),
    ("src/handlers/inventory.rs", "inventory"),
    ("src/handlers/shipping.rs", "shipping"),
    ("src/handlers/audit.rs", "audit"),
];

/// Handlers that define their own `validate` (three files, one name).
const VALIDATING: [&str; 3] = ["users", "orders", "payments"];

fn handler(name: &str) -> String {
    let validating = VALIDATING.contains(&name);
    let check = if validating {
        "validate(input)?;"
    } else {
        "let _ = crate::util::clamp(input.len() as u64, 0, 4096);"
    };
    let validate = if validating {
        format!(
            "/// Validate a {name} request body.\npub fn validate(input: &str) -> Result<(), String> {{\n    if input.is_empty() {{\n        return Err(\"{name}: empty body\".to_string());\n    }}\n    Ok(())\n}}\n\n"
        )
    } else {
        String::new()
    };
    format!(
        "//! {name} handlers.\nuse crate::logging::log_event;\n\n{validate}/// Create a {name} record.\npub fn handle_{name}_create(input: &str) -> Result<u64, String> {{\n    {check}\n    log_event(\"{name}.create\");\n    Ok(input.len() as u64)\n}}\n\n/// Update a {name} record.\npub fn handle_{name}_update(id: u64, input: &str) -> Result<u64, String> {{\n    {check}\n    log_event(\"{name}.update\");\n    Ok(id + input.len() as u64)\n}}\n\n/// Delete a {name} record.\npub fn handle_{name}_delete(id: u64) -> bool {{\n    log_event(\"{name}.delete\");\n    id > 0\n}}\n\n/// List {name} records.\npub fn handle_{name}_list(limit: usize) -> Vec<u64> {{\n    log_event(\"{name}.list\");\n    (0..limit as u64).collect()\n}}\n"
    )
}

fn engine() -> String {
    let mut text = String::from("//! Processing pipeline.\nuse crate::logging::log_event;\n\n");
    for n in 1..=12 {
        text +=
            &format!("fn stage_{n:02}(v: u64) -> u64 {{\n    v.wrapping_mul({n}) + {n}\n}}\n\n");
    }
    text += "/// Run all twelve stages in order.\npub fn run_pipeline(input: u64) -> u64 {\n    let mut value = input;\n";
    for n in 1..=12 {
        text += &format!(
            "    log_event(\"pipeline.stage_{n:02}\");\n    value = stage_{n:02}(value);\n    if value > {limit} {{\n        value %= {limit};\n    }}\n",
            limit = 1000 + n * 7
        );
    }
    text += "    value\n}\n";
    text
}

const ADRS: [(&str, &str, &str, &str, &str); 9] = [
    (
        "docs/adrs/004-auth-tokens.md",
        "Authentication tokens",
        "Clients must prove who they are on every call.",
        "Issue short-lived signed bearer tokens.",
        "- Session cookies. Rejected: awkward for non-browser clients.",
    ),
    (
        "docs/adrs/005-rate-limiting.md",
        "Rate limiting",
        "A burst from one client starved the others.",
        "Enforce a token bucket in `handle_payments_create` and `handle_orders_create`.",
        "- Fixed windows. Rejected: bursts at window edges.",
    ),
    (
        "docs/adrs/006-logging.md",
        "Structured logging",
        "Operators grep unstructured lines during incidents.",
        "Emit one structured event per operation through `log_event`.",
        "- Free-form text. Rejected: not machine readable.",
    ),
    (
        "docs/adrs/007-pagination.md",
        "Pagination",
        "List endpoints returned unbounded arrays.",
        "Page with opaque cursors and a fixed maximum page size.",
        "- Offset pages. Rejected: unstable while rows are written.",
    ),
    (
        "docs/adrs/008-caching.md",
        "Response caching",
        "Repeated reads dominated database load.",
        "Cache responses for sixty seconds keyed by `cache_key`.",
        "- No cache. Rejected: load.\n- Cache forever. Rejected: stale reads after updates.",
    ),
    (
        "docs/adrs/009-error-model.md",
        "Error model",
        "Callers could not tell a retryable failure from a fatal one.",
        "Return typed errors with a stable code and a retry hint.",
        "- Plain strings. Rejected: callers parse text.",
    ),
    (
        "docs/adrs/010-versioning.md",
        "API versioning",
        "Breaking changes surprised existing clients.",
        "Version the API in the path and keep two versions alive.",
        "- Header versioning. Rejected: invisible in logs.",
    ),
    (
        "docs/adrs/011-deployment.md",
        "Deployment",
        "Releases were manual and slow.",
        "Deploy from the main branch with a staged rollout.",
        "- Weekly releases. Rejected: large batches.",
    ),
    (
        "docs/adrs/012-observability.md",
        "Observability",
        "We could not see saturation before users did.",
        "Export request rate, error rate and latency per handler.",
        "- Logs only. Rejected: no aggregates.",
    ),
];

fn adr(number: &str, title: &str, context: &str, decision: &str, alternatives: &str) -> String {
    format!(
        "# ADR-{number}: {title}\n\n## Context\n\n{context}\n\n## Decision\n\n{decision}\n\n## Alternatives considered\n\n{alternatives}\n\n## Consequences\n\nSee the follow-up notes.\n"
    )
}

pub fn corpus() -> Corpus {
    let mut files: Corpus = FILES.iter().map(|(p, t)| (*p, (*t).to_string())).collect();
    for (path, name) in HANDLERS {
        files.push((path, handler(name)));
    }
    files.push((
        "src/logging.rs",
        "//! Structured logging.\n/// Emit one structured event.\npub fn log_event(name: &str) {\n    let _ = name;\n}\n\n/// Emit one structured error event.\npub fn log_error(name: &str, reason: &str) {\n    let _ = (name, reason);\n}\n".to_string(),
    ));
    files.push((
        "src/util.rs",
        "//! Small numeric helpers.\n/// Keep `value` inside `low..=high`.\npub fn clamp(value: u64, low: u64, high: u64) -> u64 {\n    if value < low {\n        low\n    } else if value > high {\n        high\n    } else {\n        value\n    }\n}\n\n/// Linear interpolation between `a` and `b`.\npub fn lerp(a: f64, b: f64, t: f64) -> f64 {\n    a + (b - a) * t\n}\n\n/// True for even numbers.\npub fn is_even(n: u64) -> bool {\n    n % 2 == 0\n}\n".to_string(),
    ));
    files.push((
        "src/cache.rs",
        "//! Response cache.\n/// Key a cached response by route and user.\npub fn cache_key(route: &str, user: u64) -> String {\n    format!(\"{route}:{user}\")\n}\n".to_string(),
    ));
    files.push(("src/engine.rs", engine()));
    files.push((
        "tests/handlers.rs",
        "fn test_orders_create_rejects_empty_body() {\n    let result = app::handlers::orders::handle_orders_create(\"\");\n    assert!(result.is_err());\n}\n\nfn test_users_create_counts_bytes() {\n    let created = app::handlers::users::handle_users_create(\"abc\");\n    assert_eq!(created, Ok(3));\n}\n".to_string(),
    ));
    for (i, (path, title, context, decision, alternatives)) in ADRS.iter().enumerate() {
        files.push((
            path,
            adr(
                &format!("{:03}", i + 4),
                title,
                context,
                decision,
                alternatives,
            ),
        ));
    }
    files.push((
        "docs/guides/operations.md",
        "# Operations guide\n\n## Overview\n\nHow the service is run.\n\n## Setup\n\nProvision the database first.\n\n## Troubleshooting\n\nIf requests stall, restart the worker and check the queue depth.\n".to_string(),
    ));
    files.push((
        "docs/guides/development.md",
        "# Development guide\n\n## Overview\n\nHow the code is organised.\n\n## Setup\n\nInstall the toolchain, then run cargo test before opening a change.\n\n## Troubleshooting\n\nIf the build fails, clear the target directory.\n".to_string(),
    ));
    files.push((
        "docs/guides/security.md",
        "# Security guide\n\n## Overview\n\nHow secrets are handled.\n\n## Setup\n\nStore secrets in the vault.\n\n## Troubleshooting\n\nIf a token leaks, rotate the signing key immediately.\n".to_string(),
    ));
    files.push((
        "docs/notes/shipping.md",
        "# Shipping notes\n\nCarrier selection follows [[011-deployment]] for rollout and `handle_shipping_update` for changes.\n\n## Open questions\n\nWho owns carrier contracts?\n".to_string(),
    ));
    files
}

fn line_in(files: &Corpus, path: &'static str, marker: &str) -> u32 {
    let text = &files
        .iter()
        .find(|(p, _)| *p == path)
        .unwrap_or_else(|| panic!("{path} not in the corpus"))
        .1;
    let at = text
        .lines()
        .position(|l| l.contains(marker))
        .unwrap_or_else(|| panic!("`{marker}` not in {path}"));
    at as u32 + 1
}

struct B<'a> {
    files: &'a Corpus,
    cases: Vec<Case>,
}

impl B<'_> {
    fn def(&self, path: &'static str, marker: &str) -> Need {
        Need::At(path, line_in(self.files, path, marker))
    }

    fn row(&self, path: &'static str, marker: &str) -> (&'static str, u32) {
        (path, line_in(self.files, path, marker))
    }

    #[allow(clippy::too_many_arguments)]
    fn add(
        &mut self,
        id: &'static str,
        category: &'static str,
        intent: Intent,
        kind: Kind,
        args: Value,
        needs: Vec<Need>,
        relevant: Vec<(&'static str, u32)>,
    ) -> &mut Case {
        let tool = match (kind, args.get("alpha").is_some()) {
            (Kind::Pack, _) => "build_context_pack",
            (Kind::Search, true) => "hybrid_search_like",
            (Kind::Search, false) => "search_symbols_like",
        };
        self.cases.push(Case {
            id,
            category,
            kind,
            tool,
            args,
            needs,
            relevant,
            intent,
            ..Default::default()
        });
        self.cases.last_mut().unwrap()
    }
}

/// The extended cases over `corpus()`.
pub fn cases(files: &Corpus) -> Vec<Case> {
    use Intent::{Browse, DocText, Locate, Nothing, Relations, Source};
    use Kind::{Pack, Search};
    let mut b = B {
        files,
        cases: Vec::new(),
    };

    // --- exact symbols
    let r = b.row("src/billing.rs", "pub fn calculate_tax");
    let n = b.def("src/billing.rs", "pub fn calculate_tax");
    b.add(
        "exact-calculate-tax",
        "exact symbols",
        Locate,
        Search,
        json!({"query": "calculate_tax"}),
        vec![n],
        vec![r],
    );
    let r = b.row("src/retry.rs", "pub fn retry_with_backoff");
    let n = b.def("src/retry.rs", "pub fn retry_with_backoff");
    b.add(
        "exact-retry",
        "exact symbols",
        Locate,
        Search,
        json!({"query": "retry_with_backoff"}),
        vec![n],
        vec![r],
    );
    let r = b.row("src/util.rs", "pub fn lerp");
    let n = b.def("src/util.rs", "pub fn lerp");
    b.add(
        "exact-lerp",
        "exact symbols",
        Locate,
        Search,
        json!({"query": "lerp"}),
        vec![n],
        vec![r],
    );
    let n = b.def("src/config.rs", "pub struct Config");
    b.add(
        "exact-config-struct",
        "exact symbols",
        Locate,
        Search,
        json!({"query": "Config"}),
        vec![n],
        vec![],
    );
    let r = b.row("src/engine.rs", "fn stage_07");
    let n = b.def("src/engine.rs", "fn stage_07");
    b.add(
        "exact-stage-07",
        "exact symbols",
        Locate,
        Search,
        json!({"query": "stage_07"}),
        vec![n],
        vec![r],
    );
    let n = b.def("src/handlers/shipping.rs", "pub fn handle_shipping_delete");
    b.add(
        "exact-shipping-delete",
        "exact symbols",
        Locate,
        Pack,
        json!({"symbol": "handle_shipping_delete"}),
        vec![n],
        vec![],
    );
    let n = b.def("src/engine.rs", "pub fn run_pipeline");
    b.add(
        "exact-run-pipeline",
        "exact symbols",
        Locate,
        Pack,
        json!({"symbol": "run_pipeline"}),
        vec![n],
        vec![],
    );
    let r = b.row("src/http.rs", "fn route");
    let n = b.def("src/http.rs", "fn route");
    b.add(
        "exact-route",
        "exact symbols",
        Locate,
        Search,
        json!({"query": "route"}),
        vec![n],
        vec![r],
    );

    // --- ambiguous symbols
    let needs = vec![
        b.def("src/handlers/users.rs", "pub fn validate"),
        b.def("src/handlers/orders.rs", "pub fn validate"),
        b.def("src/handlers/payments.rs", "pub fn validate"),
    ];
    let relevant = vec![
        b.row("src/handlers/users.rs", "pub fn validate"),
        b.row("src/handlers/orders.rs", "pub fn validate"),
        b.row("src/handlers/payments.rs", "pub fn validate"),
    ];
    b.add(
        "ambiguous-validate-search",
        "ambiguous symbols",
        Locate,
        Search,
        json!({"query": "validate"}),
        needs,
        relevant,
    );
    let n = b.def("src/handlers/orders.rs", "pub fn validate");
    let forbid = format!(
        "src/handlers/users.rs:L{}-L",
        line_in(files, "src/handlers/users.rs", "pub fn validate")
    );
    b.add(
        "ambiguous-validate-pack-orders",
        "ambiguous symbols",
        Locate,
        Pack,
        json!({"symbol": "validate", "path": "src/handlers/orders.rs"}),
        vec![n],
        vec![],
    )
    .forbid = vec![forbid];
    b.add(
        "ambiguous-validate-body",
        "ambiguous symbols",
        Source,
        Pack,
        json!({"symbol": "validate", "path": "src/handlers/payments.rs"}),
        vec![Need::Text("payments: empty body")],
        vec![],
    )
    .forbid = vec!["users: empty body".to_string()];
    let n = b.def("src/http.rs", "pub fn parse");
    let forbid = format!(
        "src/config.rs:L{}-L",
        line_in(files, "src/config.rs", "pub fn parse")
    );
    b.add(
        "ambiguous-parse-http",
        "ambiguous symbols",
        Locate,
        Pack,
        json!({"symbol": "parse", "path": "src/http.rs"}),
        vec![n, Need::Text("handle_request")],
        vec![],
    )
    .forbid = vec![forbid];
    b.add(
        "ambiguous-parse-config-body",
        "ambiguous symbols",
        Source,
        Pack,
        json!({"symbol": "parse", "path": "src/config.rs"}),
        vec![Need::Text("bad port")],
        vec![],
    )
    .forbid = vec!["raw.split_whitespace".to_string()];
    let needs = vec![
        b.def("src/billing.rs", "pub fn create_invoice"),
        b.def("src/handlers/orders.rs", "pub fn handle_orders_create"),
    ];
    b.add(
        "ambiguous-create-search",
        "ambiguous symbols",
        Locate,
        Search,
        json!({"query": "create"}),
        needs,
        vec![],
    );

    // --- references and dependencies
    b.add(
        "refs-calculate-tax-caller",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "calculate_tax"}),
        vec![Need::Text("create_invoice")],
        vec![],
    );
    b.add(
        "refs-invoice-test",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "create_invoice"}),
        vec![Need::Text("test_create_invoice_applies_tax")],
        vec![],
    );
    b.add(
        "deps-create-invoice",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "create_invoice"}),
        vec![Need::Text("calculate_tax"), Need::Text("apply_discount")],
        vec![],
    );
    b.add(
        "deps-run-pipeline",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "run_pipeline"}),
        vec![Need::Text("stage_01"), Need::Text("stage_12")],
        vec![],
    );
    b.add(
        "refs-log-event-callers",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "log_event"}),
        vec![
            Need::Text("handle_users_create"),
            Need::Text("handle_audit_list"),
        ],
        vec![],
    );
    b.add(
        "refs-validate-orders-caller",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "validate", "path": "src/handlers/orders.rs"}),
        vec![Need::Text("handle_orders_create")],
        vec![],
    );
    b.add(
        "wikilink-billing-notes",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "Billing notes", "path": "docs/notes/billing.md"}),
        vec![Need::Text("001-storage")],
        vec![],
    );
    b.add(
        "wikilink-shipping-notes",
        "references and dependencies",
        Relations,
        Pack,
        json!({"symbol": "Shipping notes", "path": "docs/notes/shipping.md"}),
        vec![Need::Text("011-deployment")],
        vec![],
    );

    // --- small and large functions
    b.add(
        "small-clamp-body",
        "small and large functions",
        Source,
        Pack,
        json!({"symbol": "clamp"}),
        vec![Need::Text("else if value > high")],
        vec![],
    );
    b.add(
        "small-lerp-body",
        "small and large functions",
        Source,
        Pack,
        json!({"symbol": "lerp"}),
        vec![Need::Text("a + (b - a) * t")],
        vec![],
    );
    b.add(
        "large-pipeline-start",
        "small and large functions",
        Source,
        Pack,
        json!({"symbol": "run_pipeline"}),
        vec![Need::Text("value = stage_01(value);")],
        vec![],
    );
    b.add(
        "large-pipeline-end-wide",
        "small and large functions",
        Source,
        Pack,
        json!({"symbol": "run_pipeline", "source_lines": 200}),
        vec![Need::Text("value = stage_12(value);")],
        vec![],
    );
    b.add(
        "large-pipeline-end-default",
        "small and large functions",
        Source,
        Pack,
        json!({"symbol": "run_pipeline"}),
        vec![Need::Text("value = stage_12(value);")],
        vec![],
    );
    let r = b.row("src/util.rs", "pub fn is_even");
    let n = b.def("src/util.rs", "pub fn is_even");
    b.add(
        "small-is-even",
        "small and large functions",
        Locate,
        Search,
        json!({"query": "is even"}),
        vec![n],
        vec![r],
    );

    // --- Markdown sections
    let n = b.def("docs/adrs/005-rate-limiting.md", "## Decision");
    b.add(
        "md-decision-rate-limiting",
        "markdown sections",
        DocText,
        Pack,
        json!({"symbol": "Decision", "path": "docs/adrs/005-rate-limiting.md"}),
        vec![n, Need::Text("token bucket")],
        vec![],
    )
    .forbid = vec!["Issue short-lived signed bearer tokens".to_string()];
    b.add(
        "md-consequences-caching",
        "markdown sections",
        DocText,
        Pack,
        json!({"symbol": "Alternatives considered", "path": "docs/adrs/008-caching.md"}),
        vec![Need::Text("stale reads")],
        vec![],
    );
    b.add(
        "md-troubleshooting-operations",
        "markdown sections",
        DocText,
        Pack,
        json!({"symbol": "Troubleshooting", "path": "docs/guides/operations.md"}),
        vec![Need::Text("restart the worker")],
        vec![],
    )
    .forbid = vec!["rotate the signing key".to_string()];
    b.add(
        "md-troubleshooting-security",
        "markdown sections",
        DocText,
        Pack,
        json!({"symbol": "Troubleshooting", "path": "docs/guides/security.md"}),
        vec![Need::Text("rotate the signing key")],
        vec![],
    )
    .forbid = vec!["restart the worker".to_string()];
    b.add(
        "md-setup-development",
        "markdown sections",
        DocText,
        Pack,
        json!({"symbol": "Setup", "path": "docs/guides/development.md"}),
        vec![Need::Text("cargo test")],
        vec![],
    );
    let n = b.def("docs/guides/security.md", "## Overview");
    b.add(
        "md-overview-headings",
        "markdown sections",
        Browse,
        Search,
        json!({"query": "Overview"}),
        vec![n],
        vec![],
    );

    // --- ADR and technical documentation
    let n = b.def("docs/adrs/007-pagination.md", "# ADR-007");
    b.add(
        "adr-pagination-title",
        "ADR and documentation",
        Locate,
        Search,
        json!({"query": "pagination"}),
        vec![n],
        vec![],
    );
    let n = b.def("docs/adrs/002-retry-policy.md", "# ADR-002");
    b.add(
        "adr-retry-title",
        "ADR and documentation",
        Locate,
        Search,
        json!({"query": "retry policy"}),
        vec![n],
        vec![],
    );
    let n = b.def("docs/adrs/010-versioning.md", "## Alternatives considered");
    b.add(
        "adr-alternatives-versioning",
        "ADR and documentation",
        Browse,
        Search,
        json!({"query": "alternatives considered"}),
        vec![n],
        vec![],
    );
    b.add(
        "adr-decision-mentions-code",
        "ADR and documentation",
        DocText,
        Pack,
        json!({"symbol": "Decision", "path": "docs/adrs/005-rate-limiting.md"}),
        vec![Need::Text("handle_payments_create")],
        vec![],
    );
    let n = b.def("docs/adrs/003-config-format.md", "# ADR-003");
    b.add(
        "adr-config-format-title",
        "ADR and documentation",
        Locate,
        Search,
        json!({"query": "configuration format"}),
        vec![n],
        vec![],
    );
    let n = b.def("docs/adrs/009-error-model.md", "# ADR-009");
    b.add(
        "adr-error-model-title",
        "ADR and documentation",
        Locate,
        Search,
        json!({"query": "error model"}),
        vec![n],
        vec![],
    );

    // --- mixed code and documentation
    let needs = vec![
        b.def("src/retry.rs", "pub fn retry_with_backoff"),
        b.def("docs/adrs/002-retry-policy.md", "# ADR-002"),
    ];
    b.add(
        "mixed-retry",
        "mixed code and documentation",
        Locate,
        Search,
        json!({"query": "retry"}),
        needs,
        vec![],
    );
    // Words of the query must be words of the expected name or title, which is
    // what the search documents: `logging` is not in `log_event`.
    let needs = vec![
        b.def("src/logging.rs", "//! Structured logging."),
        b.def("docs/adrs/006-logging.md", "# ADR-006"),
    ];
    b.add(
        "mixed-logging",
        "mixed code and documentation",
        Locate,
        Search,
        json!({"query": "logging"}),
        needs,
        vec![],
    );
    // `cache` is not a prefix of `caching`; the search does not stem.
    let needs = vec![
        b.def("src/cache.rs", "pub fn cache_key"),
        b.def("src/cache.rs", "//! Response cache."),
    ];
    b.add(
        "mixed-cache",
        "mixed code and documentation",
        Locate,
        Search,
        json!({"query": "cache"}),
        needs,
        vec![],
    );
    let needs = vec![b.def("src/retry.rs", "pub fn retry_with_backoff")];
    b.add(
        "mixed-hybrid-backoff",
        "mixed code and documentation",
        Locate,
        Search,
        json!({"query": "exponential backoff", "alpha": 0.0}),
        needs,
        vec![],
    );
    let needs = vec![
        b.def("docs/notes/billing.md", "# Billing notes"),
        b.def("src/billing.rs", "//! Invoices."),
    ];
    b.add(
        "mixed-billing",
        "mixed code and documentation",
        Locate,
        Search,
        json!({"query": "billing"}),
        needs,
        vec![],
    );

    // --- no evidence
    b.add(
        "none-search",
        "no evidence",
        Nothing,
        Search,
        json!({"query": "zzqxv unobtainium"}),
        vec![],
        vec![],
    )
    .expect_none = true;
    b.add(
        "none-pack",
        "no evidence",
        Nothing,
        Pack,
        json!({"symbol": "reconcile_ledger"}),
        vec![],
        vec![],
    )
    .expect_none = true;
    b.add(
        "none-hybrid",
        "no evidence",
        Nothing,
        Search,
        json!({"query": "qwxzy plugh", "alpha": 0.0}),
        vec![],
        vec![],
    )
    .expect_none = true;
    let c = b.add(
        "none-wrong-path",
        "no evidence",
        Nothing,
        Pack,
        json!({"symbol": "calculate_tax", "path": "src/http.rs"}),
        vec![],
        vec![],
    );
    c.expect_none = true;

    // --- many results
    let n = b.def("src/handlers/inventory.rs", "pub fn handle_inventory_list");
    b.add(
        "many-handle",
        "numerous results",
        Browse,
        Search,
        json!({"query": "handle"}),
        vec![n],
        vec![],
    );
    let n = b.def("src/handlers/audit.rs", "pub fn handle_audit_list");
    b.add(
        "many-list",
        "numerous results",
        Browse,
        Search,
        json!({"query": "list"}),
        vec![n],
        vec![],
    );
    let needs = vec![
        b.def("src/handlers/shipping.rs", "pub fn handle_shipping_update"),
        b.def("docs/notes/shipping.md", "# Shipping notes"),
    ];
    b.add(
        "many-shipping",
        "numerous results",
        Browse,
        Search,
        json!({"query": "shipping"}),
        needs,
        vec![],
    );
    let n = b.def("src/engine.rs", "fn stage_12");
    b.add(
        "many-stage",
        "numerous results",
        Browse,
        Search,
        json!({"query": "stage"}),
        vec![n],
        vec![],
    );

    // --- budgets too small for the answer
    let n = b.def("src/handlers/orders.rs", "pub fn handle_orders_delete");
    b.add(
        "budget-handle-page",
        "insufficient budget",
        Browse,
        Search,
        json!({"query": "handle"}),
        vec![n],
        vec![],
    )
    .task_budget = Some(120);
    let n = b.def("src/engine.rs", "pub fn run_pipeline");
    b.add(
        "budget-pipeline-pack",
        "insufficient budget",
        Relations,
        Pack,
        json!({"symbol": "run_pipeline"}),
        vec![n, Need::Text("stage_01")],
        vec![],
    )
    .task_budget = Some(300);
    let c = b.add(
        "budget-below-minimum",
        "insufficient budget",
        Browse,
        Search,
        json!({"query": "handle"}),
        vec![],
        vec![],
    );
    c.task_budget = Some(1);
    c.expect_signal = true;
    let n = b.def("docs/adrs/010-versioning.md", "## Alternatives considered");
    b.add(
        "budget-adr-alternatives",
        "insufficient budget",
        Browse,
        Search,
        json!({"query": "alternatives considered"}),
        vec![n],
        vec![],
    )
    .task_budget = Some(100);

    // Dev/eval: alternate inside each category, in the order written above.
    let mut seen: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for case in &mut b.cases {
        let k = seen.entry(case.category).or_insert(0);
        case.split = if k.is_multiple_of(2) {
            Split::Dev
        } else {
            Split::Eval
        };
        *k += 1;
    }
    b.cases
}
