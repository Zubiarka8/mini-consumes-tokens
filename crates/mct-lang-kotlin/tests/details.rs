//! Declaration details (ADR-034, issue #321) the Kotlin parser proves from
//! syntax: the header without annotations or body, parameters (`vararg`,
//! defaults, an extension's receiver type as `this`), the written return
//! type, generics, visibility (public by default) and modifiers.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{Modifier, ParsedFile, SymbolDetail, SymbolKind, Visibility};
use mct_lang_kotlin::KotlinParser;

// `Api` spans lines on purpose: the grammar rejects a bodyless member
// followed by `}` on the same line (`interface Api { fun run(a: Int) }`).
const SOURCE: &str = r#"
@Serializable
open class Repo<T : Item>(private val id: Int, name: String) : Base(), Api {
    @Deprecated("x") internal lateinit var cache: Map<String, T>
    override suspend fun <K> load(id: Int = 3, vararg names: String): List<K>? {
        return null
    }
    private fun String.shout() = uppercase()
}
interface Api {
    fun run(a: Int)
}
object Registry
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&KotlinParser, "src/Repo.kt", "Kotlin", SOURCE)
}

fn detail(parsed: &ParsedFile, name: &str) -> SymbolDetail {
    let symbols: Vec<_> = parsed
        .symbols
        .iter()
        .filter(|s| s.name == name && s.kind != SymbolKind::Module)
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
fn a_function_records_defaults_varargs_return_type_and_modifiers() {
    let parsed = parsed();
    let load = detail(&parsed, "load");
    assert_eq!(
        load.signature.as_deref(),
        Some("override suspend fun <K> load(id: Int = 3, vararg names: String): List<K>?")
    );
    assert_eq!(load.visibility, Some(Visibility::Public));
    assert_eq!(load.modifiers, [Modifier::Override]);
    assert_eq!(load.generics.as_deref(), Some("<K>"));
    assert_eq!(
        load.return_type.as_ref().map(|t| t.text.as_str()),
        Some("List<K>?")
    );
    assert_eq!(
        params(&load),
        ["id:Int=3:positional", "names:String=-:variadic"]
    );
}

#[test]
fn an_extension_has_its_receiver_type_and_no_unwritten_return_type() {
    let parsed = parsed();
    let shout = detail(&parsed, "shout");
    assert_eq!(
        shout.signature.as_deref(),
        Some("private fun String.shout()")
    );
    assert_eq!(shout.visibility, Some(Visibility::Private));
    assert_eq!(params(&shout), ["this:String=-:receiver"]);
    assert!(shout.return_type.is_none());
}

#[test]
fn classes_and_properties_get_their_header_without_annotations() {
    let parsed = parsed();
    let repo = detail(&parsed, "Repo");
    assert_eq!(
        repo.signature.as_deref(),
        Some("open class Repo<T : Item>(private val id: Int, name: String) : Base(), Api")
    );
    assert_eq!(repo.modifiers, [Modifier::Open]);
    assert_eq!(repo.generics.as_deref(), Some("<T : Item>"));
    let cache = detail(&parsed, "cache");
    assert_eq!(
        cache.signature.as_deref(),
        Some("internal lateinit var cache: Map<String, T>")
    );
    assert_eq!(cache.visibility, Some(Visibility::Internal));
    let id = detail(&parsed, "id");
    assert_eq!(id.signature.as_deref(), Some("private val id: Int"));
    assert_eq!(id.visibility, Some(Visibility::Private));
    assert_eq!(
        detail(&parsed, "Registry").signature.as_deref(),
        Some("object Registry")
    );
    assert_eq!(detail(&parsed, "run").visibility, Some(Visibility::Public));
}

#[test]
fn details_reach_the_index() {
    let root = std::env::temp_dir().join(format!("mct-lang-kotlin-details-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/Repo.kt"), SOURCE).unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(KotlinParser));
    let load = index
        .symbol_details("src/Repo.kt", 5, "load")
        .unwrap()
        .unwrap();
    assert_eq!(load.modifiers, ["override"]);
    assert_eq!(load.parameters.len(), 2);
    std::fs::remove_dir_all(root).unwrap();
}
