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
    assert!(mcp_dead(&root, 0, 10).starts_with("No dead-code candidates"));
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
    assert!(mcp_dead(&root, 0, 10).starts_with("No dead-code candidates"));
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
