// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-php/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, ParsedFile, RelationKind, SourceFile, SymbolKind};
use mct_lang_php::PhpParser;

fn parse(src: &str) -> ParsedFile {
    PhpParser
        .parse(&SourceFile {
            relative_path: "fixture.php".to_string(),
            contents: src.to_string(),
        })
        .expect("valid php source should parse")
}

#[test]
fn extracts_class_extends_and_implements() {
    let parsed = parse(
        "<?php\ninterface Payable { public function pay(): bool; }\nclass Invoice extends Base implements Payable {}\n",
    );
    let invoice = parsed.symbols.iter().find(|s| s.name == "Invoice").unwrap();
    assert_eq!(invoice.kind, SymbolKind::Class);

    let extends: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Extends)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(extends.contains(&"Base"));

    let implements: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(implements.contains(&"Payable"));
}

#[test]
fn methods_and_fields_are_parented_to_their_class() {
    let parsed = parse("<?php\nclass Invoice {\n  public float $amount;\n  public function pay(): bool { return true; }\n}\n");
    let pay = parsed.symbols.iter().find(|s| s.name == "pay").unwrap();
    assert_eq!(pay.kind, SymbolKind::Method);
    assert_eq!(pay.parent.as_deref(), Some("Invoice"));

    let amount = parsed.symbols.iter().find(|s| s.name == "amount").unwrap();
    assert_eq!(amount.kind, SymbolKind::Field);
    assert_eq!(amount.parent.as_deref(), Some("Invoice"));
}

#[test]
fn constructor_property_promotion_is_indexed_as_a_field() {
    let parsed = parse("<?php\nclass Invoice {\n  public function __construct(private readonly string $id, public int $amount = 0) {}\n}\n");
    let id_field = parsed.symbols.iter().find(|s| s.name == "id").unwrap();
    assert_eq!(id_field.kind, SymbolKind::Field);
    assert_eq!(id_field.parent.as_deref(), Some("Invoice"));

    let amount_field = parsed.symbols.iter().find(|s| s.name == "amount").unwrap();
    assert_eq!(amount_field.kind, SymbolKind::Field);
    assert_eq!(amount_field.parent.as_deref(), Some("Invoice"));
}

#[test]
fn trait_use_is_recorded_as_implements_on_the_class() {
    let parsed = parse(
        "<?php\ntrait Loggable { public function log(string $m): void {} }\nclass Invoice {\n  use Loggable;\n  public function noop(): void {}\n}\n",
    );
    let loggable = parsed
        .symbols
        .iter()
        .find(|s| s.name == "Loggable")
        .unwrap();
    assert_eq!(loggable.kind, SymbolKind::Trait);

    let invoice = parsed.symbols.iter().find(|s| s.name == "Invoice").unwrap();
    let uses: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == invoice.id && r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(uses.contains(&"Loggable"));
}

#[test]
fn trait_use_with_adaptation_block_does_not_leak_adaptation_names() {
    let parsed = parse(
        "<?php\ntrait A { public function foo(): void {} }\ntrait B { public function foo(): void {} }\nclass C {\n  use A, B {\n    A::foo insteadof B;\n    B::foo as protected bar;\n  }\n}\n",
    );
    let c = parsed.symbols.iter().find(|s| s.name == "C").unwrap();
    let uses: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.from == c.id && r.kind == RelationKind::Implements)
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(
        uses.len(),
        2,
        "only A and B, not names from the insteadof/as adaptation block: {uses:?}"
    );
    assert!(uses.contains(&"A"));
    assert!(uses.contains(&"B"));
}

#[test]
fn scoped_and_member_calls_are_all_recorded() {
    let parsed = parse(
        "<?php\nclass Invoice extends Base {\n  public function process(): void {\n    $this->log('x');\n    parent::process();\n    self::validate();\n    static::hook();\n    Invoice::create();\n    helper();\n  }\n}\n",
    );
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    for expected in ["log", "process", "validate", "hook", "create", "helper"] {
        assert!(
            calls.contains(&expected),
            "missing call to {expected}: {calls:?}"
        );
    }
}

