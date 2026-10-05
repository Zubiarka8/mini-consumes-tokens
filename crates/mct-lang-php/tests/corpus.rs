//! Long, multi-file fixture corpus (issue #74): a lending-library app split
//! into `Support`, `Domain`, `Repository`, `Services`, `Reports` and the
//! `console` entry point, each 300–600 lines and using the others. The
//! shared checks (size, line ranges, golden snapshot, index round trip,
//! malformed input) come from `mct-corpus`; the tests below pin the
//! constructs and cross-file relations this language is expected to extract.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-php/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_php::PhpParser;

mct_corpus::standard_tests!(PhpParser);

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn declarations_of_every_kind_are_extracted() {
    use SymbolKind::{Class, Constant, Enum, Function, Interface, Trait};
    let c = corpus();
    for (path, name, kind) in [
        ("Support.php", "ShelfException", Class),
        ("Support.php", "Arrayable", Interface),
        ("Support.php", "HasEvents", Trait),
        ("Support.php", "Auditable", Trait),
        ("Support.php", "money_format_cents", Function),
        // `const A = 1, B = 2;` at namespace level: one symbol per element.
        ("Support.php", "DEFAULT_CURRENCY", Constant),
        ("Support.php", "MAX_PAGE_SIZE", Constant),
        ("Domain.php", "Genre", Enum),
        ("Domain.php", "LoanStatus", Enum),
        ("Domain.php", "Isbn", Class),
        ("Repository.php", "Searchable", Interface),
        ("Services.php", "wire_services", Function),
        ("Reports.php", "formatter_for", Function),
        ("console.php", "bootstrap", Function),
    ] {
        c.symbol(path, name, kind);
    }
}

#[test]
fn members_hang_off_their_type() {
    use SymbolKind::{Constant, Field, Method};
    // Backed-enum cases, constants and methods.
    assert_eq!(parent("Domain.php", "Mystery", Field), Some("Genre"));
    assert_eq!(parent("Domain.php", "DEFAULT", Constant), Some("Genre"));
    assert_eq!(parent("Domain.php", "fromLabel", Method), Some("Genre"));
    assert_eq!(parent("Domain.php", "isOpen", Method), Some("LoanStatus"));
    // Interface constants and trait members.
    assert_eq!(
        parent("Repository.php", "MIN_QUERY", Constant),
        Some("Searchable")
    );
    assert_eq!(
        parent("Support.php", "releaseEvents", Method),
        Some("HasEvents")
    );
    assert_eq!(parent("Support.php", "audit", Method), Some("Auditable"));
    // Promoted constructor parameters are fields of the class.
    assert_eq!(parent("Domain.php", "isbn", Field), Some("Book"));
    assert_eq!(
        parent("Services.php", "from", Field),
        Some("NotificationService")
    );
    assert_eq!(parent("Reports.php", "columns", Field), Some("Report"));
    // Methods of a `final readonly class`.
    assert_eq!(
        parent("Domain.php", "checksumMatches", Method),
        Some("Isbn")
    );
}

#[test]
fn closures_bound_to_variables_are_top_level_functions() {
    use RelationKind::Calls;
    use SymbolKind::Function;
    let c = corpus();
    // A closure inside a method is a local, not a member of the class.
    assert_eq!(parent("Reports.php", "pad", Function), None);
    c.relation("Reports.php", "pad", Calls, "str_pad");
    // Top-level closures in the entry point, with their own calls.
    assert_eq!(parent("console.php", "main", Function), None);
    c.relation("console.php", "main", Calls, "bootstrap");
    c.relation("console.php", "main", Calls, "run");
    c.relation("console.php", "format", Calls, "formatter_for");
    c.relation("console.php", "format", Calls, "extension");
}

#[test]
fn anonymous_class_members_are_not_symbols() {
    use RelationKind::{Calls, References};
    let c = corpus();
    // `new class ($clock) implements EventListener { ... }` inside
    // wire_services: no symbols for its members, its interface is a
    // reference and its methods' calls belong to wire_services.
    assert!(c.symbols_named("stamp").is_empty());
    let invokes = c.symbols_named("__invoke");
    let owners: Vec<_> = invokes.iter().map(|(_, s)| s.parent.as_deref()).collect();
    assert_eq!(owners, [Some("EventListener"), Some("NotificationService")]);
    c.relation("Services.php", "wire_services", References, "EventListener");
    c.relation("Services.php", "wire_services", Calls, "stamp");
    c.relation("Services.php", "wire_services", Calls, "error_log");
}

#[test]
fn long_bodies_keep_exact_line_ranges() {
    use SymbolKind::Module;
    // The file-level module ends on the file's last line, not one past it.
    assert_eq!(lines("Support.php", "Support", Module), (1, Some(465)));
    assert_eq!(lines("Domain.php", "Domain", Module), (1, Some(492)));
    assert_eq!(lines("console.php", "console", Module), (1, Some(362)));
}

