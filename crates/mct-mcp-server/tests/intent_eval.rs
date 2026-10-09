//! Tool-preselection evaluation for issue #19 over the held-out set in
//! `fixtures/intent_eval.toon` (scoring defined in
//! `benchmarks/intent-selection.md`, committed before the rules existed).
//!
//! `rules_beat_the_full_catalog` runs in the normal suite with no model.
//! `embeddings_compared` needs the local embedding model:
//! `cargo test -p mct-mcp-server --features semantic --test intent_eval -- --ignored --nocapture`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::time::{Duration, Instant};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::intent::{self, Source, ToolVectors};
use mct_mcp_server::server::MctServer;
use mct_mcp_server::toon::decode_table;
use rmcp::model::Tool;

struct Case {
    id: String,
    query: String,
    /// Required groups; each group is a set of acceptable tools.
    need: Vec<Vec<String>>,
    forbid: Vec<String>,
}

fn cases() -> Vec<Case> {
    let table = decode_table(include_str!("fixtures/intent_eval.toon")).unwrap();
    assert_eq!(table.headers, ["id", "category", "query", "need", "forbid"]);
    table
        .rows
        .into_iter()
        .map(|row| {
            let split = |s: &str, sep: char| -> Vec<String> {
                s.split(sep)
                    .filter(|p| !p.is_empty())
                    .map(str::to_string)
                    .collect()
            };
            Case {
                id: row[0].clone(),
                query: row[2].clone(),
                need: split(&row[3], ';').iter().map(|g| split(g, '|')).collect(),
                forbid: split(row.get(4).map_or("", String::as_str), '|'),
            }
        })
        .collect()
}

fn catalog_and_languages() -> (Vec<Tool>, Vec<&'static str>) {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let registry = mct_mcp_server::registry::build_registry();
    let languages = registry.language_ids();
    let index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    (MctServer::new(index, registry).tool_catalog(), languages)
}

/// Same heuristic as `mct_eval::approx_tokens` / `tests/batch.rs`: a run of
/// alphanumerics/`_` is one token, every other non-whitespace char is one.
/// Approximate — no model tokenizer is a dependency of this workspace.
fn approx_tokens(s: &str) -> usize {
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

/// What one selector delivers for one query.
struct Delivery {
    tools: Vec<String>,
    /// Whether `tools` is ordered best-first (top-1 is meaningful).
    ordered: bool,
    fallback: bool,
    intent_line: String,
}

#[derive(Default)]
struct Score {
    top1_hits: usize,
    top1_cases: usize,
    groups_hit: usize,
    groups: usize,
    precision_sum: f64,
    precision_cases: usize,
    no_selection_ok: usize,
    no_selection_cases: usize,
    forbidden: usize,
    fallbacks: usize,
    tokens: usize,
    bytes: usize,
    cases: usize,
    elapsed: Duration,
    misses: Vec<String>,
}

impl Score {
    fn top1(&self) -> Option<f64> {
        (self.top1_cases > 0).then(|| self.top1_hits as f64 / self.top1_cases as f64)
    }
    fn recall(&self) -> f64 {
        self.groups_hit as f64 / self.groups.max(1) as f64
    }
    fn precision(&self) -> f64 {
        self.precision_sum / self.precision_cases.max(1) as f64
    }
    fn row(&self, name: &str) -> String {
        let top1 = self
            .top1()
            .map_or("n/a".to_string(), |v| format!("{:.3}", v));
        format!(
            "  {name},{top1},{:.3},{:.3},{}/{},{},{},{},{},{:.3}",
            self.recall(),
            self.precision(),
            self.no_selection_ok,
            self.no_selection_cases,
            self.forbidden,
            self.fallbacks,
            self.tokens,
            self.bytes,
            self.elapsed.as_secs_f64() * 1000.0 / self.cases.max(1) as f64,
        )
    }
}

fn score(cases: &[Case], catalog: &[Tool], mut select: impl FnMut(&str) -> Delivery) -> Score {
    let mut s = Score::default();
    for case in cases {
        let started = Instant::now();
        let d = select(&case.query);
        s.elapsed += started.elapsed();
        s.cases += 1;

        let entries: Vec<&Tool> = d
            .tools
            .iter()
            .filter_map(|n| catalog.iter().find(|t| t.name == n.as_str()))
            .collect();
        let payload = format!(
            "{}\n{}\n{}",
            case.query,
            d.intent_line,
            serde_json::to_string(&entries).unwrap()
        );
        s.tokens += approx_tokens(&payload);
        s.bytes += payload.len();
        if d.fallback {
            s.fallbacks += 1;
        }
        if case.forbid.iter().any(|f| d.tools.contains(f)) {
            s.forbidden += 1;
        }
        if case.need.is_empty() {
            s.no_selection_cases += 1;
            if d.fallback {
                s.no_selection_ok += 1;
            } else {
                s.misses.push(format!("{}: guessed {:?}", case.id, d.tools));
            }
            continue;
        }
        let wanted = |t: &String| case.need.iter().any(|g| g.contains(t));
        for group in &case.need {
            s.groups += 1;
            if group.iter().any(|t| d.tools.contains(t)) {
                s.groups_hit += 1;
            } else {
                s.misses
                    .push(format!("{}: missing {:?} in {:?}", case.id, group, d.tools));
            }
        }
        let useful = d.tools.iter().filter(|t| wanted(t)).count();
        s.precision_sum += useful as f64 / d.tools.len().max(1) as f64;
        s.precision_cases += 1;
        if d.ordered {
            s.top1_cases += 1;
            if d.tools.first().is_some_and(wanted) {
                s.top1_hits += 1;
            } else {
                s.misses
                    .push(format!("{}: top-1 {:?}", case.id, d.tools.first()));
            }
        }
    }
    s
}

fn full_catalog(catalog: &[Tool]) -> impl FnMut(&str) -> Delivery + '_ {
    |_| Delivery {
        tools: catalog.iter().map(|t| t.name.to_string()).collect(),
        ordered: false,
        fallback: true,
        intent_line: String::new(),
    }
}

