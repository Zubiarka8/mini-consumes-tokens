//! Duplicate analysis over C# through the real index: the real `CSharpParser`
//! symbols and the real `CSharpTokenSource` in the shared `mct-index` engine.
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
use mct_lang_csharp::{CSharpParser, CSharpTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-csharp-duplicates-{}-{}",
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
        registry.register(Arc::new(CSharpParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(CSharpTokenSource));
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
        "{{\n        int sum = 0;\n        foreach (var item in items) {{\n            sum += item.Price * {factor};\n        }}\n        return sum;\n    }}"
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
fn csharp_is_a_supported_language() {
    let project = Project::new(&[(
        "A.cs",
        &format!(
            "class A {{\n    int Total(List<Item> items) {}\n}}\n",
            body("2")
        ),
    )]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["csharp"]);
    assert!(report.unsupported_languages.is_empty());
}

#[test]
fn methods_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "Billing/Cart.cs",
            &format!(
                "class Cart {{\n    int CartTotal(List<Item> items) {}\n}}\n",
                body("2")
            ),
        ),
        (
            "Billing/Order.cs",
            &format!(
                "class Order {{\n    /// <summary>The order total.</summary>\n    int OrderTotal(List<Item> items) {}\n}}\n",
                body("3")
            ),
        ),
        (
            "Billing/Other.cs",
            "class Other {\n    int Other(int? x) {\n        if (x != null) {\n            return x.Value;\n        }\n        return 0;\n    }\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("Billing/Cart.cs", "CartTotal", "method"),
            member("Billing/Order.cs", "OrderTotal", "method"),
        ]]
    );
}

/// Operators, conversion operators and destructors are named by the parser
/// (`operator +`, `operator int`, `~Cart`), not by a `name` field; a local
/// bound to a lambda is the parser's `Function` over its declarator.
#[test]
fn operators_destructors_local_functions_and_bound_lambdas_are_matched_not_skipped() {
    let project = Project::new(&[(
        "A.cs",
        "interface IStore {\n    int Get(string key);\n}\n\nclass Cart {\n    [Obsolete]\n    public Cart() { }\n    ~Cart() { }\n    public static Cart operator +(Cart a, Cart b) => a;\n    public static implicit operator int(Cart c) => 0;\n    void Run() {\n        int Local(int z) => z;\n        Func<int, int> twice = x => x * 2;\n    }\n}\n",
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // `Get`, the constructor, the destructor, both operators, `Run`,
    // `Local` and `twice`.
    assert_eq!(report.candidates_analyzed, 8);
}
