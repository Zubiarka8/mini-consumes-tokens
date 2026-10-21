//! CLI contract checks; HTTP and index integration live in web::tests.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use std::process::Command;

#[test]
fn web_requires_an_existing_index_and_documents_options() {
    let root = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .arg("--root")
        .arg(root.path())
        .arg("web")
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("run `mct-cli init` first"));
    assert!(!root.path().join(".mct-index").exists());
    let help = Command::new(env!("CARGO_BIN_EXE_mct-cli"))
        .args(["web", "--help"])
        .output()
        .unwrap();
    let help = String::from_utf8_lossy(&help.stdout);
    assert!(help.contains("--no-obsidian") && help.contains("--port"));
}