#[test]
fn enum_case_and_backed_enum_methods_are_indexed() {
    let parsed = parse(
        "<?php\nenum Suit: string {\n  case Hearts = 'H';\n  public function label(): string { return 'x'; }\n}\n",
    );
    let hearts = parsed.symbols.iter().find(|s| s.name == "Hearts").unwrap();
    assert_eq!(hearts.kind, SymbolKind::Field);
    assert_eq!(hearts.parent.as_deref(), Some("Suit"));

    let label = parsed.symbols.iter().find(|s| s.name == "label").unwrap();
    assert_eq!(label.kind, SymbolKind::Method);
    assert_eq!(label.parent.as_deref(), Some("Suit"));
}

#[test]
fn top_level_and_class_constants_are_indexed() {
    let parsed = parse("<?php\nconst LIMIT = 100;\nclass Invoice {\n  const VERSION = '1.0';\n}\n");
    let limit = parsed.symbols.iter().find(|s| s.name == "LIMIT").unwrap();
    assert_eq!(limit.kind, SymbolKind::Constant);

    let version = parsed.symbols.iter().find(|s| s.name == "VERSION").unwrap();
    assert_eq!(version.kind, SymbolKind::Constant);
    assert_eq!(version.parent.as_deref(), Some("Invoice"));
}

#[test]
fn anonymous_and_arrow_functions_assigned_to_a_variable_are_indexed() {
    let parsed = parse("<?php\n$formatter = function (float $x): string { return number_format($x, 2); };\n$adder = fn($a, $b) => $a + $b;\n");
    let formatter = parsed
        .symbols
        .iter()
        .find(|s| s.name == "formatter")
        .unwrap();
    assert_eq!(formatter.kind, SymbolKind::Function);

    let adder = parsed.symbols.iter().find(|s| s.name == "adder").unwrap();
    assert_eq!(adder.kind, SymbolKind::Function);

    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(
        calls.contains(&"number_format"),
        "call inside the closure body must still be recorded: {calls:?}"
    );
}

#[test]
fn use_and_require_are_imports() {
    let parsed = parse(
        "<?php\nnamespace App;\nuse App\\Models\\User;\nuse App\\Models\\{Order, Invoice as Inv};\nrequire_once 'bootstrap.php';\nrequire __DIR__ . '/computed.php';\n",
    );
    let imports: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Imports)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(imports.contains(&"User"));
    assert!(imports.contains(&"Order"));
    assert!(
        imports.contains(&"Invoice") && !imports.contains(&"Inv"),
        "an aliased import names the imported class, not the file-local alias"
    );
    assert!(imports.contains(&"bootstrap.php"));
    assert!(
        !imports.contains(&"__DIR__"),
        "a computed require path has nothing static to record"
    );
}

#[test]
fn define_call_is_an_ordinary_call_not_a_constant_symbol() {
    let parsed = parse("<?php\ndefine('LEGACY_FLAG', true);\n");
    assert!(
        parsed.symbols.iter().all(|s| s.name != "LEGACY_FLAG"),
        "define() arguments must not synthesize a symbol"
    );
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"define"));
}

#[test]
fn syntax_error_is_reported_not_panicked() {
    let result = PhpParser.parse(&SourceFile {
        relative_path: "broken.php".to_string(),
        contents: "<?php\nclass Foo {\n".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}

// Regression test for a real crash: CI's fuzz-smoke job found a native
// stack-overflow (AddressSanitizer) from adversarially deep nesting, since
// the Walker's visit/visit_children recursion had no depth limit. This
// input is exactly the shape that triggered it — thousands of nested
// parenthesized expressions — and must return `Ok` (truncated past
// `MAX_TRAVERSAL_DEPTH`, never crashing the process) rather than panic.
#[test]
fn deeply_nested_expression_does_not_stack_overflow() {
    let nesting = 5_000;
    let mut source = String::from("<?php\n$x = ");
    source.push_str(&"(".repeat(nesting));
    source.push('1');
    source.push_str(&")".repeat(nesting));
    source.push_str(";\n");

    let result = PhpParser.parse(&SourceFile {
        relative_path: "deep.php".to_string(),
        contents: source,
    });
    assert!(
        result.is_ok(),
        "deeply nested input must not crash the parser, even if truncated"
    );
}

// --- Namespace-aware evidence -------------------------------------------

use mct_core::RelationTarget;

/// The `RelationTarget` attached to the first relation of `kind` to `to`.
fn target_of<'a>(
    parsed: &'a ParsedFile,
    kind: RelationKind,
    to: &str,
) -> Option<&'a RelationTarget> {
    let index = parsed
        .relations
        .iter()
        .position(|r| r.kind == kind && r.to_name == to)?;
    parsed.relation_targets.iter().find(|t| t.relation == index)
}

