//! Dashboard decisions as seen by the AI-facing outputs (AUD-02), dead-symbol
//! identity across same-name siblings (AUD-10) and the conservative
//! duplicate-group membership contract. Every decision here is saved through
//! the real dashboard `Site`/`Store`; the outputs are the real MCP tool and
//! CLI report code paths.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::i18n::Lang;
use super::reviews::{Kind, State, Status};
use super::Site;
use std::fs;
use std::path::{Path, PathBuf};

const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";
const OTHER: &str = "(items: &[u8]) -> usize {\n let mut seen = 0;\n for item in items {\n  if *item > 3 { seen += 2; }\n  else { seen += 1; }\n }\n seen * 7\n}\n";

fn root(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().canonicalize().unwrap()
}

fn reindex(root: &Path) {
    let mut index = mct_index::Index::open(
        root,
        &root.join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    let report = index
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

fn load(root: &Path) -> Site {
    Site::load(root, true).unwrap()
}

/// Saves a decision exactly as the server-rendered review form does.
fn decide(site: &mut Site, kind: Kind, identity: &str, action: &str) {
    let fingerprint = site.fingerprint_of(kind, identity).unwrap();
    let revision = site
        .reviews
        .decision_revision(kind, identity)
        .unwrap()
        .to_string();
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("csrf", site.csrf.as_str()),
            ("kind", kind.value()),
            ("id", identity),
            ("action", action),
            ("reason", "reviewed"),
            ("fingerprint", fingerprint.as_str()),
            ("expected_revision", revision.as_str()),
        ])
        .finish();
    let response = site.post("review", &body, Lang::En);
    assert_eq!(response.status, 303, "{}", response.body);
}

fn delete_orphan(site: &mut Site, kind: Kind, identity: &str) {
    let revision = site
        .reviews
        .decision_revision(kind, identity)
        .unwrap()
        .to_string();
    let body = form_urlencoded::Serializer::new(String::new())
        .extend_pairs([
            ("csrf", site.csrf.as_str()),
            ("kind", kind.value()),
            ("id", identity),
            ("expected_revision", revision.as_str()),
        ])
        .finish();
    let response = site.post("review/delete", &body, Lang::En);
    assert_eq!(response.status, 303, "{}", response.body);
}

fn dead_id(site: &Site, name: &str, nth: usize) -> String {
    site.dead
        .iter()
        .filter(|dead| dead.entry.name == name)
        .nth(nth)
        .unwrap()
        .identity
        .clone()
}

fn dead_states(site: &Site, name: &str) -> Vec<State> {
    (0..site.dead.len())
        .filter(|id| site.dead[*id].entry.name == name)
        .map(|id| site.dead_state(id))
        .collect()
}

fn decided(state: &State) -> bool {
    matches!(state, State::Decided { .. })
}

/// The real MCP `find_dead_code` tool, dispatched by name as `mct-cli call` does.
fn mcp_dead(root: &Path, offset: usize, limit: usize) -> String {
    let mut args = serde_json::Map::new();
    args.insert("offset".into(), offset.into());
    args.insert("limit".into(), limit.into());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let value = runtime
        .block_on(crate::call_one(root, "find_dead_code", args))
        .unwrap();
    crate::result_text(&value)
}

/// The real MCP `find_duplicate_code` tool, dispatched through the CLI client.
fn mcp_duplicates(root: &Path) -> String {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    let value = runtime
        .block_on(crate::call_one(
            root,
            "find_duplicate_code",
            serde_json::Map::new(),
        ))
        .unwrap();
    crate::result_text(&value)
}

/// Names reported by the CLI dead-code CSV.
fn cli_dead(root: &Path, include_reviewed: bool) -> Vec<String> {
    let index = mct_index::Index::open_existing_read_only(
        root,
        &root.join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    let output = root.join("dead.csv");
    crate::dead_code_report(&index, None, None, Some(output.clone()), include_reviewed).unwrap();
    fs::read_to_string(output)
        .unwrap()
        .lines()
        .skip(1)
        .map(|line| line.split(',').nth(2).unwrap().to_string())
        .collect()
}

#[derive(clap::Parser)]
struct DuplicatesCli {
    #[command(flatten)]
    args: crate::duplicates::Args,
}

/// The CLI duplicates TOON report, through its normal cache handling.
fn cli_duplicates(root: &Path, extra: &[&str]) -> String {
    use clap::Parser as _;
    let output = root.join("duplicates.toon");
    let output_arg = output.to_string_lossy().into_owned();
    let mut argv = vec!["duplicates", "--output", output_arg.as_str()];
    argv.extend_from_slice(extra);
    let cli = DuplicatesCli::try_parse_from(argv).unwrap();
    crate::duplicates::run(root, cli.args).unwrap();
    fs::read_to_string(output).unwrap()
}

fn two_duplicates_project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/a.rs"), format!("pub fn alpha{BODY}")).unwrap();
    fs::write(dir.path().join("src/b.rs"), format!("pub fn beta{BODY}")).unwrap();
    reindex(&root(&dir));
    dir
}

