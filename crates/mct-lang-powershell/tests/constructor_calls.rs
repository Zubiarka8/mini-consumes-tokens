//! `[Location]::new(…)` and `New-Object Location` construct a class: each
//! gives `calls Location` from its owner, and only a `class` can be the
//! target, so a method or function of that name never is. A dotted or
//! generic type (`[System.Text.StringBuilder]::new()`,
//! `[List[Location]]::new()`), another static member (`[math]::Abs(1)`) and
//! `New-Object -ComObject Shell` construct no class of the script and give
//! no such call.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::RelationKind;
use mct_lang_powershell::PowerShellParser;

const MODEL: &str =
    "class Location {\n  [string] $Code\n  [void] Shell() { }\n}\nfunction Location { }\n";

const SCRIPT: &str = r#"function Get-Spot {
  $a = [Location]::new('A-1')
  $b = New-Object Location 'B-2'
  $c = New-Object -TypeName Location
  $s = [System.Text.StringBuilder]::new()
  $l = [System.Collections.Generic.List[Location]]::new()
  $n = [math]::Abs(1)
  $w = New-Object -ComObject Shell
}
"#;

#[test]
fn a_constructor_call_names_its_class() {
    let parsed = mct_corpus::parse_source(&PowerShellParser, "Run.ps1", "PowerShell", SCRIPT);
    let mut calls: Vec<(&str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls && r.to_name != "New-Object")
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    calls.sort();
    assert_eq!(calls, [("Location", 2), ("Location", 3), ("Location", 4)]);
}

#[test]
fn a_constructor_call_reaches_only_the_class() {
    let root = std::env::temp_dir().join(format!("mct-ps-ctor-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Model.psm1"), MODEL).unwrap();
    std::fs::write(root.join("Run.ps1"), SCRIPT).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PowerShellParser));
    let mut hits: Vec<String> = ["Location", "Shell"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{}:{} {} {:?}",
                h.relative_path, h.line, h.to_name, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "Run.ps1:2 Location Resolved",
            "Run.ps1:3 Location Resolved",
            "Run.ps1:4 Location Resolved",
        ]
    );
}
