//! A type literal `[T]` (`[Zone]::Cold`, `[Spot] $s`, `[OutputType([Spot])]`)
//! names a class or enum: when this file declares one of that name (any
//! case: PowerShell type names ignore it) it is a `references` to it,
//! pinned to this file and to its kind, so a method named like the type
//! (`[Zone] Zone()`) is never a candidate. A script class is visible elsewhere only through
//! `using module`, so any other name (`[string]`, `[IO.File]`, a class of
//! another file) gives no relation. `[T]::new()` stays a `calls`.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::RelationKind;
use mct_lang_powershell::PowerShellParser;

const SOURCE: &str = r#"enum Zone { Cold }
class Spot {
  [Zone] $Kind
  [Zone] Pick([spot] $other) { return [Zone]::Cold }
  [Zone] Zone() { return 0 }
}
function Get-Spot {
  [OutputType([Spot])]
  param([Spot] $s, [string] $name)
  $x = [Spot]::new()
  [IO.File]::Exists($name)
  [Remote]::Do()
}
"#;

#[test]
fn a_type_literal_of_this_file_references_it() {
    let parsed = mct_corpus::parse_source(&PowerShellParser, "Map.psm1", "PowerShell", SOURCE);
    let mut uses: Vec<(RelationKind, &str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelationKind::References | RelationKind::Calls))
        .filter(|r| !r.to_name.contains('-'))
        .map(|r| (r.kind, r.to_name.as_str(), r.location.line))
        .collect();
    uses.sort_by_key(|u| (u.2, u.1));
    assert_eq!(
        uses,
        [
            (RelationKind::References, "Zone", 3),
            (RelationKind::References, "Spot", 4),
            (RelationKind::References, "Zone", 4),
            (RelationKind::References, "Zone", 4),
            (RelationKind::References, "Zone", 5),
            (RelationKind::References, "Spot", 8),
            (RelationKind::References, "Spot", 9),
            (RelationKind::Calls, "Spot", 10),
        ]
    );
}

#[test]
fn the_references_resolve_to_this_file() {
    let root = std::env::temp_dir().join(format!("mct-ps-type-literals-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Map.psm1"), SOURCE).unwrap();
    std::fs::write(
        root.join("Other.psm1"),
        "class Zone { }\nclass Remote { }\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PowerShellParser));
    let mut hits: Vec<String> = ["Zone", "Spot", "Remote"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "references")
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
            "Map.psm1:3 Zone Resolved",
            "Map.psm1:4 Spot Resolved",
            "Map.psm1:4 Zone Resolved",
            "Map.psm1:4 Zone Resolved",
            "Map.psm1:5 Zone Resolved",
            "Map.psm1:8 Spot Resolved",
            "Map.psm1:9 Spot Resolved",
        ]
    );
}
