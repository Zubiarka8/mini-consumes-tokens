//! Shared measurement method for standalone language benchmarks.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use std::fs;
use std::path::Path;

pub struct Query {
    pub label: &'static str,
    pub kind: QueryKind,
    pub term: &'static str,
}

pub enum QueryKind {
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

pub fn run_language(name: &str, root: &Path, registry: LanguageRegistry, queries: &[Query]) {
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