#[test]
fn dashboard_dismissals_reach_mcp_and_cli_before_pagination_and_only_while_unchanged() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let mut site = load(&root);
    let (alpha, beta) = (dead_id(&site, "alpha", 0), dead_id(&site, "beta", 0));
    decide(&mut site, Kind::Dead, &alpha, "false_positive");
    decide(&mut site, Kind::Dead, &beta, "confirmed");

    // Valid dismissal suppressed; confirmed stays visible; totals and the
    // first page are computed after filtering.
    let page = mcp_dead(&root, 0, 1);
    assert!(page.starts_with("1 dead-code candidate(s)"), "{page}");
    assert!(page.contains("beta") && !page.contains("alpha"), "{page}");
    assert!(!mcp_dead(&root, 1, 1).contains("  function"));
    assert_eq!(cli_dead(&root, false), ["beta"]);
    assert_eq!(cli_dead(&root, true), ["alpha", "beta"]);

    // Ignored is suppressed too; restoring to Pending makes it visible again.
    // (`mcp_dead` refreshes the index, so the dashboard reloads its snapshot.)
    let mut site = load(&root);
    decide(&mut site, Kind::Dead, &beta, "ignored");
    assert!(cli_dead(&root, false).is_empty());
    let suppressed = mcp_dead(&root, 0, 10);
    assert!(suppressed.starts_with("No actionable dead-code candidates"));
    assert!(
        suppressed.contains("2 candidate(s) suppressed by valid saved review decisions"),
        "{suppressed}"
    );
    let mut site = load(&root);
    decide(&mut site, Kind::Dead, &beta, "restore");
    assert_eq!(cli_dead(&root, false), ["beta"]);

    // A move keeps the dismissal; a body edit brings the finding back.
    let file = root.join("src/a.rs");
    fs::write(&file, format!("// moved\n\npub fn alpha{BODY}")).unwrap();
    reindex(&root);
    assert_eq!(cli_dead(&root, false), ["beta"]);
    assert!(!mcp_dead(&root, 0, 10).contains("alpha"));
    fs::write(
        &file,
        format!("pub fn alpha{BODY}").replace("total += 8", "total += 9"),
    )
    .unwrap();
    reindex(&root);
    assert_eq!(cli_dead(&root, false), ["alpha", "beta"]);
    assert!(mcp_dead(&root, 0, 10).contains("alpha"));
    let site = load(&root);
    assert!(matches!(
        dead_states(&site, "alpha")[0],
        State::NeedsReevaluation { .. }
    ));
}

#[test]
fn an_existing_duplicate_cache_cannot_hide_a_new_decision_and_pages_follow_the_filter() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    fs::write(root.join("src/c.rs"), format!("pub fn gamma{OTHER}")).unwrap();
    fs::write(root.join("src/d.rs"), format!("pub fn delta{OTHER}")).unwrap();
    reindex(&root);
    // Raw report cached before any dashboard state exists.
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 2"));
    assert!(root.join(".mct-index/duplicates-cache.toon").is_file());
    // An empty dashboard database changes nothing.
    let mut site = load(&root);
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 2"));

    let first = site.duplicate_keys[0].identity.clone();
    let shown = site.duplicates.groups[0].members[0].symbol_name.clone();
    decide(&mut site, Kind::Duplicate, &first, "ignored");
    let filtered = cli_duplicates(&root, &[]);
    assert!(filtered.contains("total_groups: 1"), "{filtered}");
    assert!(!filtered.contains(&shown), "{filtered}");
    let page = cli_duplicates(&root, &["--limit", "1"]);
    assert!(page.contains("total_groups: 1"), "{page}");
    assert!(cli_duplicates(&root, &["--offset", "1", "--limit", "1"]).contains("total_groups: 1"));
    let raw = cli_duplicates(&root, &["--include-reviewed"]);
    assert!(raw.contains("total_groups: 2"), "{raw}");

    // A later restore is honoured immediately, cache or not.
    decide(&mut site, Kind::Duplicate, &first, "restore");
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 2"));
}

#[test]
fn a_deleted_orphan_decision_never_resurrects_for_ai_outputs() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let site = load(&root);
    // An ADR-015 dismissal of the current group, as an old dashboard saved it.
    let legacy = super::reviews::legacy_group_keys(
        &site.duplicates.groups,
        &site.navigation,
        site.duplicates.options.normalization,
    )
    .unwrap()
    .remove(0);
    drop(site);
    fs::write(
        root.join(".mct-index/duplicate-reviews.toon"),
        format!("version: 1\nreviews[1]{{key,reason}}:\n  {legacy},dismissed\n"),
    )
    .unwrap();
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 0"));
    let mut site = load(&root);
    let group = site.duplicate_keys[0].identity.clone();
    let alpha = dead_id(&site, "alpha", 0);
    decide(&mut site, Kind::Duplicate, &group, "confirmed");
    decide(&mut site, Kind::Dead, &alpha, "false_positive");

    // The findings disappear, their orphan decisions are deleted explicitly...
    let file = root.join("src/a.rs");
    let original = fs::read_to_string(&file).unwrap();
    fs::remove_file(&file).unwrap();
    reindex(&root);
    let mut site = load(&root);
    delete_orphan(&mut site, Kind::Duplicate, &group);
    delete_orphan(&mut site, Kind::Dead, &alpha);

    // ...and the restored code is fresh work everywhere.
    fs::write(&file, original).unwrap();
    reindex(&root);
    let site = load(&root);
    assert_eq!(site.duplicate_state(0), State::Pending);
    assert_eq!(dead_states(&site, "alpha"), [State::Pending]);
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 1"));
    assert_eq!(cli_dead(&root, false), ["alpha", "beta"]);
    assert!(mcp_dead(&root, 0, 10).contains("alpha"));
}

