//! CLI reports share the dashboard's effective decision semantics.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use mct_index::{review_keys, DuplicateOptions, ExcludeSet, Index};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
static NEXT: AtomicU64 = AtomicU64::new(0);
const BODY:&str="(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";
struct Project(PathBuf);
impl Project {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "mct-cli-review-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        let root = root.canonicalize().unwrap();
        fs::write(root.join("src/a.rs"), format!("pub fn alpha{BODY}")).unwrap();
        fs::write(root.join("src/b.rs"), format!("pub fn beta{BODY}")).unwrap();
        let project = Self(root);
        project.run(&["init", "--no-obsidian"]);
        project
    }
    fn run(&self, args: &[&str]) -> String {
        let output = Command::new(env!("CARGO_BIN_EXE_mct-cli"))
            .arg("--root")
            .arg(&self.0)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap()
    }
    fn index(&self) -> Index {
        Index::open_existing_read_only(
            &self.0,
            &self.0.join(".mct-index/index.sqlite3"),
            ExcludeSet::default(),
        )
        .unwrap()
    }
}
impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
#[test]
fn duplicate_cache_cannot_bypass_new_dismissals_and_raw_mode_remains_available() {
    let project = Project::new();
    assert!(project.run(&["duplicates"]).contains("total_groups: 1"));
    let index = project.index();
    let navigation = index.navigation_snapshot().unwrap();
    let report = mct_index::find_duplicate_groups(
        &index,
        &mct_languages::build_token_sources(),
        DuplicateOptions::default(),
    )
    .unwrap();
    let identity = review_keys::group_identities(&report.groups).remove(0);
    let fingerprint = review_keys::Fingerprints::new(&project.0, &navigation)
        .group_keys(&report.groups, report.options.normalization)
        .remove(0);
    drop(index);
    let file = project.0.join(".mct-index/reviews.toon");
    fs::write(&file,format!("version: 2\nreviews[1]{{kind,identity,fingerprint,status,at,reason}}:\n  duplicate,{identity},{fingerprint},ignored,1,keep both\nhistory[0]{{kind,identity,fingerprint,status,at,reason}}:\n")).unwrap();
    assert!(project.run(&["duplicates"]).contains("total_groups: 0"));
    let reviews = mct_index::review_state::ReviewSnapshot::load(&project.0).unwrap();
    assert_eq!(
        reviews
            .duplicate_visibility(
                &project.0,
                &navigation,
                &[report.groups[0].clone(), report.groups[0].clone()],
                report.options.normalization
            )
            .unwrap(),
        vec![true, true],
        "ambiguous duplicate identities stay visible"
    );
    fs::write(
        project.0.join("src/a.rs"),
        "// source no longer matches its index\n",
    )
    .unwrap();
    assert_eq!(
        reviews
            .duplicate_visibility(
                &project.0,
                &navigation,
                &report.groups,
                report.options.normalization
            )
            .unwrap(),
        vec![true],
        "empty fingerprints from unverified source never suppress a group"
    );
    fs::write(project.0.join("src/a.rs"), format!("pub fn alpha{BODY}")).unwrap();

    assert!(project
        .run(&["duplicates", "--include-reviewed"])
        .contains("total_groups: 1"));
    assert!(project
        .run(&["duplicates", "--offset", "1", "--limit", "1"])
        .contains("total_groups: 0"));
    fs::write(
        project.0.join("src/a.rs"),
        format!("pub fn alpha{BODY}").replace("total += 8", "total += 9"),
    )
    .unwrap();
    project.run(&["reindex", "--no-obsidian"]);
    assert!(project.run(&["duplicates"]).contains("total_groups: 1"));
}
#[test]
fn dead_code_csv_excludes_only_valid_dismissals_unless_raw_is_requested() {
    let project = Project::new();
    let index = project.index();
    let navigation = index.navigation_snapshot().unwrap();
    let candidates = mct_index::find_dead_code_candidates(&index, None, None).unwrap();
    let keys =
        review_keys::Fingerprints::new(&project.0, &navigation).dead_keys(&candidates, &navigation);
    let alpha = candidates.iter().position(|c| c.name == "alpha").unwrap();
    let (identity, fingerprint) = &keys[alpha];
    fs::write(project.0.join(".mct-index/reviews.toon"),format!("version: 2\nreviews[1]{{kind,identity,fingerprint,status,at,reason}}:\n  dead,{identity},{fingerprint},false_positive,1,public API\nhistory[0]{{kind,identity,fingerprint,status,at,reason}}:\n")).unwrap();
    drop(index);
    project.run(&["dead-code"]);
    let csv = project.0.join(".mct-index/dead-code-report.csv");
    let contents = fs::read_to_string(&csv).unwrap();
    assert!(!contents.contains(",alpha,"));
    assert!(contents.contains(",beta,"));
    project.run(&["dead-code", "--include-reviewed"]);
    assert!(fs::read_to_string(csv).unwrap().contains(",alpha,"));
}

#[test]
fn sqlite_only_changes_invalidate_dismissals_without_reindexing_or_toon_writes() {
    let project = Project::new();
    assert!(project.run(&["duplicates"]).contains("total_groups: 1"));
    let index = project.index();
    let navigation = index.navigation_snapshot().unwrap();
    let report = mct_index::find_duplicate_groups(
        &index,
        &mct_languages::build_token_sources(),
        DuplicateOptions::default(),
    )
    .unwrap();
    let id = review_keys::group_identities(&report.groups).remove(0);
    let fp = review_keys::Fingerprints::new(&project.0, &navigation)
        .group_keys(&report.groups, report.options.normalization)
        .remove(0);
    drop(index);
    let db = rusqlite::Connection::open(project.0.join(".mct-index/dashboard.sqlite3")).unwrap();
    // The reader's required schema subset; no legacy file or index change.
    db.execute_batch(
        "CREATE TABLE meta(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE reviews(kind TEXT,identity TEXT,fingerprint TEXT,status TEXT);",
    )
    .unwrap();
    db.execute(
        "INSERT INTO reviews VALUES ('duplicate',?1,?2,'false_positive')",
        rusqlite::params![id, fp],
    )
    .unwrap();
    assert!(project.run(&["duplicates"]).contains("total_groups: 0"));
    assert!(project
        .run(&["duplicates", "--include-reviewed"])
        .contains("total_groups: 1"));
    db.execute("UPDATE reviews SET status='pending'", [])
        .unwrap();
    assert!(project.run(&["duplicates"]).contains("total_groups: 1"));
    db.execute(
        "UPDATE reviews SET status='ignored',fingerprint=?1",
        ["0".repeat(64)],
    )
    .unwrap();
    assert!(project.run(&["duplicates"]).contains("total_groups: 1"));
    assert!(!project.0.join(".mct-index/reviews.toon").exists());
}
