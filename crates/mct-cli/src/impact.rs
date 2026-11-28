use std::collections::BTreeSet;

use anyhow::Context;
use mct_core::LanguageRegistry;
use mct_index::{Index, QueryScope};

const DISPLAY_LIMIT: usize = 30;

/// `mct-cli impact` (issue #632): refreshes the index, then prints the
/// definitions the change from the merge base of `base` to `head` (the
/// working tree when `None`) touches, with their callers and tests — the
/// `diff_impact` MCP tool's report as text.
pub(crate) fn run(
    index: &mut Index,
    registry: &LanguageRegistry,
    base: &str,
    head: Option<&str>,
) -> anyhow::Result<()> {
    index.reindex(registry, false)?;
    let (diff, report) = index
        .diff_impact(registry, base, head)
        .with_context(|| format!("diff from the merge base of {base}"))?;
    print!(
        "{}",
        mct_mcp_server::format::diff_impact(&diff, &report, false)
    );
    Ok(())
}

pub(crate) fn print_summary(index: &Index, paths: Vec<String>) -> anyhow::Result<()> {
    let paths: BTreeSet<_> = paths.into_iter().collect();
    if paths.is_empty() {
        println!("No changed or newly indexed files.");
        return Ok(());
    }
    println!("Symbols found in changed files:");
    let mut candidates = BTreeSet::new();
    let mut shown_symbols = 0;
    for path in paths.iter().take(DISPLAY_LIMIT) {
        println!("  {path}");
        let symbols = index.list_symbols(path, None, None)?;
        for symbol in symbols
            .iter()
            .take(DISPLAY_LIMIT.saturating_sub(shown_symbols))
        {
            println!("    {} {}:{}", symbol.kind, symbol.name, symbol.line);
            shown_symbols += 1;
            if matches!(symbol.kind.as_str(), "function" | "method") {
                let tests = index.affected_test_page(
                    &symbol.name,
                    8,
                    (DISPLAY_LIMIT, 0),
                    QueryScope {
                        path: None,
                        language: Some(&symbol.language),
                    },
                )?;
                candidates.extend(
                    tests
                        .hits
                        .into_iter()
                        .map(|hit| format!("{} ({})", hit.from_symbol, hit.relative_path)),
                );
            }
        }
    }
    if paths.len() > DISPLAY_LIMIT || shown_symbols >= DISPLAY_LIMIT {
        println!("  Output limited to {DISPLAY_LIMIT} files and symbols.");
    }
    println!("Candidate affected tests (symbol graph):");
    if candidates.is_empty() {
        println!("  None found in the current index.");
    } else {
        for test in candidates.iter().take(DISPLAY_LIMIT) {
            println!("  {test}");
        }
        if candidates.len() > DISPLAY_LIMIT {
            println!("  … and {} more.", candidates.len() - DISPLAY_LIMIT);
        }
    }
    Ok(())
}
