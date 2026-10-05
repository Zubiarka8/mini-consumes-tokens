//! Long, multi-file fixture corpus (issue #74): a warehouse service split
//! into `Errors`, `Models`, `Repository`, `Services`, `Reports` and
//! `Program`, each 300–600 lines and using the others. The shared checks
//! (size, line ranges, golden snapshot, index round trip, malformed input)
//! come from `mct-corpus`; the tests below pin the constructs and
//! cross-file relations this language is expected to extract.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-csharp/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_csharp::CSharpParser;

mct_corpus::standard_tests!(CSharpParser);

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn types_of_every_kind_are_extracted() {
    use SymbolKind::{Class, Constant, Enum, Interface, Struct, TypeAlias};
    let c = corpus();
    for (path, name, kind) in [
        ("Errors.cs", "WarehouseException", Class),
        ("Errors.cs", "ErrorCode", Enum),
        ("Errors.cs", "Fatal", Constant),
        // `record struct` and `readonly struct` are structs, `record` a class.
        ("Errors.cs", "FieldError", Struct),
        ("Errors.cs", "Result", Struct),
        ("Errors.cs", "Entry", Class),
        ("Models.cs", "Sku", Struct),
        ("Models.cs", "Price", Struct),
        ("Models.cs", "Location", Class),
        ("Models.cs", "IEntity", Interface),
        ("Models.cs", "StockChanged", TypeAlias),
        ("Models.cs", "Projection", TypeAlias),
        ("Repository.cs", "IRepository", Interface),
        ("Repository.cs", "ChangeKind", Enum),
        ("Services.cs", "IClock", Interface),
        ("Reports.cs", "InventoryRow", Class),
        ("Reports.cs", "Summary", Struct),
        ("Program.cs", "CommandRegistry", Class),
    ] {
        c.symbol(path, name, kind);
    }
    assert_eq!(parent("Errors.cs", "Fatal", Constant), Some("Severity"));
}

#[test]
fn namespaces_block_nested_and_file_scoped_own_their_types() {
    use SymbolKind::{Class, Enum, Module};
    // Block namespace.
    assert_eq!(
        parent("Errors.cs", "ErrorCatalog", Class),
        Some("Warehouse.Errors")
    );
    // File-scoped `namespace X;`: every later declaration belongs to it,
    // and it runs to the end of the file.
    assert_eq!(
        parent("Models.cs", "Product", Class),
        Some("Warehouse.Models")
    );
    assert_eq!(parent("Models.cs", "Unit", Enum), Some("Warehouse.Models"));
    assert_eq!(
        parent("Services.cs", "InventoryService", Class),
        Some("Warehouse.Services")
    );
    assert_eq!(
        lines("Services.cs", "Warehouse.Services", Module),
        (15, Some(321))
    );
    // A namespace nested in a block namespace.
    assert_eq!(
        parent("Repository.cs", "Specialized", Module),
        Some("Warehouse.Data")
    );
    assert_eq!(
        parent("Repository.cs", "ProductRepository", Class),
        Some("Specialized")
    );
}