fn module_aliases(parsed: &ParsedFile) -> Vec<&str> {
    parsed
        .symbol_aliases
        .iter()
        .map(|a| a.name.as_str())
        .collect()
}

#[test]
fn the_module_answers_to_its_namespaces_and_declared_types() {
    let parsed = parse("<?php\nnamespace App\\Models;\nclass User {}\ninterface Owned {}\n");
    let aliases = module_aliases(&parsed);
    assert!(aliases.contains(&"\\App\\Models"), "{aliases:?}");
    assert!(
        aliases.contains(&"User") && aliases.contains(&"Owned"),
        "{aliases:?}"
    );
    assert!(parsed.symbol_aliases.iter().all(|a| a.symbol == 0));
}

#[test]
fn a_file_without_a_namespace_is_in_the_global_one() {
    let parsed = parse("<?php\nclass Plain {}\n");
    assert!(module_aliases(&parsed).contains(&"\\"));
}

#[test]
fn a_braced_namespace_block_ends_with_its_brace() {
    let parsed = parse(
        "<?php\nnamespace A { class One extends Base {} }\nnamespace { class Two extends Base {} }\n",
    );
    let aliases = module_aliases(&parsed);
    assert!(
        aliases.contains(&"\\A") && aliases.contains(&"\\"),
        "{aliases:?}"
    );
    let bases: Vec<_> = parsed
        .relation_targets
        .iter()
        .filter_map(|t| t.target_module.as_deref())
        .collect();
    assert_eq!(bases, ["\\A", "\\"]);
}

#[test]
fn extends_and_implements_carry_the_namespace_they_resolve_to() {
    let parsed = parse(
        "<?php\nnamespace App\\Http;\nuse Illuminate\\Routing\\Controller as Base;\nclass C extends Base implements \\Countable, Contracts\\Thing {}\n",
    );
    let namespace = |kind, to| {
        target_of(&parsed, kind, to)
            .and_then(|t| t.target_module.as_deref())
            .map(str::to_string)
    };
    // The alias resolves to the imported class, under its real name.
    assert_eq!(
        namespace(RelationKind::Extends, "Controller").as_deref(),
        Some("\\Illuminate\\Routing")
    );
    assert_eq!(
        namespace(RelationKind::Implements, "Countable").as_deref(),
        Some("\\")
    );
    assert_eq!(
        namespace(RelationKind::Implements, "Thing").as_deref(),
        Some("\\App\\Http\\Contracts")
    );
}

#[test]
fn type_declarations_are_references_from_the_declaring_function() {
    let parsed = parse(
        "<?php\nnamespace App;\nuse App\\Models\\Order;\nclass S {\n  private ?Customer $c;\n  public function f(Order|int $o, self $s, string $n): Receipt { try {} catch (Boom | \\RuntimeException $e) {} }\n}\n",
    );
    let references = |to: &str| {
        let owner = parsed
            .relations
            .iter()
            .find(|r| r.kind == RelationKind::References && r.to_name == to)
            .map(|r| r.from);
        owner
            .and_then(|id| parsed.symbols.iter().find(|s| s.id == id))
            .map(|s| s.name.as_str())
    };
    assert_eq!(references("Order"), Some("f"));
    assert_eq!(references("Receipt"), Some("f"));
    assert_eq!(references("Boom"), Some("f"));
    assert_eq!(references("RuntimeException"), Some("f"));
    // A property type belongs to the class.
    assert_eq!(references("Customer"), Some("S"));
    // Built-in and relative types name no declaration.
    for builtin in ["int", "string", "self"] {
        assert_eq!(references(builtin), None, "{builtin}");
    }
    assert_eq!(
        target_of(&parsed, RelationKind::References, "Customer")
            .and_then(|t| t.target_module.as_deref()),
        Some("\\App")
    );
}

#[test]
fn class_constants_instanceof_and_static_access_reference_the_class() {
    let parsed = parse(
        "<?php\nnamespace App;\nuse Lib\\Post;\nfunction f($x) { $a = Post::class; $b = Post::STATUS; $c = Post::$registry; return $x instanceof Post; }\n",
    );
    let count = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::References && r.to_name == "Post")
        .count();
    assert_eq!(count, 4);
    assert!(parsed
        .relation_targets
        .iter()
        .all(|t| t.target_module.as_deref() == Some("\\Lib")));
}

