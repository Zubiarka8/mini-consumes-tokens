//! Duplicate analysis over Kotlin through the real index: the real `KotlinParser`
//! symbols and the real `KotlinTokenSource` in the shared `mct-index` engine.
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
use mct_lang_kotlin::{KotlinParser, KotlinTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-kotlin-duplicates-{}-{}",
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
        registry.register(Arc::new(KotlinParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(KotlinTokenSource));
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
        "{{\n        var sum = 0\n        for (item in items) {{\n            sum += item.price * {factor}\n        }}\n        return sum\n    }}"
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
fn kotlin_is_a_supported_language() {
    let project = Project::new(&[(
        "A.kt",
        &format!("fun a(items: List<Item>): Int {}\n", body("2")),
    )]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["kotlin"]);
    assert!(report.unsupported_languages.is_empty());
}

#[test]
fn functions_and_methods_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "billing/Cart.kt",
            &format!(
                "class Cart {{\n    fun cartTotal(items: List<Item>): Int {}\n}}\n",
                body("2")
            ),
        ),
        (
            "billing/Order.kt",
            &format!(
                "/** The order total. */\nfun orderTotal(items: List<Item>): Int {}\n",
                body("3")
            ),
        ),
        (
            "billing/Other.kt",
            "fun other(x: Int?): Int {\n    if (x != null) {\n        return x\n    }\n    return 0\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("billing/Cart.kt", "cartTotal", "method"),
            member("billing/Order.kt", "orderTotal", "function"),
        ]]
    );
}

/// A lambda or anonymous function bound to a name outside a class body is
/// the parser's `Function` over the whole `val`; it groups like a `fun`.
#[test]
fn named_lambdas_group_with_their_whole_binding() {
    let lambda = |name: &str, factor: &str| {
        format!(
            "val {name} = {{ items: List<Item> ->\n    var sum = 0\n    for (item in items) {{\n        sum += item.price * {factor}\n    }}\n    sum\n}}\n"
        )
    };
    let project = Project::new(&[
        ("a/A.kt", &lambda("cartTotal", "2")),
        ("b/B.kt", &lambda("orderTotal", "3")),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("a/A.kt", "cartTotal", "function"),
            member("b/B.kt", "orderTotal", "function"),
        ]]
    );
}

#[test]
fn annotated_local_interface_and_anonymous_functions_are_matched_not_skipped() {
    let project = Project::new(&[(
        "A.kt",
        "interface Store {\n    @Deprecated(\"old\")\n    fun get(key: String): Int\n}\n\nclass Cart {\n    val onTap = { x: Int -> x }\n    fun outer() {\n        fun inner() = 1\n        val twice = fun(x: Int): Int = x * 2\n    }\n}\n\nfun Cart.ext() = 1\n",
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // `get`, `outer`, `inner`, `twice` and `ext`; the member lambda `onTap`
    // is a field.
    assert_eq!(report.candidates_analyzed, 5);
}