#[test]
fn members_of_every_kind_hang_off_their_type() {
    use SymbolKind::{Class, Field, Method};
    assert_eq!(parent("Models.cs", "TryParse", Method), Some("Sku"));
    assert_eq!(parent("Models.cs", "operator +", Method), Some("Price"));
    assert_eq!(parent("Models.cs", "operator string", Method), Some("Sku"));
    assert_eq!(
        parent("Errors.cs", "operator Result", Method),
        Some("Result")
    );
    assert_eq!(parent("Models.cs", "~Order", Method), Some("Order"));
    assert_eq!(parent("Models.cs", "this", Field), Some("Order"));
    assert_eq!(
        parent("Services.cs", "StockChanged", Field),
        Some("InventoryService")
    );
    // An event with add/remove accessors, and its interface declaration.
    let changed = corpus().symbols_named("Changed");
    let owners: Vec<_> = changed.iter().map(|(_, s)| s.parent.as_deref()).collect();
    assert_eq!(owners, [Some("IRepository"), Some("InMemoryRepository")]);
    // Both halves of a partial class are classes named Order (plus its
    // constructor).
    let orders = corpus().symbols_named("Order");
    assert_eq!(orders.iter().filter(|(_, s)| s.kind == Class).count(), 2);
    assert_eq!(parent("Models.cs", "MoveTo", Method), Some("Order"));
    // Types nested in types, at every level.
    assert_eq!(parent("Errors.cs", "Http", Class), Some("ErrorCatalog"));
    assert_eq!(parent("Reports.cs", "Palette", Class), Some("Style"));
    assert_eq!(parent("Reports.cs", "Emphasize", Method), Some("Palette"));
    assert_eq!(
        parent("Errors.cs", "QuotaExceededException", Class),
        Some("RateLimiter")
    );
    // Overloaded constructors are separate symbols.
    let ctors = corpus().symbols_named("ValidationException");
    assert_eq!(ctors.iter().filter(|(_, s)| s.kind == Method).count(), 2);
}

#[test]
fn local_functions_are_functions_with_their_own_calls() {
    use SymbolKind::Function;
    let c = corpus();
    assert_eq!(lines("Services.cs", "Rollback", Function), (203, Some(210)));
    assert_eq!(parent("Services.cs", "Rollback", Function), None);
    c.relation("Services.cs", "Rollback", RelationKind::Calls, "Release");
    c.relation("Reports.cs", "WriteRow", RelationKind::Calls, "Pad");
    // Local functions among top-level statements.
    c.relation("Program.cs", "RunAsync", RelationKind::Calls, "Parse");
    c.relation("Program.cs", "PrintUsage", RelationKind::Calls, "Describe");
    // Top-level statements themselves belong to the file.
    c.relation("Program.cs", "Program", RelationKind::Calls, "Build");
    c.relation("Program.cs", "Program", RelationKind::Calls, "RunAsync");
}

#[test]
fn long_bodies_keep_exact_line_ranges() {
    use SymbolKind::{Class, Method, Module, Struct};
    assert_eq!(lines("Errors.cs", "ErrorCatalog", Class), (271, Some(322)));
    assert_eq!(lines("Errors.cs", "Result", Struct), (161, Some(197)));
    assert_eq!(lines("Models.cs", "MoveTo", Method), (329, Some(338)));
    assert_eq!(lines("Services.cs", "PlaceAsync", Method), (178, Some(211)));
    assert_eq!(
        lines("Repository.cs", "Specialized", Module),
        (198, Some(264))
    );
    // The file-level module ends on the file's last line.
    assert_eq!(lines("Errors.cs", "Errors", Module), (1, Some(379)));
    assert_eq!(lines("Program.cs", "Program", Module), (1, Some(302)));
}

#[test]
fn base_lists_split_into_extends_and_implements_by_bare_name() {
    use RelationKind::{Extends, Implements};
    let c = corpus();
    c.relation(
        "Errors.cs",
        "OutOfStockException",
        Extends,
        "ValidationException",
    );
    // Generic and primary-constructor bases resolve to the bare type name.
    c.relation("Models.cs", "Product", Extends, "Entity");
    c.relation("Models.cs", "StockLine", Extends, "Entity");
    c.relation("Program.cs", "CancelCommand", Extends, "Command");
    c.relation(
        "Repository.cs",
        "ProductRepository",
        Extends,
        "InMemoryRepository",
    );
    // An `IFoo` first in the list is an interface, not a base class.
    c.relation("Models.cs", "Entity", Implements, "IEntity");
    c.relation("Services.cs", "SystemClock", Implements, "IClock");
    c.relation(
        "Repository.cs",
        "InMemoryRepository",
        Implements,
        "IRepository",
    );
    c.relation("Models.cs", "Order", Implements, "IEnumerable");
    // An interface's bases are extends; a record's interfaces implements.
    c.relation("Repository.cs", "IRepository", Extends, "IReadRepository");
    c.relation("Reports.cs", "InventoryRow", Implements, "IRow");
}

