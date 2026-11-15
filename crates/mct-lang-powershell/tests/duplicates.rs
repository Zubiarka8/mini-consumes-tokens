//! Duplicate analysis over PowerShell through the real index: the real `PowerShellParser`
//! symbols and the real `PowerShellTokenSource` in the shared `mct-index` engine.
//! Every callable form the parser indexes as a function or method has a
//! callable boundary that matches its symbol, so none is skipped for an
//! invalid range.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{LanguageRegistry, SymbolKind, TokenSource, TokenSourceRegistry};
use mct_index::{find_duplicate_groups, DuplicateOptions, DuplicateReport, ExcludeSet, Index};
use mct_lang_powershell::{PowerShellParser, PowerShellTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-powershell-duplicates-{}-{}",
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
        registry.register(Arc::new(PowerShellParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(PowerShellTokenSource));
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
        "{{\n    param([object[]]$Items)\n    $sum = 0\n    foreach ($item in $Items) {{\n        $sum += $item.Price * {factor}\n    }}\n    return $sum\n}}"
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
fn powershell_is_a_supported_language() {
    let project = Project::new(&[("a.ps1", &format!("function Get-Total {}\n", body("2")))]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["powershell"]);
    assert!(report.unsupported_languages.is_empty());
}

/// Two functions of the same shape group across files; names, literals and
/// comments do not matter.
#[test]
fn functions_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "src/Cart.ps1",
            &format!("function Get-CartTotal {}\n", body("2")),
        ),
        (
            "src/Order.psm1",
            &format!(
                "# The order total.\nfunction Get-OrderTotal {}\n",
                body("3")
            ),
        ),
        (
            "src/Other.ps1",
            "function Get-Other {\n    param($X)\n    if ($null -ne $X) {\n        return $X\n    }\n    return 0\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("src/Cart.ps1", "Get-CartTotal", "function"),
            member("src/Order.psm1", "Get-OrderTotal", "function"),
        ]]
    );
}

/// Functions, filters, nested functions, class methods and constructors
/// have no `name` field; each still spans its definition.
const SHOP: &str = "function Invoke-Outer {\n    function Invoke-Inner { 'i' }\n    Invoke-Inner\n}\nfilter Select-Even { if ($_ % 2 -eq 0) { $_ } }\nclass Cart {\n    [int] $N\n    Cart([int]$n) { $this.N = $n }\n    [int] Total() {\n        return $this.N\n    }\n    static [Cart] Empty() { return [Cart]::new(0) }\n}\n$block = { 'not a symbol' }\n";

#[test]
fn functions_filters_methods_and_constructors_are_matched_not_skipped() {
    let project = Project::new(&[("src/Shop.psm1", SHOP)]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // `Invoke-Outer`, `Invoke-Inner`, `Select-Even`, the constructor
    // `Cart`, `Total` and `Empty`.
    assert_eq!(report.candidates_analyzed, 6);
}

/// No node here has a `name` field, so every span comes from the callback.
/// Without one the engine would fall back to the symbol's own range, so
/// the spans are compared with the parser's symbols directly.
#[test]
fn every_function_and_method_has_a_span_like_its_symbol() {
    let parsed = mct_corpus::parse_source(&PowerShellParser, "src/Shop.psm1", "PowerShell", SHOP);
    let mut symbols: Vec<(String, u32, Option<u32>)> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Function | SymbolKind::Method))
        .map(|s| (s.name.clone(), s.location.line, s.location.end_line))
        .collect();
    let file = PowerShellTokenSource
        .tokenize("src/Shop.psm1", SHOP)
        .unwrap();
    let mut spans: Vec<(String, u32, Option<u32>)> = file
        .callables
        .unwrap_or_default()
        .iter()
        .map(|span| (span.name.clone(), span.lines.start, Some(span.lines.end)))
        .collect();
    symbols.sort();
    spans.sort();
    assert_eq!(symbols.len(), 6);
    assert_eq!(spans, symbols);
}
