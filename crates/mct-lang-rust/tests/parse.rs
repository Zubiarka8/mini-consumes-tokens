// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-rust/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, RelationTarget, SourceFile, SymbolKind};
use mct_lang_rust::RustParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    RustParser
        .parse(&SourceFile {
            relative_path: "src/lib.rs".to_string(),
            contents: src.to_string(),
        })
        .expect("valid Rust source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse(
        r#"
        fn helper() -> i32 { 42 }
        fn main() {
            let x = helper();
            println!("{x}");
        }
        "#,
    );
    let names: Vec<_> = parsed.symbols.iter().map(|s| s.name.as_str()).collect();
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"main"));

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"helper"));
}

#[test]
fn extracts_generic_struct_trait_and_impl_methods() {
    let parsed = parse(
        r#"
        trait Shape {
            fn area(&self) -> f64;
        }

        struct Rect<T> {
            width: T,
            height: T,
        }

        impl<T> Shape for Rect<T> {
            fn area(&self) -> f64 {
                0.0
            }
        }
        "#,
    );

    let struct_sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Rect")
        .expect("Rect struct should be indexed");
    assert_eq!(struct_sym.kind, SymbolKind::Struct);

    let trait_sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Shape")
        .expect("Shape trait should be indexed");
    assert_eq!(trait_sym.kind, SymbolKind::Trait);

    let methods: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Method && s.name == "area")
        .collect();
    // one from the trait's signature, one from the impl block's body
    assert_eq!(methods.len(), 2);
    assert!(methods.iter().any(|m| m.parent.as_deref() == Some("Shape")));
    assert!(methods
        .iter()
        .any(|m| m.parent.as_deref() == Some("Rect<T>")));

    let implements: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Implements)
        .collect();
    assert!(implements.iter().any(|r| r.to_name == "Shape"));
}

#[test]
fn extracts_use_imports() {
    let parsed = parse("use std::collections::HashMap;\nuse std::fmt::{Debug, Display};\n");
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"HashMap"));
    assert!(imports.contains(&"Debug"));
    assert!(imports.contains(&"Display"));
}

#[test]
fn function_end_line_spans_the_whole_multiline_body() {
    let parsed = parse("fn multiline() -> i32 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n");
    let sym = parsed
        .symbols
        .iter()
        .find(|s| s.name == "multiline")
        .expect("multiline function should be indexed");
    // Line 1: `fn multiline() -> i32 {`, line 5: the closing `}`.
    assert_eq!(sym.location.line, 1);
    assert_eq!(sym.location.end_line, Some(5));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = RustParser.parse(&SourceFile {
        relative_path: "src/broken.rs".to_string(),
        contents: "fn main( { this is not valid rust @@@".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}

fn literal_texts(parsed: &mct_core::ParsedFile) -> Vec<(&str, u32)> {
    parsed
        .literals
        .iter()
        .map(|l| (l.text.as_str(), l.line))
        .collect()
}

#[test]
fn prose_literals_keep_only_fixed_format_fragments() {
    let parsed = parse(
        r#"
fn connect(url: &str) -> Result<(), String> {
    Err(format!("Error de conexión con la BD: {url} (retry {}/{} failed)", 1, 3))
}
"#,
    );
    assert_eq!(
        literal_texts(&parsed),
        vec![("Error de conexión con la BD:", 3)],
        "`(retry `, `/` and ` failed)` are too short to be prose"
    );
}

#[test]
fn literals_resolve_escapes_and_skip_non_prose() {
    let parsed = parse(
        r####"
const KEY: &str = "relative_path";
const TOKEN: &str = "Xk9mQ2vR7tLp4wZs8bNc3hJd";
fn run() {
    log("first line\nsecond \"quoted\" line");
    log(r"raw \d+ pattern kept as written");
    log("could not open the file");
    log("could not open the file");
    log("braces {{escaped}} stay in text");
}
"####,
    );
    assert_eq!(
        literal_texts(&parsed),
        vec![
            ("first line second \"quoted\" line", 5),
            (r"raw \d+ pattern kept as written", 6),
            ("could not open the file", 7),
            ("braces {escaped} stay in text", 9),
        ]
    );
}

#[test]
fn a_multiline_literal_fragment_reports_the_line_it_starts_on() {
    let parsed = parse("fn f() {\n    let _ = \"{}\nthe second fragment starts here\";\n}\n");
    assert_eq!(
        literal_texts(&parsed),
        vec![("the second fragment starts here", 3)]
    );
}

fn references(parsed: &mct_core::ParsedFile) -> Vec<&str> {
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect()
}

#[test]
fn a_same_file_function_used_as_a_value_is_a_reference() {
    let parsed = parse(
        r#"
        pub fn actual_use() {}
        pub fn entry() { let _f = actual_use; }
        pub fn mapper(v: Vec<u8>) { v.into_iter().for_each(drop_it); }
        fn drop_it(_: u8) {}
        "#,
    );
    let refs = references(&parsed);
    assert!(refs.contains(&"actual_use"), "{refs:?}");
    assert!(refs.contains(&"drop_it"), "{refs:?}");
}

#[test]
fn a_same_file_function_inside_a_macro_token_tree_is_a_reference_not_a_call() {
    // The shape of `write_parsed_file`'s `params![.., symbol_kind_str(kind)]`.
    let parsed = parse(
        r#"
        fn symbol_kind_str(k: u8) -> &'static str { "x" }
        fn write(conn: &C, kind: u8) {
            conn.execute("INSERT", params![1, symbol_kind_str(kind)]);
        }
        "#,
    );
    assert_eq!(references(&parsed), vec!["symbol_kind_str"]);
    assert!(!parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to_name == "symbol_kind_str"));
}

#[test]
#[ignore = "known bug, not filed yet: a method call inside a macro's token tree leaves no relation, so find_callers/find_references/impact_analysis miss it (benchmarks/agent-benchmark.md)"]
fn a_method_call_inside_a_macro_token_tree_leaves_a_relation() {
    // The shape of mct-cli's `the_cli_and_mcp_server_registries_ship_the_same_languages_and_extensions` test.
    let parsed = parse(
        r#"
        fn agree(cli: &R, server: &R) {
            assert_eq!(
                cli.for_extension("rs"),
                server.for_extension("rs"),
            );
        }
        "#,
    );
    let lines: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.to_name == "for_extension")
        .map(|r| r.location.line)
        .collect();
    assert_eq!(lines, vec![4, 5], "{:?}", parsed.relations);
}

