// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-js-ts/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_js_ts::JsTsParser;

fn parse(relative_path: &str, src: &str) -> mct_core::ParsedFile {
    mct_corpus::parse_source(&JsTsParser, relative_path, "JS/TS", src)
}

#[test]
fn file_module_ends_on_the_last_line() {
    // A trailing newline must not push the module one line past the file,
    // in every grammar the crate dispatches to.
    let module = |path: &str, src: &str| {
        let parsed = parse(path, src);
        let m = parsed
            .symbols
            .iter()
            .find(|s| s.kind == SymbolKind::Module)
            .unwrap();
        (m.location.line, m.location.end_line)
    };
    let body = "function f() {\n  return 1;\n}";
    for path in ["a.js", "a.ts", "a.tsx"] {
        assert_eq!(module(path, &format!("{body}\n")), (1, Some(3)), "{path}");
        assert_eq!(module(path, body), (1, Some(3)), "{path}");
    }
    assert_eq!(module("a.js", "f();\n"), (1, Some(1)));
}

#[test]
fn arrow_function_assigned_to_variable_is_a_function_symbol() {
    let parsed = parse(
        "math.js",
        "const add = (a, b) => {\n    return helper(a, b);\n};\n\nfunction helper(a, b) {\n    return a + b;\n}\n",
    );
    let add = parsed.symbols.iter().find(|s| s.name == "add").unwrap();
    assert_eq!(add.kind, SymbolKind::Function);

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        calls.contains(&"helper"),
        "call inside the arrow function body should be attributed to it: {calls:?}"
    );
}

#[test]
fn es_module_import_and_named_export_are_recorded() {
    let parsed = parse(
        "utils.ts",
        "import { log } from \"./logger\";\n\nexport function greet(name: string): void {\n    log(name);\n}\n\nexport { greet as sayHello };\n",
    );
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"log"));

    let greet = parsed.symbols.iter().find(|s| s.name == "greet").unwrap();
    assert_eq!(greet.kind, SymbolKind::Function);

    let references: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        references.contains(&"greet"),
        "`export {{ greet as sayHello }}` should reference the local name: {references:?}"
    );
}

#[test]
fn import_alias_uses_local_bound_name() {
    let parsed = parse(
        "consumer.js",
        "import { foo as bar } from \"./mod\";\n\nbar();\n",
    );
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        imports.contains(&"bar"),
        "aliased import should record the local alias, not the source name: {imports:?}"
    );
    assert!(!imports.contains(&"foo"));
}

#[test]
fn commonjs_require_and_module_exports_are_recorded() {
    let parsed = parse(
        "mathUtils.js",
        "function add(a, b) {\n    return a + b;\n}\n\nmodule.exports = { add };\n",
    );
    let add = parsed
        .symbols
        .iter()
        .find(|s| s.name == "add" && s.kind == SymbolKind::Function)
        .unwrap();
    assert_eq!(add.kind, SymbolKind::Function);

    let references: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        references.contains(&"add"),
        "module.exports shorthand should reference the exported name: {references:?}"
    );
}

#[test]
fn require_call_is_an_import_relation() {
    let parsed = parse(
        "consumer.js",
        "const { add } = require(\"./mathUtils\");\n\nadd(1, 2);\n",
    );
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        imports.contains(&"./mathUtils"),
        "require() should record the module path: {imports:?}"
    );
}

#[test]
fn class_implements_interface_and_extends_base() {
    let parsed = parse(
        "shapes.ts",
        "interface Shape {\n    area(): number;\n}\n\nclass BaseShape {}\n\nclass Circle extends BaseShape implements Shape {\n    area(): number {\n        return 0;\n    }\n}\n",
    );
    let shape = parsed.symbols.iter().find(|s| s.name == "Shape").unwrap();
    assert_eq!(shape.kind, SymbolKind::Interface);

    let extends: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Extends)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(extends.contains(&"BaseShape"));

    let implements: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(implements.contains(&"Shape"));

    // The interface's signature and the class's implementation are each a
    // method of their own type.
    let areas: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.name == "area")
        .map(|s| (s.kind, s.parent.as_deref()))
        .collect();
    assert_eq!(
        areas,
        [
            (SymbolKind::Method, Some("Shape")),
            (SymbolKind::Method, Some("Circle"))
        ]
    );
}

