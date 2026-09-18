//! Validates that the index can hold and query symbols from several
//! languages in the same project without them bleeding into each other.
//! Fixture at `tests/fixtures/polyglot-app/`: a Go backend, a TypeScript
//! frontend, and a Python deploy script — representative of a real small
//! service, not a synthetic single-symbol-per-language sample. This is the
//! fixture deferred since the polyglot repo was first validated manually
//! (Rust + Python only) back when just 2 real languages existed; now that
//! 7 real languages plus Lua are implemented, it is fixed as a proper
//! versioned integration fixture covering 3 of them at once.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;

use mct_index::{ExcludeSet, Index};

fn fixture_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/polyglot-app")
}

fn open_indexed() -> Index {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture_root(), ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(
        report.files_parsed, 7,
        "backend/{{invoice,logger,server}}.go, frontend/{{format,apiClient}}.ts, scripts/{{notify,deploy}}.py"
    );
    assert!(report.issues.is_empty(), "no parse issues expected: {:?}", report.issues);
    index
}

#[test]
fn status_reports_coverage_for_all_three_languages_independently() {
    let index = open_indexed();
    let status = index.status().unwrap();

    let go = status.languages.iter().find(|l| l.language == "go").unwrap();
    assert_eq!(go.file_count, 3, "invoice.go, logger.go, server.go");

    let ts = status
        .languages
        .iter()
        .find(|l| l.language == "javascript_typescript")
        .unwrap();
    assert_eq!(ts.file_count, 2, "format.ts, apiClient.ts");

    let python = status.languages.iter().find(|l| l.language == "python").unwrap();
    assert_eq!(python.file_count, 2, "notify.py, deploy.py");

    assert_eq!(status.languages.len(), 3, "no unexpected fourth language: {:?}", status.languages);
    assert!(status.unsupported_languages.is_empty());
    assert!(status.syntax_errors.is_empty());
}

#[test]
fn find_symbol_resolves_each_language_independently() {
    let index = open_indexed();

    let go_hits = index.find_symbol("AddItem").unwrap();
    assert!(go_hits.iter().any(|h| h.relative_path == "backend/invoice.go"), "{go_hits:?}");

    let ts_hits = index.find_symbol("createInvoice").unwrap();
    assert!(ts_hits.iter().any(|h| h.relative_path == "frontend/apiClient.ts"), "{ts_hits:?}");

    let py_hits = index.find_symbol("deploy").unwrap();
    assert!(py_hits.iter().any(|h| h.relative_path == "scripts/deploy.py"), "{py_hits:?}");
}

#[test]
fn find_calls_stay_within_their_own_language() {
    let index = open_indexed();

    // Go: HandleCreateInvoice -> AddItem -> Log.
    let go_calls = index.find_calls("HandleCreateInvoice").unwrap();
    assert!(go_calls.iter().any(|c| c.to_name == "AddItem" && c.relative_path == "backend/server.go"));

    // TS: createInvoice -> formatTotal.
    let ts_calls = index.find_calls("createInvoice").unwrap();
    assert!(ts_calls.iter().any(|c| c.to_name == "formatTotal" && c.relative_path == "frontend/apiClient.ts"));

    // Python: deploy -> send_alert.
    let py_calls = index.find_calls("deploy").unwrap();
    assert!(py_calls.iter().any(|c| c.to_name == "send_alert" && c.relative_path == "scripts/deploy.py"));
}

#[test]
fn find_references_do_not_cross_the_three_languages() {
    let index = open_indexed();

    // "Log" only exists as a Go symbol; a name collision with the TS/Python
    // fixtures would show up here as extra hits from the wrong language.
    let refs = index.find_references("Log").unwrap();
    assert!(!refs.is_empty(), "Log is called from invoice.go");
    assert!(
        refs.iter().all(|r| r.relative_path.ends_with(".go")),
        "Log must only resolve within the Go fixture: {refs:?}"
    );
}
