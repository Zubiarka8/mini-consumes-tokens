//! File-level `Imports` evidence: each cross-file import names the target
//! file's `Module` symbol and carries enough evidence to resolve to it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::SymbolKind;
use mct_index::Index;
use mct_lang_js_ts::JsTsParser;

fn parse(path: &str, src: &str) -> mct_core::ParsedFile {
    mct_corpus::parse_source(&JsTsParser, path, "JS/TS", src)
}

fn open_indexed() -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/depgraph");
    mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser))
}

use mct_corpus::import_target;

#[test]
fn relative_imports_resolve_to_the_target_files_module() {
    let index = open_indexed();
    assert_eq!(
        import_target(&index, "util", "app.ts").as_deref(),
        Some("lib/util.ts")
    );
    assert_eq!(
        import_target(&index, "legacy", "app.ts").as_deref(),
        Some("legacy.ts")
    );
    assert_eq!(
        import_target(&index, "old", "app.ts").as_deref(),
        Some("lib/old.js")
    );
    assert_eq!(
        import_target(&index, "lazy", "app.ts").as_deref(),
        Some("lib/lazy.ts")
    );
    assert_eq!(
        import_target(&index, "util", "reexport.ts").as_deref(),
        Some("lib/util.ts")
    );
}

#[test]
fn evidence_is_set_only_where_the_source_proves_it() {
    let parsed = parse(
        "src/app.ts",
        "import a from \"./a\";\nimport b from \"./b.ts\";\nimport c from \"react\";\nimport d from \"node:fs\";\nimport e from \"./data.json\";\n",
    );
    let evidence = |name: &str| mct_corpus::import_evidence(&parsed, name);
    // Extensionless: only the stem and the directory are proven.
    let a = evidence("a").unwrap();
    assert_eq!(
        (a.module.as_deref(), a.path.as_deref()),
        (Some("src"), None)
    );
    assert_eq!(a.kind, Some(SymbolKind::Module));
    // Literal TypeScript extension: the exact file is named.
    assert_eq!(evidence("b").unwrap().path.as_deref(), Some("src/b.ts"));
    // A `node:` builtin is provably outside the repository.
    assert!(evidence("node:fs").unwrap().external);
    // A bare specifier or an asset is neither external nor a local module.
    assert!(!parsed.relations.iter().any(|r| r.to_name == "react"));
    assert!(!parsed.relations.iter().any(|r| r.to_name == "data"));
}
