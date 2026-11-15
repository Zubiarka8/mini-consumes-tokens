//! Declaration details (ADR-034, issue #321) the JS/TS parser proves: the
//! header as written (no decorators, doc comment or body), parameters with
//! their annotations, defaults and rest, the return type, generics, class
//! member visibility and modifiers. In JavaScript, a JSDoc `@param`/`@returns`
//! type is kept with `doc` provenance; a written TS annotation always wins.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, Provenance, SymbolDetail, Visibility};
use mct_lang_js_ts::JsTsParser;

const TS: &str = r#"
/** Loads one. */
export async function load<T extends object>(
  id: string,
  retries: number = 3,
  ...rest: T[]
): Promise<T> {
  return {} as T;
}

@Injectable()
export abstract class Store<K> extends Base implements Api {
  private readonly cache: Map<K, string> = new Map();
  protected static async *items(this: Store<K>, limit?: number): AsyncGenerator<K> {}
  abstract size(): number;
  #secret() {}
  get(key: K) {
    return key;
  }
  handle = async (event: Event): Promise<void> => {};
}

export interface Api {
  get(key: string): unknown;
}

export type Pair<A> = [A, A];
export const enum Color { Red }

export const twice = (n: number): number => n * 2;
"#;

const JS: &str = r#"
/**
 * Formats a price.
 * @param {number} amount - in cents
 * @param {{ code: string }} [currency={code: "EUR"}]
 * @returns {string}
 */
function format(amount, { code } = {}, ...parts) {
  return "";
}

const shout = (text) => text.toUpperCase();

function* ids() {}
"#;

fn parse(path: &str, source: &str) -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, path, "JavaScript/TypeScript", source)
}

/// The detail of the only symbol named `name`.
fn detail(parsed: &ParsedFile, name: &str) -> SymbolDetail {
    detail_where(parsed, name, |_| true)
}

/// The detail of the only member `name` of `parent`.
fn member_detail(parsed: &ParsedFile, parent: &str, name: &str) -> SymbolDetail {
    detail_where(parsed, name, |s| s.parent.as_deref() == Some(parent))
}

fn detail_where(
    parsed: &ParsedFile,
    name: &str,
    keep: impl Fn(&mct_core::SymbolRecord) -> bool,
) -> SymbolDetail {
    let symbols: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.name == name && keep(s))
        .collect();
    assert_eq!(symbols.len(), 1, "{name}: {symbols:?}");
    parsed
        .symbol_details
        .iter()
        .find(|d| d.symbol == symbols[0].id)
        .cloned()
        .unwrap_or_else(|| panic!("no detail for {name}"))
        .normalized()
}

/// `name:type(provenance)=default:kind` per parameter.
fn params(detail: &SymbolDetail) -> Vec<String> {
    detail
        .parameters
        .iter()
        .map(|p| {
            format!(
                "{}:{}={}:{}",
                p.name.as_deref().unwrap_or("_"),
                p.type_text.as_ref().map_or("?".to_string(), |t| format!(
                    "{}({})",
                    t.text,
                    t.provenance.as_str()
                )),
                p.default.as_deref().unwrap_or("-"),
                p.kind.as_str()
            )
        })
        .collect()
}

#[test]
fn a_ts_function_records_annotations_defaults_rest_and_return_type() {
    let parsed = parse("src/store.ts", TS);
    let load = detail(&parsed, "load");
    assert_eq!(
        load.signature.as_deref(),
        Some(
            "async function load<T extends object>( id: string, retries: number = 3, \
             ...rest: T[] ): Promise<T>"
        )
    );
    assert_eq!(load.modifiers, [Modifier::Async]);
    assert_eq!(load.generics.as_deref(), Some("<T extends object>"));
    assert_eq!(load.visibility, None);
    assert_eq!(
        params(&load),
        [
            "id:string(syntax)=-:positional",
            "retries:number(syntax)=3:positional",
            "rest:T[](syntax)=-:variadic"
        ]
    );
    let ret = load.return_type.unwrap();
    assert_eq!(
        (ret.text.as_str(), ret.provenance),
        ("Promise<T>", Provenance::Syntax)
    );
}

