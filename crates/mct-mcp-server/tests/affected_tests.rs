//! Affected tests carry why each one is a test (issue #630): a declared
//! test attribute first, then the path convention, and a test reached
//! through an ambiguous relation is only a possible one.

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

/// The "Likely affected tests" section alone.
fn tests_section(out: &str) -> &str {
    let start = out
        .find("Likely affected tests")
        .unwrap_or_else(|| panic!("no tests section in:\n{out}"));
    let rest = &out[start..];
    rest.find("\nDirect callers")
        .map_or(rest, |end| &rest[..end])
}

fn write_fixture(name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    for (path, contents) in files {
        let path = root.join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, contents).unwrap();
    }
    root
}

const TEXT_RS: &str = "pub fn render(x: u32) -> u32 {\n    x\n}\n\n\
pub fn attest_render() -> u32 {\n    render(3)\n}\n\n\
#[cfg(test)]\nmod tests {\n    use super::render;\n\n\
    #[test]\n    fn renders_one() {\n        assert_eq!(render(1), 1);\n    }\n\n\
    #[tokio::test]\n    async fn renders_async() {\n        render(2);\n    }\n}\n";

#[tokio::test]
async fn an_affected_test_says_why_it_is_one() {
    let root = write_fixture(
        "mct-affected-tests",
        &[
            ("src/text.rs", TEXT_RS),
            (
                "tests/render.rs",
                "fn check_render() {\n    render(4);\n}\n",
            ),
        ],
    );
    let server = build_server(&root).await;

    let toon = call(
        &server,
        "impact_analysis",
        json!({ "symbol": "render", "format": "toon" }),
    )
    .await;
    let tests = tests_section(&toon);
    assert!(
        tests.contains("{path,line,column,language,from,kind,to,depth,resolution,evidence,candidates,targets,test,affected}:"),
        "{tests}"
    );
    // Declared inline tests under src/, by the attribute as written.
    let row = |from: &str| {
        tests
            .lines()
            .find(|line| line.contains(&format!(",{from},")))
            .unwrap_or_else(|| panic!("no `{from}` row in:\n{tests}"))
            .to_string()
    };
    assert!(row("renders_one").ends_with(",#[test],likely"), "{tests}");
    assert!(
        row("renders_async").ends_with(",#[tokio::test],likely"),
        "{tests}"
    );
    // No attribute, but under tests/.
    assert!(row("check_render").contains(",path,"), "{tests}");
    // A caller whose name only contains `test` is no test.
    assert!(!tests.contains("attest_render"), "{tests}");
    assert!(toon.contains("attest_render"), "still a caller:\n{toon}");

    let text = call(
        &server,
        "impact_analysis",
        json!({ "symbol": "render", "format": "text" }),
    )
    .await;
    assert!(
        tests_section(&text).contains("[rust] renders_one (#[test])"),
        "{text}"
    );
    assert!(
        tests_section(&text).contains("[rust] check_render (test path"),
        "{text}"
    );

    let pack = call(
        &server,
        "build_context_pack",
        json!({ "symbol": "render", "path": "src/text.rs", "format": "text" }),
    )
    .await;
    assert!(pack.contains("caller,test:#[test] renders_one"), "{pack}");
    assert!(pack.contains("test:path check_render"), "{pack}");
    assert!(!pack.contains("test:name attest_render"), "{pack}");
}

#[tokio::test]
async fn a_test_reached_through_an_ambiguous_relation_is_only_possible() {
    let root = write_fixture(
        "mct-possible-tests",
        &[
            ("src/a.rs", "pub fn shared() {}\n"),
            ("src/b.rs", "pub fn shared() {}\n"),
            (
                "src/c.rs",
                "#[cfg(test)]\nmod tests {\n    #[test]\n    fn uses_shared() {\n        shared();\n    }\n}\n",
            ),
        ],
    );
    let server = build_server(&root).await;

    let toon = call(
        &server,
        "impact_analysis",
        json!({ "symbol": "shared", "format": "toon" }),
    )
    .await;
    let tests = tests_section(&toon);
    let row = tests
        .lines()
        .find(|line| line.contains(",uses_shared,"))
        .unwrap_or_else(|| panic!("no uses_shared row in:\n{toon}"));
    assert!(row.contains(",ambiguous,candidates,2,"), "{row}");
    assert!(row.ends_with(",#[test],possible"), "{row}");

    let text = call(
        &server,
        "impact_analysis",
        json!({ "symbol": "shared", "format": "text" }),
    )
    .await;
    assert!(
        tests_section(&text)
            .contains("uses_shared (#[test], possible) (ambiguous among 2, several candidates:"),
        "{text}"
    );
}