#[test]
fn attributes_reference_their_attribute_class() {
    use RelationKind::References;
    let c = corpus();
    c.relation(
        "Errors.cs",
        "NotFoundException",
        References,
        "ErrorCodeAttribute",
    );
    c.relation(
        "Program.cs",
        "ReceiveCommand",
        References,
        "CommandAttribute",
    );
    // On a parameter, and on a record's primary-constructor parameter.
    c.relation(
        "Errors.cs",
        "NotBlank",
        References,
        "CallerArgumentExpressionAttribute",
    );
    c.relation("Reports.cs", "InventoryRow", References, "ColumnAttribute");
}

#[test]
fn calls_through_every_expression_shape_are_extracted() {
    use RelationKind::Calls;
    let c = corpus();
    // Null-conditional, generic and `new` calls.
    c.relation("Repository.cs", "OnChanged", Calls, "Invoke");
    c.relation("Reports.cs", "Render", Calls, "RenderWith");
    c.relation("Program.cs", "RunAsync", Calls, "StatusFor");
    c.relation("Errors.cs", "Found", Calls, "NotFoundException");
    c.relation("Models.cs", "Reserve", Calls, "OutOfStockException");
    // Constructor initializers, expression-bodied and initialized members.
    c.relation("Errors.cs", "ValidationException", Calls, "FieldError");
    c.relation("Models.cs", "Total", Calls, "Aggregate");
    c.relation("Errors.cs", "Entries", Calls, "Build");
    c.relation("Reports.cs", "Text", Calls, "Register");
    c.relation("Models.cs", "this", Calls, "FirstOrDefault");
    // `nameof` is an operator, never a call.
    assert!(c.index().find_callers("nameof").unwrap().is_empty());
}

#[test]
fn cross_file_usings_and_calls_are_extracted() {
    use RelationKind::{Calls, Imports};
    let c = corpus();
    c.relation("Models.cs", "Models", Imports, "Errors");
    c.relation("Services.cs", "Services", Imports, "Specialized");
    c.relation("Repository.cs", "Repository", Imports, "Tasks");
    // An alias `using` imports the alias.
    c.relation("Reports.cs", "Reports", Imports, "Table");
    c.relation("Services.cs", "ReceiveAsync", Calls, "Parse");
    c.relation("Services.cs", "DraftAsync", Calls, "ThrowIfAny");
    c.relation("Reports.cs", "InventoryAsync", Calls, "CountAsync");
    c.relation("Program.cs", "RunAsync", Calls, "Required");
    assert!(c.name_matched_relation_count() >= 120);
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("Explain").unwrap();
    let files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    for file in [
        "Warehouse/Services.cs",
        "Warehouse/Reports.cs",
        "Warehouse/Program.cs",
    ] {
        assert!(
            files.contains(&file),
            "Explain should be called from {file}"
        );
    }

    // Subclassed in Errors.cs, thrown (`new`) from other files.
    let refs = index.find_references("ValidationException").unwrap();
    for file in [
        "Warehouse/Errors.cs",
        "Warehouse/Models.cs",
        "Warehouse/Program.cs",
    ] {
        assert!(refs.iter().any(|r| r.relative_path == file), "{file}");
    }

    let refs = index.find_references("Entity").unwrap();
    let in_models = refs
        .iter()
        .filter(|r| r.relative_path == "Warehouse/Models.cs")
        .count();
    assert!(
        in_models >= 3,
        "Entity referenced {in_models}× in Models.cs"
    );

    let calls = index.find_calls("PlaceAsync").unwrap();
    for callee in [
        "GetAsync",
        "ForSkuAsync",
        "Nearest",
        "Reserve",
        "MoveTo",
        "Rollback",
    ] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "PlaceAsync should call {callee}"
        );
    }
}
