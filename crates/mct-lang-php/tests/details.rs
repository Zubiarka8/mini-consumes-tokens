//! Declaration details (ADR-034, issue #321) the PHP parser proves from
//! syntax, or from the PHPDoc right above when the syntax has no type: the
//! header without attributes or body, parameters (variadic, defaults,
//! by-reference), the return type, visibility (public by default for a
//! member) and modifiers.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, Provenance, SymbolDetail, SymbolKind, Visibility};
use mct_lang_php::PhpParser;

const SOURCE: &str = r#"<?php
#[Entity]
abstract class Repo extends Base implements Api {
    #[Deprecated] private static ?int $limit = 3, $max;
    public function __construct(private readonly int $id, string $name = "x") {}
    /**
     * @param int $id the id
     * @return list<K>
     */
    final protected static function load($id = 3, string ...$names): ?array { return null; }
    abstract function run(int &$a): void;
}
interface Api { public function run(int $a): void; }
function top(callable $f) { return 1; }
/** @return array<string, int> */
function counts($xs) { return []; }
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&PhpParser, "src/Repo.php", "PHP", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str, line: u32) -> SymbolDetail {
    let symbol = parsed
        .symbols
        .iter()
        .find(|s| s.name == name && s.location.line == line && s.kind != SymbolKind::Module)
        .unwrap_or_else(|| panic!("no {name} on line {line}"));
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbol.id)
        .cloned()
        .unwrap_or_else(|| panic!("no detail for {name}"))
        .normalized()
}

/// `name:type=default:kind` per parameter.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}={}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?", |t| t.text.as_str()),
                p.default.as_deref().unwrap_or("-"),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_method_takes_a_missing_parameter_type_from_its_phpdoc() {
    let parsed = parsed();
    let load = detail(&parsed, "load", 10);
    assert_eq!(
        load.signature.as_deref(),
        Some("final protected static function load($id = 3, string ...$names): ?array")
    );
    assert_eq!(load.visibility, Some(Visibility::Protected));
    assert_eq!(load.modifiers, [Modifier::Final, Modifier::Static]);
    assert_eq!(
        params(&load),
        ["id:int=3:positional", "names:string=-:variadic"]
    );
    let provenance: Vec<_> = load
        .parameters
        .iter()
        .map(|p| p.type_text.as_ref().map(|t| t.provenance))
        .collect();
    assert_eq!(
        provenance,
        [Some(Provenance::Doc), Some(Provenance::Syntax)]
    );
    // The written return type wins over `@return`.
    let returns = load.return_type.unwrap();
    assert_eq!(
        (returns.text.as_str(), returns.provenance),
        ("?array", Provenance::Syntax)
    );
}

#[test]
fn a_function_without_a_written_return_type_takes_the_phpdoc_one() {
    let parsed = parsed();
    let counts = detail(&parsed, "counts", 16);
    let returns = counts.return_type.clone().unwrap();
    assert_eq!(
        (returns.text.as_str(), returns.provenance),
        ("array<string, int>", Provenance::Doc)
    );
    assert_eq!(params(&counts), ["xs:?=-:positional"]);
    let top = detail(&parsed, "top", 14);
    assert_eq!(top.signature.as_deref(), Some("function top(callable $f)"));
    assert_eq!(top.visibility, None);
    assert!(top.return_type.is_none());
}

#[test]
fn a_member_without_a_visibility_keyword_is_public() {
    let parsed = parsed();
    let run = detail(&parsed, "run", 11);
    assert_eq!(
        run.signature.as_deref(),
        Some("abstract function run(int &$a): void")
    );
    assert_eq!(run.visibility, Some(Visibility::Public));
    assert_eq!(run.modifiers, [Modifier::Abstract]);
    assert_eq!(params(&run), ["a:int=-:positional"]);
}