#[test]
fn a_static_call_is_qualified_by_the_class_it_names() {
    let parsed = parse(
        "<?php\nnamespace App;\nuse Lib\\Post;\nclass S extends Base {\n  function f() { Post::find(1); self::a(); static::b(); parent::c(); $k::d(); }\n}\n",
    );
    let qualifier = |to: &str| {
        target_of(&parsed, RelationKind::Calls, to)
            .map(|t| (t.qualifier.clone(), t.target_module.clone(), t.member))
    };
    assert_eq!(
        qualifier("find"),
        Some((Some("Post".into()), Some("\\Lib".into()), false))
    );
    assert_eq!(
        qualifier("a"),
        Some((Some("S".into()), Some("\\App".into()), false))
    );
    assert_eq!(qualifier("b"), qualifier("a"));
    assert_eq!(
        qualifier("c"),
        Some((Some("Base".into()), Some("\\App".into()), false))
    );
    // `$k::d()`: the class is a runtime value, so only "some method".
    assert_eq!(qualifier("d"), Some((None, None, true)));
}

#[test]
fn this_is_qualified_in_a_class_but_not_in_a_trait_or_anonymous_class() {
    let parsed = parse(
        "<?php\nclass K { function f() { $this->own(); $o->other(); } }\ntrait T { function g() { $this->mixed(); } }\nfunction h() { return new class { function i() { $this->anon(); } }; }\n",
    );
    let target = |to| target_of(&parsed, RelationKind::Calls, to).unwrap();
    assert_eq!(target("own").qualifier.as_deref(), Some("K"));
    assert!(!target("own").member);
    // Another receiver's class is unknown.
    assert!(target("other").member && target("other").qualifier.is_none());
    // A trait's `$this` is whichever class uses it; an anonymous class has no name.
    assert!(target("mixed").member && target("mixed").qualifier.is_none());
    assert!(target("anon").member && target("anon").qualifier.is_none());
}

#[test]
fn a_function_call_is_qualified_only_when_its_namespace_is_known() {
    let parsed = parse(
        "<?php\nnamespace App;\nuse function Lib\\helper;\nfunction f() { helper(); strlen('x'); \\Other\\run(); $dynamic(); }\n",
    );
    let namespace =
        |to| target_of(&parsed, RelationKind::Calls, to).and_then(|t| t.target_module.clone());
    assert_eq!(namespace("helper").as_deref(), Some("\\Lib"));
    assert_eq!(namespace("run").as_deref(), Some("\\Other"));
    // `strlen` may be `App\strlen` or the global one: nothing is claimed.
    assert_eq!(namespace("strlen"), None);
    // `$dynamic()` has no static name.
    assert!(parsed.relations.iter().all(|r| r.to_name != "dynamic"));
}

#[test]
fn dynamic_class_names_record_no_relation() {
    let parsed = parse(
        "<?php\nfunction f($class, $m) { new $class(); $class::make(); $o->$m(); new static; new self; }\n",
    );
    let names: Vec<_> = parsed
        .relations
        .iter()
        .map(|r| r.to_name.as_str())
        .collect();
    assert_eq!(names, ["make"], "{names:?}");
}

// --- Laravel routes -------------------------------------------------------

fn calls_with_qualifier(parsed: &ParsedFile) -> Vec<(String, Option<String>)> {
    parsed
        .relations
        .iter()
        .enumerate()
        .filter(|(_, r)| r.kind == RelationKind::Calls)
        .map(|(i, r)| {
            let qualifier = parsed
                .relation_targets
                .iter()
                .find(|t| t.relation == i)
                .and_then(|t| t.qualifier.clone());
            (r.to_name.clone(), qualifier)
        })
        // The builder calls themselves, not the actions they route to.
        .filter(|(name, _)| {
            !matches!(
                name.as_str(),
                "resource"
                    | "apiResource"
                    | "singleton"
                    | "apiSingleton"
                    | "softDeletableResources"
                    | "creatable"
                    | "destroyable"
                    | "only"
                    | "except"
                    | "controller"
                    | "group"
                    | "prefix"
            )
        })
        .collect()
}

const ROUTE_HEAD: &str = "<?php\nuse App\\Http\\Controllers\\PhotoController as Photos;\nuse Illuminate\\Support\\Facades\\Route;\n";

