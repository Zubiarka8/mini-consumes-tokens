// Example/bench code: a panic here means a broken benchmark precondition,
// not a path processing untrusted repo-input content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Cost of review decisions for `find_dead_code`, measured in parts that are
//! never merged:
//!
//! - **query response**: latency, bytes and estimated tokens of a default
//!   `find_dead_code` call (TOON and text), plus a SHA-256 of each response
//!   so two runs on different commits prove the output is byte-identical;
//! - **catalog**: the `find_dead_code` tool entry and the whole `tools/list`
//!   catalog, with their SHA-256, which every client that loads all schemas
//!   pays once per session; plus the *projected* entry with the proposed
//!   `reviewed` argument, which is not published;
//! - **review data**: computing review keys for the candidates, and reading
//!   and resolving synthetic review files of 0 to 100,000 rows per format.
//!
//! ```sh
//! cargo run --release -p mct-mcp-server --example review_overhead -- \
//!     --root <indexed project> [--iterations 50]
//! # Peak memory of one load + resolve, in a process that did not generate
//! # the files, for `/usr/bin/time -l` (macOS) or `/usr/bin/time -v` (Linux):
//! review_overhead --root <indexed project> --prepare all 100000 <empty dir>
//! /usr/bin/time -l review_overhead --root <indexed project> --load <that dir>
//! ```
//!
//! `--root` must already be indexed (`mct-cli --root <project> init`); the
//! index is opened read-only and never modified, and synthetic review files
//! are written to a temporary directory, never to `--root`. Tokens are the
//! project's `EstimatedCounter` heuristic, not a model tokenizer.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use mct_index::reviews::{FindingKeys, Reviews, Visibility};
use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::{FindDeadCodeArgs, MctServer};
use rmcp::handler::server::wrapper::Parameters;
use serde_json::Value;

const ROW_COUNTS: [usize; 4] = [0, 1_000, 10_000, 100_000];
const FORMATS: [&str; 4] = ["c3", "a2", "b2", "all"];
/// The per-file read bound of `mct_index::reviews`.
const C3_MAX_BYTES: usize = 32 * 1024 * 1024;

struct Options {
    root: PathBuf,
    iterations: usize,
    prepare: Option<(String, usize, PathBuf)>,
    load: Option<PathBuf>,
}

fn options() -> Options {
    let mut root = None;
    let mut iterations = 50;
    let mut prepare = None;
    let mut load = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = args.next().map(PathBuf::from),
            "--iterations" => iterations = args.next().unwrap().parse().unwrap(),
            "--prepare" => {
                let format = args.next().unwrap();
                let rows = args.next().unwrap().parse().unwrap();
                let dir = PathBuf::from(args.next().unwrap());
                prepare = Some((format, rows, dir));
            }
            "--load" => load = args.next().map(PathBuf::from),
            other => panic!("unknown argument {other}"),
        }
    }
    Options {
        root: root.expect("--root <indexed project> is required"),
        iterations,
        prepare,
        load,
    }
}

fn sha256(text: &str) -> String {
    hmac_sha256::Hash::hash(text.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// p50 and p95 of `samples` in microseconds.
fn percentiles(samples: &mut [u128]) -> (u128, u128) {
    samples.sort_unstable();
    let at = |q: f64| samples[((samples.len() - 1) as f64 * q).round() as usize];
    (at(0.50), at(0.95))
}

/// Runs `run` untimed a few times first (cold caches, lazy allocation), then
/// `iterations` times; returns p50 and p95 in microseconds.
fn timed<T>(iterations: usize, mut run: impl FnMut() -> T) -> (u128, u128) {
    const WARMUP: usize = 5;
    for _ in 0..WARMUP {
        std::hint::black_box(run());
    }
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let started = Instant::now();
        std::hint::black_box(run());
        samples.push(started.elapsed().as_micros());
    }
    percentiles(&mut samples)
}

async fn dead_code_call(server: &MctServer, format: &str) -> String {
    let result = server
        .find_dead_code(Parameters(FindDeadCodeArgs {
            path: None,
            language: None,
            limit: None,
            offset: None,
            format: Some(format.to_string()),
        }))
        .await
        .unwrap();
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|text| text.text.clone())
        .unwrap_or_default()
}

