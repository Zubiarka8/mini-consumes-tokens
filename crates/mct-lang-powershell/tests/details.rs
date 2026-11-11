//! Declaration details (ADR-034, issue #321) the PowerShell parser proves
//! from syntax: a function's header up to its body, its parameters from the
//! inline list or the `param()` block (the `[type]` literal is the type,
//! other attributes are not), defaults, and a file-level variable's
//! assignment target. Without either parameter form a function's parameters
//! are unknown: it can still read `$args`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, SymbolDetail, SymbolKind};
use mct_lang_powershell::PowerShellParser;

const SOURCE: &str = r#"
function Get-Thing {
    [CmdletBinding()]
    param(
        [Parameter(Mandatory)][string]$Name,
        [int]$Count = 3,
        $Rest
    )
    $x = 1
}
function Add([int]$a, $b = 2) { $a + $b }
filter Only-Big { $_ }
$Global:Limit = 5
[int]$Max = 9
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PowerShellParser, "tools/thing.ps1", "PowerShell", SOURCE)
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

/// `name:type=default:kind` per parameter.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}={}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?", |t| t.text.as_str()),
                p.default.as_deref().unwrap_or("-"),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_param_block_gives_the_parameters_and_their_type_literals() {
    let parsed = parsed();
    let thing = detail(&parsed, "Get-Thing");
    assert_eq!(thing.signature.as_deref(), Some("function Get-Thing"));
    assert!(thing.parameters_known);
    assert_eq!(
        params(&thing),
        [
            "Name:string=-:positional",
            "Count:int=3:positional",
            "Rest:?=-:positional"
        ]
    );
    assert_eq!(thing.visibility, None);
}

#[test]
fn an_inline_parameter_list_is_part_of_the_header() {
    let parsed = parsed();
    let add = detail(&parsed, "Add");
    assert_eq!(
        add.signature.as_deref(),
        Some("function Add([int]$a, $b = 2)")
    );
    assert_eq!(params(&add), ["a:int=-:positional", "b:?=2:positional"]);
    let filter = detail(&parsed, "Only-Big");
    assert_eq!(filter.signature.as_deref(), Some("filter Only-Big"));
    assert!(!filter.parameters_known);
}

#[test]
fn a_variable_header_is_its_assignment_target() {
    let parsed = parsed();
    assert_eq!(
        detail(&parsed, "Limit").signature.as_deref(),
        Some("$Global:Limit")
    );
    assert_eq!(
        detail(&parsed, "Max").signature.as_deref(),
        Some("[int]$Max")
    );
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-ps-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("tools")).unwrap();
    std::fs::write(root.join("tools/thing.ps1"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PowerShellParser));
    let thing = index
        .symbol_details("tools/thing.ps1", 2, "Get-Thing")
        .unwrap()
        .unwrap();
    assert_eq!(thing.parameters.len(), 3);
    std::fs::remove_dir_all(root).unwrap();
}
