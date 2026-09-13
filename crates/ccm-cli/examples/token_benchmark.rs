//! Measures token cost of three canonical queries — "find the definition of
//! X", "what calls this function", "who uses this symbol" — answered via
//! the MCP-style index query versus the realistic Read/Grep/Glob baseline
//! (grep the exact term across the repo, then Read each file grep matched,
//! in full — which is what an agent without semantic navigation actually
//! does; grep alone rarely gives enough context to stop there).
//!
//! Character counts are exact; the "approx. tokens" column is chars/4, a
//! standard rough English/code heuristic — not a real tokenizer run, but
//! good enough to see the order of magnitude of the reduction.
//!
//! Usage: cargo run -p ccm-cli --example token_benchmark

// Example code: an unwrap()/expect() here means a broken precondition of
// this benchmark script, and panicking is the correct behavior — this is not
// production code parsing untrusted repo content (see crates/ccm-cli/src/
// for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;
use std::sync::Arc;

use ccm_core::LanguageRegistry;
use ccm_index::{ExcludeSet, Index};

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
                .map(|h| format!("{}:{}:{} [{}] {} {}\n", h.relative_path, h.line, h.column, h.language, h.kind, h.name))
                .collect::<String>()
        }
        QueryKind::Callers => {
            let hits = index.find_callers(query.term).unwrap();
            hits.iter()
                .map(|h| format!("{}:{}:{} [{}] {} --{}--> {}\n", h.relative_path, h.line, h.column, h.language, h.from_symbol, h.kind, h.to_name))
                .collect::<String>()
        }
        QueryKind::References => {
            let hits = index.find_references(query.term).unwrap();
            hits.iter()
                .map(|h| format!("{}:{}:{} [{}] {} --{}--> {}\n", h.relative_path, h.line, h.column, h.language, h.from_symbol, h.kind, h.to_name))
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
    for entry in walkdir::WalkDir::new(root).into_iter().filter_map(|e| e.ok()) {
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
    println!("| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |");
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
    let java_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ccm-lang-java/tests/fixtures/billing-app");
    let mut java_registry = LanguageRegistry::new();
    java_registry.register(Arc::new(ccm_lang_java::JavaParser));
    run_language(
        "Java (billing-app fixture)",
        &java_root,
        java_registry,
        &[
            Query { label: "find the definition of", kind: QueryKind::Symbol, term: "Invoice" },
            Query { label: "what calls this function", kind: QueryKind::Callers, term: "log" },
            Query { label: "who uses this symbol", kind: QueryKind::References, term: "addItem" },
        ],
    );

    let csharp_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ccm-lang-csharp/tests/fixtures/billing-app");
    let mut csharp_registry = LanguageRegistry::new();
    csharp_registry.register(Arc::new(ccm_lang_csharp::CSharpParser));
    run_language(
        "C# (billing-app fixture)",
        &csharp_root,
        csharp_registry,
        &[
            Query { label: "find the definition of", kind: QueryKind::Symbol, term: "Invoice" },
            Query { label: "what calls this function", kind: QueryKind::Callers, term: "Log" },
            Query { label: "who uses this symbol", kind: QueryKind::References, term: "AddItem" },
        ],
    );

    let js_ts_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ccm-lang-js-ts/tests/fixtures/webapp");
    let mut js_ts_registry = LanguageRegistry::new();
    js_ts_registry.register(Arc::new(ccm_lang_js_ts::JsTsParser));
    run_language(
        "JavaScript/TypeScript (webapp fixture)",
        &js_ts_root,
        js_ts_registry,
        &[
            Query { label: "find the definition of", kind: QueryKind::Symbol, term: "Invoice" },
            Query { label: "what calls this function", kind: QueryKind::Callers, term: "log" },
            Query { label: "who uses this symbol", kind: QueryKind::References, term: "addItem" },
        ],
    );

    let cpp_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ccm-lang-cpp/tests/fixtures/billing-app");
    let mut cpp_registry = LanguageRegistry::new();
    cpp_registry.register(Arc::new(ccm_lang_cpp::CppParser));
    run_language(
        "C++ (billing-app fixture)",
        &cpp_root,
        cpp_registry,
        &[
            Query { label: "find the definition of", kind: QueryKind::Symbol, term: "Invoice" },
            Query { label: "what calls this function", kind: QueryKind::Callers, term: "log" },
            Query { label: "who uses this symbol", kind: QueryKind::References, term: "addItem" },
        ],
    );

    let go_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../ccm-lang-go/tests/fixtures/billing-app");
    let mut go_registry = LanguageRegistry::new();
    go_registry.register(Arc::new(ccm_lang_go::GoParser));
    run_language(
        "Go (billing-app fixture)",
        &go_root,
        go_registry,
        &[
            Query { label: "find the definition of", kind: QueryKind::Symbol, term: "Invoice" },
            Query { label: "what calls this function", kind: QueryKind::Callers, term: "Log" },
            Query { label: "who uses this symbol", kind: QueryKind::References, term: "AddItem" },
        ],
    );
}
