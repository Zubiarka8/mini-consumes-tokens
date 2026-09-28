//! Long, multi-file fixture corpus (issue #74): a warehouse inventory system
//! split into two headers (`include/inventory/model.hpp`, `storage.hpp`) and
//! three sources (`src/storage.cpp`, `src/service.cpp`, `src/main.cpp`), each
//! 300–600 lines and `#include`-ing the headers. The shared checks (size,
//! line ranges, golden snapshot, index round trip, malformed input) come
//! from `mct-corpus`; the tests below pin the constructs and cross-file
//! relations this language is expected to extract.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-cpp/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_cpp::CppParser;

mct_corpus::standard_tests!(CppParser);

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

/// `(parent, kind)` of every symbol called `name`, across the corpus.
fn identities(name: &str) -> Vec<(Option<String>, SymbolKind)> {
    corpus()
        .symbols_named(name)
        .into_iter()
        .map(|(_, s)| (s.parent.clone(), s.kind))
        .collect()
}

#[test]
fn types_enums_and_aliases_are_extracted_with_kinds() {
    use SymbolKind::{Class, Constant, Enum, Struct, TypeAlias};
    for (path, name, kind) in [
        ("model.hpp", "ProductId", Class),
        ("model.hpp", "Quantity", Class),
        ("model.hpp", "Dimensions", Struct),
        ("model.hpp", "Unit", Enum),
        ("model.hpp", "LegacyFlag", Enum),
        // `using X = …;` and `typedef … X;`
        ("model.hpp", "Clock", TypeAlias),
        ("model.hpp", "Count", TypeAlias),
        ("model.hpp", "WarehouseId", TypeAlias),
        ("model.hpp", "Tags", TypeAlias),
        // A union is recorded as a struct.
        ("model.hpp", "ScanValue", Struct),
        ("storage.hpp", "Repository", Class),
        ("storage.hpp", "StockKey", Struct),
        ("main.cpp", "ExitCode", Enum),
        ("main.cpp", "Args", TypeAlias),
    ] {
        corpus().symbol(path, name, kind);
    }
    // Enumerators are constants of their enum.
    assert_eq!(
        parent("model.hpp", "kFragile", Constant),
        Some("LegacyFlag")
    );
    assert_eq!(parent("model.hpp", "Kilogram", Constant), Some("Unit"));
    // A nested enum belongs to its class.
    assert_eq!(parent("storage.hpp", "Status", Enum), Some("StoreHealth"));
}

#[test]
fn namespace_members_are_functions_and_variables_not_methods_and_fields() {
    use SymbolKind::{Function, Module, Variable};
    assert_eq!(parent("model.hpp", "make_sku", Function), Some("inv"));
    assert_eq!(parent("model.hpp", "parse_unit", Function), Some("inv"));
    assert_eq!(parent("model.hpp", "kMaxNameLength", Variable), Some("inv"));
    assert_eq!(
        parent("model.hpp", "kSchemaVersion", Variable),
        Some("detail")
    );
    assert_eq!(
        parent("service.cpp", "average_stock", Function),
        Some("inv::service")
    );
    // Nested namespaces, `inline namespace`, `namespace a::b`.
    assert_eq!(parent("model.hpp", "detail", Module), Some("inv"));
    assert_eq!(parent("service.cpp", "v2", Module), Some("inv::service"));
    // An anonymous namespace adds no scope and no nameless symbol.
    assert!(corpus().symbols_named("").is_empty());
    assert_eq!(parent("storage.cpp", "split", Function), None);
}

#[test]
fn members_hang_off_their_class_including_nested_and_template_ones() {
    use SymbolKind::{Class, Field, Method};
    assert_eq!(parent("model.hpp", "operator+", Method), Some("Quantity"));
    assert_eq!(parent("model.hpp", "Builder", Class), Some("Product"));
    assert_eq!(parent("model.hpp", "tag", Method), Some("Builder"));
    assert_eq!(
        parent("storage.hpp", "put_if_version", Method),
        Some("MemoryRepository")
    );
    assert_eq!(
        parent("storage.hpp", "data_", Field),
        Some("MemoryRepository")
    );
    assert_eq!(
        parent("service.cpp", "instance_count_", Field),
        Some("InventoryService")
    );
}

#[test]
fn locals_are_not_symbols_even_when_they_look_like_prototypes() {
    let c = corpus();
    // `std::shared_lock lock(mutex_);` parses like a function prototype.
    assert!(c.symbols_named("lock").is_empty());
    // `auto level = level_at(...)`, `Transaction tx(...)`, …
    for local in ["level", "tx", "listener", "shipped", "movements"] {
        assert!(c.symbols_named(local).is_empty(), "{local} is a local");
    }
    // …but the calls in their initializers are kept.
    c.relation("service.cpp", "receive", RelationKind::Calls, "level_at");
    c.relation("service.cpp", "transfer", RelationKind::Calls, "ship");
    c.relation("storage.cpp", "compact", RelationKind::Calls, "load_all");
    // A class local to a function is still a class, owned by nothing.
    assert_eq!(parent("service.cpp", "Tier", SymbolKind::Struct), None);
}