const TWINS: &str = "def twin():\n    return 1\n\ndef twin():\n    return 2\n";
const SURVIVOR: &str = "def twin():\n    return 2\n";

fn twins_project() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("twins.py"), TWINS).unwrap();
    let path = root(&dir);
    reindex(&path);
    (dir, path)
}

#[test]
fn a_pre_provenance_decision_on_a_surviving_twin_is_shared_by_dashboard_and_ai() {
    let (_dir, root) = twins_project();
    let site = load(&root);
    let id = dead_id(&site, "twin", 1);
    let fingerprint = site.fingerprint_of(Kind::Dead, &id).unwrap();
    drop(site);
    // Saved without identity provenance, as dashboards before AUD-10 did.
    super::store::Store::open(&root)
        .unwrap()
        .set(
            (Kind::Dead, &id, &fingerprint),
            Status::FalsePositive,
            "entry point",
            1,
        )
        .unwrap();
    fs::write(root.join("twins.py"), SURVIVOR).unwrap();
    reindex(&root);
    // The AI outputs are asked first, before any dashboard load can record
    // provenance: both must resolve the same surviving identity.
    assert!(cli_dead(&root, false).is_empty());
    let suppressed = mcp_dead(&root, 0, 10);
    assert!(suppressed.starts_with("No actionable dead-code candidates"));
    assert!(
        suppressed.contains("1 candidate(s) suppressed by valid saved review decisions"),
        "{suppressed}"
    );
    let site = load(&root);
    assert_eq!(dead_id(&site, "twin", 0), id);
    assert!(decided(&dead_states(&site, "twin")[0]));
    assert!(cli_dead(&root, false).is_empty());
}

#[test]
fn a_dashboard_decision_on_the_second_twin_survives_removal_of_the_first() {
    let (_dir, root) = twins_project();
    let mut site = load(&root);
    let id = dead_id(&site, "twin", 1);
    decide(&mut site, Kind::Dead, &id, "ignored");
    assert_eq!(cli_dead(&root, false), ["twin"]);
    fs::write(root.join("twins.py"), SURVIVOR).unwrap();
    reindex(&root);
    assert!(cli_dead(&root, false).is_empty());
    let site = load(&root);
    assert_eq!(dead_id(&site, "twin", 0), id);
    assert!(decided(&dead_states(&site, "twin")[0]));
    assert!(site.reviews.stored_findings().unwrap().len() == 1);

    // A new sibling inserted first is fresh work and cannot take the identity.
    fs::write(
        root.join("twins.py"),
        format!("def twin():\n    return 0\n\n{SURVIVOR}"),
    )
    .unwrap();
    reindex(&root);
    let site = load(&root);
    assert_eq!(dead_id(&site, "twin", 1), id);
    let states = dead_states(&site, "twin");
    assert_eq!(states[0], State::Pending);
    assert!(decided(&states[1]));
    assert_eq!(cli_dead(&root, false), ["twin"]);

    // Editing the survivor's body asks for review everywhere.
    fs::write(root.join("twins.py"), "def twin():\n    return 3\n").unwrap();
    reindex(&root);
    assert_eq!(cli_dead(&root, false), ["twin"]);
    let site = load(&root);
    assert!(matches!(
        dead_states(&site, "twin")[0],
        State::NeedsReevaluation { .. }
    ));
}

#[test]
fn ambiguous_identical_twins_never_inherit_a_decision() {
    let (_dir, root) = twins_project();
    let mut site = load(&root);
    let id = dead_id(&site, "twin", 1);
    decide(&mut site, Kind::Dead, &id, "false_positive");
    // Both definitions now have the reviewed body: neither is provably it.
    fs::write(root.join("twins.py"), format!("{SURVIVOR}\n{SURVIVOR}")).unwrap();
    reindex(&root);
    assert_eq!(cli_dead(&root, false), ["twin", "twin"]);
    assert!(mcp_dead(&root, 0, 10).starts_with("2 dead-code candidate(s)"));
    let site = load(&root);
    assert!(dead_states(&site, "twin")
        .iter()
        .all(|state| !decided(state)));
}