#[test]
fn extends_implements_and_trait_use_are_extracted() {
    use RelationKind::{Extends, Implements};
    let c = corpus();
    c.relation(
        "Support.php",
        "NotFoundException",
        Extends,
        "ShelfException",
    );
    c.relation(
        "Support.php",
        "LoanLimitExceeded",
        Extends,
        "ValidationException",
    );
    c.relation("Support.php", "ShelfException", Extends, "RuntimeException");
    c.relation("Domain.php", "Book", Extends, "Entity");
    c.relation(
        "Repository.php",
        "BookRepository",
        Extends,
        "InMemoryRepository",
    );
    c.relation("Reports.php", "TableFormatter", Extends, "TextFormatter");
    c.relation("console.php", "BorrowCommand", Extends, "Command");
    // Interfaces extending interfaces, including a global one (`\Countable`).
    c.relation("Repository.php", "Searchable", Extends, "Repository");
    c.relation("Repository.php", "Repository", Extends, "Countable");
    // Implemented interfaces, on classes and backed enums.
    c.relation("Domain.php", "Entity", Implements, "Identifiable");
    c.relation("Domain.php", "Entity", Implements, "JsonSerializable");
    c.relation("Domain.php", "Genre", Implements, "HasLabel");
    c.relation("Repository.php", "BookRepository", Implements, "Searchable");
    c.relation("Support.php", "Collection", Implements, "IteratorAggregate");
    // Trait composition, including a trait using traits with an
    // `insteadof`/`as` adaptation block.
    c.relation("Domain.php", "Entity", Implements, "HasEvents");
    c.relation(
        "Repository.php",
        "InMemoryRepository",
        Implements,
        "Loggable",
    );
    c.relation("Services.php", "LendingService", Implements, "Auditable");
    c.relation("Support.php", "Auditable", Implements, "HasEvents");
    c.relation("Support.php", "Auditable", Implements, "Loggable");
}

#[test]
fn attributes_reference_their_attribute_class() {
    use RelationKind::References;
    let c = corpus();
    c.relation("Support.php", "NotFoundException", References, "ErrorCode");
    c.relation("Support.php", "ErrorCode", References, "Attribute");
    c.relation("console.php", "BorrowCommand", References, "AsCommand");
    c.relation("console.php", "HelpCommand", References, "AsCommand");
}

#[test]
fn calls_through_every_expression_shape_are_extracted() {
    use RelationKind::Calls;
    let c = corpus();
    // `$obj->f()`, `Class::f()`, `parent::f()`, `f()`.
    c.relation("Services.php", "borrow", Calls, "byEmail");
    c.relation("Services.php", "borrow", Calls, "forMember");
    c.relation("Support.php", "__construct", Calls, "__construct");
    c.relation("Domain.php", "fromLabel", Calls, "slugify");
    // Nullsafe `?->` calls.
    c.relation("Services.php", "publish", Calls, "dispatchAll");
    c.relation("Support.php", "log", Calls, "__invoke");
    // `new Foo(...)`, `throw new Foo`, and `new` in a promoted parameter's
    // default value.
    c.relation("Domain.php", "close", Calls, "BookReturned");
    c.relation("Support.php", "notBlank", Calls, "ValidationException");
    c.relation("Services.php", "__construct", Calls, "SystemClock");
    c.relation("console.php", "bootstrap", Calls, "JsonFileStore");
    // First-class callables (`$this->inventory(...)`).
    c.relation("Reports.php", "build", Calls, "inventory");
    c.relation("Reports.php", "format", Calls, "cell");
    // `new self`/`new static` name no class.
    assert!(!c.has_relation("of", Calls, "static"));
    assert!(!c.has_relation("parse", Calls, "self"));
}

#[test]
fn cross_file_imports_and_calls_are_extracted() {
    use RelationKind::{Calls, Imports};
    let c = corpus();
    // Plain, grouped, `use function`, `use const` and aliased imports.
    c.relation("Repository.php", "Repository", Imports, "Book");
    c.relation("Domain.php", "Domain", Imports, "Assert");
    c.relation("Domain.php", "Domain", Imports, "days_between");
    c.relation("Domain.php", "Domain", Imports, "DATE_FORMAT");
    c.relation("Domain.php", "Domain", Imports, "Invalid");
    // `require_once 'x.php'` with a literal path; a computed one is skipped.
    c.relation("console.php", "console", Imports, "Support.php");
    c.relation("console.php", "console", Imports, "Reports.php");
    assert!(!c.has_relation("console", Imports, "Repository.php"));
    c.relation("Services.php", "totalFor", Calls, "fineFor");
    c.relation("Reports.php", "overdue", Calls, "money_format_cents");
    c.relation("console.php", "bootstrap", Calls, "wire_services");
    c.relation("console.php", "seed", Calls, "parse");
    assert!(c.name_matched_relation_count() >= 250);
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("money_format_cents").unwrap();
    let files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    for file in [
        "library/Domain.php",
        "library/Services.php",
        "library/Reports.php",
    ] {
        assert!(
            files.contains(&file),
            "money_format_cents should be called from {file}"
        );
    }

    // Declared in Support.php, thrown (`new`) and subclassed elsewhere.
    let refs = index.find_references("ShelfException").unwrap();
    for file in [
        "library/Support.php",
        "library/Repository.php",
        "library/Services.php",
        "library/console.php",
    ] {
        assert!(refs.iter().any(|r| r.relative_path == file), "{file}");
    }

    let calls = index.find_calls("borrow").unwrap();
    for callee in [
        "byEmail", "byIsbn", "openFor", "totalFor", "start", "commit", "audit",
    ] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "borrow should call {callee}"
        );
    }
}
