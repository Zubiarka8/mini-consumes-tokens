//! Visibility normalization through the shipped token sources and index analysis.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;

use mct_index::{
    find_duplicate_groups, DuplicateOptions, DuplicateReport, ExcludeSet, Index, Normalization,
};

const MODES: [Normalization; 3] = [
    Normalization::Broad,
    Normalization::Consistent,
    Normalization::Exact,
];
const VISIBILITIES: [&str; 5] = [
    "",
    "pub",
    "pub(crate)",
    "pub(super)",
    "pub(in crate::scope)",
];

struct Fixture {
    root: PathBuf,
    index: Index,
}

impl Fixture {
    fn new(tag: &str, sources: impl IntoIterator<Item = String>) -> Self {
        let root =
            std::env::temp_dir().join(format!("mct-visibility-{tag}-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        for (number, source) in sources.into_iter().enumerate() {
            fs::write(root.join(format!("copy_{number}.rs")), source).unwrap();
        }
        let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
        index
            .reindex(&mct_languages::build_registry(), false)
            .unwrap();
        Self { root, index }
    }

    fn report(&self, mode: Normalization, min_tokens: usize, min_lines: usize) -> DuplicateReport {
        let report = find_duplicate_groups(
            &self.index,
            &mct_languages::build_token_sources(),
            DuplicateOptions {
                normalization: mode,
                min_tokens,
                min_lines,
                excerpts: false,
                ..DuplicateOptions::default()
            },
        )
        .unwrap();
        assert!(report.complete, "{:?}", report.skipped);
        report
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn visibility_is_ignored_in_every_mode_and_at_both_threshold_boundaries() {
    // Put visibility on its own line: it must add neither tokens nor lines.
    // The function has exactly 14 source tokens on three token-bearing lines.
    let fixture = Fixture::new(
        "thresholds",
        VISIBILITIES
            .map(|visibility| format!("{visibility}\nfn twin(x: u8) -> u8 {{\n    x + 1\n}}\n")),
    );
    for mode in MODES {
        let report = fixture.report(mode, 14, 3);
        assert_eq!(report.candidates_analyzed, 5, "{mode:?}");
        assert_eq!(report.below_threshold, 0, "{mode:?}");
        assert_eq!(report.total_groups, 1, "{mode:?}");
        let group = &report.groups[0];
        assert_eq!(group.members.len(), 5, "{mode:?}");
        for member in &group.members {
            assert_eq!(member.token_count, 14, "{mode:?}: {member:?}");
            assert_eq!(member.line_count, 3, "{mode:?}: {member:?}");
        }
        for (tokens, lines) in [(15, 3), (14, 4)] {
            let below = fixture.report(mode, tokens, lines);
            assert_eq!(below.candidates_analyzed, 5, "{mode:?}");
            assert_eq!(below.below_threshold, 5, "{mode:?}");
            assert_eq!(below.total_groups, 0, "{mode:?}");
        }
    }
}

#[test]
fn nested_item_and_field_visibility_are_ignored_after_attributes_in_every_mode() {
    let fixture = Fixture::new(
        "nested",
        VISIBILITIES.map(|visibility| {
            format!(
                "#[inline]\n{visibility} fn twin() -> u8 {{\n\
                 struct S {{ {visibility} value: u8 }}\n\
                 #[inline]\n{visibility} fn nested() -> u8 {{ 1 }}\n\
                 nested()\n}}\n"
            )
        }),
    );
    for mode in MODES {
        let report = fixture.report(mode, 1, 1);
        // Both the outer function and its nested function must form full groups.
        for name in ["twin", "nested"] {
            let groups: Vec<_> = report
                .groups
                .iter()
                .filter(|group| {
                    group
                        .members
                        .iter()
                        .any(|member| member.symbol_name == name)
                })
                .collect();
            assert_eq!(groups.len(), 1, "{mode:?}: {name}");
            assert_eq!(groups[0].members.len(), 5, "{mode:?}: {name}");
            assert!(groups[0]
                .members
                .iter()
                .all(|member| member.symbol_name == name));
        }
    }
}

#[test]
fn ignoring_visibility_does_not_hide_an_operator_difference_in_any_mode() {
    let fixture = Fixture::new(
        "operators",
        [
            "fn twin(x: u8) -> u8 { x + 1 }".to_string(),
            "pub fn twin(x: u8) -> u8 { x + 1 }".to_string(),
            "pub(crate) fn twin(x: u8) -> u8 { x - 1 }".to_string(),
        ],
    );
    for mode in MODES {
        let report = fixture.report(mode, 1, 1);
        assert_eq!(report.candidates_analyzed, 3, "{mode:?}");
        assert_eq!(report.total_groups, 1, "{mode:?}");
        let paths: Vec<_> = report.groups[0]
            .members
            .iter()
            .map(|member| member.path.as_str())
            .collect();
        assert_eq!(paths, ["copy_0.rs", "copy_1.rs"], "{mode:?}");
    }
}