#[test]
fn a_grown_duplicate_group_is_new_pending_work_and_keeps_the_old_decision_as_orphan() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let mut site = load(&root);
    let old = site.duplicate_keys[0].identity.clone();
    decide(&mut site, Kind::Duplicate, &old, "ignored");
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 0"));

    fs::write(root.join("src/c.rs"), format!("pub fn gamma{BODY}")).unwrap();
    reindex(&root);
    let site = load(&root);
    assert_eq!(site.duplicates.groups.len(), 1);
    assert_eq!(site.duplicates.groups[0].members.len(), 3);
    assert_ne!(site.duplicate_keys[0].identity, old);
    assert_eq!(site.duplicate_state(0), State::Pending);
    assert!(site.fingerprint_of(Kind::Duplicate, &old).is_none());
    assert!(site
        .reviews
        .stored_findings()
        .unwrap()
        .iter()
        .any(|record| record.identity == old && record.status == Status::Ignored));
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 1"));
}

#[test]
fn retained_annotations_keep_ai_outputs_as_uncertain_as_the_dashboard() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("twins.py"), SURVIVOR).unwrap();
    let root = root(&dir);
    reindex(&root);
    let mut site = load(&root);
    let reviewed = dead_id(&site, "twin", 0);
    let fingerprint = site.fingerprint_of(Kind::Dead, &reviewed).unwrap();
    decide(&mut site, Kind::Dead, &reviewed, "false_positive");
    assert!(cli_dead(&root, false).is_empty());
    // Another same-name identity once held this exact content: its decision
    // was deleted, then a new note reopened the retained finding. Either saved
    // identity could now be the current symbol, so neither verdict applies.
    let older = mct_index::review_keys::dead_identity_at(&site.dead[0].entry, 1);
    drop(site);
    let mut store = super::store::Store::open(&root).unwrap();
    store
        .set(
            (Kind::Dead, &older, &fingerprint),
            Status::Confirmed,
            "older copy",
            1,
        )
        .unwrap();
    assert!(store.delete_decision(Kind::Dead, &older).unwrap());
    store
        .add_comment(Kind::Dead, &older, "was this the same function?", 2)
        .unwrap();
    drop(store);
    let site = load(&root);
    assert!(dead_states(&site, "twin")
        .iter()
        .all(|state| !decided(state)));
    assert_eq!(cli_dead(&root, false), ["twin"]);
    assert!(mcp_dead(&root, 0, 10).starts_with("1 dead-code candidate(s)"));
}

#[test]
fn review_once_survives_full_incremental_and_line_movement_without_rewriting_history() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let mut site = load(&root);
    let dead = dead_id(&site, "alpha", 0);
    let group = site.duplicate_keys[0].identity.clone();
    decide(&mut site, Kind::Dead, &dead, "false_positive");
    decide(&mut site, Kind::Duplicate, &group, "false_positive");
    site.reviews
        .add_comment(Kind::Dead, &dead, "public API rationale", 42)
        .unwrap();
    site.reviews
        .add_comment(Kind::Duplicate, &group, "intentional copies", 43)
        .unwrap();
    let decisions = site.reviews.decisions().unwrap();
    let dead_history = site.reviews.history(Kind::Dead, &dead).unwrap();
    let group_history = site.reviews.history(Kind::Duplicate, &group).unwrap();
    drop(site);

    let mut writer = mct_index::Index::open(
        &root,
        &root.join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    let registry = mct_languages::build_registry();
    // Full forced parsing, unchanged full scan, then unchanged path reconciliation.
    for round in 0..3 {
        match round {
            0 => writer.reindex(&registry, true).unwrap(),
            1 => writer.reindex(&registry, false).unwrap(),
            _ => writer
                .reindex_paths(&registry, &[PathBuf::from("src/a.rs")])
                .unwrap(),
        };
        let site = load(&root);
        assert_eq!(dead_id(&site, "alpha", 0), dead);
        assert_eq!(site.duplicate_keys[0].identity, group);
        assert!(decided(&site.dead_state(site.dead_ids[&dead])));
        assert!(decided(&site.duplicate_state(0)));
        assert_eq!(site.reviews.decisions().unwrap(), decisions);
        assert_eq!(
            site.reviews.history(Kind::Dead, &dead).unwrap(),
            dead_history
        );
        assert_eq!(
            site.reviews.history(Kind::Duplicate, &group).unwrap(),
            group_history
        );
        assert_eq!(
            site.reviews.comments(Kind::Dead, &dead).unwrap()[0].body,
            "public API rationale"
        );
        assert_eq!(
            site.reviews.comments(Kind::Duplicate, &group).unwrap()[0].body,
            "intentional copies"
        );
        assert_eq!(cli_dead(&root, false), ["beta"]);
        assert!(!mcp_dead(&root, 0, 20).contains("alpha"));
        assert!(cli_duplicates(&root, &[]).contains("total_groups: 0"));
    }
    drop(writer);
    // Metamorphic property: arbitrary amounts of text outside reviewed ranges
    // do not change either identity or decision validity.
    for padding in [1, 3, 11] {
        fs::write(
            root.join("src/a.rs"),
            format!(
                "{}pub fn alpha{BODY}\n// unrelated footer\n",
                "// moved\n".repeat(padding)
            ),
        )
        .unwrap();
        reindex(&root);
        let site = load(&root);
        assert_eq!(dead_id(&site, "alpha", 0), dead);
        assert_eq!(site.duplicate_keys[0].identity, group);
        assert!(decided(&site.dead_state(site.dead_ids[&dead])));
        assert!(decided(&site.duplicate_state(0)));
        assert_eq!(site.reviews.decisions().unwrap(), decisions);
        assert!(cli_duplicates(&root, &[]).contains("total_groups: 0"));
        assert_eq!(cli_dead(&root, false), ["beta"]);
    }
}

