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

#[test]
fn external_path_names_matching_parameters_are_not_local_renamings() {
    let source = "fn first(input: i64) -> i64 {\n if input < 0 {\n return 0;\n }\n helpers::input(input)\n}\nfn second(data: i64) -> i64 {\n if data < 0 {\n return 0;\n }\n helpers::data(data)\n}\n";
    let result = report(
        &[("a.rs", source.into())],
        DuplicateOptions {
            min_tokens: 1,
            ..Default::default()
        },
    );
    assert_eq!(result.groups.len(), 1);
    let group = &result.groups[0];
    assert_eq!(group.verdict.level, mct_index::VerdictLevel::Suspicious);
    assert_eq!(group.evidence.undeclared_name_positions, 1);
}

#[test]
fn renamed_tuple_parameters_do_not_make_identical_rules_suspicious() {
    let result = report(
        &[(
            "a.rs",
            include_str!("../../../benchmarks/token-twins-labels/rust/tuple_parameters.rs").into(),
        )],
        DuplicateOptions::default(),
    );
    assert_eq!(result.groups.len(), 1);
    assert_eq!(
        result.groups[0].verdict.level,
        mct_index::VerdictLevel::Real
    );
    assert_eq!(result.groups[0].evidence.undeclared_name_positions, 0);
}

#[test]
fn namespace_qualifiers_and_selected_items_cannot_borrow_parameter_binding_roles() {
    for (left, right) in [
        ("helpers::input(input)", "helpers::data(data)"),
        ("input::convert(input)", "data::convert(data)"),
    ] {
        let a = format!("fn first(input: i64) -> i64 {{ if input < 0 {{ return 0; }} {left} }}");
        let b = format!("fn second(data: i64) -> i64 {{ if data < 0 {{ return 0; }} {right} }}");
        for normalization in [
            mct_index::Normalization::Broad,
            mct_index::Normalization::Consistent,
        ] {
            let result = report(
                &[("a.rs", a.clone()), ("b.rs", b.clone())],
                DuplicateOptions {
                    min_lines: 1,
                    min_tokens: 1,
                    normalization,
                    ..Default::default()
                },
            );
            assert_eq!(result.groups.len(), 1);
            assert_eq!(
                result.groups[0].verdict.level,
                mct_index::VerdictLevel::Suspicious,
                "{left}"
            );
        }
    }
}

#[test]
fn file_creation_order_and_reindexing_do_not_change_nested_reports_or_overlap_members() {
    let inner = "fn inner(value: i64) -> i64 {\n if value < 0 {\n return 0;\n }\n if value > 100 {\n return 100;\n }\n value + 1\n}";
    let source = format!("fn outer(value: i64) -> i64 {{\n{inner}\ninner(value)\n}}\n");
    for normalization in [
        mct_index::Normalization::Broad,
        mct_index::Normalization::Consistent,
        mct_index::Normalization::Exact,
    ] {
        let options = DuplicateOptions {
            partial_blocks: Some(mct_index::PartialBlockBudget::default()),
            min_tokens: 1,
            normalization,
            ..Default::default()
        };
        let files = [("a.rs", source.clone()), ("b.rs", source.clone())];
        let forward = report(&files, options.clone());
        let backward = report(&[files[1].clone(), files[0].clone()], options.clone());
        assert_eq!(forward, backward, "{normalization:?}");
        assert_eq!(forward, report(&files, options), "{normalization:?}");
        assert!(forward.groups.len() >= 2);
        let mut seen = std::collections::BTreeSet::new();
        for group in &forward.groups {
            for (i, member) in group.members.iter().enumerate() {
                assert!(seen.insert((
                    &member.path,
                    member.start_line,
                    member.end_line,
                    member.start_column
                )));
                for other in &group.members[i + 1..] {
                    assert!(
                        member.path != other.path
                            || member.end_line < other.start_line
                            || other.end_line < member.start_line
                    );
                }
            }
        }
    }
}
