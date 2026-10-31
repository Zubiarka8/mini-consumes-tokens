//! Duplicate analysis over Lua through the real index: the real `LuaParser`
//! symbols and the real `LuaTokenSource` in the shared `mct-index` engine.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::{LanguageRegistry, TokenSourceRegistry};
use mct_index::{find_duplicate_groups, DuplicateOptions, DuplicateReport, ExcludeSet, Index};
use mct_lang_lua::{LuaParser, LuaTokenSource};

struct Project {
    root: PathBuf,
    index: Index,
    registry: LanguageRegistry,
}

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-lua-duplicates-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let root = root.canonicalize().unwrap();
        let mut project = Self {
            index: Index::open_in_memory(&root, ExcludeSet::default()).unwrap(),
            registry: {
                let mut registry = LanguageRegistry::new();
                registry.register(Arc::new(LuaParser));
                registry
            },
            root,
        };
        for (path, contents) in files {
            project.write(path, contents);
        }
        project.index.reindex(&project.registry, false).unwrap();
        project
    }

    fn write(&self, relative: &str, contents: &str) {
        let path: &Path = &self.root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, contents).unwrap();
    }

    fn reindex(&mut self) {
        self.index.reindex(&self.registry, false).unwrap();
    }

    fn report(&self) -> DuplicateReport {
        let mut sources = TokenSourceRegistry::new();
        sources.register(Arc::new(LuaTokenSource));
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

fn total(name: &str, factor: &str) -> String {
    format!(
        "local function {name}(items)\n  local sum = 0\n  for _, item in ipairs(items) do\n    sum = sum + item.price * {factor}\n  end\n  return sum\nend\n"
    )
}

fn members(report: &DuplicateReport) -> Vec<Vec<(String, String)>> {
    report
        .groups
        .iter()
        .map(|group| {
            group
                .members
                .iter()
                .map(|m| (m.path.clone(), m.symbol_name.clone()))
                .collect()
        })
        .collect()
}

#[test]
fn lua_is_a_supported_language() {
    let project = Project::new(&[("a.lua", &total("a", "2"))]);
    assert_eq!(project.report().supported_languages, vec!["lua"]);
}

#[test]
fn duplicates_across_files_group_despite_names_literals_and_comments() {
    let project = Project::new(&[
        ("billing/cart.lua", &total("cart_total", "2")),
        (
            "billing/order.lua",
            &format!("-- the order total\n{}", total("order_total", "3")),
        ),
        (
            "billing/other.lua",
            "local function other(x)\n  if x then\n    return x\n  end\n  return nil\nend\n",
        ),
    ]);
    let report = project.report();
    assert_eq!(
        members(&report),
        vec![vec![
            ("billing/cart.lua".to_string(), "cart_total".to_string()),
            ("billing/order.lua".to_string(), "order_total".to_string()),
        ]]
    );
}

#[test]
fn duplicates_inside_one_file_group_and_methods_count() {
    let project = Project::new(&[(
        "shapes.lua",
        &format!(
            "local M = {{}}\n{}\nfunction M.tally(items)\n  local sum = 0\n  for _, item in ipairs(items) do\n    sum = sum + item.price * 5\n  end\n  return sum\nend\nreturn M\n",
            total("total", "2").replace("local function total", "function M.total")
        ),
    )]);
    let report = project.report();
    eprintln!(
        "{:?} {:?} {}",
        report.skipped, report.options, report.candidates_analyzed
    );
    let groups = members(&report);
    assert_eq!(groups.len(), 1, "{groups:?}");
    assert_eq!(groups[0].len(), 2);
}

#[test]
fn a_different_operator_or_control_flow_is_not_a_duplicate() {
    let project = Project::new(&[
        ("a.lua", &total("a", "2")),
        (
            "b.lua",
            &total("b", "2").replace("sum + item", "sum - item"),
        ),
    ]);
    assert!(project.report().groups.is_empty());
}

#[test]
fn an_edit_that_removes_the_clone_removes_the_group_incrementally() {
    let mut project = Project::new(&[("a.lua", &total("a", "2")), ("b.lua", &total("b", "2"))]);
    assert_eq!(project.report().groups.len(), 1);
    project.write("b.lua", "local function b()\n  return 1\nend\n");
    project.reindex();
    assert!(project.report().groups.is_empty());
}