/// A deterministic 64-hex key that matches no real finding.
fn synthetic_key(seed: &str, row: usize) -> String {
    sha256(&format!("{seed}-{row}"))
}

/// Writes `rows` current rows and `rows` history rows in `format` (or in all
/// three formats) under `dir/.mct-index`. About one row in ten, at most one
/// per finding, matches a real finding with a suppressing state. `C3`
/// history is cut to what fits in the 32 MiB bound, and the draft's to its
/// own 50,000-row cap.
fn write_store(dir: &Path, format: &str, rows: usize, findings: &[FindingKeys]) {
    let state_dir = dir.join(".mct-index");
    std::fs::create_dir_all(&state_dir).unwrap();
    for name in [
        "review-decisions.toon",
        "reviews.toon",
        "duplicate-reviews.toon",
    ] {
        let _ = std::fs::remove_file(state_dir.join(name));
    }
    let matching = (rows / 10).min(findings.len());
    let real = |row: usize| (row < matching).then(|| &findings[row]);
    let states = ["false-positive", "dismissed", "confirmed"];
    if format == "c3" || format == "all" {
        let mut text = format!(
            "version: 3\nreviews[{rows}]{{kind,identity,fingerprint,state,updated,note}}:\n"
        );
        let mut current: Vec<(String, String, &str)> = (0..rows)
            .map(|row| match real(row) {
                Some(f) => (f.identity.clone(), f.fingerprint.clone(), states[row % 2]),
                None => (
                    synthetic_key("c3", row),
                    synthetic_key("c3fp", row),
                    states[row % 3],
                ),
            })
            .collect();
        current.sort();
        for (row, (identity, fingerprint, state)) in current.iter().enumerate() {
            writeln!(
                text,
                "  dead-code,{identity},{fingerprint},{state},{},",
                1_700_000_000 + row
            )
            .unwrap();
        }
        // As many history rows as `rows`, or as fit in the 32 MiB bound.
        let mut history = String::new();
        let mut kept = 0;
        for (row, (identity, fingerprint, state)) in current.iter().enumerate() {
            let line = format!(
                "  dead-code,{identity},{fingerprint},unreviewed,{state},{},\n",
                1_700_000_000 + row
            );
            if text.len() + history.len() + line.len() + 80 > C3_MAX_BYTES {
                break;
            }
            history.push_str(&line);
            kept += 1;
        }
        writeln!(
            text,
            "history[{kept}]{{kind,identity,fingerprint,from,to,at,note}}:"
        )
        .unwrap();
        text.push_str(&history);
        std::fs::write(state_dir.join("review-decisions.toon"), text).unwrap();
    }
    if format == "a2" || format == "all" {
        let mut text = format!("version: 2\nreviews[{rows}]{{key,state,updated,note}}:\n");
        let keys: Vec<(String, &str)> = (0..rows)
            .map(|row| match real(row).and_then(|f| f.legacy_key.clone()) {
                Some(key) => (key, states[row % 2]),
                None => (synthetic_key("a2", row), states[row % 3]),
            })
            .collect();
        for (row, (key, state)) in keys.iter().enumerate() {
            writeln!(text, "  {key},{state},{},", 1_600_000_000 + row).unwrap();
        }
        writeln!(text, "history[{rows}]{{key,from,to,at,note}}:").unwrap();
        for (row, (key, state)) in keys.iter().enumerate() {
            writeln!(text, "  {key},none,{state},{},", 1_600_000_000 + row).unwrap();
        }
        std::fs::write(state_dir.join("duplicate-reviews.toon"), text).unwrap();
    }
    if format == "b2" || format == "all" {
        let header = "{kind,identity,fingerprint,status,at,reason}:";
        let draft_states = ["false_positive", "ignored", "confirmed"];
        let lines: Vec<String> = (0..rows)
            .map(|row| match real(row) {
                Some(f) => format!(
                    "  dead,{},{},{},{},",
                    f.draft_identity,
                    f.fingerprint,
                    draft_states[row % 2],
                    1_650_000_000 + row
                ),
                None => format!(
                    "  dead,{},{},{},{},",
                    synthetic_key("b2", row),
                    synthetic_key("b2fp", row),
                    draft_states[row % 3],
                    1_650_000_000 + row
                ),
            })
            .collect();
        // The draft caps its history at 50,000 rows.
        let history = &lines[..rows.min(50_000)];
        let mut text = format!("version: 2\nreviews[{rows}]{header}\n");
        for line in &lines {
            writeln!(text, "{line}").unwrap();
        }
        writeln!(text, "history[{}]{header}", history.len()).unwrap();
        for line in history {
            writeln!(text, "{line}").unwrap();
        }
        std::fs::write(state_dir.join("reviews.toon"), text).unwrap();
    }
}

