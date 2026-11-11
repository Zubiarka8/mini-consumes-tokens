//! Declaration details (ADR-034, issue #321) the Bash parser proves: a
//! function's header (`function f()` or `f()`), with its parameters
//! unknown — a shell function declares none, it reads `$1`, `$@` — and a
//! file-level variable's header with its declaration keyword and flags,
//! `readonly`/`-r` being the one modifier. Bash has no types or visibility.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, SymbolDetail, SymbolKind};
use mct_lang_bash::BashParser;

const SOURCE: &str = r#"
# Deploys.
function deploy() {
  local target=$1
}
build() { echo hi; }
readonly VERSION=1.2
export PATH_EXTRA=/x
declare -r -x LIMIT=3
COUNT=0
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&BashParser, "bin/deploy.sh", "Bash", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str) -> SymbolDetail {
    let symbol = parsed
        .symbols
        .iter()
        .find(|s| s.name == name && s.kind != SymbolKind::Module)
        .unwrap_or_else(|| panic!("no {name}"));
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbol.id)
        .cloned()
        .unwrap_or_else(|| panic!("no detail for {name}"))
        .normalized()
}

#[test]
fn a_function_header_has_unknown_parameters() {
    let parsed = parsed();
    let deploy = detail(&parsed, "deploy");
    assert_eq!(deploy.signature.as_deref(), Some("function deploy()"));
    assert!(!deploy.parameters_known);
    assert!(deploy.parameters.is_empty());
    assert!(deploy.return_type.is_none());
    assert_eq!(deploy.visibility, None);
    assert_eq!(
        detail(&parsed, "build").signature.as_deref(),
        Some("build()")
    );
}

#[test]
fn a_variable_header_keeps_its_keyword_and_flags_but_not_its_value() {
    let parsed = parsed();
    let version = detail(&parsed, "VERSION");
    assert_eq!(version.signature.as_deref(), Some("readonly VERSION"));
    assert_eq!(version.modifiers, [Modifier::Readonly]);
    let limit = detail(&parsed, "LIMIT");
    assert_eq!(limit.signature.as_deref(), Some("declare -r -x LIMIT"));
    assert_eq!(limit.modifiers, [Modifier::Readonly]);
    let path = detail(&parsed, "PATH_EXTRA");
    assert_eq!(path.signature.as_deref(), Some("export PATH_EXTRA"));
    assert!(path.modifiers.is_empty());
    assert_eq!(detail(&parsed, "COUNT").signature.as_deref(), Some("COUNT"));
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-bash-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("bin")).unwrap();
    std::fs::write(root.join("bin/deploy.sh"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(BashParser));
    let deploy = index
        .symbol_details("bin/deploy.sh", 3, "deploy")
        .unwrap()
        .unwrap();
    assert_eq!(deploy.signature.as_deref(), Some("function deploy()"));
    std::fs::remove_dir_all(root).unwrap();
}
