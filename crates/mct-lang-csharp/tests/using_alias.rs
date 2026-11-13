//! `using Alias = Target;` names its target, so it imports no symbol
//! called `Alias`: only the target namespace import is recorded, and a
//! tuple target (`using Point = (int X, int Y);`) imports nothing. A use of
//! the alias later in the same file is a use of the target: `new Alias()`
//! constructs it (external under `System`), and `Alias.M()` is qualified by
//! it and references it. `new M.Alias()` is no use of the alias, and a
//! `global using` alias, visible project-wide, changes nothing here. The
//! alias is no symbol yet (#446).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{ParsedFile, RelationKind};
use mct_lang_csharp::CSharpParser;

const SOURCE: &str = r#"global using G = System.Guid;
using Json = System.Text.Json.JsonSerializer;
using L = System.Collections.Generic.List<string>;
using Point = (int X, int Y);
using M = Shop.Models;
namespace Shop {
  using Dto = Shop.Models.OrderDto;
  class S {
    void Run(object x) {
      Json.Serialize(x);
      Dto.Validate(x);
      var l = new L();
      var m = new M.L();
      var g = new G();
    }
  }
}
"#;

/// Name, line, qualifier and `external` of each relation of `kind`.
type Row = (String, u32, Option<String>, bool);

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&CSharpParser, "src/S.cs", "C#", SOURCE)
}

fn rows(parsed: &ParsedFile, kind: RelationKind) -> Vec<Row> {
    let mut rows: Vec<Row> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == kind)
        .map(|(i, r)| {
            let target = parsed.relation_targets.iter().find(|t| t.relation == i);
            (
                r.to_name.clone(),
                r.location.line,
                target.and_then(|t| t.qualifier.clone()),
                target.is_some_and(|t| t.external),
            )
        })
        .collect();
    rows.sort_by(|a, b| (a.1, &a.0).cmp(&(b.1, &b.0)));
    rows
}

fn row(name: &str, line: u32, qualifier: Option<&str>, external: bool) -> Row {
    (
        name.to_string(),
        line,
        qualifier.map(str::to_string),
        external,
    )
}

#[test]
fn an_alias_imports_no_symbol_of_its_own_name() {
    let imports: Vec<(String, u32)> = rows(&parsed(), RelationKind::Imports)
        .into_iter()
        .map(|r| (r.0, r.1))
        .collect();
    let import = |name: &str, line| (name.to_string(), line);
    assert_eq!(
        imports,
        [
            import("System.Guid", 1),
            import("System.Text.Json.JsonSerializer", 2),
            import("Shop.Models", 5),
            import("Shop.Models.OrderDto", 7),
        ]
    );
}

#[test]
fn an_alias_use_is_a_use_of_its_target() {
    let parsed = parsed();
    assert_eq!(
        rows(&parsed, RelationKind::Calls),
        [
            row("Serialize", 10, Some("JsonSerializer"), false),
            row("Validate", 11, Some("OrderDto"), false),
            row("List", 12, None, true),
            // `M.L` is a type of the namespace, not the alias `L`.
            row("L", 13, None, false),
            // A `global using` alias is not this file's evidence.
            row("G", 14, None, false),
        ]
    );
    assert_eq!(
        rows(&parsed, RelationKind::References),
        [
            row("JsonSerializer", 10, None, true),
            row("OrderDto", 11, None, false),
        ]
    );
}

#[test]
fn an_alias_name_never_reaches_a_symbol_of_that_name() {
    let root = std::env::temp_dir().join(format!("mct-cs-using-alias-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/S.cs"), SOURCE).unwrap();
    std::fs::write(
        root.join("src/Other.cs"),
        "namespace Other { class Json { } class Dto { } }\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(CSharpParser));
    let hits: Vec<String> = ["Json", "Dto"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .map(|h| format!("{} {}:{}", h.kind, h.relative_path, h.line))
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    assert!(hits.is_empty(), "{hits:?}");
}