fn preselected<'a>(
    catalog: &'a [Tool],
    languages: &'a [&'static str],
    vectors: Option<&'a ToolVectors<'a>>,
) -> impl FnMut(&str) -> Delivery + 'a {
    move |q| {
        let p = intent::preselect(q, languages, catalog, vectors);
        Delivery {
            ordered: p.source != Source::Fallback,
            fallback: p.source == Source::Fallback,
            intent_line: p.intent.render(p.source),
            tools: p.tools,
        }
    }
}

const HEADER: &str =
    "selectors[{n}]{selector,top1,recall,precision,no_selection_ok,forbidden_delivered,fallbacks,approx_tokens,bytes,ms_per_query}:";

fn print_report(cases: &[Case], rows: &[(&str, &Score)]) {
    println!("{} held-out cases", cases.len());
    println!("{}", HEADER.replace("{n}", &rows.len().to_string()));
    for (name, s) in rows {
        println!("{}", s.row(name));
    }
    for (name, s) in rows {
        for miss in &s.misses {
            println!("  miss[{name}] {miss}");
        }
    }
}

#[tokio::test]
async fn rules_beat_the_full_catalog() {
    let cases = cases();
    let (catalog, languages) = catalog_and_languages();
    let baseline = score(&cases, &catalog, full_catalog(&catalog));
    let rules = score(&cases, &catalog, preselected(&catalog, &languages, None));
    print_report(&cases, &[("full_catalog", &baseline), ("rules", &rules)]);

    // Issue #19's acceptance criteria, against today's flow.
    assert!(rules.tokens < baseline.tokens, "fewer prompt tokens");
    assert!(
        rules.precision() > baseline.precision(),
        "a more precise tool selection"
    );
    assert_eq!(rules.forbidden, 0, "a negated tool was delivered");
    // Regression guards at the measured values (see benchmarks/intent-selection.md);
    // they record today's result, they are not targets.
    assert!(rules.recall() >= RECALL_GUARD, "recall {}", rules.recall());
    assert!(
        rules.top1().unwrap_or(0.0) >= TOP1_GUARD,
        "top-1 {:?}",
        rules.top1()
    );
}

/// First held-out measurement: recall 42/42 groups, top-1 33/34.
const RECALL_GUARD: f64 = 1.0;
const TOP1_GUARD: f64 = 0.97;

/// Development queries for the embedding thresholds — never the eval set.
/// `true`: a code question the rules miss; `false`: no reliable selection.
const DEV_EMBEDDING_QUERIES: &[(&str, bool)] = &[
    ("how is a json-rpc request turned into a response", true),
    ("what happens when a parser hits a syntax error", true),
    ("which part handles file watching", true),
    ("how are embeddings stored", true),
    ("thanks a lot", false),
    ("good morning", false),
    ("tell me a joke", false),
    ("speed up the build please", false),
    ("ok continue", false),
];

#[tokio::test]
#[ignore = "not a bug: needs the local embedding model; run with --features semantic -- --ignored"]
async fn embeddings_compared() {
    let cases = cases();
    let (catalog, languages) = catalog_and_languages();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let model = mct_mcp_server::embedder::SemanticModel::default();
    let embedder = model.get(&root).unwrap();
    let started = Instant::now();
    let vectors = ToolVectors::new(embedder, &catalog).unwrap();
    println!(
        "embedded {} tool descriptions with {} in {:?} (one-off)",
        catalog.len(),
        embedder.model_id(),
        started.elapsed()
    );

    for (q, code) in DEV_EMBEDDING_QUERIES {
        let ranked = vectors.rank(q).unwrap();
        let top: Vec<String> = ranked
            .iter()
            .take(3)
            .map(|(n, s)| format!("{n}={s:.3}"))
            .collect();
        println!("  dev[{code}] {q}: {}", top.join(" "));
    }

    let baseline = score(&cases, &catalog, full_catalog(&catalog));
    let rules = score(&cases, &catalog, preselected(&catalog, &languages, None));
    let hybrid = score(
        &cases,
        &catalog,
        preselected(&catalog, &languages, Some(&vectors)),
    );
    let embedding_top3 = score(&cases, &catalog, |q| Delivery {
        tools: vectors
            .rank(q)
            .unwrap()
            .into_iter()
            .take(3)
            .map(|(n, _)| n)
            .collect(),
        ordered: true,
        fallback: false,
        intent_line: String::new(),
    });
    print_report(
        &cases,
        &[
            ("full_catalog", &baseline),
            ("rules", &rules),
            ("rules+embeddings", &hybrid),
            ("embedding_top3", &embedding_top3),
        ],
    );
}
