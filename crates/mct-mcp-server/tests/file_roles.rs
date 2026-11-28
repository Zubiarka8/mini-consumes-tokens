//! File roles (issue #634): a production file's name-only relation never
//! resolves into a fixture, an explicit one still does, and `status` tells
//! a fixture's parse failure from an unexpected one.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/server.rs"]
mod server_fixture;
use server_fixture::build_server_at as build_server;

#[path = "common/all_content.rs"]
mod all_content;
use all_content::content_of;

use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

async fn call(server: &MctServer, tool: &str, args: Value) -> String {
    let Value::Object(map) = args else {
        panic!("arguments must be an object")
    };
    let result = server
        .call_read_only_tool(tool, Some(map))
        .await
        .unwrap_or_else(|err| panic!("`{tool}` failed: {}", err.message));
    content_of(&result)
}

fn write_fixture(files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("mct-file-roles-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, contents) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    root
}

/// The `find_calls` row of `from` calling `to`, as TOON.
fn call_row(out: &str, to: &str) -> String {
    out.lines()
        .find(|line| line.contains(&format!(",{to},")))
        .unwrap_or_else(|| panic!("no call to `{to}` in:\n{out}"))
        .to_string()
}

#[tokio::test]
async fn a_production_call_by_name_never_resolves_into_a_fixture() {
    let root = write_fixture(&[
        ("src/parse.rs", "pub fn parse_input() {}\n"),
        ("src/main.rs", "fn main() {\n    parse_input();\n}\n"),
        (
            "tests/fixtures/sample.rs",
            "pub fn parse_input() {}\n",
        ),
        (
            "tests/parse.rs",
            "#[test]\nfn parses() {\n    parse_input();\n}\n",
        ),
        (
            "web/app.js",
            "import { load } from \"./fixtures/data.js\";\nexport function start() {\n  load();\n}\n",
        ),
        ("web/fixtures/data.js", "export function load() {}\n"),
        ("src/broken.rs", "fn broken( {\n"),
        ("tests/fixtures/malformed.rs", "fn malformed( {\n"),
    ]);
    let server = build_server(&root).await;

    // Production by name: only the production definition is a candidate.
    let main = call(
        &server,
        "find_calls",
        json!({ "function": "main", "format": "toon" }),
    )
    .await;
    assert!(
        call_row(&main, "parse_input").ends_with(",resolved,name,1,src/parse.rs:1"),
        "{main}"
    );

    // A test may still reach the fixture: both stay candidates.
    let test = call(
        &server,
        "find_calls",
        json!({ "function": "parses", "format": "toon" }),
    )
    .await;
    assert!(
        call_row(&test, "parse_input")
            .ends_with(",ambiguous,candidates,2,\"src/parse.rs:1, tests/fixtures/sample.rs:1\""),
        "{test}"
    );

    // An import that names the fixture's path still resolves into it.
    let start = call(
        &server,
        "find_calls",
        json!({ "function": "start", "format": "toon" }),
    )
    .await;
    assert!(
        call_row(&start, "load").ends_with(",resolved,declared,1,web/fixtures/data.js:1"),
        "{start}"
    );

    let status = call(&server, "get_indexing_status", json!({})).await;
    // A file that failed to parse has no row, so no role count.
    assert!(
        status.contains("Files by role: production 3, fixture 2, test 1\n"),
        "{status}"
    );
    assert!(
        status.contains("1 file(s) failed to parse:\n  src/broken.rs: "),
        "{status}"
    );
    assert!(
        status.contains(
            "1 fixture file(s) failed to parse, likely on purpose:\n  tests/fixtures/malformed.rs: "
        ),
        "{status}"
    );
}
