use std::collections::BTreeSet;
use std::path::Path;

use anyhow::Context;
use git2::{DiffOptions, Repository};
use mct_core::LanguageRegistry;
use mct_index::{Index, QueryScope};

const DISPLAY_LIMIT: usize = 30;

pub(crate) fn run(
    root: &Path,
    index: &mut Index,
    registry: &LanguageRegistry,
    base: &str,
    head: &str,
) -> anyhow::Result<()> {
    let root = root.canonicalize().context("resolve project root")?;
    let repository = Repository::discover(&root).context("open Git repository")?;
    let repository_root = repository
        .workdir()
        .context("Git repository has no working tree")?
        .canonicalize()?;
    let project_scope = root
        .strip_prefix(&repository_root)
        .context("project root is outside the Git working tree")?;
    let base = repository.revparse_single(base)?.peel_to_commit()?;
    let head = repository.revparse_single(head)?.peel_to_commit()?;
    let base_tree = base.tree()?;
    let head_tree = head.tree()?;
    let mut options = DiffOptions::new();
    options.include_typechange(true);
    let diff =
        repository.diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut options))?;
    let mut paths = BTreeSet::new();
    for delta in diff.deltas() {
        if let Some(path) = delta.new_file().path().or_else(|| delta.old_file().path()) {
            if let Ok(path) = path.strip_prefix(project_scope) {
                if !path.as_os_str().is_empty() {
                    paths.insert(path.to_string_lossy().into_owned());
                }
            }
        }
    }
    if paths.is_empty() {
        println!("No files changed between {} and {}.", base.id(), head.id());
        return Ok(());
    }
    index.reindex(registry, false)?;
    println!("Impact report: {} -> {}", base.id(), head.id());
    print_summary(index, paths.into_iter().collect())
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
