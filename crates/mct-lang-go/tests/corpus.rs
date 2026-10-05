//! Long, multi-file fixture corpus (issue #74): a double-entry bookkeeping
//! library (`ledger/`: errors, money, account, store, service, report) plus
//! its command line front end (`cmd/ledger/main.go`), each file 300–600
//! lines and using the others. The shared checks (size, line ranges, golden
//! snapshot, index round trip, malformed input) come from `mct-corpus`; the
//! tests below pin the constructs and cross-file relations Go is expected to
//! extract, and the documented limits of the parser.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-go/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_go::GoParser;

mct_corpus::standard_tests!(GoParser);

fn lines(path: &str, name: &str, kind: SymbolKind) -> (u32, Option<u32>) {
    let s = corpus().symbol(path, name, kind);
    (s.location.line, s.location.end_line)
}

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn types_of_every_kind_are_extracted() {
    use SymbolKind::{Interface, Struct, TypeAlias};
    for (path, name, kind) in [
        ("errors.go", "Error", Struct),
        ("errors.go", "MultiError", Struct),
        ("errors.go", "Result", Struct),
        ("errors.go", "ErrorCode", TypeAlias),
        ("money.go", "Currency", TypeAlias),
        ("money.go", "Money", Struct),
        ("money.go", "Number", Interface),
        ("account.go", "AccountID", TypeAlias),
        ("account.go", "Tags", TypeAlias),
        ("account.go", "AccountOption", TypeAlias),
        ("account.go", "Store", Interface),
        ("account.go", "Clock", Interface),
        ("account.go", "FixedClock", Struct),
        ("store.go", "Index", Struct),
        ("store.go", "Stack", Struct),
        ("store.go", "Pair", Struct),
        ("service.go", "Hook", Interface),
        ("report.go", "Statement", Struct),
        ("main.go", "command", Struct),
    ] {
        corpus().symbol(path, name, kind);
    }
}

#[test]
fn type_alias_declarations_are_symbols() {
    // `type Amount = Money` is a distinct node from `type Kind uint8`.
    let (line, _) = lines("account.go", "Amount", SymbolKind::TypeAlias);
    assert!(line > 1);
    assert_eq!(
        parent("account.go", "Amount", SymbolKind::TypeAlias),
        Some("ledger")
    );
}

#[test]
fn package_clause_owns_top_level_declarations() {
    use SymbolKind::{Function, Struct};
    assert_eq!(parent("money.go", "Money", Struct), Some("ledger"));
    assert_eq!(parent("money.go", "ParseMoney", Function), Some("ledger"));
    assert_eq!(parent("main.go", "run", Function), Some("main"));
    // Every file of a package restates its own package module, so the name
    // is defined once per file (not deduplicated across files).
    assert_eq!(corpus().symbols_named("ledger").len(), 6);
}

#[test]
fn methods_attach_to_their_receiver_type() {
    use SymbolKind::Method;
    // Value and pointer receivers hang off the same type.
    assert_eq!(parent("money.go", "Add", Method), Some("Money"));
    assert_eq!(parent("errors.go", "Temporary", Method), Some("Error"));
    assert_eq!(parent("errors.go", "Difference", Method), Some("UnbalancedError"));
    assert_eq!(parent("errors.go", "ErrorOrNil", Method), Some("MultiError"));
    // Interface methods belong to the interface.
    assert_eq!(parent("account.go", "Post", Method), Some("Store"));
    assert_eq!(parent("account.go", "Next", Method), Some("IDGenerator"));
    // Same-named methods of different types stay separate symbols.
    let errors: Vec<_> = corpus()
        .symbols_named("Error")
        .into_iter()
        .filter(|(_, s)| s.kind == Method)
        .map(|(_, s)| s.parent.as_deref().unwrap())
        .collect();
    assert_eq!(
        errors,
        ["Error", "LimitError", "UnbalancedError", "MultiError"]
    );
    // Methods of a type are found in other files than its declaration.
    let owners: Vec<_> = corpus()
        .symbols_named("Account")
        .into_iter()
        .filter(|(_, s)| s.kind == Method)
        .map(|(p, s)| (p.to_string(), s.parent.clone()))
        .collect();
    assert!(owners.contains(&("ledger/account.go".into(), Some("Store".into()))));
    assert!(owners.contains(&("ledger/store.go".into(), Some("MemoryStore".into()))));
}

#[test]
fn generic_receivers_attach_to_the_bare_type_name() {
    use SymbolKind::Method;
    // `func (s *Stack[T]) Push` and `func (ix *Index[K, V]) Put`.
    assert_eq!(parent("store.go", "Push", Method), Some("Stack"));
    assert_eq!(parent("store.go", "Pop", Method), Some("Stack"));
    assert_eq!(parent("store.go", "Put", Method), Some("Index"));
    assert_eq!(parent("store.go", "Each", Method), Some("Index"));
    assert_eq!(parent("report.go", "Format", Method), Some("Summary"));
    assert_eq!(parent("errors.go", "OrElse", Method), Some("Result"));
    assert_eq!(parent("store.go", "Swap", Method), Some("Pair"));
}

