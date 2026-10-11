//! React-style JSX/TSX logic and component-reference coverage.

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

#[test]
fn jsx_component_uses_are_references_with_precise_ownership() {
    for path in ["App.jsx", "App.tsx"] {
        let parsed = parse(
            path,
            r#"function Child() { return <span />; }
function App() {
    return <section><Child onClick={() => track()}><Child /></Child></section>;
}
class Screen { render() { return <Child />; } }
const Root = <Child />;
"#,
        );
        let references: Vec<_> = parsed
            .relations
            .iter()
            .filter(|r| r.kind == RelationKind::References && r.to_name == "Child")
            .collect();
        assert_eq!(
            references.len(),
            4,
            "opening/self-closing tags only: {path}"
        );
        let owners: Vec<_> = references
            .iter()
            .map(|r| {
                parsed
                    .symbols
                    .iter()
                    .find(|s| s.id == r.from)
                    .unwrap()
                    .name
                    .as_str()
            })
            .collect();
        assert_eq!(owners, ["App", "App", "render", "App"], "{path}");
        assert_eq!(references[0].location.line, 3);
        assert!(parsed
            .relations
            .iter()
            .any(|r| r.kind == RelationKind::Calls && r.to_name == "track"));
        assert!(!parsed
            .relations
            .iter()
            .any(|r| r.kind == RelationKind::Calls && r.to_name == "Child"));
        assert!(!parsed
            .relations
            .iter()
            .any(|r| ["section", "span"].contains(&r.to_name.as_str())));
    }
}

#[test]
fn jsx_member_names_stay_qualified_and_intrinsics_are_excluded() {
    let parsed = parse(
        "View.tsx",
        r#"const View = () => <><UI.Button /><ui.Panel></ui.Panel><_Internal /><$Button /><my-widget /><svg:path /></>;"#,
    );
    let references: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(
        references,
        ["UI.Button", "ui.Panel", "_Internal", "$Button"]
    );
}
