//! A PowerShell `class` is a `class` and an `enum` an `enum`. A class
//! property (`[string] $Name`) is a `field` of its class and each method a
//! `method`, a constructor as one named after the class. An enum member is a
//! `field` of its enum, as in TS, Java, Kotlin and Python. A method owns the
//! commands its body runs; a command never names a method, so a bare
//! `Reset` reaches a function `Reset` and never a method of that name.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind, SymbolKind};
use mct_lang_powershell::PowerShellParser;

const SOURCE: &str = r#"class Animal : Base, IFoo {
  [string] $Name
  hidden static [int] $Count = (Get-Start)
  Animal([string] $n) { $this.Name = $n; Write-Host "x" }
  [string] Speak([int] $times, $loud) { return Get-Sound }
  static [void] Reset() { }
}
enum Color {
  Red
  Green = 2
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PowerShellParser, "Zoo.psm1", "PowerShell", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str) -> mct_core::SymbolDetail {
    let id = parsed.symbols.iter().find(|s| s.name == name).unwrap().id;
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == id)
        .unwrap()
        .clone()
}

#[test]
fn classes_enums_and_their_members_are_symbols() {
    let parsed = parsed();
    let symbols: Vec<(&str, SymbolKind, Option<&str>, u32)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind != SymbolKind::Module)
        .map(|s| {
            (
                s.name.as_str(),
                s.kind,
                s.parent.as_deref(),
                s.location.line,
            )
        })
        .collect();
    assert_eq!(
        symbols,
        [
            ("Animal", SymbolKind::Class, Some("Zoo"), 1),
            ("Name", SymbolKind::Field, Some("Animal"), 2),
            ("Count", SymbolKind::Field, Some("Animal"), 3),
            ("Animal", SymbolKind::Method, Some("Animal"), 4),
            ("Speak", SymbolKind::Method, Some("Animal"), 5),
            ("Reset", SymbolKind::Method, Some("Animal"), 6),
            ("Color", SymbolKind::Enum, Some("Zoo"), 8),
            ("Red", SymbolKind::Field, Some("Color"), 9),
            ("Green", SymbolKind::Field, Some("Color"), 10),
        ]
    );
}

#[test]
fn members_carry_their_declarations() {
    let parsed = parsed();
    let speak = detail(&parsed, "Speak");
    assert_eq!(
        speak.signature.as_deref(),
        Some("[string] Speak([int] $times, $loud)")
    );
    let params: Vec<(Option<&str>, Option<&str>)> = speak
        .parameters
        .iter()
        .map(|p| {
            (
                p.name.as_deref(),
                p.type_text.as_ref().map(|t| t.text.as_str()),
            )
        })
        .collect();
    assert_eq!(params, [(Some("times"), Some("int")), (Some("loud"), None)]);
    assert!(speak.parameters_known);
    assert_eq!(
        speak.return_type.as_ref().map(|t| t.text.as_str()),
        Some("string")
    );
    assert_eq!(
        detail(&parsed, "Count").signature.as_deref(),
        Some("hidden static [int] $Count")
    );
    assert_eq!(
        detail(&parsed, "Color").signature.as_deref(),
        Some("enum Color")
    );
    let animal = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Animal" && s.kind == SymbolKind::Class)
        .unwrap()
        .id;
    let class = parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == animal)
        .unwrap();
    assert_eq!(
        class.signature.as_deref(),
        Some("class Animal : Base, IFoo")
    );
}

#[test]
fn a_member_owns_the_commands_it_runs() {
    let parsed = parsed();
    let name_of = |id| {
        let s = parsed.symbols.iter().find(|s| s.id == id).unwrap();
        format!("{} {:?}", s.name, s.kind)
    };
    let mut calls: Vec<(String, &str)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| (name_of(r.from), r.to_name.as_str()))
        .collect();
    calls.sort();
    assert_eq!(
        calls,
        [
            ("Animal Class".to_string(), "Get-Start"),
            ("Animal Method".to_string(), "Write-Host"),
            ("Speak Method".to_string(), "Get-Sound"),
        ]
    );
    assert!(
        parsed
            .symbols
            .iter()
            .all(|s| s.kind != SymbolKind::Variable),
        "an assignment in a class body is no script variable"
    );
}

#[test]
fn a_command_never_reaches_a_method() {
    let root = std::env::temp_dir().join(format!("mct-ps-classes-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Zoo.psm1"), SOURCE).unwrap();
    std::fs::write(root.join("Run.ps1"), "Reset\nSpeak\n").unwrap();
    std::fs::write(root.join("Lib.psm1"), "function Reset { }\n").unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PowerShellParser));
    let mut hits: Vec<String> = ["Reset", "Speak"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| format!("{}:{} {:?}", h.relative_path, h.line, h.resolution))
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(hits, ["Run.ps1:1 Resolved", "Run.ps1:2 Unresolved"]);
}
