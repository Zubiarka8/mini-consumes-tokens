//! The on-demand partial-block search: started by a POST, read by a GET, kept
//! with the snapshot. Parity with the real `mct-cli duplicates` binary is in
//! `tests/partial_blocks_parity.rs`.
use super::*;
use crate::web::partial_blocks::{search, Search};
use mct_index::PartialBlockBudget;

const BLOCKS_RS: &str = include_str!("../../../tests/fixtures/duplicates/blocks.rs");
const BLOCKS_PY: &str = include_str!("../../../tests/fixtures/duplicates/blocks.py");
/// Labelled negative: the shared run has four token-bearing lines, under the
/// default six, so it must not be reported.
const NEAR_RS: &str = "fn near_left(values: &[u32], limit: u32) -> u32 {
    let mut total = 0;
    for value in values {
        if *value > limit {
            total += *value - limit;
        }
    }
    total.saturating_mul(2)
}

fn near_right(values: &[u32], limit: u32, scale: u32) -> Option<u32> {
    let label = format!(\"scale-{scale}\");
    record(&label);
    let mut total = 0;
    for value in values {
        if *value > limit {
            total += *value - limit;
        }
    }
    total.checked_div(scale)
}
";

fn blocks_project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    for (path, source) in files {
        fs::write(root.path().join(path), source).unwrap();
    }
    index(root.path());
    root
}

impl Server {
    fn start_partial_blocks(&self, body: &str, headers: &str) -> String {
        self.raw(&format!("POST /api/partial-blocks HTTP/1.1\r\nHost: 127.0.0.1:8787\r\n{headers}Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}", body.len()))
    }

    /// Starts the search as the page does and waits for its thread.
    fn searched(&self) {
        let body = serde_json::json!({ "csrf": self.site.borrow().csrf }).to_string();
        let response = self.start_partial_blocks(&body, "Origin: http://127.0.0.1:8787\r\n");
        assert!(response.starts_with("HTTP/1.1 200"), "{response}");
        self.site.borrow().wait_for_partial_blocks();
    }
}