#[test]
fn types_properties_and_promoted_parameters_get_their_header() {
    let parsed = parsed();
    let repo = detail(&parsed, "Repo", 2);
    assert_eq!(
        repo.signature.as_deref(),
        Some("abstract class Repo extends Base implements Api")
    );
    assert_eq!(repo.modifiers, [Modifier::Abstract]);
    assert_eq!(repo.visibility, None);
    for (name, signature) in [
        ("limit", "private static ?int $limit"),
        ("max", "private static ?int $max"),
    ] {
        let property = detail(&parsed, name, 4);
        assert_eq!(property.signature.as_deref(), Some(signature));
        assert_eq!(property.visibility, Some(Visibility::Private));
        assert_eq!(property.modifiers, [Modifier::Static]);
    }
    let constructor = detail(&parsed, "__construct", 5);
    assert_eq!(
        params(&constructor),
        ["id:int=-:positional", "name:string=\"x\":positional"]
    );
    let id = detail(&parsed, "id", 5);
    assert_eq!(id.signature.as_deref(), Some("private readonly int $id"));
    assert_eq!(id.visibility, Some(Visibility::Private));
    assert_eq!(id.modifiers, [Modifier::Readonly]);
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-php-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Repo.php"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    let load = index
        .symbol_details("src/Repo.php", 10, "load")
        .unwrap()
        .unwrap();
    assert_eq!(load.parameters[0].type_source.as_deref(), Some("doc"));
    assert_eq!(load.parameters.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}

const MODERN_SOURCE: &str = r#"<?php
namespace Audit;
interface Reader {}
interface Writer {}
class Fallback { public string $code { set => strtolower($value); } }
class Settings {
    final public const Reader|Fallback|null OPTIONS = null;
    public string $label {
        get => formatLabel($this->label);
        set (string|\Stringable $input) { $normalized = trim((string) $input); $this->label = $normalized; }
    }
    public function __construct(public ?Fallback &$fallback, public string $title { get => strtoupper($this->title); }) {}
    public function collect(#[Input] (Reader&Writer)|Fallback &$input, string &...$tags): Reader|Fallback {
        return $input;
    }
}
$render = static function (?Fallback $record = null) use ($context): Fallback {
    $output = $record ?? new Fallback();
    return $output;
};
$pick = fn (Fallback $record): Fallback => $record;
function bindings(array $rows, string $selector): void {
    foreach ($rows as $key => ['name' => $name, 'details' => [$city, &$zip]]) {}
    foreach ($rows as &$row) {}
    foreach ($rows as [$left, $right]) {}
    list($first, $second) = $rows;
    [$keyExpression => $selected, 'ignored' => $object->value, 'offset' => $array[$index]] = $rows;
    array_map(function ($anonymous) { $hidden = 1; }, $rows);
}
"#;

fn modern() -> ParsedFile {
    mct_corpus::parse_source(&PhpParser, "src/Modern.php", "PHP", MODERN_SOURCE)
}

#[test]
fn native_composite_types_and_reference_parameters_keep_their_spelling() {
    let parsed = modern();
    let collect = detail(&parsed, "collect", 13);
    assert_eq!(
        params(&collect),
        [
            "input:(Reader&Writer)|Fallback=-:positional",
            "tags:string=-:variadic"
        ]
    );
    assert!(collect.signature.unwrap().contains("string &...$tags"));
    assert_eq!(collect.return_type.unwrap().text, "Reader|Fallback");
    let constructor = detail(&parsed, "__construct", 12);
    assert_eq!(
        params(&constructor),
        [
            "fallback:?Fallback=-:positional",
            "title:string=-:positional"
        ]
    );
    assert_eq!(
        detail(&parsed, "fallback", 12).signature.as_deref(),
        Some("public ?Fallback &$fallback")
    );
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "$fallback" && s.parent.as_deref() == Some("__construct")));
}

