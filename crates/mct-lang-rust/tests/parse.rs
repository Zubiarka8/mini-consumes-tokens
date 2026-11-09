// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-rust/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, RelationKind, RelationTarget, SourceFile, SymbolKind};
use mct_lang_rust::RustParser;

fn parse(src: &str) -> mct_core::ParsedFile {
    mct_corpus::parse_source(&RustParser, "src/lib.rs", "Rust", src)
}

#[test]
fn indexes_nested_bindings_macros_and_attributes() {
    let parsed = parse(
        r#"
        macro_rules! audit_macro { ($x:expr) => { $x } }
        #[derive(Clone)]
        struct Widget;
        trait Parent<T> {}
        trait Child: crate::Parent<u8> {}
        fn outer(input: i32) {
            fn nested() {}
            let local_value = input;
            let assigned = |x: i32| nested();
            audit_macro!(local_value);
        }
    "#,
    );
    for (name, kind, parent) in [
        ("nested", SymbolKind::Function, Some("outer")),
        ("input", SymbolKind::Variable, Some("outer")),
        ("local_value", SymbolKind::Variable, Some("outer")),
        ("assigned", SymbolKind::Function, Some("outer")),
        ("audit_macro", SymbolKind::Function, None),
    ] {
        assert!(
            parsed
                .symbols
                .iter()
                .any(|s| s.name == name && s.kind == kind && s.parent.as_deref() == parent),
            "missing {name}: {:?}",
            parsed.symbols
        );
    }
    for (from, kind, target) in [
        ("outer", RelationKind::Calls, "audit_macro"),
        ("assigned", RelationKind::Calls, "nested"),
        ("Widget", RelationKind::References, "Clone"),
        ("Child", RelationKind::Extends, "Parent"),
    ] {
        let owner = parsed
            .symbols
            .iter()
            .find(|s| s.name == from)
            .expect("owner");
        assert!(
            parsed
                .relations
                .iter()
                .any(|r| r.from == owner.id && r.kind == kind && r.to_name == target),
            "missing {from} -> {target}: {:?}",
            parsed.relations
        );
    }
}

#[test]
fn destructuring_indexes_bindings_without_indexing_pattern_constructors() {
    let parsed = parse(
        r#"
        fn outer(input: Option<(i32, i32)>) {
            let Some((first, second)) = input else { return; };
            let call = |arg| arg + first;
        }
    "#,
    );
    for (name, parent) in [
        ("input", "outer"),
        ("first", "outer"),
        ("second", "outer"),
        ("arg", "call"),
    ] {
        assert!(
            parsed.symbols.iter().any(|s| s.name == name
                && s.kind == SymbolKind::Variable
                && s.parent.as_deref() == Some(parent)),
            "missing {name}"
        );
    }
    assert!(!parsed.symbols.iter().any(|s| s.name == "Some"));
}

#[test]
fn attribute_references_keep_terminal_names_and_ignore_configuration() {
    let parsed = parse(
        r#"
        #[allow(dead_code)]
        #[custom::mark]
        #[derive(Clone, custom::Copy)]
        struct Widget;
    "#,
    );
    let widget = parsed.symbols.iter().find(|s| s.name == "Widget").unwrap();
    let names: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == widget.id && r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(names, ["mark", "Clone", "Copy"]);
}