#[test]
fn locals_fields_paths_labels_and_foreign_names_are_not_references() {
    let parsed = parse(
        r#"
        fn helper() {}
        fn other_helper() {}
        fn by_param(helper: u32) -> u32 { helper + 1 }
        fn by_let() -> u32 { let helper = 2; helper }
        fn by_closure() { let f = |other_helper: u32| other_helper; }
        fn by_match(x: Option<u32>) -> u32 { match x { Some(helper) => helper, None => 0 } }
        fn by_field(s: &S) -> u32 { s.helper }
        fn by_path() { let _f = other::helper; }
        fn by_label() { 'helper: loop { break 'helper; } }
        fn foreign() { let _f = not_in_this_file; println!("{}", also_not(1)); }
        fn in_macro(s: &S) { println!("{} {}", s.helper.0, other::helper()); }
        #[cfg(helper)]
        fn attributed() {}
        "#,
    );
    assert!(references(&parsed).is_empty(), "{:?}", references(&parsed));
}

#[test]
fn a_turbofish_method_call_still_walks_its_receiver_chain() {
    let parsed = parse(
        "fn f(v: Vec<u8>) -> u32 { v.iter().map(g).sum::<u32>() }\nfn g(x: &u8) -> u32 { 0 }\n",
    );
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    for name in ["sum", "map", "iter"] {
        assert!(calls.contains(&name), "{name} missing from {calls:?}");
    }
    assert_eq!(references(&parsed), vec!["g"]);
}

#[test]
fn a_direct_call_is_recorded_once_as_a_call_only() {
    let parsed = parse("fn helper() {}\nfn run() { helper(); helper::<u8>(); }\n");
    assert!(references(&parsed).is_empty(), "{:?}", references(&parsed));
}

#[test]
fn a_harness_attribute_references_its_function() {
    let parsed = parse(
        r#"
        #[cfg(test)]
        mod tests {
            #[test]
            /// doc comments between the attribute and the fn are fine
            #[should_panic]
            fn verifies_behavior() {}

            #[tokio::test]
            async fn async_case() {}

            #[bench]
            fn bench_case(b: &mut Bencher) {}

            #[test_case(1)]
            fn case(n: u32) {}

            fn helper() {}

            #[inline]
            fn inlined() {}
        }
        "#,
    );
    let refs = references(&parsed);
    for name in ["verifies_behavior", "async_case", "bench_case", "case"] {
        assert!(refs.contains(&name), "{name} missing from {refs:?}");
    }
    assert!(!refs.contains(&"helper"), "{refs:?}");
    assert!(!refs.contains(&"inlined"), "{refs:?}");
}

#[test]
fn a_harness_attribute_does_not_leak_past_another_item() {
    let parsed = parse("#[test]\nconst X: u8 = 1;\nfn after() {}\n");
    assert!(references(&parsed).is_empty(), "{:?}", references(&parsed));
}

