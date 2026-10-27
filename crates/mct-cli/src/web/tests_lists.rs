//! Read-side contract of the JSON API the SPA consumes: list paging and
//! snapshots, finding detail, bounded source chunks, and the exact field names
//! `web/spa/spa.js` reads or sends. Drives the real handlers without sockets.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::i18n::Lang;
use super::reviews::{Kind, Status};
use super::{api, render, store, Site, REFRESH_INTERVAL};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

struct Api {
    site: Site,
}

impl Api {
    fn start(root: &Path) -> Self {
        Self {
            site: Site::load(&root.canonicalize().unwrap(), true).unwrap(),
        }
    }

    /// A GET exactly as `http::serve_connection` dispatches it.
    fn get_status(&mut self, target: &str) -> (u16, Value) {
        let route = target.split('?').next().unwrap_or_default();
        if api::wants_fresh_snapshot(route) {
            self.site.refresh_if_stale();
        }
        let response = render::route(&self.site, target, Lang::En);
        let body = if response.status == 200 {
            serde_json::from_str(&response.body).unwrap()
        } else {
            Value::Null
        };
        (response.status, body)
    }

    fn get(&mut self, target: &str) -> Value {
        let (status, body) = self.get_status(target);
        assert_eq!(status, 200, "{target}");
        body
    }

    fn post(&mut self, route: &str, body: &Value) -> (u16, Value) {
        let response = api::write(&mut self.site, route, &body.to_string());
        let body = if response.status == 200 {
            serde_json::from_str(&response.body).unwrap()
        } else {
            Value::Null
        };
        (response.status, body)
    }