/// Reads the store files without parsing them: the I/O part of `load`.
fn read_store_bytes(dir: &Path) -> usize {
    [
        "review-decisions.toon",
        "reviews.toon",
        "duplicate-reviews.toon",
    ]
    .iter()
    .filter_map(|name| std::fs::read(dir.join(".mct-index").join(name)).ok())
    .map(|bytes| bytes.len())
    .sum()
}

fn store_bytes(dir: &Path) -> u64 {
    [
        "review-decisions.toon",
        "reviews.toon",
        "duplicate-reviews.toon",
    ]
    .iter()
    .filter_map(|name| std::fs::metadata(dir.join(".mct-index").join(name)).ok())
    .map(|metadata| metadata.len())
    .sum()
}

fn dead_code_keys(index: &Index) -> Vec<FindingKeys> {
    let scan = mct_index::scan_dead_code(index, None, None).unwrap();
    let hashes = index.file_hashes().unwrap();
    mct_index::reviews::dead_code_findings(&scan.symbols, &scan.candidates, &hashes)
}

/// The `find_dead_code` entry with the proposed optional `reviewed`
/// argument, as D3 would publish it. Projection only: nothing is published.
fn projected_tool(tool: &rmcp::model::Tool) -> rmcp::model::Tool {
    let mut projected = tool.clone();
    let mut schema = (*tool.input_schema).clone();
    let Some(Value::Object(properties)) = schema.get_mut("properties") else {
        panic!("find_dead_code has an object schema");
    };
    let mut reviewed = properties["format"].clone();
    reviewed["description"] = Value::String(
        "Apply human review decisions: `show` adds a review state column, `hide` \
         drops false positives and dismissed findings, `only` lists just those. \
         Omit for the unreviewed list."
            .into(),
    );
    properties.insert("reviewed".into(), reviewed);
    // The listed description carries WHEN/NOT/TAGS only; the PRE/POST/EFFECTS
    // lines of `tools.ttc` reach clients through `get_tool_schema`, so the
    // catalog changes by the property alone.
    projected.input_schema = std::sync::Arc::new(schema);
    projected
}

fn open(root: &Path) -> Index {
    let database = root.join(".mct-index/index.sqlite3");
    Index::open_existing_read_only(root, &database, ExcludeSet::for_project(root)).unwrap()
}

/// One load and resolve of the files in `dir`, for an external memory probe.
fn load_once(root: &Path, dir: &Path) {
    let findings = dead_code_keys(&open(root));
    let reviews = Reviews::load(dir);
    assert!(!reviews.is_degraded(), "{:?}", reviews.problems());
    let hidden = reviews
        .resolve(&findings)
        .iter()
        .filter(|r| r.visibility() == Visibility::Suppressed)
        .count();
    println!(
        "{} bytes of review files: {hidden} of {} candidates hidden",
        store_bytes(dir),
        findings.len()
    );
}

fn tempfile_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "mct-review-overhead-{label}-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir.canonicalize().unwrap()
}

