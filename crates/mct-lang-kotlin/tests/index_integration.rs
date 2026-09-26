//! Runs the real `mct-index` pipeline against a small multi-file Kotlin
//! project: a data class plus a class with methods and a top-level extension
//! function (`Invoice.kt`), an `object` singleton it calls (`Logger.kt`), and
//! a caller (`Main.kt`). Proves the whole parse -> SQLite -> query round trip,
//! not just parsing: the symbols land in the index, `files.language` carries
//! `kotlin`, and the query layer returns the expected rows — including the
//! Kotlin-specific shapes (constructor property promotion, `object`
//! declarations, extension functions bound to their receiver type).

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-kotlin/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};
use mct_lang_kotlin::KotlinParser;

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/billing-app")
}

fn open_indexed() -> Index {
    let root = fixture_root();
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(KotlinParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_parsed, 3, "Invoice.kt, Logger.kt, Main.kt");
    assert!(
        report.issues.is_empty(),
        "no parse issues expected: {:?}",
        report.issues
    );
    index
}

#[test]
fn find_symbol_locates_method_on_its_class() {
    let index = open_indexed();
    let hits = index.find_symbol("addItem").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "method");
    assert_eq!(hits[0].parent.as_deref(), Some("Invoice"));
    assert_eq!(hits[0].relative_path, "Invoice.kt");
    assert_eq!(hits[0].language, "kotlin");
}

#[test]
fn find_symbol_indexes_data_class_constructor_properties_as_fields() {
    let index = open_indexed();

    let line_item: Vec<_> = index
        .find_symbol("LineItem")
        .unwrap()
        .into_iter()
        .filter(|s| s.kind == "class")
        .collect();
    assert_eq!(line_item.len(), 1);
    assert_eq!(line_item[0].relative_path, "Invoice.kt");

    // `data class LineItem(val name: String, val price: Double)` — the
    // primary-constructor `val`s are properties, and must reach SQLite as
    // fields parented on the data class.
    for field in ["name", "price"] {
        let hits = index.find_symbol(field).unwrap();
        assert_eq!(hits.len(), 1, "{field}: {hits:?}");
        assert_eq!(hits[0].kind, "field");
        assert_eq!(hits[0].parent.as_deref(), Some("LineItem"));
        assert_eq!(hits[0].language, "kotlin");
    }
}

#[test]
fn find_symbol_attaches_the_extension_function_to_its_receiver_type() {
    let index = open_indexed();
    // `fun Invoice.describe()` is written at the top level of Invoice.kt but
    // belongs to `Invoice` — the index must record the receiver as parent.
    let hits = index.find_symbol("describe").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].kind, "method");
    assert_eq!(hits[0].parent.as_deref(), Some("Invoice"));
    assert_eq!(hits[0].relative_path, "Invoice.kt");
}

#[test]
fn find_symbol_indexes_object_declaration_and_its_method() {
    let index = open_indexed();

    let logger: Vec<_> = index
        .find_symbol("Logger")
        .unwrap()
        .into_iter()
        .filter(|s| s.kind == "class")
        .collect();
    assert_eq!(
        logger.len(),
        1,
        "an `object` singleton is indexed as a class"
    );
    assert_eq!(logger[0].relative_path, "Logger.kt");

    let log = index.find_symbol("log").unwrap();
    assert_eq!(log.len(), 1);
    assert_eq!(log[0].kind, "method");
    assert_eq!(log[0].parent.as_deref(), Some("Logger"));
    assert_eq!(log[0].relative_path, "Logger.kt");
}

#[test]
fn list_symbols_reports_every_definition_in_the_invoice_file_as_kotlin() {
    let index = open_indexed();
    let entries = index.list_symbols("Invoice.kt", None, None).unwrap();
    let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"LineItem"), "{names:?}");
    assert!(names.contains(&"Invoice"), "{names:?}");
    assert!(names.contains(&"addItem"), "{names:?}");
    assert!(names.contains(&"addItemWithTax"), "{names:?}");
    assert!(names.contains(&"describe"), "{names:?}");
    assert!(
        entries.iter().all(|e| e.language == "kotlin"),
        "files.language must be kotlin for every row: {entries:?}"
    );

    let fields = index
        .list_symbols("Invoice.kt", Some("field"), Some("kotlin"))
        .unwrap();
    let field_names: Vec<_> = fields.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(field_names, vec!["name", "price", "total"]);
    assert!(index
        .list_symbols("Invoice.kt", None, Some("java"))
        .unwrap()
        .is_empty());
}

#[test]
fn status_reports_full_kotlin_coverage() {
    let index = open_indexed();
    let status = index.status().unwrap();
    let kotlin = status
        .languages
        .iter()
        .find(|l| l.language == "kotlin")
        .unwrap();
    assert_eq!(kotlin.file_count, 3);
    assert_eq!(kotlin.symbol_count, 17);
    assert_eq!(status.languages.len(), 1, "the fixture is single-language");
    assert!(status.unsupported_languages.is_empty());
    assert!(status.syntax_errors.is_empty());
}

#[test]
fn find_calls_reports_log_from_add_item() {
    let index = open_indexed();
    let calls = index.find_calls("addItem").unwrap();
    let log_calls: Vec<_> = calls.iter().filter(|c| c.to_name == "log").collect();
    assert_eq!(
        log_calls.len(),
        1,
        "`Logger.log(...)` is recorded under the called name"
    );
    assert_eq!(log_calls[0].relative_path, "Invoice.kt");
}

#[test]
fn find_callers_of_log_shows_both_invoice_methods() {
    let index = open_indexed();
    let callers = index.find_callers("log").unwrap();
    assert_eq!(
        callers.len(),
        2,
        "addItem and addItemWithTax both call log: {callers:?}"
    );
    assert!(callers.iter().all(|c| c.relative_path == "Invoice.kt"));
    assert!(callers.iter().any(|c| c.from_symbol == "addItem"));
    assert!(callers.iter().any(|c| c.from_symbol == "addItemWithTax"));
}

#[test]
fn find_references_finds_cross_file_calls_from_main() {
    let index = open_indexed();

    let refs = index.find_references("addItem").unwrap();
    assert_eq!(refs.len(), 1, "Main.kt calls addItem once");
    assert_eq!(refs[0].relative_path, "Main.kt");
    assert_eq!(refs[0].kind, "calls");
    assert_eq!(refs[0].from_symbol, "main");

    // The extension function is reached across files too.
    let describe_refs = index.find_references("describe").unwrap();
    assert_eq!(describe_refs.len(), 1);
    assert_eq!(describe_refs[0].relative_path, "Main.kt");

    // Both `LineItem(...)` constructions are recorded.
    let item_refs = index.find_references("LineItem").unwrap();
    assert_eq!(
        item_refs.len(),
        2,
        "Main.kt constructs LineItem twice: {item_refs:?}"
    );
    assert!(item_refs.iter().all(|r| r.relative_path == "Main.kt"));
}