    /// Swaps in a snapshot of the reindexed project through the real
    /// background refresh, without waiting for the polling interval.
    fn refresh_after_reindex(&mut self) {
        let before = self.site.generated_token();
        self.site.last_check = Instant::now()
            .checked_sub(REFRESH_INTERVAL * 2)
            .unwrap_or_else(Instant::now);
        self.site.refresh_if_stale();
        for _ in 0..600 {
            if self.site.rebuilding.is_none() && self.site.generated_token() != before {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
            self.site.refresh_if_stale();
        }
        panic!("the reindexed snapshot was never swapped in");
    }
}

impl Site {
    fn generated_token(&self) -> (Option<std::time::SystemTime>, u64) {
        (self.index_stamp, self.generated)
    }
}

fn index(root: &Path) {
    let root = root.canonicalize().unwrap();
    let mut index = mct_index::Index::open(
        &root,
        &root.join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    let report = index
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
}

const BODY: &str = "(value: i64) -> i64 {\n let mut total = value;\n total += 1;\n total *= 2;\n total -= 4;\n if total > 12 { total -= 3; }\n total += 8;\n total\n}\n";

fn duplicate_project() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/a.rs"), format!("pub fn alpha{BODY}")).unwrap();
    fs::write(root.path().join("src/b.rs"), format!("pub fn beta{BODY}")).unwrap();
    index(root.path());
    root
}

fn keys(value: &Value) -> BTreeSet<&str> {
    value
        .as_object()
        .unwrap_or_else(|| panic!("not an object: {value}"))
        .keys()
        .map(String::as_str)
        .collect()
}

/// Asserts `value` has every field the SPA reads.
fn has_fields(value: &Value, wanted: &[&str], what: &str) {
    let present = keys(value);
    let missing: Vec<_> = wanted
        .iter()
        .filter(|key| !present.contains(*key))
        .collect();
    assert!(missing.is_empty(), "{what} lacks {missing:?}: {value}");
}

const LIST_FIELDS: [&str; 6] = ["total", "offset", "limit", "snapshot", "counts", "items"];
const STATUSES: [&str; 6] = [
    "all",
    "pending",
    "confirmed",
    "false_positive",
    "ignored",
    "needs_reevaluation",
];

fn dead_identity(api: &Api, name: &str) -> (String, String) {
    let dead = api
        .site
        .dead
        .iter()
        .find(|dead| dead.entry.name == name)
        .unwrap();
    (dead.identity.clone(), dead.fingerprint.clone())
}

#[test]
fn list_and_detail_responses_carry_every_field_the_spa_reads() {
    let root = duplicate_project();
    let mut api = Api::start(root.path());

    let list = api.get("api/duplicates?offset=0&limit=50&q=&status=all");
    has_fields(&list, &LIST_FIELDS, "duplicates list");
    assert_eq!(
        keys(&list["counts"]),
        STATUSES.into_iter().collect::<BTreeSet<_>>()
    );
    let group = &list["items"][0];
    has_fields(
        group,
        &[
            "identity",
            "fingerprint",
            "revision",
            "language",
            "line_count",
            "token_count",
            "members",
            "state",
        ],
        "duplicate item",
    );
    has_fields(&group["state"], &["status"], "state");
    for member in group["members"].as_array().unwrap() {
        has_fields(
            member,
            &[
                "path",
                "symbol",
                "start_line",
                "end_line",
                "line_count",
                "excerpt",
                "excerpt_first",
                "excerpt_available",
                "excerpt_truncated",
                "source_next_offset",
            ],
            "duplicate member",
        );
        // A list preview is never authoritative source: it says so.
        assert_eq!(member["excerpt_truncated"], true);
    }
    let id = group["identity"].as_str().unwrap().to_string();
    let fingerprint = group["fingerprint"].as_str().unwrap().to_string();
    let snapshot = list["snapshot"].as_str().unwrap().to_string();

    // The editor's source read, exactly as `loadFullCode` builds it.
    let full = api.get(&format!(
        "api/duplicates?id={id}&full=1&source_offset=0&source_limit=400&snapshot={snapshot}"
    ));
    for member in full["items"][0]["members"].as_array().unwrap() {
        has_fields(
            member,
            &["source_offset", "source_total", "excerpt_first"],
            "source chunk",
        );
        assert_eq!(member["excerpt_available"], true);
        assert_eq!(member["excerpt_truncated"], false, "short code is complete");
        assert!(member["source_next_offset"].is_null());
        assert_eq!(member["excerpt_first"], member["start_line"]);
    }

    // A finding decision, with the body `saveReview` sends.
    let (status, saved) = api.post(
        "api/review",
        &json!({"kind":"duplicate","id":id,"action":"ignored","reason":"separate lifecycles",
            "fingerprint":fingerprint,"expected_revision":0}),
    );
    assert_eq!(status, 200);
    assert_eq!(saved["state"]["status"], "ignored");
    assert_eq!(saved["state"]["reason"], "separate lifecycles");
    assert_eq!(saved["revision"], 1);

    // A per-copy working label, with the body `saveFile` sends.
    let path = group["members"][0]["path"].as_str().unwrap().to_string();
    let (status, label) = api.post(
        "api/file-review",
        &json!({"id":id,"path":path,"status":"rejected","note":"older copy","expected_revision":0}),
    );
    assert_eq!(status, 200);
    assert_eq!(label["revision"], 1);
    // Annotations move the finding revision too; the SPA uses this to keep a
    // pending decision draft current after its own label save.
    assert_eq!(label["finding_revision"], 2);
    let (status, _) = api.post(
        "api/comment",
        &json!({"kind":"duplicate","id":id,"body":"annotation"}),
    );
    assert_eq!(status, 200);

    let detail = api.get(&format!("api/finding?kind=duplicate&id={id}"));
    has_fields(
        &detail,
        &[
            "kind",
            "identity",
            "current",
            "fingerprint",
            "state",
            "revision",
            "files",
            "history",
            "comments",
        ],
        "finding",
    );
    assert_eq!(detail["current"], true);
    assert_eq!(detail["fingerprint"], fingerprint.as_str());
    assert_eq!(detail["revision"], 3, "decision, label and comment");
    assert_eq!(detail["state"]["reason"], "separate lifecycles");
    has_fields(
        &detail["files"][0],
        &["path", "status", "note", "revision"],
        "file review",
    );
    assert_eq!(detail["files"][0]["revision"], 1);
    has_fields(
        &detail["history"][0],
        &["at", "status", "reason"],
        "history",
    );
    has_fields(&detail["comments"][0], &["body", "updated_at"], "comment");

    let dead = api.get("api/dead?offset=0&limit=50&q=&status=all");
    has_fields(&dead, &LIST_FIELDS, "dead list");
    has_fields(
        &dead["items"][0],
        &[
            "identity",
            "fingerprint",
            "revision",
            "name",
            "kind",
            "language",
            "path",
            "line",
            "end_line",
            "parent",
            "state",
        ],
        "dead item",
    );

    // Restore to pending with an empty reason, as the "restore" choice sends.
    let (status, restored) = api.post(
        "api/review",
        &json!({"kind":"duplicate","id":id,"action":"restore","reason":"",
            "fingerprint":fingerprint,"expected_revision":3}),
    );
    assert_eq!(status, 200);
    assert_eq!(restored["state"]["status"], "pending");
    assert_eq!(restored["revision"], 4);

    let reviews = api.get("api/reviews?offset=0&limit=50&q=&status=all");
    has_fields(&reviews, &LIST_FIELDS, "reviews list");
    has_fields(
        &reviews["items"][0],
        &[
            "kind", "identity", "status", "at", "reason", "revision", "current", "state",
        ],
        "stored finding",
    );
}

// AUD-04: a per-copy "rejected" label is a working label only.
#[test]
fn a_rejected_copy_label_never_dismisses_the_duplicate_group() {
    let root = duplicate_project();
    let mut api = Api::start(root.path());
    let group = api.get("api/duplicates")["items"][0].clone();
    let id = group["identity"].as_str().unwrap();
    for member in group["members"].as_array().unwrap() {
        let (status, _) = api.post(
            "api/file-review",
            &json!({"id":id,"path":member["path"],"status":"rejected","note":"","expected_revision":0}),
        );
        assert_eq!(status, 200);
    }
    let list = api.get("api/duplicates");
    assert_eq!(list["items"][0]["state"]["status"], "pending");
    assert_eq!(list["counts"]["pending"], list["counts"]["all"]);
    let detail = api.get(&format!("api/finding?kind=duplicate&id={id}"));
    assert_eq!(detail["state"]["status"], "pending");
    assert!(
        detail["history"].as_array().unwrap().is_empty(),
        "no finding decision was written"
    );
    // Only an explicit finding decision dismisses the group.
    let (status, _) = api.post(
        "api/review",
        &json!({"kind":"duplicate","id":id,"action":"false_positive","reason":"",
            "fingerprint":group["fingerprint"],"expected_revision":detail["revision"]}),
    );
    assert_eq!(status, 200);
    let dismissed = api.get("api/duplicates?status=false_positive");
    assert_eq!(dismissed["total"], 1);
}

// AUD-08: a reindex between two pages must not mix result generations.
#[test]
fn a_reindex_between_pages_is_refused_and_a_restart_reads_one_generation() {
    let root = tempfile::tempdir().unwrap();
    let bulk = root.path().join("bulk.rs");
    let text: String = (0..330)
        .map(|n| format!("pub fn item_{n:03}() {{}}\n"))
        .collect();
    fs::write(&bulk, &text).unwrap();
    index(root.path());
    let mut api = Api::start(root.path());

    let first = api.get("api/dead?offset=0&limit=50");
    assert_eq!(first["total"], 330);
    let old = first["snapshot"].as_str().unwrap().to_string();

    // Twenty new candidates sort before every old one, shifting all offsets.
    let fresh: String = (0..20)
        .map(|n| format!("pub fn aaa_{n:02}() {{}}\n"))
        .collect();
    fs::write(&bulk, format!("{fresh}{text}")).unwrap();
    index(root.path());
    api.refresh_after_reindex();

    let (status, _) = api.get_status(&format!("api/dead?offset=50&limit=50&snapshot={old}"));
    assert_eq!(status, 409, "the old offset belongs to another generation");

    // The SPA's restart: a blank snapshot, then pages pinned to the new one.
    let restart = api.get("api/dead?offset=0&limit=200&snapshot=");
    let pinned = restart["snapshot"].as_str().unwrap().to_string();
    assert_ne!(pinned, old);
    assert_eq!(restart["total"], 350);
    let rest = api.get(&format!("api/dead?offset=200&limit=200&snapshot={pinned}"));
    assert_eq!(rest["snapshot"], pinned.as_str());
    let names: Vec<String> = [&restart, &rest]
        .iter()
        .flat_map(|page| page["items"].as_array().unwrap().clone())
        .map(|item| item["name"].as_str().unwrap().to_string())
        .collect();
    let unique: BTreeSet<_> = names.iter().collect();
    assert_eq!(names.len(), 350);
    assert_eq!(unique.len(), 350, "no candidate repeated across pages");
    assert!((0..20).all(|n| unique.contains(&format!("aaa_{n:02}"))));
}

// A review saved meanwhile (another tab, the CLI) changes list membership and
// counts, so lists restart; it cannot change indexed source, so a pinned
// source read stays valid until the analysis itself changes.
#[test]
fn review_writes_restart_lists_but_not_a_pinned_source_read() {
    let root = duplicate_project();
    let mut api = Api::start(root.path());
    let list = api.get("api/duplicates");
    let token = list["snapshot"].as_str().unwrap().to_string();
    let id = list["items"][0]["identity"].as_str().unwrap().to_string();
    let (dead, fingerprint) = dead_identity(&api, "alpha");
    store::Store::open(&api.site.root)
        .unwrap()
        .set(
            (Kind::Dead, &dead, &fingerprint),
            Status::Ignored,
            "from another tab",
            1,
        )
        .unwrap();

    let (status, _) = api.get_status(&format!("api/duplicates?offset=0&snapshot={token}"));
    assert_eq!(status, 409);
    let (status, _) = api.get_status(&format!("api/dead?offset=0&snapshot={token}"));
    assert_eq!(status, 409);
    let source = api.get(&format!(
        "api/duplicates?id={id}&full=1&source_offset=0&source_limit=400&snapshot={token}"
    ));
    assert_eq!(source["items"][0]["identity"], id.as_str());

    fs::write(
        root.path().join("src/a.rs"),
        format!("pub fn alpha{BODY}\npub fn gamma() {{}}\n"),
    )
    .unwrap();
    index(root.path());
    api.refresh_after_reindex();
    let (status, _) = api.get_status(&format!(
        "api/duplicates?id={id}&full=1&source_offset=0&source_limit=400&snapshot={token}"
    ));
    assert_eq!(status, 409, "new analysis, possibly new source");
}

// AUD-09: filtering and paging happen on the server and every list is bounded.
#[test]
fn every_list_is_filtered_and_bounded_on_the_server() {
    let root = duplicate_project();
    let mut api = Api::start(root.path());
    {
        let mut reviews = store::Store::open(&api.site.root).unwrap();
        for n in 0..250_u32 {
            let identity = format!("{n:064x}");
            reviews
                .set(
                    (Kind::Dead, &identity, &"f".repeat(64)),
                    Status::Ignored,
                    "",
                    u64::from(n),
                )
                .unwrap();
        }
    }
    let orphans = api.get("api/reviews?status=orphan&limit=1000");
    assert_eq!(orphans["total"], 250);
    assert_eq!(orphans["limit"], 200);
    assert_eq!(orphans["items"].as_array().unwrap().len(), 200);
    let token = orphans["snapshot"].as_str().unwrap();
    let tail = api.get(&format!(
        "api/reviews?status=orphan&offset=200&limit=200&snapshot={token}"
    ));
    assert_eq!(tail["items"].as_array().unwrap().len(), 50);
    assert!(tail["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|item| item["current"] == false));
    assert_eq!(api.get("api/summary")["orphan_findings"], 250);

    let one = api.get("api/dead?limit=0");
    assert_eq!(one["limit"], 1);
    assert_eq!(one["items"].as_array().unwrap().len(), 1);
    let searched = api.get("api/dead?q=BETA");
    assert_eq!(searched["total"], 1);
    assert_eq!(searched["items"][0]["name"], "beta");
    assert_eq!(searched["counts"]["all"], 1, "counts follow the search");
    let none = api.get("api/duplicates?q=no-such-path");
    assert_eq!(none["total"], 0);
    for bad in [
        "api/dead?offset=-1",
        "api/dead?limit=many",
        "api/reviews?kind=other",
        "api/reviews?status=unknown",
        "api/duplicates?status=orphan",
    ] {
        assert_eq!(api.get_status(bad).0, 400, "{bad}");
    }
}

// AUD-13: a duplicate longer than one chunk can be read to its last line,
// each chunk says where it sits, and no line is skipped or repeated.
#[test]
fn full_duplicate_source_pages_cover_every_line_exactly_once() {
    let root = tempfile::tempdir().unwrap();
    let lines = 905;
    for name in ["alpha", "beta"] {
        let body: String = (0..lines)
            .map(|n| format!("    total += {n}; // {name}\n"))
            .collect();
        fs::write(
            root.path().join(format!("{name}.rs")),
            format!("pub fn {name}(mut total: i64) -> i64 {{\n{body}    total\n}}\n"),
        )
        .unwrap();
    }
    index(root.path());
    let mut api = Api::start(root.path());
    let list = api.get("api/duplicates");
    let token = list["snapshot"].as_str().unwrap().to_string();
    let group = list["items"]
        .as_array()
        .unwrap()
        .iter()
        .max_by_key(|group| group["line_count"].as_u64())
        .unwrap()
        .clone();
    let id = group["identity"].as_str().unwrap();
    let member = &group["members"][0];
    let (start, end) = (
        member["start_line"].as_u64().unwrap(),
        member["end_line"].as_u64().unwrap(),
    );
    let total = end - start + 1;
    assert!(total > 800, "the fixture spans three chunks: {total}");
    let path = member["path"].as_str().unwrap();
    let text = fs::read_to_string(root.path().join(path)).unwrap();
    let expected: Vec<&str> = text
        .lines()
        .skip(usize::try_from(start - 1).unwrap())
        .take(usize::try_from(total).unwrap())
        .collect();

    let mut seen: Vec<String> = Vec::new();
    let mut offset = Some(0_u64);
    let mut chunks = 0;
    while let Some(at) = offset {
        let page = api.get(&format!(
            "api/duplicates?id={id}&full=1&source_offset={at}&source_limit=400&snapshot={token}"
        ));
        let chunk = page["items"][0]["members"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["path"] == path)
            .unwrap()
            .clone();
        assert_eq!(chunk["source_offset"], at);
        assert_eq!(chunk["source_total"], total);
        assert_eq!(chunk["excerpt_first"], start + at);
        assert_eq!(chunk["excerpt_available"], true);
        assert_eq!(
            chunk["excerpt_truncated"], true,
            "every chunk of a longer range is partial"
        );
        let returned = chunk["excerpt"].as_array().unwrap();
        assert!(returned.len() <= 400);
        seen.extend(returned.iter().map(|l| l.as_str().unwrap().to_string()));
        offset = chunk["source_next_offset"].as_u64();
        if let Some(next) = offset {
            assert_eq!(next, at + returned.len() as u64);
        }
        chunks += 1;
        assert!(chunks <= 3);
    }
    assert_eq!(chunks, 3);
    assert_eq!(seen, expected);

    // An oversized chunk request is clamped; past the end is empty, not an error.
    let clamped = api.get(&format!(
        "api/duplicates?id={id}&full=1&source_offset=0&source_limit=5000"
    ));
    assert_eq!(
        clamped["items"][0]["members"][0]["excerpt"]
            .as_array()
            .unwrap()
            .len(),
        400
    );
    let past = api.get(&format!(
        "api/duplicates?id={id}&full=1&source_offset={total}"
    ));
    let past = &past["items"][0]["members"][0];
    assert!(past["excerpt"].as_array().unwrap().is_empty());
    assert_eq!(past["excerpt_available"], true);
    assert!(past["source_next_offset"].is_null());
    for bad in ["source_offset=-1", "source_limit=x"] {
        assert_eq!(
            api.get_status(&format!("api/duplicates?id={id}&full=1&{bad}"))
                .0,
            400
        );
    }
}

#[test]
fn full_duplicate_source_carries_the_lines_around_each_copy() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    let header: String = (1..=9).map(|n| format!("// header {n}\n")).collect();
    let a = format!("{header}pub fn alpha{BODY}// after 1\n// after 2\n");
    fs::write(root.path().join("src/a.rs"), &a).unwrap();
    fs::write(root.path().join("src/b.rs"), format!("pub fn beta{BODY}")).unwrap();
    index(root.path());
    let mut api = Api::start(root.path());
    let list = api.get("api/duplicates");
    let id = list["items"][0]["identity"].as_str().unwrap().to_string();

    // A list preview reads no source around the copies.
    for member in list["items"][0]["members"].as_array().unwrap() {
        assert!(member["context_before"].as_array().unwrap().is_empty());
        assert!(member["file_lines"].is_null());
    }

    let full = api.get(&format!("api/duplicates?id={id}&full=1"));
    let members = full["items"][0]["members"].as_array().unwrap();
    let alpha = members.iter().find(|m| m["path"] == "src/a.rs").unwrap();
    let beta = members.iter().find(|m| m["path"] == "src/b.rs").unwrap();
    let lines: Vec<&str> = a.lines().collect();
    let start = alpha["start_line"].as_u64().unwrap() as usize;
    let end = alpha["end_line"].as_u64().unwrap() as usize;
    let context = super::source::DUPLICATE_CONTEXT as usize;
    assert_eq!(alpha["file_lines"], lines.len());
    assert_eq!(
        alpha["context_before"],
        json!(lines[start - 1 - context..start - 1])
    );
    // Clamped to the file: fewer lines below than the context asks for.
    assert_eq!(alpha["context_after"], json!(lines[end..]));
    assert_eq!(beta["start_line"], 1);
    assert!(beta["context_before"].as_array().unwrap().is_empty());
}
