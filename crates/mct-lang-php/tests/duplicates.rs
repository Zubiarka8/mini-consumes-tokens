//! Duplicate analysis over PHP through the real index: the real `PhpParser`
//! symbols and the real `PhpTokenSource` in the shared `mct-index` engine.
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
use mct_lang_php::{PhpParser, PhpTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-php-duplicates-{}-{}",
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
        registry.register(Arc::new(PhpParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(PhpTokenSource));
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
        "{{\n    $sum = 0;\n    foreach ($items as $item) {{\n        $sum += $item->price * {factor};\n    }}\n    return $sum;\n}}"
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
fn php_is_a_supported_language() {
    let project = Project::new(&[(
        "a.php",
        &format!("<?php\nfunction total(array $items): int {}\n", body("2")),
    )]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["php"]);
    assert!(report.unsupported_languages.is_empty());
}

/// Two methods of the same shape group across files; names, literals,
/// comments and inline HTML do not matter.
#[test]
fn methods_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "src/Cart.php",
            &format!(
                "<?php\nclass Cart {{\n    public function cartTotal(array $items): int {}\n}}\n",
                body("2")
            ),
        ),
        (
            "src/Order.php",
            &format!(
                "<h1>Order</h1>\n<?php\nclass Order {{\n    // The order total.\n    public function orderTotal(array $items): int {}\n}}\n",
                body("3")
            ),
        ),
        (
            "src/other.php",
            "<?php\nfunction other(?int $x): int {\n    if ($x !== null) {\n        return $x;\n    }\n    return 0;\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("src/Cart.php", "cartTotal", "method"),
            member("src/Order.php", "orderTotal", "method"),
        ]]
    );
}

/// Attributed, abstract and interface methods span their declaration;
/// a closure or arrow function assigned to a variable is a function named
/// after it that spans the closure.
#[test]
fn methods_and_bound_closures_are_matched_not_skipped() {
    let project = Project::new(&[(
        "src/shop.php",
        "<?php\nnamespace Shop;\n#[Pure]\nfunction helper(int $x): int { return $x * 2; }\nabstract class Cart {\n    public function __construct(private int $n) {}\n    #[\\Override]\n    public function total(): int { return $this->n; }\n    abstract protected function f();\n}\ninterface I { public function g(): void; }\ntrait T {\n    public function h() {\n        $inner = fn($y) => $y + 1;\n        return $inner(2);\n    }\n}\n$cb = function (int $x) use ($y) {\n    return $x * 2;\n};\n$ar = fn($x) => $x + 1;\n$obj->prop = function () {};\narray_map(function ($z) { return $z; }, []);\n",
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // `helper`, `__construct`, `total`, `f`, `g`, `h`, `inner`, `cb` and
    // `ar`; the closure in a property and the unnamed one are no symbols.
    assert_eq!(report.candidates_analyzed, 9);
}
