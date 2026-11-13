//! A PowerShell class's base list (`class Dog : Animal, IComparable`) names
//! its base class first, then interfaces. As in C#, the first base is
//! `extends` unless it is named like an interface (`IFoo`, the .NET
//! convention), and the rest are `implements`. A script can declare no
//! interface, so an `extends` target must be a `class` and an
//! `implements` one an `interface`: a same-named function never answers.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::RelationKind;
use mct_lang_powershell::PowerShellParser;

const SOURCE: &str = "class Animal { }\nclass Dog : Animal, IComparable, IDisposable { }\nclass Box : IDisposable { }\nfunction IComparable { }\n";

#[test]
fn the_first_base_is_extended_and_the_rest_implemented() {
    let parsed = mct_corpus::parse_source(&PowerShellParser, "Zoo.psm1", "PowerShell", SOURCE);
    let name_of = |id| {
        parsed
            .symbols
            .iter()
            .find(|s| s.id == id)
            .unwrap()
            .name
            .as_str()
    };
    let bases: Vec<(&str, RelationKind, &str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| matches!(r.kind, RelationKind::Extends | RelationKind::Implements))
        .map(|r| (name_of(r.from), r.kind, r.to_name.as_str(), r.location.line))
        .collect();
    assert_eq!(
        bases,
        [
            ("Dog", RelationKind::Extends, "Animal", 2),
            ("Dog", RelationKind::Implements, "IComparable", 2),
            ("Dog", RelationKind::Implements, "IDisposable", 2),
            ("Box", RelationKind::Implements, "IDisposable", 3),
        ]
    );
}

#[test]
fn a_base_reaches_only_a_type_of_its_kind() {
    let root = std::env::temp_dir().join(format!("mct-ps-bases-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(root.join("Zoo.psm1"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PowerShellParser));
    let mut hits: Vec<String> = ["Animal", "IComparable"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .map(|h| format!("{}:{} {} {:?}", h.kind, h.line, h.to_name, h.resolution))
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            "extends:2 Animal Resolved",
            "implements:2 IComparable Unresolved",
        ]
    );
}
