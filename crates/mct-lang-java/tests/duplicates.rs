//! Duplicate analysis over Java through the real index: the real `JavaParser`
//! symbols and the real `JavaTokenSource` in the shared `mct-index` engine.
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
use mct_lang_java::{JavaParser, JavaTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-java-duplicates-{}-{}",
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
        registry.register(Arc::new(JavaParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(JavaTokenSource));
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
        "{{\n        int sum = 0;\n        for (Item item : items) {{\n            sum += item.price * {factor};\n        }}\n        return sum;\n    }}"
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
fn java_is_a_supported_language() {
    let project = Project::new(&[(
        "A.java",
        &format!(
            "class A {{\n    int a(List<Item> items) {}\n}}\n",
            body("2")
        ),
    )]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["java"]);
    assert!(report.unsupported_languages.is_empty());
}

#[test]
fn methods_group_across_files_despite_names_literals_comments_and_annotations() {
    let project = Project::new(&[
        (
            "billing/Cart.java",
            &format!(
                "class Cart {{\n    int cartTotal(List<Item> items) {}\n}}\n",
                body("2")
            ),
        ),
        (
            "billing/Order.java",
            &format!(
                "class Order {{\n    /** The order total. */\n    int orderTotal(List<Item> items) {}\n}}\n",
                body("3")
            ),
        ),
        (
            "billing/Other.java",
            "class Other {\n    int other(Integer x) {\n        if (x != null) {\n            return x;\n        }\n        return 0;\n    }\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("billing/Cart.java", "cartTotal", "method"),
            member("billing/Order.java", "orderTotal", "method"),
        ]]
    );
}

#[test]
fn constructors_interface_methods_and_record_accessors_are_matched_not_skipped() {
    let project = Project::new(&[(
        "A.java",
        &format!(
            "interface Store {{\n    int get(String key);\n}}\n\nrecord Point(int x, int y) {{\n    public int y() {{ return y; }}\n}}\n\nclass Cart {{\n    @Deprecated\n    Cart(List<Item> items) {}\n}}\n",
            body("2")
        ),
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // `get`, the accessor `x` (from its component), the explicit `y()` and
    // the constructor.
    assert_eq!(report.candidates_analyzed, 4);
}