#[test]
fn a_struct_pattern_shorthand_binding_shadows_a_same_named_function() {
    // `let Config { root, .. } = c;` binds `root` as a local exactly like
    // `let root = c.root;` does: its later uses are the local, never `fn root`.
    let parsed = parse(
        r#"
        fn root() {}
        struct Config { root: u32, depth: u32 }
        fn by_let(c: Config) -> u32 { let Config { root, .. } = c; root }
        fn by_param(Config { root, depth }: Config) -> u32 { root + depth }
        fn by_match(c: Config) -> u32 { match c { Config { ref root, .. } => *root } }
        fn by_closure(v: Vec<Config>) -> Vec<u32> { v.into_iter().map(|Config { root, .. }| root).collect() }
        "#,
    );
    assert!(references(&parsed).is_empty(), "{:?}", references(&parsed));
}

/// `(to_name, evidence)` of every call, in source order; the evidence's
/// `relation` index is zeroed for terse expectations.
fn call_evidence(parsed: &mct_core::ParsedFile) -> Vec<(String, RelationTarget)> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let t = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .cloned()
                .unwrap_or_default();
            (r.to_name.clone(), RelationTarget { relation: 0, ..t })
        })
        .collect()
}

#[test]
fn calls_record_only_the_qualification_the_source_proves() {
    let parsed = parse(
        r#"
struct Stack<T>(Vec<T>);
impl<T> Stack<T> {
    fn new() -> Self { Self::empty() }
    fn empty() -> Self { todo!() }
    fn push(&mut self) { self.grow(); other.grow(); }
    fn grow(&mut self) {}
}
fn helper() {}
fn main() {
    helper();
    Stack::<u8>::new();
    crate::m::run();
    std::mem::take(&mut 1);
    let local = |x: u8| x;
    local(1);
    rand::random();
    serde_json::Value::from(1);
    self::helper();
    crate::top();
}
"#,
    );
    let call = |name: &str, target: RelationTarget| (name.to_string(), target);
    let qualified = |q: &str| RelationTarget {
        qualifier: Some(q.to_string()),
        ..Default::default()
    };
    let module = |m: &str| RelationTarget {
        module: Some(m.to_string()),
        ..Default::default()
    };
    let this_file = RelationTarget {
        path: Some("src/lib.rs".to_string()),
        ..Default::default()
    };
    assert_eq!(
        call_evidence(&parsed),
        vec![
            call("empty", qualified("Stack")),
            call("grow", qualified("Stack")),
            // An unknown receiver can only reach a method, never provably one.
            call(
                "grow",
                RelationTarget {
                    member: true,
                    ..Default::default()
                }
            ),
            // A same-file free function no local shadows.
            call("helper", this_file.clone()),
            call("new", qualified("Stack")),
            // A module path names where the target lives, not its type.
            call("run", module("m")),
            call(
                "take",
                RelationTarget {
                    external: true,
                    ..Default::default()
                }
            ),
            call("local", RelationTarget::default()),
            call("random", module("rand")),
            call(
                "from",
                RelationTarget {
                    module: Some("serde_json".to_string()),
                    ..qualified("Value")
                }
            ),
            call("helper", this_file),
            // A bare `crate::` proves nothing about where in the crate.
            call("top", RelationTarget::default()),
        ]
    );
}

#[test]
fn std_imports_are_external_and_crate_imports_are_not() {
    let parsed = parse("use std::collections::HashMap;\nuse crate::index::Index;\n");
    let external: Vec<(String, bool)> = parsed
        .relations
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let ext = parsed
                .relation_targets
                .iter()
                .any(|t| t.relation == i && t.external);
            (r.to_name.clone(), ext)
        })
        .collect();
    assert_eq!(
        external,
        vec![("HashMap".to_string(), true), ("Index".to_string(), false)]
    );
}

#[test]
fn constant_and_static_initializers_record_the_root_call() {
    let parsed = parse("const fn helper()->u32{1} const X:u32=helper(); static Y:u32=helper();");
    for name in ["X", "Y"] {
        let id = parsed.symbols.iter().find(|s| s.name == name).unwrap().id;
        assert!(parsed
            .relations
            .iter()
            .any(|r| r.from == id && r.kind == RelationKind::Calls && r.to_name == "helper"));
    }
}

#[test]
fn deeply_nested_use_lists_do_not_overflow_a_small_stack() {
    std::thread::Builder::new()
        .stack_size(2 * 1024 * 1024)
        .spawn(|| {
            let source = format!("use {}leaf{};", "a::{".repeat(8000), "}".repeat(8000));
            let parsed = parse(&source);
            assert!(
                parsed.relations.is_empty(),
                "over-depth subtree should be pruned"
            );
        })
        .unwrap()
        .join()
        .unwrap();
}
