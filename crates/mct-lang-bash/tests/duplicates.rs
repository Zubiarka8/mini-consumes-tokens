//! Duplicate analysis over Bash through the real index: the real `BashParser`
//! symbols and the real `BashTokenSource` in the shared `mct-index` engine.
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
use mct_lang_bash::{BashParser, BashTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-bash-duplicates-{}-{}",
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
        registry.register(Arc::new(BashParser));
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index.reindex(&registry, false).unwrap();
        Self { root, index }
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(BashTokenSource));
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
        "{{\n  local sum=0\n  for item in \"$@\"; do\n    sum=$((sum + item * {factor}))\n  done\n  echo \"$sum\"\n}}"
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
fn bash_is_a_supported_language() {
    let project = Project::new(&[("a.sh", &format!("total() {}\n", body("2")))]);
    let report = project.report();
    assert_eq!(report.supported_languages, vec!["bash"]);
    assert!(report.unsupported_languages.is_empty());
}

/// Two functions of the same shape group across files; names, literals and
/// comments do not matter.
#[test]
fn functions_group_across_files_despite_names_literals_and_comments() {
    let project = Project::new(&[
        (
            "bin/cart.sh",
            &format!("#!/bin/bash\nfunction cart_total() {}\n", body("2")),
        ),
        (
            "bin/order.sh",
            &format!(
                "#!/bin/sh\n# The order total.\nfunction order_total() {}\n",
                body("3")
            ),
        ),
        (
            "bin/other.sh",
            "other() {\n  if [ -n \"$1\" ]; then\n    echo \"$1\"\n  fi\n  return 0\n}\n",
        ),
    ]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(
        members(&report),
        vec![vec![
            member("bin/cart.sh", "cart_total", "function"),
            member("bin/order.sh", "order_total", "function"),
        ]]
    );
}

/// Both forms, a nested function, a redirected one and one whose body is a
/// subshell each span their definition.
#[test]
fn every_function_form_is_matched_not_skipped() {
    let project = Project::new(&[(
        "bin/tool.sh",
        "#!/bin/bash\nfunction a { echo a; }\nb() { echo b; }\nouter() {\n  inner() { echo i; }\n  inner\n}\nquiet() { echo q; } > /dev/null\nsub() (\n  cd /tmp && ls\n)\n",
    )]);
    let report = project.report();
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    // `a`, `b`, `outer`, `inner`, `quiet` and `sub`.
    assert_eq!(report.candidates_analyzed, 6);
}