#[test]
fn exact_block_changes_require_review_and_content_reverts_reactivate_only_the_saved_decision() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let mut site = load(&root);
    let dead = dead_id(&site, "alpha", 0);
    let group = site.duplicate_keys[0].identity.clone();
    decide(&mut site, Kind::Dead, &dead, "false_positive");
    decide(&mut site, Kind::Duplicate, &group, "ignored");
    let decisions = site.reviews.decisions().unwrap();
    drop(site);
    let original = format!("pub fn alpha{BODY}");
    // Structural normalization still detects the group; exact reviewed text
    // changed, even if only whitespace or a comment changed.
    for changed in [
        original.replace(" let mut total", "  let mut total"),
        original.replace(
            " let mut total",
            " // reviewed block changed\n let mut total",
        ),
        original.replace("total += 8", "total += 9"),
    ] {
        fs::write(root.join("src/a.rs"), changed).unwrap();
        reindex(&root);
        let site = load(&root);
        assert_eq!(dead_id(&site, "alpha", 0), dead);
        assert_eq!(site.duplicate_keys[0].identity, group);
        assert!(matches!(
            site.dead_state(site.dead_ids[&dead]),
            State::NeedsReevaluation { .. }
        ));
        assert!(matches!(
            site.duplicate_state(0),
            State::NeedsReevaluation { .. }
        ));
        assert_eq!(site.reviews.decisions().unwrap(), decisions);
        assert_eq!(cli_dead(&root, false), ["alpha", "beta"]);
        assert!(mcp_dead(&root, 0, 20).contains("alpha"));
        assert!(cli_duplicates(&root, &[]).contains("total_groups: 1"));
    }
    fs::write(root.join("src/a.rs"), &original).unwrap();
    reindex(&root);
    assert_eq!(cli_dead(&root, false), ["beta"]);
    assert!(cli_duplicates(&root, &[]).contains("total_groups: 0"));
    let site = load(&root);
    assert_eq!(site.reviews.decisions().unwrap(), decisions);
}

#[test]
fn renames_and_disappearance_preserve_orphans_without_transferring_their_decisions() {
    for rename_symbol in [false, true] {
        let dir = two_duplicates_project();
        let root = root(&dir);
        let mut site = load(&root);
        let dead = dead_id(&site, "alpha", 0);
        let group = site.duplicate_keys[0].identity.clone();
        decide(&mut site, Kind::Dead, &dead, "false_positive");
        decide(&mut site, Kind::Duplicate, &group, "false_positive");
        let decisions = site.reviews.decisions().unwrap();
        drop(site);
        let file = root.join("src/a.rs");
        let original = fs::read_to_string(&file).unwrap();
        let renamed = root.join("src/renamed.rs");
        let name = if rename_symbol {
            fs::write(&file, original.replace("alpha", "renamed")).unwrap();
            "renamed"
        } else {
            fs::rename(&file, &renamed).unwrap();
            "alpha"
        };
        reindex(&root);
        let site = load(&root);
        assert_ne!(dead_id(&site, name, 0), dead);
        assert_ne!(site.duplicate_keys[0].identity, group);
        assert_eq!(site.duplicate_state(0), State::Pending);
        assert_eq!(dead_states(&site, name), [State::Pending]);
        assert_eq!(site.reviews.decisions().unwrap(), decisions);
        assert!(cli_dead(&root, false).contains(&name.to_string()));
        assert!(cli_duplicates(&root, &[]).contains("total_groups: 1"));
        assert!(mcp_dead(&root, 0, 20).contains(name));
        let moved_duplicate = mcp_duplicates(&root);
        assert!(
            moved_duplicate.contains("actionable: 1"),
            "{moved_duplicate}"
        );
        assert!(
            moved_duplicate.contains("status: groups"),
            "{moved_duplicate}"
        );
        drop(site);
        if !rename_symbol {
            fs::remove_file(&renamed).unwrap();
        } else {
            fs::remove_file(&file).unwrap();
        }
        reindex(&root);
        let site = load(&root);
        assert!(site.fingerprint_of(Kind::Dead, &dead).is_none());
        assert!(site.fingerprint_of(Kind::Duplicate, &group).is_none());
        assert_eq!(site.reviews.decisions().unwrap(), decisions);
        assert!(!mcp_dead(&root, 0, 20).contains(name));
        assert!(mcp_duplicates(&root).contains("status: no_groups"));
        drop(site);
        fs::write(&file, original).unwrap();
        reindex(&root);
        assert_eq!(cli_dead(&root, false), ["beta"]);
        assert!(cli_duplicates(&root, &[]).contains("total_groups: 0"));
        assert!(!mcp_dead(&root, 0, 20).contains("alpha"));
        assert!(mcp_duplicates(&root).contains("status: all_suppressed"));
        assert_eq!(load(&root).reviews.decisions().unwrap(), decisions);
    }
}

