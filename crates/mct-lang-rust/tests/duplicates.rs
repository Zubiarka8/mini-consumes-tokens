//! Real Rust parser/tokenizer coverage for duplicate candidates.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::TokenSourceRegistry;
use mct_index::{find_duplicate_groups, DuplicateOptions};
use mct_lang_rust::{RustParser, RustTokenSource};
use std::{
    fs,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

fn report(files: &[(&str, String)], options: DuplicateOptions) -> mct_index::DuplicateReport {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "mct-rust-duplicate-audit-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    fs::create_dir_all(&root).unwrap();
    for (path, source) in files {
        fs::write(root.join(path), source).unwrap();
    }
    let index = mct_corpus::open_indexed(
        &root,
        Arc::new(RustParser),
        files.len(),
        "Rust duplicate fixtures",
    );
    let mut sources = TokenSourceRegistry::new();
    sources.register(Arc::new(RustTokenSource));
    let report = find_duplicate_groups(&index, &sources, options).unwrap();
    fs::remove_dir_all(root).unwrap();
    report
}

#[test]
fn four_bound_closures_in_two_files_are_four_members_at_default_thresholds() {
    let body = "|x: i64| {\n let mut total = x;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}";
    let report = report(
        &[
            (
                "a.rs",
                format!("fn a() {{ let first = {body}; let second = {body}; }}"),
            ),
            (
                "b.rs",
                format!("fn b() {{ let third = {body}; let fourth = {body}; }}"),
            ),
        ],
        DuplicateOptions::default(),
    );
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    let group = report.groups.iter().find(|g| g.members.len() == 4).unwrap();
    assert_eq!(
        group
            .members
            .iter()
            .map(|m| m.symbol_name.as_str())
            .collect::<Vec<_>>(),
        ["first", "second", "third", "fourth"]
    );
}

#[test]
fn same_named_methods_on_one_line_keep_separate_byte_boundaries() {
    let report = report(&[("a.rs", "#![allow(dead_code)]\r\nconst NOTE: &str = \"á\"; struct A; struct B; impl A { fn read(x: u32) -> u32 { x + x } } impl B { fn read(y: u32) -> u32 { y + y } }".into())], DuplicateOptions { min_lines: 1, min_tokens: 1, ..Default::default() });
    assert!(report.skipped.is_empty(), "{:?}", report.skipped);
    assert_eq!(report.groups.len(), 1);
    assert_eq!(report.groups[0].members.len(), 2);
    assert_eq!(report.groups[0].token_count, 14);
    assert!(report.groups[0].members[0].start_column < report.groups[0].members[1].start_column);
    assert_eq!(
        report.groups[0].evidence.identifier_pattern.as_str(),
        "one-to-one"
    );
}
