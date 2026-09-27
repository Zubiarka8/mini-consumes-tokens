//! Long, multi-file fixture corpus (issue #74): a warehouse inventory
//! service split into `model`, `errors`, `storage`, `service`, `report` and
//! `main`, each 300–600 lines and `use`-ing the others. The shared checks
//! (size, line ranges, golden snapshot, index round trip, malformed input)
//! come from `mct-corpus`; the tests below pin the constructs and
//! cross-file relations this language is expected to extract.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-rust/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_rust::RustParser;

mct_corpus::standard_tests!(RustParser);

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

#[test]
fn types_traits_and_aliases_are_extracted_with_kinds() {
    let c = corpus();
    for (path, name, kind) in [
        ("model.rs", "ProductId", SymbolKind::Struct),
        ("model.rs", "Virtual", SymbolKind::Struct),
        ("model.rs", "Unit", SymbolKind::Enum),
        ("model.rs", "Movement", SymbolKind::Enum),
        ("model.rs", "Identified", SymbolKind::Trait),
        ("model.rs", "Versioned", SymbolKind::Struct),
        ("model.rs", "StockRecord", SymbolKind::TypeAlias),
        ("model.rs", "Catalog", SymbolKind::TypeAlias),
        ("model.rs", "MAX_NAME_LEN", SymbolKind::Constant),
        ("model.rs", "SKU_PREFIX", SymbolKind::Constant),
        ("model.rs", "dimensions", SymbolKind::Module),
        ("errors.rs", "InventoryError", SymbolKind::Enum),
        ("errors.rs", "Result", SymbolKind::TypeAlias),
        ("errors.rs", "AUDIT", SymbolKind::Constant),
        ("storage.rs", "Repository", SymbolKind::Trait),
        ("storage.rs", "MemoryRepository", SymbolKind::Struct),
        ("service.rs", "BoxFuture", SymbolKind::TypeAlias),
        ("service.rs", "Outbox", SymbolKind::Struct),
        ("report.rs", "Section", SymbolKind::Trait),
        ("main.rs", "Command", SymbolKind::Enum),
    ] {
        c.symbol(path, name, kind);
    }
    // `mod errors;` declarations in main.rs, next to the file-level module.
    assert_eq!(c.symbols_named("errors").len(), 2);
}

#[test]
fn methods_hang_off_their_impl_type_including_generic_ones() {
    let c = corpus();
    let owner = |path, name| {
        c.symbol(path, name, SymbolKind::Method)
            .parent
            .clone()
            .unwrap()
    };
    assert_eq!(owner("model.rs", "with_child"), "Category");
    assert_eq!(owner("model.rs", "update"), "Versioned<T>");
    assert_eq!(owner("service.rs", "flush_once"), "Outbox<S>");
    assert_eq!(owner("report.rs", "widths"), "Table");
    // Trait signatures and default methods belong to the trait.
    assert_eq!(owner("storage.rs", "require"), "Repository");
    assert_eq!(owner("service.rs", "publish"), "MovementSink");
    // Associated items of an impl/trait are owned by it too.
    let ids: Vec<_> = c
        .symbols_named("Id")
        .into_iter()
        .map(|(_, s)| s.parent.clone().unwrap())
        .collect();
    assert_eq!(ids, ["Identified", "Product", "StockLevel"]);
    assert_eq!(
        c.symbol("model.rs", "ZERO", SymbolKind::Constant)
            .parent
            .as_deref(),
        Some("Size")
    );
}

#[test]
fn nested_functions_are_not_methods_of_the_enclosing_impl() {
    let c = corpus();
    let walk = c.symbol("report.rs", "walk", SymbolKind::Function);
    assert_eq!(walk.parent, None);
    assert_eq!(
        lines("report.rs", "walk", SymbolKind::Function),
        (269, Some(274))
    );
    c.symbol("report.rs", "bump", SymbolKind::Function);
    // Recursive call inside the nested function.
    c.relation("report.rs", "walk", RelationKind::Calls, "walk");
}

#[test]
fn long_bodies_keep_exact_line_ranges() {
    assert_eq!(
        lines("service.rs", "transfer", SymbolKind::Method),
        (127, Some(157))
    );
    assert_eq!(
        lines("model.rs", "delta_at", SymbolKind::Method),
        (257, Some(276))
    );
    assert_eq!(
        lines("main.rs", "execute", SymbolKind::Function),
        (167, Some(239))
    );
    assert_eq!(
        lines("errors.rs", "tests", SymbolKind::Module),
        (274, Some(397))
    );
    // The file-level module ends on the file's last line.
    assert_eq!(
        lines("errors.rs", "errors", SymbolKind::Module),
        (1, Some(397))
    );
}

#[test]
fn trait_impls_are_recorded_as_implements() {
    let c = corpus();
    for (path, to) in [
        ("service.rs", "MovementListener"),
        ("report.rs", "Section"),
        ("storage.rs", "Repository<T>"),
        ("model.rs", "Identified"),
        ("errors.rs", "fmt::Display"),
        ("errors.rs", "From<std::io::Error>"),
    ] {
        assert!(
            c.relations()
                .iter()
                .any(|r| r.path == path && r.kind == RelationKind::Implements && r.to == to),
            "{path} should implement {to}"
        );
    }
}

#[test]
fn cross_file_imports_and_calls_are_extracted() {
    let c = corpus();
    c.relation("service.rs", "service", RelationKind::Imports, "StockTable");
    c.relation("main.rs", "main", RelationKind::Imports, "full_report");
    c.relation("report.rs", "report", RelationKind::Imports, "Warehouse");
    c.relation(
        "service.rs",
        "update_level",
        RelationKind::Calls,
        "with_retry",
    );
    c.relation("service.rs", "update_level", RelationKind::Calls, "modify");
    c.relation("main.rs", "execute", RelationKind::Calls, "full_report");
    c.relation("main.rs", "execute", RelationKind::Calls, "transfer");
    c.relation("storage.rs", "load", RelationKind::Calls, "make_sku");
    // Qualified paths call the last segment: `crate::model::dimensions::stacked_volume`.
    c.relation(
        "report.rs",
        "shelf_usage",
        RelationKind::Calls,
        "stacked_volume",
    );
    // Calls inside closures are attributed to the enclosing function.
    c.relation(
        "main.rs",
        "parse_command",
        RelationKind::Calls,
        "parse_unit",
    );
    assert!(c.cross_file_relation_count() >= 150);
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("with_retry").unwrap();
    assert!(callers
        .iter()
        .any(|h| h.from_symbol == "update_level" && h.relative_path == "service.rs"));

    let refs = index.find_references("Warehouse").unwrap();
    let files: Vec<_> = refs.iter().map(|r| r.relative_path.as_str()).collect();
    assert!(files.contains(&"main.rs") && files.contains(&"report.rs"));

    let calls = index.find_calls("execute").unwrap();
    for callee in ["load", "subscribe", "full_report", "reorder_suggestions"] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "execute should call {callee}"
        );
    }

    let hits = index.find_symbol("Size").unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!((hits[0].line, hits[0].end_line), (361, Some(365)));
}