#[test]
fn a_resource_route_lists_its_conventional_methods() {
    let parsed = parse(&format!("{ROUTE_HEAD}Route::resource('p', Photos::class);\nRoute::apiResource('q', Photos::class)->except('destroy');\n"));
    let calls = calls_with_qualifier(&parsed);
    let methods = |count: std::ops::Range<usize>| -> Vec<&str> {
        calls[count].iter().map(|(m, _)| m.as_str()).collect()
    };
    assert_eq!(
        methods(0..7),
        ["index", "create", "store", "show", "edit", "update", "destroy"]
    );
    assert_eq!(methods(7..11), ["index", "store", "show", "update"]);
    assert!(calls
        .iter()
        .all(|(_, q)| q.as_deref() == Some("PhotoController")));
}

#[test]
fn a_resource_with_a_computed_method_list_records_no_action() {
    let parsed = parse(&format!("{ROUTE_HEAD}Route::resource('p', Photos::class)->only($methods);\nRoute::resource('p', Photos::class)->only(['a', $b]);\n"));
    assert!(calls_with_qualifier(&parsed).is_empty());
}

#[test]
fn only_the_route_facade_defines_routes() {
    // `Route` in a namespaced file without the facade import is some other class.
    let parsed = parse("<?php\nnamespace App;\nRoute::get('/', [A::class, 'x']);\n");
    assert!(calls_with_qualifier(&parsed)
        .iter()
        .all(|(m, _)| m == "get"));
    // An aliased import of the facade still is the facade.
    let aliased = parse(
        "<?php\nuse Illuminate\\Support\\Facades\\Route as R;\nR::get('/', [A::class, 'x']);\n",
    );
    assert!(calls_with_qualifier(&aliased).contains(&("x".to_string(), Some("A".to_string()))));
}

#[test]
fn string_actions_outside_a_controller_group_are_not_guessed() {
    let parsed = parse("<?php\nRoute::get('/a', 'Legacy@index');\nRoute::get('/b', 'index');\nRoute::get('/c', fn () => 1);\n");
    let gets = vec![("get".to_string(), Some("Route".to_string())); 3];
    assert_eq!(calls_with_qualifier(&parsed), gets);
}

#[test]
fn a_controller_group_binds_string_actions_only_inside_its_closure() {
    let parsed = parse(&format!(
        "{ROUTE_HEAD}Route::prefix('x')->controller(Photos::class)->group(function () {{ Route::get('/a', 'show'); }});\nRoute::get('/b', 'show');\n"
    ));
    let bound: Vec<_> = calls_with_qualifier(&parsed)
        .into_iter()
        .filter(|(m, _)| m == "show")
        .collect();
    assert_eq!(
        bound,
        [("show".to_string(), Some("PhotoController".to_string()))]
    );
}

#[test]
fn a_migration_extending_something_else_keeps_its_members_unindexed() {
    let parsed = parse("<?php\nreturn new class extends Other { public function up() {} };\n");
    assert!(parsed.symbols.iter().all(|s| s.name != "up"));
}

#[test]
fn singleton_resources_route_their_own_methods() {
    let parsed = parse(&format!(
        "{ROUTE_HEAD}Route::singleton('a', Photos::class);\nRoute::apiSingleton('b', Photos::class)->creatable();\nRoute::singleton('c', Photos::class)->destroyable();\nRoute::softDeletableResources(['p' => Photos::class]);\n"
    ));
    let methods: Vec<_> = calls_with_qualifier(&parsed)
        .into_iter()
        .map(|(m, _)| m)
        .collect();
    let expected = [
        &["show", "edit", "update"][..],
        &["show", "update", "store", "destroy"],
        &["show", "edit", "update", "destroy"],
        &[
            "index", "create", "store", "show", "edit", "update", "destroy",
        ],
    ]
    .concat();
    assert_eq!(methods, expected);
}

#[test]
fn calling_a_closure_through_its_variable_is_a_call_to_the_closure() {
    let parsed = parse(
        "<?php\n$format = function ($x) { return $x; };\necho $format(1);\n$param = 'strlen';\necho $param('x');\n",
    );
    let calls: Vec<_> = parsed
        .relations
        .iter()
        .filter(|r| r.kind == RelationKind::Calls)
        .map(|r| r.to_name.as_str())
        .collect();
    assert!(calls.contains(&"format"), "{calls:?}");
    // A variable never bound to a closure is not a call to a symbol of
    // that name: it may hold any callable.
    assert!(!calls.contains(&"param"), "{calls:?}");
}