#[test]
fn indexes_rust_data_declarations_and_supertraits() {
    let parsed = parse(
        r#"
        struct Inner;
        struct Widget { value: i32, inner: Inner }
        union RawValue { integer: i32, float: f32 }
        enum Color { Red, Rgb(u8, u8, u8) }
        struct Pair(Inner, i32);
        static COUNT: i32 = 1;
        const LIMIT: i32 = 2;
        trait Parent {}
        trait Child: Parent {}
    "#,
    );
    for (name, kind, parent) in [
        ("RawValue", SymbolKind::Struct, None),
        ("value", SymbolKind::Field, Some("Widget")),
        ("inner", SymbolKind::Field, Some("Widget")),
        ("integer", SymbolKind::Field, Some("RawValue")),
        ("Red", SymbolKind::Constant, Some("Color")),
        ("Rgb", SymbolKind::Constant, Some("Color")),
        ("0", SymbolKind::Field, Some("Pair")),
        ("1", SymbolKind::Field, Some("Pair")),
        ("COUNT", SymbolKind::Variable, None),
        ("LIMIT", SymbolKind::Constant, None),
    ] {
        assert!(
            parsed
                .symbols
                .iter()
                .any(|s| s.name == name && s.kind == kind && s.parent.as_deref() == parent),
            "missing {name}"
        );
    }
    let child = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Child")
        .expect("trait");
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.from == child.id && r.kind == RelationKind::Extends && r.to_name == "Parent"));
    let inner = parsed
        .symbols
        .iter()
        .find(|s| s.name == "inner")
        .expect("field");
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.from == inner.id && r.kind == RelationKind::References && r.to_name == "Inner"));
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
    mct_corpus::assert_imports(&parsed, &["HashMap", "Debug", "Display"]);
}

#[test]
fn function_end_line_spans_the_whole_multiline_body() {
    let parsed = parse("fn multiline() -> i32 {\n    let x = 1;\n    let y = 2;\n    x + y\n}\n");
    mct_corpus::assert_symbol_span(&parsed, "multiline", 1, 5);
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

/// [`references`] without type references: the tests below are about
/// functions used as values, and every type they mention is capitalized.
fn value_references(parsed: &mct_core::ParsedFile) -> Vec<&str> {
    references(parsed)
        .into_iter()
        .filter(|name| !name.starts_with(|c: char| c.is_ascii_uppercase()))
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
    assert_eq!(value_references(&parsed), vec!["symbol_kind_str"]);
    assert!(!parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to_name == "symbol_kind_str"));
}

#[test]
fn a_method_call_inside_a_macro_token_tree_leaves_a_relation() {
    // The shape of the former CLI/MCP-server registry parity test.
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
fn a_path_in_value_position_is_a_reference_qualified_by_its_module() {
    // The shape of `mct_languages::build_registry` (errors/REGISTRY.md
    // ERR-010). A value use gets the same `module` evidence as a call through
    // the same path (`m::f()`), so it never resolves to a same-named item of
    // this file; the call itself stays a single `Calls`.
    let parsed = parse(
        r#"
        fn build(registry: &mut R) {
            registry.register(Arc::new(mct_lang_rust::RustParser));
            other::helper();
        }
        "#,
    );
    assert_eq!(references(&parsed), vec!["R", "Arc", "RustParser"]);
    let hit = parsed
        .relations
        .iter()
        .enumerate()
        .find(|(_, r)| r.kind == RelationKind::References && r.to_name == "RustParser");
    let (index, relation) = hit.unwrap_or_else(|| panic!("no reference: {:?}", parsed.relations));
    assert_eq!(relation.location.line, 3);
    let target = parsed.relation_targets.iter().find(|t| t.relation == index);
    assert_eq!(
        target.and_then(|t| t.module.as_deref()),
        Some("mct_lang_rust")
    );
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
        fn by_label() { 'helper: loop { break 'helper; } }
        fn foreign() { let _f = not_in_this_file; println!("{}", also_not(1)); }
        fn in_macro(s: &S) { println!("{}", s.helper.0); }
        #[cfg(helper)]
        fn attributed() {}
        "#,
    );
    assert!(
        value_references(&parsed).is_empty(),
        "{:?}",
        value_references(&parsed)
    );
}

#[test]
fn a_path_to_another_module_never_names_this_files_item() {
    let parsed = parse("fn helper() {}\nfn by_path() { let _f = other::helper; }\n");
    let module: Vec<_> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let t = parsed.relation_targets.iter().find(|t| t.relation == i);
            (r.to_name.as_str(), t.and_then(|t| t.module.as_deref()))
        })
        .collect();
    assert_eq!(module, vec![("helper", Some("other"))]);
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
    assert_eq!(value_references(&parsed), vec!["g"]);
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
    assert!(
        value_references(&parsed).is_empty(),
        "{:?}",
        value_references(&parsed)
    );
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

