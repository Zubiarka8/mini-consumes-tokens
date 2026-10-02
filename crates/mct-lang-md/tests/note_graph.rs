//! Markdown note graph (issue #98) end to end through the real index:
//! path identity vs. duplicate names, note-scoped anchors, and the
//! edit/rename/delete/reindex lifecycle. Each test builds a throwaway vault.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-md/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, RelationHit};
use mct_lang_md::MarkdownParser;

struct Vault {
    dir: PathBuf,
    index: Index,
    registry: LanguageRegistry,
}

impl Vault {
    fn new(files: &[(&str, &str)]) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!(
            "mct-md-note-graph-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut registry = LanguageRegistry::new();
        registry.register(Arc::new(MarkdownParser));
        let index = Index::open_in_memory(&dir, ExcludeSet::default()).unwrap();
        let mut vault = Self {
            dir,
            index,
            registry,
        };
        for (path, contents) in files {
            vault.write(path, contents);
        }
        vault.reindex();
        vault
    }

    fn write(&self, path: &str, contents: &str) {
        let file = self.dir.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, contents).unwrap();
    }

    fn reindex(&mut self) {
        let report = self.index.reindex(&self.registry, false).unwrap();
        assert!(report.issues.is_empty(), "{:?}", report.issues);
    }

    /// The relation(s) to `to_name` made from the note/heading `from` in `path`.
    fn link(&self, path: &str, to_name: &str) -> Vec<RelationHit> {
        self.index
            .find_references(to_name)
            .unwrap()
            .into_iter()
            .filter(|h| h.relative_path == path)
            .collect()
    }

    /// `status:path:kind:name` of the single relation to `to_name` in `path`:
    /// the status and every candidate, for terse assertions.
    fn outcome(&self, path: &str, to_name: &str) -> String {
        let hits = self.link(path, to_name);
        assert_eq!(hits.len(), 1, "{path} -> {to_name}: {hits:?}");
        let candidates: Vec<String> = self
            .index
            .relation_candidates(hits[0].relation_id)
            .unwrap()
            .iter()
            .map(|s| format!("{}:{}:{}", s.relative_path, s.kind, s.name))
            .collect();
        format!("{} {}", hits[0].resolution.as_str(), candidates.join(" | "))
    }
}

