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