#[test]
fn long_bodies_keep_exact_line_ranges() {
    use SymbolKind::{Function, Method, Module};
    // The file-level module and the package module end on the last line.
    assert_eq!(lines("errors.go", "errors", Module), (1, Some(475)));
    assert_eq!(lines("errors.go", "ledger", Module), (1, Some(475)));
    let mains: Vec<_> = corpus()
        .symbols_named("main")
        .into_iter()
        .filter(|(_, s)| s.kind == Module)
        .map(|(_, s)| (s.location.line, s.location.end_line))
        .collect();
    assert_eq!(mains, [(1, Some(342)), (1, Some(342))]);
    assert_eq!(lines("service.go", "PostAll", Method), (254, Some(293)));
    let (start, end) = lines("errors.go", "Retry", Function);
    assert!(end.unwrap() - start > 20);
}

#[test]
fn imports_use_the_full_path_of_the_package() {
    use RelationKind::Imports;
    let c = corpus();
    c.relation("errors.go", "errors", Imports, "context");
    c.relation("money.go", "money", Imports, "math/big");
    c.relation("report.go", "report", Imports, "text/tabwriter");
    // Aliased and blank imports keep the path, not the alias.
    c.relation(
        "main.go",
        "main",
        Imports,
        "example.com/ledger/internal/logging",
    );
    c.relation(
        "main.go",
        "main",
        Imports,
        "example.com/ledger/internal/metrics",
    );
    c.relation("main.go", "main", Imports, "example.com/ledger");
}

#[test]
fn calls_through_every_expression_shape_are_extracted() {
    use RelationKind::Calls;
    let c = corpus();
    // Plain, selector and chained calls.
    c.relation("errors.go", "NotFound", Calls, "newError");
    c.relation("account.go", "NewAccount", Calls, "Validate");
    c.relation("service.go", "Transfer", Calls, "Build");
    c.relation("service.go", "Transfer", Calls, "Credit");
    c.relation("service.go", "Transfer", Calls, "NewTx");
    // Calls inside function literals belong to the enclosing function.
    c.relation("errors.go", "Recover", Calls, "recover");
    c.relation("service.go", "PostAll", Calls, "Recover");
    c.relation("service.go", "PostAll", Calls, "Post");
    // Explicit type arguments do not hide the callee.
    c.relation("service.go", "Snapshot", Calls, "Map");
    c.relation("errors.go", "Try", Calls, "Fail");
    c.relation("errors.go", "Try", Calls, "Ok");
    c.relation("store.go", "NewMemoryStore", Calls, "NewIndex");
    // Calls in `defer`, `go` and `select` statements.
    c.relation("store.go", "Stream", Calls, "Walk");
    c.relation("service.go", "reserve", Calls, "Lock");
    c.relation("main.go", "run", Calls, "NotifyContext");
    // Builtins and type conversions look like calls: no type information.
    c.relation("errors.go", "Error", Calls, "len");
    c.relation("money.go", "pow10", Calls, "int64");
}

#[test]
fn cross_file_calls_are_extracted() {
    use RelationKind::Calls;
    let c = corpus();
    c.relation("account.go", "Validate", Calls, "Check");
    c.relation("account.go", "Validate", Calls, "Require");
    c.relation("money.go", "Allocate", Calls, "Invalid");
    c.relation("store.go", "Open", Calls, "Conflict");
    c.relation("store.go", "Post", Calls, "NotFound");
    c.relation("service.go", "NewService", Calls, "NewSequentialIDs");
    c.relation("report.go", "TrialBalance", Calls, "NewWriter");
    c.relation("main.go", "newApp", Calls, "NewMemoryStore");
    c.relation("main.go", "newApp", Calls, "NewService");
    c.relation("main.go", "newApp", Calls, "AuditHook");
    c.relation("main.go", "runTransfer", Calls, "ParseMoney");
    c.relation("main.go", "report", Calls, "Explain");
    assert!(c.cross_file_relation_count() >= 200);
}

#[test]
fn declarations_without_a_symbol_are_the_documented_limits() {
    let c = corpus();
    // package-level const/var and struct fields are not symbols…
    for name in ["ErrNotFound", "DefaultRetry", "SystemClock", "codeNames"] {
        assert!(c.symbols_named(name).is_empty(), "{name} became a symbol");
    }
    assert!(c.symbols_named("Units").is_empty());
    // …so a call in a package-level initializer belongs to the file module.
    c.relation("errors.go", "errors", RelationKind::Calls, "New");
    // Embedding and structural interface satisfaction leave no relation.
    assert!(!c.has_relation("Service", RelationKind::Implements, "Store"));
    assert!(!c.has_relation("MemoryStore", RelationKind::Implements, "Store"));
    assert!(!c.has_relation("Service", RelationKind::Extends, "Store"));
    // Method values and expressions are references, not calls.
    assert!(!c.has_relation("Names", RelationKind::Calls, "name"));
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("newError").unwrap();
    let files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    for file in [
        "ledger/errors.go",
        "ledger/store.go",
        "ledger/service.go",
        "ledger/money.go",
    ] {
        assert!(files.contains(&file), "newError should be called in {file}");
    }

    let callers = index.find_callers("Invalid").unwrap();
    for file in ["ledger/money.go", "ledger/account.go", "ledger/report.go"] {
        assert!(callers.iter().any(|h| h.relative_path == file), "{file}");
    }

    let refs = index.find_references("NewService").unwrap();
    assert!(refs
        .iter()
        .any(|r| r.relative_path == "cmd/ledger/main.go"));

    let calls = index.find_calls("Transfer").unwrap();
    for callee in ["Account", "Convert", "NewTx", "Post", "Build"] {
        assert!(
            calls.iter().any(|h| h.to_name == callee),
            "Transfer should call {callee}"
        );
    }
}
