//! React-style TSX logic coverage; JSX render relationships are not modeled.

use mct_core::{LanguageParser, RelationKind, SourceFile, SymbolKind};

use crate::JsTsParser;

fn parse(relative_path: &str, src: &str) -> mct_core::ParsedFile {
    JsTsParser
        .parse(&SourceFile {
            relative_path: relative_path.to_string(),
            contents: src.to_string(),
        })
        .expect("valid JS/TS source should parse")
}

#[test]
fn tsx_component_logic_is_indexed_without_structuring_jsx() {
    let parsed = parse(
        "Widget.tsx",
        "function useCount(): number {\n    return 1;\n}\n\nexport function Widget() {\n    const count = useCount();\n    return (\n        <div>\n            <button onClick={() => useCount()}>{count}</button>\n        </div>\n    );\n}\n",
    );
    // "Widget" is ambiguous by name alone: the file-level module
    // pseudo-symbol (named after Widget.tsx) collides with the exported
    // component of the same name — disambiguate by kind, same as the Java
    // plugin's `Calculator`/`Calculator.java` test.
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "Widget" && s.kind == SymbolKind::Function));

    // No JSX-specific SymbolKind/RelationKind exists (or is expected) — only
    // the logic embedded in the component (the hook call from the render
    // body, and the one inside the onClick handler) should show up as Calls.
    let use_count_calls = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls && r.to_name == "useCount")
        .count();
    assert_eq!(
        use_count_calls, 2,
        "one call in the component body, one inside the JSX event handler"
    );
}
