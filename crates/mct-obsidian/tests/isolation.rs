//! `mct-index` stays Obsidian-agnostic, and the evaluation harnesses never
//! write a vault: none of them may depend on `mct-obsidian`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::Path;

fn manifest(krate: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join(krate)
        .join("Cargo.toml");
    fs::read_to_string(path).unwrap()
}

#[test]
fn only_the_cli_and_the_server_depend_on_the_vault_writer() {
    for krate in [
        "mct-index",
        "mct-eval",
        "mct-corpus",
        "mct-lang-md",
        "mct-core",
    ] {
        assert!(
            !manifest(krate).contains("mct-obsidian"),
            "{krate} must not depend on mct-obsidian"
        );
    }
    for krate in ["mct-cli", "mct-mcp-server"] {
        assert!(
            manifest(krate).contains("mct-obsidian"),
            "{krate} registers the hook"
        );
    }
}