impl Drop for Vault {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn two_notes_with_the_same_shared_heading_do_not_cross_link() {
    // The audit repro: both notes have `## Shared`; the link names `beta`.
    let v = Vault::new(&[
        ("alpha.md", "# Alpha\n\n## Shared\n\ntext\n"),
        ("beta.md", "# Beta\n\n## Shared\n\ntext\n"),
        (
            "src.md",
            "[[beta#Shared]] and [[alpha#Shared]] and [[#Shared]]\n",
        ),
    ]);
    let hits = v.link("src.md", "Shared");
    assert_eq!(hits.len(), 3, "{hits:?}");
    let mut seen: Vec<String> = hits
        .iter()
        .map(|h| {
            let c = v.index.relation_candidates(h.relation_id).unwrap();
            format!(
                "{}:{}",
                h.resolution.as_str(),
                c.iter()
                    .map(|s| s.relative_path.clone())
                    .collect::<Vec<_>>()
                    .join(",")
            )
        })
        .collect();
    seen.sort();
    // `[[#Shared]]` scopes to src.md, which has no such heading.
    assert_eq!(
        seen,
        ["resolved:alpha.md", "resolved:beta.md", "unresolved:"]
    );
}

#[test]
fn duplicate_basenames_are_ambiguous_but_a_path_picks_one() {
    let v = Vault::new(&[
        ("a/beta.md", "# One\n"),
        ("b/beta.md", "# Two\n"),
        ("src.md", "[[beta]]\n\n[[a/beta]]\n\n[[b/beta.md]]\n"),
    ]);
    let hits = v.link("src.md", "beta");
    assert_eq!(hits.len(), 3);
    let by_line = |i: usize| {
        let c = v.index.relation_candidates(hits[i].relation_id).unwrap();
        (
            hits[i].resolution.as_str(),
            c.iter()
                .map(|s| s.relative_path.as_str())
                .collect::<Vec<_>>()
                .join(","),
        )
    };
    assert_eq!(by_line(0), ("ambiguous", "a/beta.md,b/beta.md".to_string()));
    assert_eq!(by_line(1), ("resolved", "a/beta.md".to_string()));
    assert_eq!(by_line(2), ("resolved", "b/beta.md".to_string()));
}

#[test]
fn an_anchor_on_an_ambiguous_note_stays_ambiguous_not_first_match() {
    let v = Vault::new(&[
        ("a/beta.md", "# One\n\n## Sec\n"),
        ("b/beta.md", "# Two\n\n## Sec\n"),
        ("src.md", "[[beta#Sec]]\n"),
    ]);
    assert_eq!(
        v.outcome("src.md", "Sec"),
        "ambiguous a/beta.md:element:Sec | b/beta.md:element:Sec"
    );
}

#[test]
fn a_duplicate_heading_inside_one_note_is_ambiguous() {
    let v = Vault::new(&[
        ("beta.md", "# B\n\n## Dup\n\n## Dup\n"),
        ("src.md", "[[beta#Dup]]\n"),
    ]);
    assert!(v.outcome("src.md", "Dup").starts_with("ambiguous "));
}

#[test]
fn a_heading_in_another_note_is_never_a_fallback() {
    let v = Vault::new(&[
        ("beta.md", "# B\n"),
        ("other.md", "# O\n\n## Only Here\n"),
        ("src.md", "[[beta#Only Here]]\n"),
    ]);
    assert_eq!(v.outcome("src.md", "Only Here"), "unresolved ");
}

#[test]
fn relative_paths_resolve_from_the_source_note_and_escapes_stay_unresolved() {
    let v = Vault::new(&[
        ("x/y/src.md", "[[./near]] [[../up]] [[../../../escape]]\n"),
        ("x/y/near.md", "# n\n"),
        ("x/up.md", "# u\n"),
        ("escape.md", "# e\n"),
    ]);
    assert_eq!(
        v.outcome("x/y/src.md", "near"),
        "resolved x/y/near.md:module:near"
    );
    assert_eq!(v.outcome("x/y/src.md", "up"), "resolved x/up.md:module:up");
    assert_eq!(v.outcome("x/y/src.md", "../../../escape"), "unresolved ");
}

#[test]
fn a_heading_named_like_a_note_is_not_the_note() {
    // `[[Topic]]` has no `Topic.md`; another note's `# Topic` heading must not
    // satisfy it.
    let v = Vault::new(&[("other.md", "# Topic\n"), ("src.md", "[[Topic]]\n")]);
    assert_eq!(v.outcome("src.md", "Topic"), "unresolved ");
}

#[test]
fn editing_a_link_removes_the_stale_edge() {
    let mut v = Vault::new(&[("beta.md", "# B\n"), ("src.md", "[[beta]]\n")]);
    assert_eq!(v.outcome("src.md", "beta"), "resolved beta.md:module:beta");
    v.write("src.md", "no link now\n");
    v.reindex();
    assert!(v.link("src.md", "beta").is_empty());
    assert!(v.index.find_references("beta").unwrap().is_empty());
}

#[test]
fn renaming_a_note_leaves_old_links_unresolved_and_new_ones_resolve() {
    let mut v = Vault::new(&[("beta.md", "# B\n"), ("src.md", "[[beta]]\n")]);
    fs::rename(v.dir.join("beta.md"), v.dir.join("gamma.md")).unwrap();
    v.reindex();
    assert_eq!(v.outcome("src.md", "beta"), "unresolved ");
    assert!(v.index.find_symbol("beta").unwrap().is_empty());
    v.write("src.md", "[[gamma]]\n");
    v.reindex();
    assert_eq!(
        v.outcome("src.md", "gamma"),
        "resolved gamma.md:module:gamma"
    );
}

#[test]
fn moving_a_note_to_another_folder_breaks_its_path_link_but_not_a_bare_one() {
    let mut v = Vault::new(&[
        ("old/beta.md", "# B\n"),
        ("src.md", "[[old/beta]]\n\n[[beta]]\n"),
    ]);
    fs::create_dir_all(v.dir.join("new")).unwrap();
    fs::rename(v.dir.join("old/beta.md"), v.dir.join("new/beta.md")).unwrap();
    v.reindex();
    let hits = v.link("src.md", "beta");
    let status: Vec<(&str, usize)> = hits
        .iter()
        .map(|h| (h.resolution.as_str(), h.candidate_count))
        .collect();
    assert_eq!(status, [("unresolved", 0), ("resolved", 1)]);
}

#[test]
fn deleting_a_note_drops_it_as_a_target() {
    let mut v = Vault::new(&[("beta.md", "# B\n\n## S\n"), ("src.md", "[[beta#S]]\n")]);
    assert_eq!(v.outcome("src.md", "S"), "resolved beta.md:element:S");
    fs::remove_file(v.dir.join("beta.md")).unwrap();
    v.reindex();
    assert_eq!(v.outcome("src.md", "beta"), "unresolved ");
    assert_eq!(v.outcome("src.md", "S"), "unresolved ");
}

#[test]
fn a_second_note_with_the_same_name_makes_a_bare_link_ambiguous_not_rebound() {
    let mut v = Vault::new(&[("a/beta.md", "# B\n"), ("src.md", "[[beta]]\n")]);
    assert_eq!(
        v.outcome("src.md", "beta"),
        "resolved a/beta.md:module:beta"
    );
    v.write("b/beta.md", "# B2\n");
    v.reindex();
    assert!(v.outcome("src.md", "beta").starts_with("ambiguous "));
}

#[test]
fn incremental_and_full_reindex_agree() {
    let mut v = Vault::new(&[
        ("a/beta.md", "---\ntags: [t]\n---\n# B\n\n## S\n"),
        ("src.md", "[[beta#S]] ![[a/beta]]\n"),
    ]);
    v.write("src.md", "[[a/beta#S]] ![[beta]] #new\n");
    v.reindex();
    let snapshot = |v: &Vault| {
        let mut rows: Vec<String> = ["S", "beta", "tag:new", "tag:t"]
            .iter()
            .flat_map(|n| v.index.find_references(n).unwrap())
            .map(|h| {
                format!(
                    "{}:{}:{}:{}:{}",
                    h.relative_path,
                    h.to_name,
                    h.kind,
                    h.resolution.as_str(),
                    h.candidate_count
                )
            })
            .collect();
        rows.sort();
        rows
    };
    let incremental = snapshot(&v);
    v.index.reindex(&v.registry, true).unwrap();
    assert_eq!(incremental, snapshot(&v));
    assert!(!incremental.is_empty());
}

#[test]
fn a_heading_less_or_empty_file_is_a_searchable_note() {
    let v = Vault::new(&[("bare.md", "just text\n"), ("empty.md", "")]);
    for stem in ["bare", "empty"] {
        let hits = v.index.find_symbol(stem).unwrap();
        assert_eq!(hits.len(), 1, "{hits:?}");
        assert_eq!(hits[0].kind, "module");
        assert_eq!(hits[0].relative_path, format!("{stem}.md"));
    }
    let found = v
        .index
        .search_symbols("bare", mct_index::QueryScope::default())
        .unwrap();
    assert!(found.iter().any(|h| h.name == "bare"), "{found:?}");
}