#[tokio::main]
async fn main() {
    let options = options();
    let root = options.root.canonicalize().unwrap();
    if let Some((format, rows, dir)) = &options.prepare {
        write_store(dir, format, *rows, &dead_code_keys(&open(&root)));
        return;
    }
    if let Some(dir) = &options.load {
        load_once(&root, dir);
        return;
    }
    let index = open(&root);
    let files = index.file_hashes().unwrap().len();
    let findings = dead_code_keys(&index);
    // The scan is what `find_dead_code` already runs; review keys add the
    // file hashes query and the identities.
    let (scan_p50, _) = timed(options.iterations, || {
        mct_index::scan_dead_code(&index, None, None).unwrap()
    });
    let scan = mct_index::scan_dead_code(&index, None, None).unwrap();
    let (keys_p50, keys_p95) = timed(options.iterations, || {
        let hashes = index.file_hashes().unwrap();
        mct_index::reviews::dead_code_findings(&scan.symbols, &scan.candidates, &hashes)
    });
    // The same work split into its two parts, so the cost can be attributed.
    let (hashes_p50, _) = timed(options.iterations, || index.file_hashes().unwrap());
    let hashes = index.file_hashes().unwrap();
    let (identities_p50, _) = timed(options.iterations, || {
        mct_index::reviews::dead_code_findings(&scan.symbols, &scan.candidates, &hashes)
    });
    let server = MctServer::new(index, mct_mcp_server::registry::build_registry());

    println!("# Review overhead\n");
    println!(
        "Project: {} indexed files, {} dead-code candidates; {} iterations.\n",
        files,
        findings.len(),
        options.iterations
    );

    println!("## Query response: default `find_dead_code`\n");
    println!("| format | p50 µs | p95 µs | bytes | EstimatedCounter | SHA-256 |");
    println!("|---|---|---|---|---|---|");
    for format in ["toon", "text"] {
        let response = dead_code_call(&server, format).await;
        let mut samples = Vec::with_capacity(options.iterations);
        for _ in 0..options.iterations {
            let started = Instant::now();
            let again = dead_code_call(&server, format).await;
            samples.push(started.elapsed().as_micros());
            assert_eq!(again, response, "the response must be deterministic");
        }
        let (p50, p95) = percentiles(&mut samples);
        println!(
            "| {format} | {p50} | {p95} | {} | {} | `{}` |",
            response.len(),
            EstimatedCounter.count(&response).value,
            sha256(&response)
        );
    }

    println!("\n## Catalog\n");
    println!("| entry | bytes | EstimatedCounter | SHA-256 |");
    println!("|---|---|---|---|");
    let tools = server.tool_catalog();
    let tool = tools
        .iter()
        .find(|tool| tool.name == "find_dead_code")
        .expect("find_dead_code is registered");
    let projected = projected_tool(tool);
    for (name, json) in [
        (
            "find_dead_code".to_string(),
            serde_json::to_string(tool).unwrap(),
        ),
        (
            format!("all {} tools", tools.len()),
            serde_json::to_string(&tools).unwrap(),
        ),
        (
            "find_dead_code with `reviewed` (projected, unpublished)".to_string(),
            serde_json::to_string(&projected).unwrap(),
        ),
    ] {
        println!(
            "| {name} | {} | {} | `{}` |",
            json.len(),
            EstimatedCounter.count(&json).value,
            sha256(&json)
        );
    }

    println!("\n## Review data\n");
    println!(
        "Dead-code scan (already part of `find_dead_code`): p50 {scan_p50} µs. \
         Review keys on top of it for all {} candidates (file hashes + identities): \
         p50 {keys_p50} µs, p95 {keys_p95} µs. Of that: file hashes query p50 \
         {hashes_p50} µs, identities p50 {identities_p50} µs.\n",
        findings.len()
    );
    println!(
        "| format | rows | file bytes | read p50 µs | load p50 µs | load p95 µs | resolve p50 µs | hidden |"
    );
    println!("|---|---|---|---|---|---|---|---|");
    let dir = tempfile_dir("stores");
    for format in FORMATS {
        for rows in ROW_COUNTS {
            write_store(&dir, format, rows, &findings);
            let reviews = Reviews::load(&dir);
            assert!(!reviews.is_degraded(), "{:?}", reviews.problems());
            let hidden = reviews
                .resolve(&findings)
                .iter()
                .filter(|r| r.visibility() == Visibility::Suppressed)
                .count();
            let (read_p50, _) = timed(options.iterations, || read_store_bytes(&dir));
            let (load_p50, load_p95) = timed(options.iterations, || Reviews::load(&dir));
            let (resolve_p50, _) = timed(options.iterations, || reviews.resolve(&findings));
            println!(
                "| {format} | {rows} | {} | {read_p50} | {load_p50} | {load_p95} | {resolve_p50} | {hidden} |",
                store_bytes(&dir)
            );
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}
