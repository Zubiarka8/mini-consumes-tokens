//! Duplicate analysis over C++ through the real index: the real `CppParser`
//! symbols and the real `CppTokenSource` in the shared `mct-index` engine.
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
use mct_lang_cpp::{CppParser, CppTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-cpp-duplicates-{}-{}",
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
        registry.register(Arc::new(CppParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(CppTokenSource));
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
        "{{\n    int sum = 0;\n    for (const auto& item : items) {{\n        sum += item.price * {factor};\n    }}\n    return sum;\n}}"
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
fn cpp_is_a_supported_language() {
    let project = Project::new(&[(
        "a.cpp",
        &format!("int total(const std::vector<Item>& items) {}\n", body("2")),
    )]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["cpp"]);
    assert!(report.unsupported_languages.is_empty());
}

/// An out-of-line definition (`Cart::cartTotal`) is the method `cartTotal`,
/// named by its last segment; two of the same shape group.
#[test]
fn definitions_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "src/cart.cpp",
            &format!(
                "int Cart::cartTotal(const std::vector<Item>& items) {}\n",
                body("2")
            ),
        ),
        (
            "src/order.cpp",
            &format!(
                "// The order total.\nint Order::orderTotal(const std::vector<Item>& items) {}\n",
                body("3")
            ),
        ),
        (
            "src/other.cpp",
            "int other(int* x) {\n    if (x != nullptr) {\n        return *x;\n    }\n    return 0;\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("src/cart.cpp", "cartTotal", "method"),
            member("src/order.cpp", "orderTotal", "method"),
        ]]
    );
}

/// Prototypes span their whole declaration; operators and destructors are
/// named by their declarator's text; a named lambda spans its declarator;
/// a function-like macro is a function.
#[test]
fn prototypes_operators_destructors_lambdas_and_macros_are_matched_not_skipped() {
    let project = Project::new(&[(
        "src/cart.hpp",
        "#define SQ(x) ((x) * (x))\nnamespace shop {\nclass Cart {\npublic:\n    Cart(int n) : n_(n) {}\n    ~Cart();\n    int total() const;\n    bool operator==(const Cart& o) const { return n_ == o.n_; }\nprivate:\n    int n_;\n};\nvoid run();\n}\nvoid shop::run() {\n    auto twice = [](int x) { return x * 2; };\n    int local(int);\n}\n",
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // `SQ`, the constructor, `~Cart`, `total`, `operator==`, the prototype
    // and the definition of `run`, and `twice`; `local` is a local.
    assert_eq!(report.candidates_analyzed, 8);
}