#[test]
fn assigned_closures_and_arrows_keep_signatures_parameters_and_returns() {
    let parsed = modern();
    let render = detail(&parsed, "render", 17);
    assert_eq!(params(&render), ["record:?Fallback=null:positional"]);
    assert_eq!(render.return_type.unwrap().text, "Fallback");
    assert_eq!(render.visibility, None);
    assert_eq!(render.modifiers, [Modifier::Static]);
    assert!(render.signature.unwrap().contains("use ($context)"));
    let pick = detail(&parsed, "pick", 21);
    assert_eq!(params(&pick), ["record:Fallback=-:positional"]);
    assert_eq!(pick.return_type.unwrap().text, "Fallback");
    assert_eq!(
        pick.signature.as_deref(),
        Some("fn (Fallback $record): Fallback")
    );
    for (variable, owner) in [
        ("$record", "render"),
        ("$record", "pick"),
        ("$output", "render"),
    ] {
        assert!(parsed
            .symbols
            .iter()
            .any(|s| s.name == variable && s.parent.as_deref() == Some(owner)));
    }
    for owner in ["render", "pick"] {
        let symbol = parsed.symbols.iter().find(|s| s.name == owner).unwrap();
        assert!(parsed.relations.iter().any(|r| r.from == symbol.id
            && r.to_name == "Fallback"
            && r.kind == mct_core::RelationKind::References));
    }
}

#[test]
fn property_hooks_and_typed_constants_have_details_and_relations() {
    let parsed = modern();
    let constant = detail(&parsed, "OPTIONS", 7);
    assert_eq!(
        constant.signature.as_deref(),
        Some("final public const Reader|Fallback|null OPTIONS")
    );
    assert_eq!(constant.visibility, Some(Visibility::Public));
    assert_eq!(constant.modifiers, [Modifier::Final]);
    let get = detail(&parsed, "$label::get", 9);
    assert!(get.parameters_known && get.parameters.is_empty());
    assert_eq!(get.signature.as_deref(), Some("get"));
    assert_eq!(get.return_type.unwrap().text, "string");
    let implicit = detail(&parsed, "$code::set", 5);
    assert!(implicit.parameters_known);
    assert_eq!(params(&implicit), ["value:string=-:positional"]);
    let set = detail(&parsed, "$label::set", 10);
    assert_eq!(params(&set), ["input:string|\\Stringable=-:positional"]);
    assert!(parsed
        .symbols
        .iter()
        .any(|s| s.name == "$normalized" && s.parent.as_deref() == Some("$label::set")));
    assert_eq!(
        detail(&parsed, "$title::get", 12).signature.as_deref(),
        Some("get")
    );
    for (name, target, kind) in [
        ("OPTIONS", "Reader", mct_core::RelationKind::References),
        ("$label::get", "formatLabel", mct_core::RelationKind::Calls),
        (
            "$label::set",
            "Stringable",
            mct_core::RelationKind::References,
        ),
    ] {
        let symbol = parsed.symbols.iter().find(|s| s.name == name).unwrap();
        assert!(
            parsed
                .relations
                .iter()
                .any(|r| r.from == symbol.id && r.to_name == target && r.kind == kind),
            "{name}: {target}"
        );
    }
}

#[test]
fn foreach_and_destructuring_index_only_bindings() {
    let parsed = modern();
    let names: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable && s.parent.as_deref() == Some("bindings"))
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "$rows",
            "$selector",
            "$key",
            "$name",
            "$city",
            "$zip",
            "$row",
            "$left",
            "$right",
            "$first",
            "$second",
            "$selected"
        ]
    );
}