fn occurrences(block: &serde_json::Value) -> Vec<(String, u64, u64, String)> {
    block["occurrences"]
        .as_array()
        .unwrap()
        .iter()
        .map(|occurrence| {
            (
                occurrence["path"].as_str().unwrap().to_string(),
                occurrence["start"].as_u64().unwrap(),
                occurrence["end"].as_u64().unwrap(),
                occurrence["function"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

#[test]
fn the_search_runs_only_on_demand_and_leaves_the_group_report_alone() {
    let root = blocks_project(&[("src/blocks.rs", BLOCKS_RS), ("src/near.rs", NEAR_RS)]);
    let server = Server::start(root.path(), true);
    let groups_before = server.json("api/findings?type=duplicate");
    assert_eq!(groups_before["total"], 1, "{groups_before}");
    assert_eq!(server.json("api/partial-blocks")["status"], "idle");

    server.searched();
    let found = server.json("api/partial-blocks");
    assert_eq!(found["status"], "done", "{found}");
    assert_eq!(found["total"], 1, "{found}");
    let block = &found["items"][0];
    assert_eq!(
        (
            &block["id"],
            &block["language"],
            &block["tokens"],
            &block["lines"]
        ),
        (&1.into(), &"rust".into(), &68.into(), &11.into()),
        "{block}"
    );
    // The shared run only: neither the whole-function copies (their group
    // reports them) nor the four-line run of `near.rs`.
    assert_eq!(
        occurrences(block),
        [
            (
                "src/blocks.rs".to_string(),
                12,
                22,
                "summarize_orders".to_string()
            ),
            (
                "src/blocks.rs".to_string(),
                28,
                38,
                "summarize_refunds".to_string()
            ),
        ]
    );
    assert_eq!(found["budget"]["exhausted"], serde_json::Value::Null);
    assert_eq!(found["budget"]["minTokens"], 40);
    assert_eq!(found["budget"]["minLines"], 6);
    assert_eq!(found["budget"]["functionsSearched"], 6);
    assert_eq!(found["indexCoverage"], "no-recorded-errors");
    assert_eq!(found["skipped"], 0);
    assert!(found["warning"]
        .as_str()
        .unwrap()
        .contains("not a proposal to extract"));
    // Each occurrence opens through the guarded source route.
    let source = server.json("api/source?path=src/blocks.rs&start=12&end=22");
    assert_eq!(source["lines"].as_array().unwrap().len(), 11, "{source}");
    assert_eq!(server.json("api/findings?type=duplicate"), groups_before);
}

#[test]
fn pages_are_cut_from_the_kept_report_without_searching_again() {
    let root = blocks_project(&[("src/blocks.rs", BLOCKS_RS), ("blocks.py", BLOCKS_PY)]);
    let server = Server::start(root.path(), true);
    server.searched();
    let first = server.json("api/partial-blocks?size=1");
    assert_eq!(
        (&first["total"], &first["page"]),
        (&2.into(), &0.into()),
        "{first}"
    );
    assert_eq!(first["items"].as_array().unwrap().len(), 1);
    let second = server.json("api/partial-blocks?size=1&page=1");
    assert_eq!(second["items"][0]["id"], 2, "{second}");
    assert_ne!(first["items"][0], second["items"][0]);
    // A page past the end is the last one, as on every list of the API.
    assert_eq!(server.json("api/partial-blocks?size=1&page=9")["page"], 1);
    // One search answered all of them: the window table was built once.
    assert!(matches!(
        *server.site.borrow().partial_blocks.borrow(),
        Search::Done(_)
    ));
    assert_eq!(first["elapsedMs"], second["elapsedMs"]);
    assert_eq!(first["budget"], second["budget"]);
}

#[test]
fn zero_blocks_is_a_complete_answer_and_not_an_unfinished_one() {
    let root = project();
    let server = Server::start(root.path(), true);
    server.searched();
    let found = server.json("api/partial-blocks");
    assert_eq!(found["status"], "done", "{found}");
    assert_eq!(found["total"], 0);
    assert_eq!(found["items"], serde_json::json!([]));
    assert_eq!(found["budget"]["exhausted"], serde_json::Value::Null);
    assert_eq!(found["indexCoverage"], "no-recorded-errors");
}

#[test]
fn an_exhausted_budget_is_named_and_the_list_is_marked_partial() {
    let root = blocks_project(&[("src/blocks.rs", BLOCKS_RS), ("blocks.py", BLOCKS_PY)]);
    let server = Server::start(root.path(), true);
    for (budget, name, total) in [
        (
            PartialBlockBudget {
                max_blocks: 1,
                ..PartialBlockBudget::default()
            },
            "max-blocks",
            1,
        ),
        (
            PartialBlockBudget {
                max_windows: 1,
                ..PartialBlockBudget::default()
            },
            "max-windows",
            0,
        ),
    ] {
        let finished = search(&root.path().canonicalize().unwrap(), budget).unwrap();
        *server.site.borrow().partial_blocks.borrow_mut() = Search::Done(Box::new(finished));
        let found = server.json("api/partial-blocks");
        assert_eq!(found["status"], "done", "{found}");
        assert_eq!(found["budget"]["exhausted"], name, "{found}");
        assert_eq!(found["total"], total, "{found}");
    }
}

#[test]
fn an_index_with_syntax_errors_is_reported_as_partial_coverage() {
    let root = tempfile::tempdir().unwrap();
    fs::create_dir(root.path().join("src")).unwrap();
    fs::write(root.path().join("src/blocks.rs"), BLOCKS_RS).unwrap();
    fs::write(root.path().join("src/broken.rs"), "fn broken( {\n").unwrap();
    let canonical = root.path().canonicalize().unwrap();
    let mut index = mct_index::Index::open(
        &canonical,
        &canonical.join(".mct-index/index.sqlite3"),
        mct_index::ExcludeSet::default(),
    )
    .unwrap();
    index
        .reindex(&mct_languages::build_registry(), false)
        .unwrap();
    drop(index);
    let server = Server::start(root.path(), true);
    server.searched();
    let found = server.json("api/partial-blocks");
    assert_eq!(found["status"], "done", "{found}");
    assert_eq!(found["indexCoverage"], "partial", "{found}");
    assert_eq!(found["indexSyntaxErrors"], 1, "{found}");
    assert_eq!(
        found["total"], 1,
        "what was indexed is still searched: {found}"
    );
}

#[test]
fn a_republished_index_drops_the_blocks_of_the_previous_snapshot() {
    let root = blocks_project(&[("src/blocks.rs", BLOCKS_RS)]);
    let server = Server::start(root.path(), true);
    server.searched();
    assert_eq!(server.json("api/partial-blocks")["status"], "done");
    // A search that finishes after the index was republished describes files
    // the snapshot on screen does not: its blocks are not shown.
    fs::write(root.path().join("src/more.rs"), NEAR_RS).unwrap();
    index(root.path());
    let finished = search(
        &root.path().canonicalize().unwrap(),
        PartialBlockBudget::default(),
    );
    let (send, result) = std::sync::mpsc::channel();
    send.send(finished).unwrap();
    *server.site.borrow().partial_blocks.borrow_mut() = Search::Running(result);
    assert_eq!(
        server.site.borrow().partial_blocks(&BTreeMap::new())["status"],
        "outdated"
    );
    // The next request reloads the snapshot, which starts without a search.
    assert_eq!(server.json("api/partial-blocks")["status"], "idle");
}

#[test]
fn starting_a_search_needs_the_same_origin_and_the_page_token() {
    let root = project();
    let server = Server::start(root.path(), true);
    let origin = "Origin: http://127.0.0.1:8787\r\n";
    let csrf = server.site.borrow().csrf.clone();
    let valid = serde_json::json!({ "csrf": csrf }).to_string();
    for (body, headers, status) in [
        (valid.as_str(), "", "403"),
        (valid.as_str(), "Origin: http://evil.example\r\n", "403"),
        (r#"{"csrf":"wrong"}"#, origin, "403"),
        ("not json", origin, "400"),
        (&valid.replace('}', r#","budget":1}"#), origin, "400"),
    ] {
        let response = server.start_partial_blocks(body, headers);
        assert!(
            response.starts_with(&format!("HTTP/1.1 {status}")),
            "{body}: {response}"
        );
        assert_eq!(
            server.json("api/partial-blocks")["status"],
            "idle",
            "{body}"
        );
    }
}
