//! Duplicate analysis over Go through the real index: the real `GoParser`
//! symbols and the real `GoTokenSource` in the shared `mct-index` engine.
//! Every callable form the parser indexes as a function or method has a
//! callable boundary that matches its symbol, so none is skipped for an
//! invalid range.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{LanguageRegistry, TokenSourceRegistry};
use mct_index::{find_duplicate_groups, DuplicateOptions, DuplicateReport, ExcludeSet, Index};
use mct_lang_go::{GoParser, GoTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-go-duplicates-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        for (relative, contents) in files {
            let path: &Path = &root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(GoParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(GoTokenSource));
        find_duplicate_groups(
            &self.index,
            &sources,
            DuplicateOptions {
                min_lines: 4,
                min_tokens: 20,
                ..DuplicateOptions::default()
            },
        )
        .unwrap()
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

/// The loop body every duplicate below shares; `factor` is a literal, which
/// the default normalization ignores.
fn body(factor: &str) -> String {
    format!(
        "{{\n\tsum := 0\n\tfor _, item := range items {{\n\t\tsum += item.Price * {factor}\n\t}}\n\treturn sum\n}}"
    )
}

fn members(report: &DuplicateReport) -> Vec<Vec<(String, String, String)>> {
    report
        .groups
        .iter()
        .map(|group| {
            group
                .members
                .iter()
                .map(|m| (m.path.clone(), m.symbol_name.clone(), m.symbol_kind.clone()))
                .collect()
        })
        .collect()
}

fn member(path: &str, name: &str, kind: &str) -> (String, String, String) {
    (path.to_string(), name.to_string(), kind.to_string())
}

#[test]
fn go_is_a_supported_language() {
    let project = Project::new(&[(
        "a.go",
        &format!("package a\n\nfunc A(items []Item) int {}\n", body("2")),
    )]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["go"]);
    assert!(report.unsupported_languages.is_empty());
}

#[test]
fn functions_and_methods_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "billing/cart.go",
            &format!(
                "package billing\n\nfunc CartTotal(items []Item) int {}\n",
                body("2")
            ),
        ),
        (
            "billing/order.go",
            &format!(
                "package billing\n\n// OrderTotal sums the order.\nfunc OrderTotal(items []Item) int {}\n\nfunc (o *Order) Total(items []Item) int {}\n",
                body("3"),
                body("4")
            ),
        ),
        (
            "billing/other.go",
            "package billing\n\nfunc other(x *int) int {\n\tif x != nil {\n\t\treturn *x\n\t}\n\treturn 0\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // The receiver `(o *Order)` is part of the method's tokens, so the
    // method is a shape of its own.
    assert_eq!(
        members(&report),
        vec![vec![
            member("billing/cart.go", "CartTotal", "function"),
            member("billing/order.go", "OrderTotal", "function"),
        ]]
    );
}

#[test]
fn interface_methods_and_func_values_are_matched_not_skipped() {
    let project = Project::new(&[(
        "a.go",
        &format!(
            "package a\n\ntype Store interface {{\n\tGet(key string) (int, error)\n}}\n\nvar total = func(items []Item) int {}\n",
            body("2")
        ),
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(report.candidates_analyzed, 2);
}