#[test]
fn a_class_header_skips_decorators_and_members_get_visibility_and_modifiers() {
    let parsed = parse("src/store.ts", TS);
    let store = detail(&parsed, "Store");
    assert_eq!(
        store.signature.as_deref(),
        Some("abstract class Store<K> extends Base implements Api")
    );
    assert_eq!(store.modifiers, [Modifier::Abstract]);
    assert_eq!(store.generics.as_deref(), Some("<K>"));

    let cache = detail(&parsed, "cache");
    assert_eq!(cache.visibility, Some(Visibility::Private));
    assert_eq!(cache.modifiers, [Modifier::Readonly]);
    assert_eq!(
        cache.signature.as_deref(),
        Some("private readonly cache: Map<K, string>")
    );

    let items = detail(&parsed, "items");
    assert_eq!(items.visibility, Some(Visibility::Protected));
    assert_eq!(
        items.modifiers,
        [Modifier::Async, Modifier::Generator, Modifier::Static]
    );
    assert_eq!(
        params(&items),
        [
            "this:Store<K>(syntax)=-:receiver",
            "limit:number(syntax)=-:positional"
        ]
    );

    assert_eq!(
        detail(&parsed, "#secret").visibility,
        Some(Visibility::Private)
    );
    // A class member without a modifier is public by the language rule.
    assert_eq!(
        member_detail(&parsed, "Store", "get").visibility,
        Some(Visibility::Public)
    );
    assert_eq!(
        member_detail(&parsed, "Api", "get").signature.as_deref(),
        Some("get(key: string): unknown")
    );

    let handle = detail(&parsed, "handle");
    assert_eq!(
        handle.signature.as_deref(),
        Some("handle = async (event: Event): Promise<void> =>")
    );
    assert_eq!(handle.modifiers, [Modifier::Async]);
    assert_eq!(params(&handle), ["event:Event(syntax)=-:positional"]);
}

#[test]
fn types_and_arrow_constants_get_their_header() {
    let parsed = parse("src/store.ts", TS);
    assert_eq!(
        detail(&parsed, "Api").signature.as_deref(),
        Some("interface Api")
    );
    assert_eq!(
        detail(&parsed, "Pair").signature.as_deref(),
        Some("type Pair<A> = [A, A]")
    );
    let color = detail(&parsed, "Color");
    assert_eq!(color.signature.as_deref(), Some("const enum Color"));
    assert_eq!(color.modifiers, [Modifier::Const]);
    let twice = detail(&parsed, "twice");
    assert_eq!(
        twice.signature.as_deref(),
        Some("const twice = (n: number): number =>")
    );
    assert_eq!(twice.return_type.map(|t| t.text).as_deref(), Some("number"));
}

#[test]
fn a_js_function_takes_its_types_from_jsdoc_with_doc_provenance() {
    let parsed = parse("src/price.js", JS);
    let format = detail(&parsed, "format");
    assert_eq!(
        format.signature.as_deref(),
        Some("function format(amount, { code } = {}, ...parts)")
    );
    assert_eq!(
        params(&format),
        [
            "amount:number(doc)=-:positional",
            // A destructured parameter has no name for JSDoc to match.
            "_:?={}:positional",
            "parts:?=-:variadic"
        ]
    );
    let ret = format.return_type.unwrap();
    assert_eq!(
        (ret.text.as_str(), ret.provenance),
        ("string", Provenance::Doc)
    );

    let shout = detail(&parsed, "shout");
    assert_eq!(params(&shout), ["text:?=-:positional"]);
    assert!(shout.return_type.is_none());
    assert_eq!(detail(&parsed, "ids").modifiers, [Modifier::Generator]);
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-js-ts-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/price.js"), JS).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(JsTsParser));
    let format = index
        .symbol_details("src/price.js", 8, "format")
        .unwrap()
        .unwrap();
    assert_eq!(format.return_source.as_deref(), Some("doc"));
    assert_eq!(format.parameters[0].type_source.as_deref(), Some("doc"));
    std::fs::remove_dir_all(root).unwrap();
}