/// Names of the `References` relations `from` (a symbol name) records.
fn references_from<'a>(parsed: &'a mct_core::ParsedFile, from: &str) -> Vec<&'a str> {
    let ids: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.name == from)
        .map(|s| s.id)
        .collect();
    parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References && ids.contains(&r.from))
        .map(|r| r.to_name.as_str())
        .collect()
}

#[test]
fn types_used_only_in_type_positions_are_referenced() {
    let parsed = parse(
        r#"
        enum Dimension { Kind }
        struct Request { post: bool }
        struct Assessor;
        enum Unavailable { Missing }
        type Alias = Holder;
        const LIMIT: Limit = 0;
        fn validate(skip: Option<Dimension>) -> Result<Request, Unavailable> {
            let _kind: Kind = Dimension::Kind;
            let _assessor = Assessor::new();
            if skip.is_none() { return Err(Unavailable::Missing); }
            Ok(Request { post: true })
        }
    "#,
    );
    let refs = references_from(&parsed, "validate");
    for name in [
        "Option",
        "Dimension",
        "Result",
        "Request",
        "Unavailable",
        "Kind",
        "Assessor",
    ] {
        assert!(
            refs.contains(&name),
            "{name} not referenced by validate: {refs:?}"
        );
    }
    assert!(references_from(&parsed, "Alias").contains(&"Holder"));
    assert!(references_from(&parsed, "LIMIT").contains(&"Limit"));
}

#[test]
fn type_parameters_self_and_module_segments_are_not_type_references() {
    let parsed = parse(
        r#"
        struct Wrapper<T> { inner: T }
        impl<U: Clone> Wrapper<U> {
            fn make(value: U) -> Self { Self { inner: value } }
        }
        fn pick<F: Fn()>(f: F) -> std::io::Result<()> { f(); Ok(()) }
    "#,
    );
    let all: Vec<&str> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References)
        .map(|r| r.to_name.as_str())
        .collect();
    for name in ["T", "U", "F", "Self", "std", "io"] {
        assert!(
            !all.contains(&name),
            "{name} recorded as a reference: {all:?}"
        );
    }
    assert!(references_from(&parsed, "pick").contains(&"Result"));
}

#[test]
fn a_let_closure_used_as_a_value_is_referenced() {
    let parsed = parse(
        r#"
        fn list(items: Vec<u8>) -> usize {
            let eligible = |item: &&u8| **item > 0;
            items.iter().filter(eligible).count()
        }
        fn other(eligible: u8) -> u8 { eligible }
    "#,
    );
    assert!(references_from(&parsed, "list").contains(&"eligible"));
    // A parameter of another function with the same name is that local.
    assert!(!references_from(&parsed, "other").contains(&"eligible"));
}

#[test]
fn path_calls_and_macros_inside_a_macro_token_tree_are_recorded() {
    // The shapes of `format::semantic_hits(..)` inside `vec![..]` and
    // `shadowed_module_predicate!()` inside another macro's arguments.
    let parsed = parse(
        r#"
        macro_rules! predicate { () => { "x" } }
        fn render(hits: &[u8]) -> Vec<String> {
            let _sql = concat!("a", predicate!(), "b");
            vec![format::semantic_hits(hits), Error::Validation(1).code()]
        }
    "#,
    );
    let hit = parsed
        .relations
        .iter()
        .enumerate()
        .find(|(_, r)| r.kind == RelationKind::References && r.to_name == "semantic_hits");
    let (index, _) = hit.unwrap_or_else(|| panic!("no reference: {:?}", parsed.relations));
    let target = parsed.relation_targets.iter().find(|t| t.relation == index);
    assert_eq!(target.and_then(|t| t.module.as_deref()), Some("format"));
    assert!(
        references(&parsed).contains(&"Validation"),
        "{:?}",
        references(&parsed)
    );
    assert!(parsed
        .relations
        .iter()
        .any(|r| r.kind == RelationKind::Calls && r.to_name == "predicate"));
}