#[test]
fn sqlite_reviews_do_not_follow_directory_moves_and_reactivate_only_on_exact_restoration() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let first_path = root.join("src/a.rs");
    let second_path = root.join("src/b.rs");
    let first_source = fs::read(&first_path).unwrap();
    let second_source = fs::read(&second_path).unwrap();
    let mut site = load(&root);
    let dead = dead_id(&site, "alpha", 0);
    let duplicate = site.duplicate_keys[0].identity.clone();
    decide(&mut site, Kind::Dead, &dead, "false_positive");
    decide(&mut site, Kind::Duplicate, &duplicate, "ignored");
    let decisions = site.reviews.decisions().unwrap();
    let dead_history = site.reviews.history(Kind::Dead, &dead).unwrap();
    let duplicate_history = site.reviews.history(Kind::Duplicate, &duplicate).unwrap();
    drop(site);

    fs::rename(root.join("src"), root.join("moved")).unwrap();
    reindex(&root);
    let moved = load(&root);
    let moved_dead = dead_id(&moved, "alpha", 0);
    let moved_duplicate = moved.duplicate_keys[0].identity.clone();
    assert_ne!(moved_dead, dead);
    assert_ne!(moved_duplicate, duplicate);
    assert_eq!(dead_states(&moved, "alpha"), [State::Pending]);
    assert_eq!(moved.duplicate_state(0), State::Pending);
    assert_eq!(moved.reviews.decisions().unwrap(), decisions);
    assert_eq!(
        moved.reviews.history(Kind::Dead, &dead).unwrap(),
        dead_history
    );
    assert_eq!(
        moved.reviews.history(Kind::Duplicate, &duplicate).unwrap(),
        duplicate_history
    );
    drop(moved);
    assert!(mcp_dead(&root, 0, 20).contains("alpha"));
    let duplicate_result = mcp_duplicates(&root);
    assert!(
        duplicate_result.contains(&moved_duplicate),
        "{duplicate_result}"
    );
    assert!(
        duplicate_result.contains("actionable: 1"),
        "{duplicate_result}"
    );
    assert!(!duplicate_result.contains(&duplicate), "{duplicate_result}");

    fs::remove_dir_all(root.join("moved")).unwrap();
    reindex(&root);
    assert!(!mcp_dead(&root, 0, 20).contains("alpha"));
    assert!(mcp_duplicates(&root).contains("status: no_groups"));
    let deleted = load(&root);
    assert_eq!(deleted.reviews.decisions().unwrap(), decisions);
    assert_eq!(
        deleted.reviews.history(Kind::Dead, &dead).unwrap(),
        dead_history
    );
    assert_eq!(
        deleted
            .reviews
            .history(Kind::Duplicate, &duplicate)
            .unwrap(),
        duplicate_history
    );
    drop(deleted);

    fs::create_dir(root.join("src")).unwrap();
    fs::write(&first_path, first_source).unwrap();
    fs::write(&second_path, second_source).unwrap();
    reindex(&root);
    let restored = load(&root);
    assert_eq!(dead_id(&restored, "alpha", 0), dead);
    assert_eq!(restored.duplicate_keys[0].identity, duplicate);
    assert!(decided(&restored.dead_state(restored.dead_ids[&dead])));
    assert!(decided(&restored.duplicate_state(0)));
    assert_eq!(restored.reviews.decisions().unwrap(), decisions);
    assert_eq!(
        restored.reviews.history(Kind::Dead, &dead).unwrap(),
        dead_history
    );
    assert_eq!(
        restored
            .reviews
            .history(Kind::Duplicate, &duplicate)
            .unwrap(),
        duplicate_history
    );
    drop(restored);
    let dead_result = mcp_dead(&root, 0, 20);
    assert!(!dead_result.contains("alpha"), "{dead_result}");
    assert!(
        dead_result.contains("beta"),
        "unreviewed finding must stay visible: {dead_result}"
    );
    assert!(mcp_duplicates(&root).contains("status: all_suppressed"));
}