#[test]
fn type_alias_is_extracted() {
    let parsed = parse("types.ts", "export type Id = string | number;\n");
    let id_type = parsed.symbols.iter().find(|s| s.name == "Id").unwrap();
    assert_eq!(id_type.kind, SymbolKind::TypeAlias);
}

#[test]
fn function_end_line_spans_the_whole_multiline_body() {
    let parsed = parse(
        "multi.js",
        "function helper(a, b) {\n    const sum = a + b;\n    return sum;\n}\n",
    );
    let sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "helper")
        .expect("helper function should be indexed");
    assert_eq!(sym.location.line, 1);
    assert_eq!(sym.location.end_line, Some(4));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    mct_corpus::assert_syntax_error_reported(&JsTsParser, "Broken.ts", "function broken( {\n");
}

/// `(from symbol name, to_name)` of every `References` relation.
fn value_references(parsed: &mct_core::ParsedFile) -> Vec<(String, String)> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| {
            let from = parsed.symbols.iter().find(|s| s.id == r.from).unwrap();
            (from.name.clone(), r.to_name.clone())
        })
        .collect()
}

#[test]
fn a_same_file_function_used_as_a_value_is_a_reference() {
    for path in ["main.js", "main.ts"] {
        let parsed = parse(
            path,
            r#"function cycleTheme() {}
function notificationsMenu() {}
function followStatus() {}
function debounce(fn) {
  const wrapped = () => fn();
  return wrapped;
}
function build() {
  const toggleSidebar = () => {};
  const away = (event) => {};
  document.addEventListener("click", away, true);
  setInterval(followStatus, 1000);
  return { run: cycleTheme, onclick: notificationsMenu, toggleSidebar, other: toggleSidebar };
}
"#,
        );
        let refs = value_references(&parsed);
        for (from, to) in [
            ("debounce", "wrapped"),
            ("build", "away"),
            ("build", "followStatus"),
            ("build", "cycleTheme"),
            ("build", "notificationsMenu"),
            ("build", "toggleSidebar"),
        ] {
            assert!(
                refs.contains(&(from.to_string(), to.to_string())),
                "{path}: {from} -> {to} missing from {refs:?}"
            );
        }
    }
}

#[test]
fn locals_callees_and_own_names_are_not_value_references() {
    let parsed = parse(
        "main.js",
        r#"function levels() {}
function available() {}
function tick() { setTimeout(tick, 10); }
function run(available) {
  const levels = load();
  levels();
  try { work(); } catch (levels) { return levels; }
  return available + levels.length;
}
function destructure({ levels }, [available]) { return levels + available; }
"#,
    );
    let refs = value_references(&parsed);
    assert!(refs.is_empty(), "{refs:?}");
}

#[test]
fn ts_types_named_in_annotations_are_references_except_type_parameters() {
    let parsed = parse(
        "ReviewPanel.tsx",
        r#"interface Payload { note: string }
interface Problem { retry?: Payload }
export interface ReviewPanelProps { item: string }
export type Tone = NonNullable<Variant>;
export function ReviewPanel(props: ReviewPanelProps): Result {
  const [problem] = useState<Problem | null>(null);
  const other: Shape = make();
  return problem;
}
function wrap<T>(value: T): T { return value; }
"#,
    );
    let refs = value_references(&parsed);
    for (from, to) in [
        ("ReviewPanel", "ReviewPanelProps"),
        ("ReviewPanel", "Result"),
        ("ReviewPanel", "Problem"),
        ("ReviewPanel", "Shape"),
        ("Tone", "NonNullable"),
        ("Tone", "Variant"),
    ] {
        assert!(
            refs.contains(&(from.to_string(), to.to_string())),
            "{from} -> {to} missing from {refs:?}"
        );
    }
    assert!(refs.iter().any(|(_, to)| to == "Payload"), "{refs:?}");
    assert!(!refs.iter().any(|(_, to)| to == "T"), "{refs:?}");
}

#[test]
fn an_exported_default_call_walks_its_arguments() {
    let parsed = parse(
        "vite.config.ts",
        r#"const sonnerWithoutInjectedStyle = () => ({ name: "x" });
export default defineConfig({ plugins: [react(), sonnerWithoutInjectedStyle()] });
"#,
    );
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    for name in ["defineConfig", "react", "sonnerWithoutInjectedStyle"] {
        assert!(calls.contains(&name), "{name} missing from {calls:?}");
    }
}