#[test]
fn out_of_line_definitions_match_their_in_class_declarations() {
    use SymbolKind::{Class, Function, Method};
    for (name, parent, kind, expected) in [
        // `Class::method` in a namespace block.
        ("format_conflict", "Conflict", Method, 2),
        // `Outer::Inner::method`.
        ("build", "Builder", Method, 2),
        // `Tmpl<T>::method` / `Tmpl<K, V>::method`: template args dropped.
        ("throw_unit_mismatch", "Quantity", Method, 2),
        ("version", "MemoryRepository", Method, 2),
        // `ns::Class::method` outside any namespace block.
        ("label", "Location", Method, 2),
        ("status_name", "StoreHealth", Method, 2),
        // `ns::free_fn` outside the block: a function of the namespace.
        ("describe_error", "inv", Function, 2),
        ("normalize_name", "detail", Function, 2),
        // `class Outer::Inner { … }` defined out of line (the header only
        // forward-declares it, which is not a symbol).
        ("Writer", "FileStore", Class, 1),
    ] {
        let found = identities(name);
        let matching = found
            .iter()
            .filter(|(p, k)| p.as_deref() == Some(parent) && *k == kind)
            .count();
        assert_eq!(
            matching, expected,
            "{name}: expected {expected} {kind:?} of {parent}; got {found:?}"
        );
    }
}

#[test]
fn long_bodies_keep_exact_line_ranges() {
    use SymbolKind::{Class, Function, Method, Module};
    assert_eq!(lines("model.hpp", "Product", Class), (177, Some(231)));
    assert_eq!(lines("model.hpp", "Quantity", Class), (95, Some(145)));
    assert_eq!(lines("main.cpp", "run", Function), (275, Some(302)));
    assert_eq!(lines("storage.cpp", "decode", Method), (227, Some(243)));
    // The file-level module ends on the file's last line.
    assert_eq!(lines("model.hpp", "model", Module), (1, Some(342)));
    assert_eq!(lines("main.cpp", "main", Module), (1, Some(308)));
}

#[test]
fn inheritance_is_recorded_as_extends() {
    let c = corpus();
    c.relation(
        "model.hpp",
        "PerishableProduct",
        RelationKind::Extends,
        "Product",
    );
    c.relation(
        "storage.hpp",
        "NotFound",
        RelationKind::Extends,
        "StorageError",
    );
    // Qualified base: last segment.
    c.relation(
        "storage.hpp",
        "StorageError",
        RelationKind::Extends,
        "runtime_error",
    );
    // Template bases keep their arguments, CRTP included.
    c.relation(
        "storage.hpp",
        "MemoryRepository",
        RelationKind::Extends,
        "Repository<K, V>",
    );
    c.relation(
        "service.cpp",
        "LoggingListener",
        RelationKind::Extends,
        "CountingListener<LoggingListener>",
    );
    c.relation(
        "service.cpp",
        "FlatPricing",
        RelationKind::Extends,
        "PricingStrategy",
    );
}

#[test]
fn includes_and_cross_file_calls_are_extracted() {
    let c = corpus();
    c.relation(
        "storage.hpp",
        "storage",
        RelationKind::Imports,
        "inventory/model.hpp",
    );
    c.relation(
        "main.cpp",
        "main",
        RelationKind::Imports,
        "inventory/storage.hpp",
    );
    c.relation("main.cpp", "main", RelationKind::Imports, "iostream");

    c.relation("main.cpp", "cmd_demo", RelationKind::Calls, "seed_demo");
    c.relation(
        "main.cpp",
        "make_context",
        RelationKind::Calls,
        "make_product_repository",
    );
    c.relation("main.cpp", "cmd_reorder", RelationKind::Calls, "make_sku");
    c.relation("service.cpp", "transfer", RelationKind::Calls, "label");
    c.relation(
        "storage.cpp",
        "describe",
        RelationKind::Calls,
        "unit_symbol",
    );
    // Constructor initializer lists belong to the constructor.
    c.relation(
        "storage.cpp",
        "Product",
        RelationKind::Calls,
        "normalize_name",
    );
    // Calls in lambdas go to the enclosing function.
    c.relation(
        "service.cpp",
        "attach_logging",
        RelationKind::Calls,
        "notify",
    );
    assert!(c.cross_file_relation_count() >= 100);
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    // Header declaration and source definition come back together.
    let hits = index.find_symbol("label").unwrap();
    let files: Vec<_> = hits.iter().map(|h| h.relative_path.as_str()).collect();
    assert!(
        files.contains(&"include/inventory/model.hpp") && files.contains(&"src/storage.cpp"),
        "{files:?}"
    );
    assert!(hits.iter().all(|h| h.parent.as_deref() == Some("Location")));

    let callers = index.find_callers("make_sku").unwrap();
    for file in ["include/inventory/model.hpp", "src/main.cpp"] {
        assert!(callers.iter().any(|h| h.relative_path == file), "{file}");
    }

    let calls = index.find_calls("ship").unwrap();
    for callee in ["check_product", "level_at", "available", "record"] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "ship should call {callee}"
        );
    }
}