#[test]
fn reordered_same_name_symbols_keep_only_the_fingerprint_matched_decision() {
    let (_dir, root) = twins_project();
    let mut site = load(&root);
    let reviewed_id = dead_id(&site, "twin", 1);
    let reviewed_fingerprint = site.fingerprint_of(Kind::Dead, &reviewed_id).unwrap();
    decide(&mut site, Kind::Dead, &reviewed_id, "false_positive");
    let decisions = site.reviews.decisions().unwrap();
    drop(site);

    // Put the reviewed body first so its ordinal changes from one to zero.
    fs::write(
        root.join("twins.py"),
        format!("{SURVIVOR}\ndef twin():\n    return 1\n"),
    )
    .unwrap();
    reindex(&root);
    let reordered = load(&root);
    assert_eq!(dead_id(&reordered, "twin", 0), reviewed_id);
    assert_eq!(
        reordered
            .fingerprint_of(Kind::Dead, &reviewed_id)
            .as_deref(),
        Some(reviewed_fingerprint.as_str())
    );
    let states = dead_states(&reordered, "twin");
    assert!(decided(&states[0]));
    assert_eq!(states[1], State::Pending);
    assert_eq!(reordered.reviews.decisions().unwrap(), decisions);
    drop(reordered);
    let visible = mcp_dead(&root, 0, 10);
    assert!(visible.starts_with("1 dead-code candidate(s)"), "{visible}");
    assert!(
        visible.contains("function,twin,twins.py,4,5"),
        "the unreviewed return-1 twin must remain actionable after reordering: {visible}"
    );

    // Editing that exact body invalidates the same identity's old fingerprint.
    fs::write(
        root.join("twins.py"),
        "def twin():\n    return 3\n\ndef twin():\n    return 1\n",
    )
    .unwrap();
    reindex(&root);
    let changed = load(&root);
    assert!(dead_states(&changed, "twin")
        .iter()
        .all(|state| !decided(state)));
    assert_eq!(changed.reviews.decisions().unwrap(), decisions);
    drop(changed);
    let visible = mcp_dead(&root, 0, 10);
    assert!(visible.starts_with("2 dead-code candidate(s)"), "{visible}");
    assert!(visible.contains("function,twin,twins.py,1,2"), "{visible}");
    assert!(visible.contains("function,twin,twins.py,4,5"), "{visible}");
}

#[test]
fn materially_changed_restoration_keeps_old_reviews_historical_and_actionable() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let mut site = load(&root);
    let dead = dead_id(&site, "alpha", 0);
    let duplicate = site.duplicate_keys[0].identity.clone();
    decide(&mut site, Kind::Dead, &dead, "false_positive");
    decide(&mut site, Kind::Duplicate, &duplicate, "ignored");
    let decisions = site.reviews.decisions().unwrap();
    let original_a = fs::read_to_string(root.join("src/a.rs")).unwrap();
    let original_b = fs::read_to_string(root.join("src/b.rs")).unwrap();
    drop(site);

    fs::remove_file(root.join("src/a.rs")).unwrap();
    fs::remove_file(root.join("src/b.rs")).unwrap();
    reindex(&root);
    assert!(mcp_dead(&root, 0, 10).starts_with("No dead-code candidates"));
    let deleted_duplicates = mcp_duplicates(&root);
    assert!(
        deleted_duplicates.contains("status: no_groups"),
        "{deleted_duplicates}"
    );
    assert_eq!(load(&root).reviews.decisions().unwrap(), decisions);

    fs::write(
        root.join("src/a.rs"),
        original_a.replace("total += 8", "total += 9"),
    )
    .unwrap();
    fs::write(
        root.join("src/b.rs"),
        original_b.replace("total += 8", "total += 9"),
    )
    .unwrap();
    reindex(&root);

    let restored = load(&root);
    assert_eq!(dead_id(&restored, "alpha", 0), dead);
    assert_eq!(restored.duplicate_keys[0].identity, duplicate);
    assert_eq!(restored.reviews.decisions().unwrap(), decisions);
    assert!(matches!(
        restored.dead_state(restored.dead_ids[&dead]),
        State::NeedsReevaluation { .. }
    ));
    assert!(matches!(
        restored.duplicate_state(0),
        State::NeedsReevaluation { .. }
    ));
    let dead_output = mcp_dead(&root, 0, 10);
    assert!(dead_output.contains("alpha"), "{dead_output}");
    assert!(mcp_duplicates(&root).contains("actionable: 1"));
}

/// The same visibility matrix must hold after a real unchanged reindex. A
/// confirmed problem remains actionable; it is not an accepted-copy status.
#[test]
fn every_finding_decision_has_the_same_visibility_after_reindex_on_all_consumers() {
    let dir = two_duplicates_project();
    let root = root(&dir);
    let mut site = load(&root);
    let dead = dead_id(&site, "alpha", 0);
    let group = site.duplicate_keys[0].identity.clone();
    let mut checked = 0;
    for (action, suppressed) in [
        ("false_positive", true),
        ("confirmed", false),
        ("ignored", true),
        ("restore", false),
    ] {
        // MCP initialization can publish refreshed derived metadata. Reload the
        // editor snapshot before each next explicit user action.
        site = load(&root);
        decide(&mut site, Kind::Dead, &dead, action);
        decide(&mut site, Kind::Duplicate, &group, action);
        let stored = site.reviews.decisions().unwrap();
        drop(site);
        reindex(&root);
        site = load(&root);
        assert_eq!(dead_id(&site, "alpha", 0), dead);
        assert_eq!(site.duplicate_keys[0].identity, group);
        assert_eq!(site.reviews.decisions().unwrap(), stored);
        assert_eq!(
            !cli_dead(&root, false).contains(&"alpha".into()),
            suppressed
        );
        assert_eq!(!mcp_dead(&root, 0, 20).contains("alpha"), suppressed);
        assert_eq!(
            cli_duplicates(&root, &[]).contains("total_groups: 0"),
            suppressed
        );
        site = load(&root);
        let visible = !matches!(
            site.dead_state(site.dead_ids[&dead]),
            State::Decided {
                status: Status::FalsePositive | Status::Ignored,
                ..
            }
        );
        assert_eq!(visible, !suppressed);
        checked += 2;
    }
    assert_eq!(checked, 8);
    assert_eq!(site.reviews.history(Kind::Dead, &dead).unwrap().len(), 4);
    assert_eq!(
        site.reviews.history(Kind::Duplicate, &group).unwrap().len(),
        4
    );
}