#[test]
fn a_path_call_inside_a_macro_keeps_its_module_evidence() {
    // `other::helper()` inside `println!` must never name this file's
    // `helper`: it gets the `module` evidence a call outside a macro gets.
    let parsed = parse("fn helper() {}\nfn f() { println!(\"{}\", other::helper()); }\n");
    let module: Vec<_> = parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::References)
        .map(|(i, r)| {
            let t = parsed.relation_targets.iter().find(|t| t.relation == i);
            (r.to_name.as_str(), t.and_then(|t| t.module.as_deref()))
        })
        .collect();
    assert_eq!(module, vec![("helper", Some("other"))]);
}

#[test]
fn a_macro_named_through_a_path_is_a_call_with_module_evidence() {
    let parsed = parse("fn t() {}\nmct_corpus::standard_tests!(Parser);\n");
    let hit = parsed
        .relations
        .iter()
        .enumerate()
        .find(|(_, r)| r.kind == RelationKind::Calls && r.to_name == "standard_tests");
    let (index, _) = hit.unwrap_or_else(|| panic!("no call: {:?}", parsed.relations));
    let target = parsed.relation_targets.iter().find(|t| t.relation == index);
    assert_eq!(target.and_then(|t| t.module.as_deref()), Some("mct_corpus"));
}

#[test]
fn a_unit_struct_used_as_a_value_is_referenced() {
    let parsed = parse(
        r#"
        struct Broken;
        struct Named { x: u8 }
        fn use_it() { let _v = build(&Broken); }
    "#,
    );
    assert!(references_from(&parsed, "use_it").contains(&"Broken"));
}

#[test]
fn an_imported_function_called_inside_a_macro_is_a_call() {
    let parsed = parse(
        r#"
        use other::helper;
        use other::Wrapper as W;
        fn run(local: i32) {
            println!("{}", helper(local));
            println!("{}", unrelated(local));
            println!("{}", x.helper(local));
            println!("{}", m::helper(local));
        }
    "#,
    );
    let run = parsed
        .symbols
        .iter()
        .find(|s| s.name == "run")
        .expect("run");
    let calls: Vec<(&str, u32)> = parsed
        .relations
        .iter()
        .filter(|r| r.from == run.id && r.kind == RelationKind::Calls)
        .map(|r| (r.to_name.as_str(), r.location.line))
        .collect();
    // Only the bare, imported `helper(..)` is a call: `unrelated` is not
    // imported, and `x.helper` / `m::helper` are an accessor and a path.
    assert_eq!(calls, vec![("helper", 5)], "{:?}", parsed.relations);
}

#[test]
fn a_local_shadowing_an_import_is_not_a_macro_call() {
    let parsed = parse(
        r#"
        use other::helper;
        fn run() {
            let helper = |x: i32| x;
            println!("{}", helper(1));
        }
    "#,
    );
    assert!(
        !parsed
            .relations
            .iter()
            .any(|r| r.kind == RelationKind::Calls
                && r.to_name == "helper"
                && r.location.line == 5),
        "{:?}",
        parsed.relations
    );
}

#[test]
fn loop_conditional_and_match_bindings_are_variables_of_their_function() {
    let parsed = parse(
        r#"
        enum Color { Red, Blue }
        const LIMIT: i32 = 3;
        fn run(items: Vec<i32>, c: Color) {
            for item in items.iter() {}
            for (i, v) in items.iter().enumerate() {}
            while let Some(top) = items.first() {}
            if let Some(found) = items.get(0) {}
            match items.len() {
                n if n > 1 => {}
                other @ 0 => {}
                LIMIT => {}
                _ => {}
            }
            match c { Color::Red => {} Blue => {} }
        }
    "#,
    );
    let variables: Vec<(&str, Option<&str>)> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| (s.name.as_str(), s.parent.as_deref()))
        .collect();
    for name in ["item", "i", "v", "top", "found", "n", "other"] {
        assert_eq!(
            variables.iter().filter(|(n, _)| *n == name).count(),
            1,
            "{name}: {variables:?}"
        );
        assert!(variables.contains(&(name, Some("run"))), "{name}");
    }
    // A pattern constructor, a constant and a unit variant bind nothing.
    for name in ["Some", "LIMIT", "Blue", "Red"] {
        assert!(!variables.iter().any(|(n, _)| *n == name), "{name}");
    }
}
