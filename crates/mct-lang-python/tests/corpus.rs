//! Long, multi-file fixture corpus (issue #74): a lending-library package
//! split into `errors`, `models`, `repository`, `services`, `reports` and
//! `cli`, each 300–600 lines and importing the others. The shared checks
//! (size, line ranges, golden snapshot, index round trip, malformed input)
//! come from `mct-corpus`; the tests below pin the constructs and
//! cross-file relations this language is expected to extract.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-python/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_python::PythonParser;

mct_corpus::standard_tests!(PythonParser);

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn classes_functions_and_modules_are_extracted_with_kinds() {
    let c = corpus();
    for (path, name, kind) in [
        ("errors.py", "LibraryError", SymbolKind::Class),
        ("errors.py", "RateLimiter", SymbolKind::Class),
        ("errors.py", "collect_group", SymbolKind::Function),
        ("models.py", "Genre", SymbolKind::Class),
        ("models.py", "Page", SymbolKind::Class),
        ("models.py", "classify", SymbolKind::Function),
        ("models.py", "café_hours", SymbolKind::Function),
        ("repository.py", "Repository", SymbolKind::Class),
        ("repository.py", "HasKey", SymbolKind::Class),
        ("services.py", "Clock", SymbolKind::Class),
        ("services.py", "first_available", SymbolKind::Function),
        ("services.py", "Pair", SymbolKind::TypeAlias),
        ("reports.py", "Row", SymbolKind::Class),
        ("reports.py", "GenreStats", SymbolKind::Class),
        ("reports.py", "søk", SymbolKind::Function),
        ("reports.py", "概要", SymbolKind::Function),
        ("cli.py", "Command", SymbolKind::Class),
        ("cli.py", "main", SymbolKind::Function),
        ("cli.py", "cli", SymbolKind::Module),
    ] {
        c.symbol(path, name, kind);
    }
}

#[test]
fn methods_and_nested_classes_hang_off_their_class() {
    use SymbolKind::{Class, Method};
    assert_eq!(parent("services.py", "place", Method), Some("HoldQueue"));
    assert_eq!(
        parent("services.py", "send_all", Method),
        Some("ReminderService")
    );
    assert_eq!(
        parent("repository.py", "put_if_version", Method),
        Some("MemoryRepository")
    );
    assert_eq!(parent("reports.py", "rule", Method), Some("Style"));
    // Classes nested in a class body belong to it, at every level.
    assert_eq!(parent("errors.py", "Entry", Class), Some("ErrorCatalog"));
    assert_eq!(parent("reports.py", "Style", Class), Some("Report"));
    assert_eq!(parent("reports.py", "Palette", Class), Some("Style"));
    assert_eq!(parent("reports.py", "pick", Method), Some("Palette"));
    // A class defined inside a function has no class owner; its methods do.
    assert_eq!(parent("reports.py", "Totals", Class), None);
    assert_eq!(parent("reports.py", "count", Method), Some("Totals"));
    // Overload stubs and the implementation are all methods of the class.
    let overloads = corpus().symbols_named("find_loans");
    assert_eq!(overloads.len(), 3);
    assert!(overloads
        .iter()
        .all(|(_, s)| s.kind == Method && s.parent.as_deref() == Some("LendingService")));
    // Property getter, setter and deleter are three methods of one name.
    assert_eq!(corpus().symbols_named("weekday").len(), 3);
}

#[test]
fn nested_functions_and_closures_are_functions_not_methods() {
    use SymbolKind::Function;
    let c = corpus();
    // Inside a method body a `def` is a plain function.
    let late = c.symbol("services.py", "late", Function);
    assert_eq!(late.parent, None);
    for name in ["increment", "reset", "on_open", "on_close", "register"] {
        assert!(
            c.symbols_named(name)
                .iter()
                .all(|(_, s)| s.kind == Function),
            "{name} should be a function"
        );
    }
    // Decorator factories nest two levels deep.
    assert_eq!(c.symbols_named("wrapper").len(), 2);
    assert_eq!(lines("errors.py", "retry", Function), (119, Some(143)));
}