/// Reproducible Phase 8.1 fixture benchmark. This compares the existing CLI
/// discovery fallback with MCP discovery, not two equivalent MCP workflows.
#[test]
#[ignore = "not a bug: manual timing and token report; run with --ignored --nocapture"]
fn duplicate_mcp_workflow_benchmark() {
    use mct_index::{EstimatedCounter, TokenCounter};
    use std::time::Instant;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    for groups in [1, 10, 50] {
        let dir = tempfile::tempdir().unwrap();
        let root = root(&dir);
        for g in 0..groups {
            let body = BODY.replace(
                "total += 8;",
                &format!("{}total += 8;", "total *= 3;\n".repeat(g)),
            );
            for c in 0..2 {
                fs::write(
                    root.join(format!("g{g}c{c}.rs")),
                    format!("pub fn f{g}_{c}{body}"),
                )
                .unwrap();
            }
        }
        reindex(&root);
        let index = mct_index::Index::open_existing_read_only(
            &root,
            &root.join(".mct-index/index.sqlite3"),
            mct_index::ExcludeSet::default(),
        )
        .unwrap();
        let server = mct_mcp_server::server::MctServer::new(index, mct_languages::build_registry());
        let catalog = server.tool_catalog();
        let old = catalog
            .iter()
            .filter(|t| t.name != "find_duplicate_code")
            .cloned()
            .collect::<Vec<_>>();
        let before_tokens = EstimatedCounter
            .count(&serde_json::to_string(&old).unwrap())
            .value;
        let after_tokens = EstimatedCounter
            .count(&serde_json::to_string(&catalog).unwrap())
            .value;
        let call = || {
            runtime
                .block_on(
                    server.call_read_only_tool(
                        "find_duplicate_code",
                        Some(
                            serde_json::json!({"limit":100,"max_tokens":16000})
                                .as_object()
                                .unwrap()
                                .clone(),
                        ),
                    ),
                )
                .unwrap()
        };
        call(); // warm-up, excluded
        cli_duplicates(&root, &["--no-cache"]); // warm-up, excluded
        let mut mcp_times = Vec::new();
        let mut cli_times = Vec::new();
        let mut mcp_response = String::new();
        let mut cli_response = String::new();
        for _ in 0..11 {
            let start = Instant::now();
            let result = call();
            mcp_times.push(start.elapsed().as_secs_f64() * 1000.0);
            mcp_response = result.content[0].as_text().unwrap().text.clone();
            let start = Instant::now();
            cli_response = cli_duplicates(&root, &["--no-cache"]);
            cli_times.push(start.elapsed().as_secs_f64() * 1000.0);
        }
        fn stats(mut samples: Vec<f64>) -> (f64, f64, f64) {
            samples.sort_by(f64::total_cmp);
            let mean = samples.iter().sum::<f64>() / samples.len() as f64;
            let variance =
                samples.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / samples.len() as f64;
            (
                samples[samples.len() / 2],
                samples[(samples.len() * 95).div_ceil(100) - 1],
                variance,
            )
        }
        let (mp50, mp95, mvar) = stats(mcp_times);
        let (cp50, cp95, cvar) = stats(cli_times);
        let mt = EstimatedCounter.count(&mcp_response).value;
        let ct = EstimatedCounter.count(&cli_response).value;
        let returned = mcp_response
            .lines()
            .find_map(|l| l.strip_prefix("returned: "))
            .unwrap();
        assert!(cli_response.contains(&format!("total_groups: {groups}")));
        assert!(mcp_response.contains(&format!("detected: {groups}")));
        println!(
            "phase81 groups={groups} copies=2 n=11 mcp_returned={returned} mcp_p50_ms={mp50:.3} mcp_p95_ms={mp95:.3} mcp_variance={mvar:.3} cli_p50_ms={cp50:.3} cli_p95_ms={cp95:.3} cli_variance={cvar:.3} response_tokens_mcp={mt} response_tokens_cli={ct} catalog_before={before_tokens} catalog_after={after_tokens} total_mcp={} total_cli_assisted={} calls_mcp=1 calls_cli_assisted=1",
            after_tokens + mt,
            before_tokens + ct
        );
    }
}
