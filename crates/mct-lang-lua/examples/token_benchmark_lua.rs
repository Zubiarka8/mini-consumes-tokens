//! Standalone Lua token-benchmark, mirroring `mct-cli`'s
//! `examples/token_benchmark.rs` measurement methodology *exactly*
//! (identical `mcp_chars`/`grep_then_read_chars` logic and output format) but
//! kept local to this crate: `mct-lang-lua` is deliberately not wired into
//! `mct-cli`'s or `mct-mcp-server`'s production `LanguageRegistry` (see this
//! crate's `Cargo.toml` description — Lua is an architecture-validation
//! exercise outside the project's original 7-language scope), so this
//! benchmark does not add it as a `mct-cli` dependency either. Only the
//! measurement method is duplicated; the fixture and result are real.
//!
//! Usage: cargo run -p mct-lang-lua --example token_benchmark_lua
//!
//! Named `token_benchmark_lua` (not `token_benchmark`) to avoid an
//! example-binary filename collision with `mct-cli`'s own
//! `examples/token_benchmark.rs` when both crates are built together.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_lua::LuaParser;

struct Query {
    label: &'static str,
    kind: QueryKind,
    term: &'static str,
}

enum QueryKind {
    Symbol,
    Callers,
    References,
}

fn mcp_chars(index: &Index, query: &Query) -> (usize, String) {
    let text = match query.kind {
        QueryKind::Symbol => {
            let hits = index.find_symbol(query.term).unwrap();
            hits.iter()
                .map(|h| {
                    format!(
                        "{}:{}:{} [{}] {} {}\n",
                        h.relative_path, h.line, h.column, h.language, h.kind, h.name
                    )
                })
                .collect::<String>()
        }
        QueryKind::Callers => {
            let hits = index.find_callers(query.term).unwrap();
            hits.iter()
                .map(|h| {
                    format!(
                        "{}:{}:{} [{}] {} --{}--> {}\n",
                        h.relative_path,
                        h.line,
                        h.column,
                        h.language,
                        h.from_symbol,
                        h.kind,
                        h.to_name
                    )
                })
                .collect::<String>()
        }
        QueryKind::References => {
            let hits = index.find_references(query.term).unwrap();
            hits.iter()
                .map(|h| {
                    format!(
                        "{}:{}:{} [{}] {} --{}--> {}\n",
                        h.relative_path,
                        h.line,
                        h.column,
                        h.language,
                        h.from_symbol,
                        h.kind,
                        h.to_name
                    )
                })
                .collect::<String>()
        }
    };
    (text.chars().count(), text)
}

/// grep the exact term across every file in `root`, then Read (in full)
/// each distinct file that matched — the realistic Read/Grep/Glob baseline.
fn grep_then_read_chars(root: &Path, term: &str) -> (usize, usize, usize) {
    let mut grep_output = String::new();
    let mut matched_files: Vec<std::path::PathBuf> = Vec::new();

    for entry in walkdir_files(root) {
        let contents = fs::read_to_string(&entry).unwrap_or_default();
        let mut matched = false;
        for (i, line) in contents.lines().enumerate() {
            if line.contains(term) {
                grep_output.push_str(&format!("{}:{}:{}\n", entry.display(), i + 1, line));
                matched = true;
            }
        }
        if matched {
            matched_files.push(entry);
        }
    }

    let read_chars: usize = matched_files
        .iter()
        .map(|f| fs::read_to_string(f).unwrap_or_default().chars().count())
        .sum();

    let grep_chars = grep_output.chars().count();
    (grep_chars, read_chars, matched_files.len())
}

fn walkdir_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut out = Vec::new();
    for entry in walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            out.push(entry.path().to_path_buf());
        }
    }
    out
}

fn run_language(name: &str, root: &Path, registry: LanguageRegistry, queries: &[Query]) {
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();

    println!("\n## {name}\n");
    println!(
        "| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |"
    );
    println!("|---|---|---|---|---|---|");
    for query in queries {
        let (mcp_c, _) = mcp_chars(&index, query);
        let (grep_c, read_c, files) = grep_then_read_chars(root, query.term);
        let baseline_c = grep_c + read_c;
        let reduction = if baseline_c > 0 {
            100.0 * (1.0 - mcp_c as f64 / baseline_c as f64)
        } else {
            0.0
        };
        println!(
            "| {} (`{}`) | {} | ~{} | {} (grep {} + read {} across {} file(s)) | ~{} | {:.1}% |",
            query.label,
            query.term,
            mcp_c,
            mcp_c / 4,
            baseline_c,
            grep_c,
            read_c,
            files,
            baseline_c / 4,
            reduction
        );
    }
}

fn main() {
    let lua_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inventory-app");
    let mut lua_registry = LanguageRegistry::new();
    lua_registry.register(Arc::new(LuaParser));
    run_language(
        "Lua (inventory-app fixture)",
        &lua_root,
        lua_registry,
        &[
            Query {
                label: "find the definition of",
                kind: QueryKind::Symbol,
                term: "Inventory",
            },
            Query {
                label: "what calls this function",
                kind: QueryKind::Callers,
                term: "log",
            },
            Query {
                label: "who uses this symbol",
                kind: QueryKind::References,
                term: "addItem",
            },
        ],
    );
}