#[test]
fn modern_details_reach_the_rust_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-php-modern-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Modern.php"), MODERN_SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    let inspect = |name, line| {
        index
            .symbol_details("src/Modern.php", line, name)
            .unwrap()
            .unwrap()
    };
    assert_eq!(
        inspect("collect", 13).parameters[0].type_text.as_deref(),
        Some("(Reader&Writer)|Fallback")
    );
    assert_eq!(
        inspect("pick", 21).parameters[0].type_text.as_deref(),
        Some("Fallback")
    );
    assert_eq!(
        inspect("$label::set", 10).parameters[0]
            .type_text
            .as_deref(),
        Some("string|\\Stringable")
    );
    assert_eq!(index.find_symbol("$selected").unwrap().len(), 1);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn modern_visibility_and_clone_arguments_parse() {
    let parsed = mct_corpus::parse_source(
        &PhpParser,
        "src/Cloneable.php",
        "PHP",
        r#"<?php
class Profile {
    public static private(set) string $prefix = '';
    public function __construct(public private(set) string $name) {}
    public function renamed(string $name): self {
        return clone($this, ['name' => $name]);
    }
}
"#,
    );
    let name = detail(&parsed, "name", 4);
    assert_eq!(name.visibility, Some(Visibility::Public));
    assert_eq!(
        name.signature.as_deref(),
        Some("public private(set) string $name")
    );
    let prefix = detail(&parsed, "prefix", 3);
    assert_eq!(prefix.visibility, Some(Visibility::Public));
    assert_eq!(prefix.modifiers, [Modifier::Static]);
    assert_eq!(
        params(&detail(&parsed, "renamed", 5)),
        ["name:string=-:positional"]
    );
}

#[test]
fn php_85_final_promoted_properties_keep_their_details() {
    let source = r#"<?php
class Profile {
    public function __construct(
        #[Name] final string $name = 'guest',
        protected FiNaL readonly ?Reader $reader = null,
        final public private(set) string $label = '' { get => strtoupper($this->label); },
        final $metadata = [],
    ) {}
}
"#;
    let file = "src/Profile.php";
    let parsed = mct_corpus::parse_source(&PhpParser, file, "PHP", source);
    for (name, line, signature, visibility, modifiers) in [
        (
            "name",
            4,
            "final string $name",
            Visibility::Public,
            &[Modifier::Final][..],
        ),
        (
            "reader",
            5,
            "protected FiNaL readonly ?Reader $reader",
            Visibility::Protected,
            &[Modifier::Final, Modifier::Readonly],
        ),
        (
            "label",
            6,
            "final public private(set) string $label",
            Visibility::Public,
            &[Modifier::Final],
        ),
        (
            "metadata",
            7,
            "final $metadata",
            Visibility::Public,
            &[Modifier::Final],
        ),
    ] {
        let field = detail(&parsed, name, line);
        assert_eq!(field.signature.as_deref(), Some(signature));
        assert_eq!(field.visibility, Some(visibility));
        assert_eq!(field.modifiers, modifiers);
        assert!(parsed.symbols.iter().any(|symbol| {
            symbol.name == name
                && symbol.kind == SymbolKind::Field
                && symbol.parent.as_deref() == Some("Profile")
        }));
    }
    assert_eq!(
        params(&detail(&parsed, "__construct", 3)),
        [
            "name:string='guest':positional",
            "reader:?Reader=null:positional",
            "label:string='':positional",
            "metadata:?=[]:positional",
        ]
    );
    assert_eq!(
        detail(&parsed, "$label::get", 6).return_type.unwrap().text,
        "string"
    );
    mct_core::TokenSource::tokenize(&mct_lang_php::PhpTokenSource, file, source).unwrap();
}

#[test]
fn php_85_constant_attributes_and_enum_case_values_have_references() {
    let parsed = mct_corpus::parse_source(
        &PhpParser,
        "src/Enums.php",
        "PHP",
        r#"<?php
#[Marked]
const FORMATTER = static function (string $input): string { return trim($input); };
enum State: string {
    #[Marked]
    case Ready = StateValues::READY;
}
"#,
    );
    assert_eq!(
        detail(&parsed, "FORMATTER", 3).signature.as_deref(),
        Some("const FORMATTER")
    );
    assert_eq!(
        detail(&parsed, "Ready", 5).signature.as_deref(),
        Some("case Ready")
    );
    for (name, target) in [
        ("FORMATTER", "Marked"),
        ("Ready", "Marked"),
        ("Ready", "StateValues"),
    ] {
        let symbol = parsed.symbols.iter().find(|s| s.name == name).unwrap();
        assert!(parsed.relations.iter().any(|r| r.from == symbol.id
            && r.to_name == target
            && r.kind == mct_core::RelationKind::References));
    }
}
