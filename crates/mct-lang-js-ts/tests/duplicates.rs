//! Duplicate analysis over JavaScript and TypeScript through the real index:
//! the real `JsTsParser` symbols and the real `JsTsTokenSource` in the shared
//! `mct-index` engine. Every callable form the parser indexes as a function
//! or method has a callable boundary that matches its symbol, so none is
//! skipped for an invalid range.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{LanguageRegistry, TokenSourceRegistry};
use mct_index::{find_duplicate_groups, DuplicateOptions, DuplicateReport, ExcludeSet, Index};
use mct_lang_js_ts::{JsTsParser, JsTsTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-js-ts-duplicates-{}-{}",
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
        registry.register(Arc::new(JsTsParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(JsTsTokenSource));
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
        "{{\n  let sum = 0;\n  for (const item of items) {{\n    sum += item.price * {factor};\n  }}\n  return sum;\n}}"
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
fn javascript_and_typescript_are_a_supported_language() {
    let project = Project::new(&[("a.js", &format!("function a(items) {}\n", body("2")))]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["javascript_typescript"]);
    assert!(report.unsupported_languages.is_empty());
}

#[test]
fn function_declarations_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "billing/cart.js",
            &format!("function cartTotal(items) {}\n", body("2")),
        ),
        (
            "billing/order.mjs",
            &format!(
                "// the order total\nexport function orderTotal(items) {}\n",
                body("3")
            ),
        ),
        (
            "billing/other.js",
            "function other(x) {\n  if (x) {\n    return x;\n  }\n  return null;\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("billing/cart.js", "cartTotal", "function"),
            member("billing/order.mjs", "orderTotal", "function"),
        ]]
    );
}

#[test]
fn class_methods_group_in_typescript_and_tsx() {
    let class = |name: &str, factor: &str| {
        format!(
            "export class {name} {{\n  total(items: Item[]): number {}\n}}\n",
            body(factor)
        )
    };
    let project = Project::new(&[
        ("a.ts", &class("Cart", "2")),
        ("b.tsx", &class("Order", "3")),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("a.ts", "total", "method"),
            member("b.tsx", "total", "method"),
        ]]
    );
}

#[test]
fn functions_bound_to_a_const_a_class_field_or_a_commonjs_export_are_candidates() {
    let arrow = |factor: &str| format!("(items) => {}", body(factor));
    let function = |factor: &str| format!("function (items) {}", body(factor));
    let project = Project::new(&[
        ("const.js", &format!("const constTotal = {};\n", arrow("2"))),
        (
            "field.ts",
            &format!("class Cart {{\n  fieldTotal = {};\n}}\n", arrow("3")),
        ),
        (
            "export.cjs",
            &format!("exports.exportTotal = {};\n", function("4")),
        ),
        (
            "pair.cjs",
            &format!("module.exports = {{ pairTotal: {} }};\n", function("5")),
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(report.candidates_analyzed, 4);
    // Each span is the symbol's own: `name = (items) => {…}` for the const
    // and the field (one shape), the bare function for the export, and
    // `key: function …` for the pair.
    assert_eq!(
        members(&report),
        vec![vec![
            member("const.js", "constTotal", "function"),
            member("field.ts", "fieldTotal", "method"),
        ]]
    );
}

#[test]
fn bodyless_signatures_are_matched_not_skipped() {
    let project = Project::new(&[(
        "a.ts",
        "export interface Bridge {\n  t(key: string): string;\n}\nexport abstract class Store<K, V> {\n  abstract get(key: K): V | undefined;\n}\n",
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(report.candidates_analyzed, 2);
}
