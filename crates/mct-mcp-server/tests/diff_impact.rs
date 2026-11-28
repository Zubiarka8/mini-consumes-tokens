//! The `diff_impact` tool (issue #632): a branch's changed definitions with
//! their callers and tests, a removed one still named, and a bad `base`
//! answered as invalid parameters.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "common/server.rs"]
mod server_fixture;
use server_fixture::build_server_at as build_server;

#[path = "common/tempdir.rs"]
mod tempdir;

#[path = "common/all_content.rs"]
mod all_content;
use all_content::content_of;

use git2::{IndexAddOption, Repository, Signature};
use mct_mcp_server::server::MctServer;
use serde_json::{json, Value};

const LIB_RS: &str = "pub fn parse_item() {\n    validate();\n}\nfn validate() {}\n\
pub fn retired() {}\npub fn keep() {\n    retired();\n}\n";

fn write(root: &std::path::Path, path: &str, contents: &str) {
    let path = root.join(path);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
}

fn commit(root: &std::path::Path, message: &str) {
    let repo = Repository::open(root).unwrap_or_else(|_| Repository::init(root).unwrap());
    let mut index = repo.index().unwrap();
    index
        .add_all(["src", "tests"], IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let signature = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let parents: Vec<_> = parent.iter().collect();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parents,
    )
    .unwrap();
}

/// A repository whose working tree changes `validate` and drops `retired`
/// on top of one commit.
async fn changed_project() -> MctServer {
    let dir = tempdir::tempdir("mct-diff-impact-");
    let root = dir.as_path();
    write(root, "src/lib.rs", LIB_RS);
    write(root, "src/other.rs", "pub fn validate() {}\n");
    write(
        root,
        "tests/parse_item.rs",
        "#[test]\nfn test_parse_item() {\n    parse_item();\n}\n",
    );
    commit(root, "base");
    let changed = LIB_RS
        .replace("fn validate() {}", "fn validate() { let _ = 1; }")
        .replace("pub fn retired() {}\n", "");
    write(root, "src/lib.rs", &changed);
    build_server(root).await
}

async fn call(server: &MctServer, args: Value) -> Result<String, String> {
    let Value::Object(map) = args else {
        panic!("arguments must be an object")
    };
    server
        .call_read_only_tool("diff_impact", Some(map))
        .await
        .map(|result| content_of(&result))
        .map_err(|err| err.message.to_string())
}

#[tokio::test]
async fn the_working_tree_change_lists_callers_tests_and_what_still_names_a_removal() {
    let server = changed_project().await;
    let out = call(&server, json!({"base": "HEAD"})).await.unwrap();
    assert!(out.starts_with("Diff impact "), "{out}");
    assert!(
        out.contains("..working tree: 1 file(s) changed, 1 definition(s) changed, 1 removed.\n"),
        "{out}"
    );
    // TOON: the changed definition, its caller and its test, and the
    // removed definition still called by `keep`.
    assert!(
        out.contains("validate,function,src/lib.rs:4,4,1,1,0"),
        "{out}"
    );
    assert!(
        out.contains("validate,parse_item,src/lib.rs:2,calls"),
        "{out}"
    );
    assert!(
        out.contains("validate,test_parse_item,tests/parse_item.rs:3,#[test],confirmed"),
        "{out}"
    );
    assert!(out.contains("retired,function,src/lib.rs:5,1"), "{out}");
    // The homonym in another file is untouched.
    assert!(!out.contains("src/other.rs"), "{out}");
}

#[tokio::test]
async fn a_base_that_is_no_commit_is_an_invalid_parameter() {
    let server = changed_project().await;
    let err = call(&server, json!({"base": "no-such-branch"}))
        .await
        .unwrap_err();
    assert!(
        err.contains("is `base`/`head` a commit of this repository?"),
        "{err}"
    );
    let err = call(&server, json!({"base": "--output=x"}))
        .await
        .unwrap_err();
    assert!(
        err.contains("base must be a commit, branch or tag name"),
        "{err}"
    );
}