#[test]
fn long_bodies_keep_exact_line_ranges() {
    use SymbolKind::{Class, Function, Method, Module};
    assert_eq!(
        lines("services.py", "LendingService", Class),
        (169, Some(299))
    );
    // A decorated definition starts at `def`, not at the decorator.
    assert_eq!(lines("services.py", "checkout", Method), (210, Some(228)));
    assert_eq!(lines("reports.py", "Report", Class), (56, Some(111)));
    assert_eq!(lines("reports.py", "inventory", Function), (220, Some(241)));
    assert_eq!(lines("cli.py", "main", Function), (267, Some(282)));
    // The file-level module ends on the file's last line.
    assert_eq!(lines("errors.py", "errors", Module), (1, Some(320)));
    assert_eq!(lines("cli.py", "cli", Module), (1, Some(307)));
}

#[test]
fn inheritance_is_recorded_as_extends() {
    let c = corpus();
    c.relation(
        "errors.py",
        "ValidationError",
        RelationKind::Extends,
        "LibraryError",
    );
    c.relation(
        "errors.py",
        "ValidationError",
        RelationKind::Extends,
        "ValueError",
    );
    // Dotted and subscripted bases resolve to their last name.
    c.relation("models.py", "Genre", RelationKind::Extends, "Enum");
    c.relation("repository.py", "Repository", RelationKind::Extends, "ABC");
    c.relation(
        "services.py",
        "EmailNotifier",
        RelationKind::Extends,
        "Notifier",
    );
    c.relation("cli.py", "AddBookCommand", RelationKind::Extends, "Command");
}

#[test]
fn decorators_are_references_and_their_arguments_are_calls() {
    let c = corpus();
    c.relation(
        "services.py",
        "checkout",
        RelationKind::References,
        "audited",
    );
    c.relation("repository.py", "get", RelationKind::References, "retry");
    c.relation(
        "cli.py",
        "ReportCommand",
        RelationKind::References,
        "command",
    );
    c.relation(
        "services.py",
        "on_open",
        RelationKind::References,
        "subscribe",
    );
    // `@renderer(fmt_name("CSV"))` calls fmt_name from the module.
    c.relation("reports.py", "reports", RelationKind::Calls, "fmt_name");
}

#[test]
fn cross_file_imports_and_calls_are_extracted() {
    let c = corpus();
    c.relation(
        "services.py",
        "services",
        RelationKind::Imports,
        "Catalogue",
    );
    c.relation("services.py", "services", RelationKind::Imports, "audited");
    c.relation(
        "reports.py",
        "reports",
        RelationKind::Imports,
        "LendingService",
    );
    // `from . import models as m` imports the alias.
    c.relation("reports.py", "reports", RelationKind::Imports, "m");
    c.relation("cli.py", "cli", RelationKind::Imports, "svc");
    c.relation("cli.py", "cli", RelationKind::Imports, "SqlRepo");
    // An import inside a function belongs to that function.
    c.relation("cli.py", "main", RelationKind::Imports, "audit_trail");
    c.relation("cli.py", "repl", RelationKind::Imports, "shlex");

    c.relation("services.py", "checkout", RelationKind::Calls, "classify");
    c.relation(
        "services.py",
        "return_copy",
        RelationKind::Calls,
        "overdue_fee",
    );
    c.relation(
        "reports.py",
        "inventory",
        RelationKind::Calls,
        "describe_book",
    );
    c.relation(
        "cli.py",
        "open_catalogue",
        RelationKind::Calls,
        "load_fixture",
    );
    c.relation("cli.py", "__init__", RelationKind::Calls, "build_services");
    // Calls in lambdas and comprehensions go to the enclosing symbol.
    c.relation(
        "cli.py",
        "ReportCommand",
        RelationKind::Calls,
        "circulation_report",
    );
    c.relation(
        "reports.py",
        "total_fees",
        RelationKind::Calls,
        "overdue_fee",
    );
    assert!(c.cross_file_relation_count() >= 100);
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("normalize_isbn").unwrap();
    let files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    assert!(files.contains(&"library/models.py") && files.contains(&"library/cli.py"));

    let refs = index.find_references("LibraryError").unwrap();
    for file in ["library/errors.py", "library/services.py", "library/cli.py"] {
        assert!(refs.iter().any(|r| r.relative_path == file), "{file}");
    }

    let calls = index.find_calls("checkout").unwrap();
    for callee in ["is_blocked", "require", "open_loans", "classify", "publish"] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "checkout should call {callee}"
        );
    }
}
