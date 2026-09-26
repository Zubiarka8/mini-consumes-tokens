//! Storage-layer validation of a genuinely polyglot project: does
//! per-language parsed data actually land in SQLite correctly?
//!
//! `tests/fixtures/polyglot-app/` only spans 3 languages (Go, TypeScript,
//! Python). This second, broader fixture — `tests/fixtures/omni-app/` — puts
//! **every language registered in `registry::build_registry()`** into one
//! indexed project at once, plus a `.lua` file for the one implemented-but-
//! deliberately-unregistered parser, plus three ecosystems' manifest files.
//! The point is the storage layer, not the grammars: one `files.language`
//! column, one `symbols` table, one `symbol_relations` table shared by all
//! of them, so this file asserts that nothing collides, nothing is dropped,
//! and each language's cross-file relations still resolve.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index};

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/omni-app")
}

fn open_indexed() -> Index {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&fixture_root(), ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(
        report.issues.is_empty(),
        "no parse issue expected across the whole fixture: {:?}",
        report.issues
    );
    index
}

/// Every source file under `dir`, with its extension, as a flat list —
/// deliberately computed by walking the filesystem rather than by asking the
/// index, so it can be compared *against* the index.
fn walk_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            walk_files(&path, out);
        } else {
            out.push(path);
        }
    }
}

fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default()
        .to_string()
}

/// The expected `(language, file_count, symbol_count)` table for the fixture
/// — every language `build_registry()` wires in, each contributing real
/// symbols. Counts are exact on purpose: a parser regression that silently
/// stops emitting symbols for one language would otherwise hide behind a
/// `> 0` assertion.
const EXPECTED_COVERAGE: &[(&str, usize, usize)] = &[
    ("bash", 2, 6),
    ("cpp", 2, 7),
    ("csharp", 2, 9),
    ("css", 1, 5),
    ("go", 2, 9),
    ("html", 1, 3),
    ("java", 2, 8),
    ("javascript_typescript", 2, 8),
    ("kotlin", 2, 9),
    ("markdown", 2, 4),
    ("php", 2, 7),
    ("powershell", 2, 5),
    ("python", 2, 7),
    ("rust", 2, 6),
    ("xaml", 1, 4),
    ("xml", 1, 4),
];

#[test]
fn every_registered_language_lands_in_the_files_language_column_with_real_symbols() {
    let index = open_indexed();
    let status = index.status().unwrap();

    for (language, files, symbols) in EXPECTED_COVERAGE {
        let coverage = status
            .languages
            .iter()
            .find(|l| l.language == *language)
            .unwrap_or_else(|| {
                panic!(
                    "no `{language}` row in files.language: {:?}",
                    status.languages
                )
            });
        assert_eq!(coverage.file_count, *files, "{language} file count");
        assert_eq!(coverage.symbol_count, *symbols, "{language} symbol count");
    }

    assert_eq!(
        status.languages.len(),
        EXPECTED_COVERAGE.len(),
        "unexpected extra language row: {:?}",
        status.languages
    );
    assert_eq!(
        status.total_files,
        EXPECTED_COVERAGE.iter().map(|(_, f, _)| f).sum::<usize>()
    );
    assert_eq!(
        status.total_symbols,
        EXPECTED_COVERAGE.iter().map(|(_, _, s)| s).sum::<usize>()
    );
    assert!(
        status.syntax_errors.is_empty(),
        "{:?}",
        status.syntax_errors
    );
}

#[test]
fn the_registry_covers_every_language_the_fixture_exercises() {
    // Guards the fixture itself: if a language is added to `build_registry()`
    // and nobody extends `omni-app/`, the coverage table above would keep
    // passing while silently testing one language less than the product ships.
    let registry = mct_mcp_server::registry::build_registry();
    let mut registered: Vec<&str> = registry.language_ids();
    registered.sort_unstable();
    let mut covered: Vec<&str> = EXPECTED_COVERAGE.iter().map(|(l, _, _)| *l).collect();
    covered.sort_unstable();
    assert_eq!(
        registered, covered,
        "omni-app must exercise exactly the languages build_registry() wires in"
    );
}

#[test]
fn no_supported_file_is_silently_dropped_from_the_index() {
    let registry: LanguageRegistry = mct_mcp_server::registry::build_registry();
    let mut on_disk = Vec::new();
    walk_files(&fixture_root(), &mut on_disk);

    let supported: Vec<&PathBuf> = on_disk
        .iter()
        .filter(|p| registry.for_extension(&extension_of(p)).is_some())
        .collect();
    assert!(!supported.is_empty(), "fixture walk found nothing");

    let index = open_indexed();
    let status = index.status().unwrap();
    assert_eq!(
        status.total_files,
        supported.len(),
        "every file with a registered extension must have a `files` row; on disk: {supported:?}"
    );
}

#[test]
fn a_lua_file_is_skipped_without_any_diagnostic_because_lua_is_not_registered() {
    // `crates/mct-lang-lua` implements `LanguageParser` but is deliberately
    // NOT wired into `build_registry()` (it is the plugin-architecture proof,
    // not a shipped language — see internal/checklist.md). `.lua` is also
    // absent from `KNOWN_PENDING_LANGUAGES`, so the file is dropped with
    // *no* issue, *no* `unsupported_languages` entry and *no* `files` row.
    // Pinned here because it is a silent hole, not because it is desirable.
    assert!(
        fixture_root().join("luatools/build.lua").is_file(),
        "fixture precondition: the .lua file must exist"
    );

    let index = open_indexed();
    let status = index.status().unwrap();

    assert!(
        !status.languages.iter().any(|l| l.language == "lua"),
        "lua is not a registered language: {:?}",
        status.languages
    );
    assert!(
        status.unsupported_languages.is_empty(),
        "a .lua file produces no `unsupported language` diagnostic at all: {:?}",
        status.unsupported_languages
    );
    assert!(
        index
            .list_symbols("luatools", None, None)
            .unwrap()
            .is_empty(),
        "nothing from the .lua file reaches the index"
    );
}

#[test]
fn same_named_symbols_from_different_languages_coexist_as_distinct_rows() {
    let index = open_indexed();

    // `deploy`: a Python function, its Python file-module, and a Bash
    // file-module — three rows under one name, each keeping its own
    // language/kind/path.
    let deploys = index.find_symbol("deploy").unwrap();
    let mut seen: Vec<(&str, &str, &str)> = deploys
        .iter()
        .map(|h| {
            (
                h.language.as_str(),
                h.relative_path.as_str(),
                h.kind.as_str(),
            )
        })
        .collect();
    seen.sort_unstable();
    assert_eq!(
        seen,
        vec![
            ("bash", "shell/deploy.sh", "module"),
            ("python", "pyscripts/deploy.py", "function"),
            ("python", "pyscripts/deploy.py", "module"),
        ],
        "{deploys:?}"
    );

    // `accumulate_cents`: declared in the C++ header and defined in the C++
    // translation unit — same name, same language, two files, two rows.
    let cpp = index.find_symbol("accumulate_cents").unwrap();
    assert_eq!(cpp.len(), 2, "{cpp:?}");
    assert!(cpp.iter().all(|h| h.language == "cpp"));
    assert_eq!(
        cpp.iter()
            .map(|h| h.relative_path.as_str())
            .collect::<Vec<_>>(),
        vec!["native/arithmetic.cpp", "native/arithmetic.hpp"]
    );

    // `Invoice`: the Java class plus its synthetic file-module entry — same
    // name, same file, different kinds, still two independent rows.
    let invoices = index.find_symbol("Invoice").unwrap();
    assert_eq!(invoices.len(), 2, "{invoices:?}");
    let mut kinds: Vec<&str> = invoices.iter().map(|h| h.kind.as_str()).collect();
    kinds.sort_unstable();
    assert_eq!(kinds, vec!["class", "module"]);
}

/// `(caller, callee, file)` — one intra-language, cross-file (or
/// cross-declaration) call per language that emits `Calls` at all.
/// Declarative languages (html/css/xml/xaml/markdown) never emit `Calls`, so
/// they are covered by `find_references`/`list_symbols` instead.
const EXPECTED_CALLS: &[(&str, &str, &str)] = &[
    ("HandlePost", "NewLedger", "backend/server.go"),
    ("renderLine", "formatAmount", "frontend/apiClient.ts"),
    ("deploy", "send_alert", "pyscripts/deploy.py"),
    ("charge", "addItem", "jvm/BillingService.java"),
    ("runOnce", "buildRepository", "jvm/Main.kt"),
    ("Process", "Authorize", "dotnet/PaymentProcessor.cs"),
    ("settle_totals", "accumulate_cents", "native/arithmetic.cpp"),
    ("handle_request", "load_rows", "web/api.php"),
    ("run_tally", "settle", "rustcore/driver.rs"),
    ("deploy_all", "build_artifact", "shell/deploy.sh"),
    ("Invoke-Deploy", "New-Artifact", "pwsh/Deploy.ps1"),
];

#[test]
fn cross_file_relations_resolve_inside_each_language() {
    let index = open_indexed();
    for (caller, callee, file) in EXPECTED_CALLS {
        let calls = index.find_calls(caller).unwrap();
        assert!(
            calls
                .iter()
                .any(|c| c.to_name == *callee && c.relative_path == *file),
            "{caller} -> {callee} missing in {file}: {calls:?}"
        );
        // The inverse direction must agree, through the same relation rows.
        let callers = index.find_callers(callee).unwrap();
        assert!(
            callers
                .iter()
                .any(|c| c.from_symbol == *caller && c.relative_path == *file),
            "inverse {callee} <- {caller} missing in {file}: {callers:?}"
        );
    }
}

#[test]
fn a_callee_defined_in_another_file_resolves_across_the_file_boundary() {
    let index = open_indexed();

    // Bash: `log_line` is defined in shell/lib.sh and called from both files.
    let refs = index.find_references("log_line").unwrap();
    let files: Vec<&str> = refs.iter().map(|r| r.relative_path.as_str()).collect();
    assert!(files.contains(&"shell/deploy.sh"), "{refs:?}");
    assert!(files.contains(&"shell/lib.sh"), "{refs:?}");
    assert!(refs.iter().all(|r| r.language == "bash"), "{refs:?}");

    // PowerShell: same shape, `Write-Line` defined in Common.psm1.
    let refs = index.find_references("Write-Line").unwrap();
    let files: Vec<&str> = refs.iter().map(|r| r.relative_path.as_str()).collect();
    assert!(files.contains(&"pwsh/Common.psm1"), "{refs:?}");
    assert!(files.contains(&"pwsh/Deploy.ps1"), "{refs:?}");

    // TypeScript: the import relation and the call relation both land.
    let refs = index.find_references("formatAmount").unwrap();
    let kinds: Vec<&str> = refs.iter().map(|r| r.kind.as_str()).collect();
    assert!(kinds.contains(&"imports"), "{refs:?}");
    assert!(kinds.contains(&"calls"), "{refs:?}");
    assert!(
        refs.iter().all(|r| r.language == "javascript_typescript"),
        "{refs:?}"
    );
}

#[test]
fn a_symbol_name_shared_by_two_languages_does_not_leak_relations_between_them() {
    let index = open_indexed();

    // `deploy` names a Python function and a Bash file-module. A name-keyed
    // relation query therefore *merges* both — documented behaviour of a
    // name-resolved (not type-resolved) graph, pinned here so a future
    // change that made it worse (e.g. dropping one side) is visible.
    let calls = index.find_calls("deploy").unwrap();
    let languages: Vec<&str> = calls.iter().map(|c| c.language.as_str()).collect();
    assert!(languages.contains(&"python"), "{calls:?}");
    assert!(languages.contains(&"bash"), "{calls:?}");
    assert!(
        calls
            .iter()
            .all(|c| c.relative_path.starts_with("pyscripts/")
                || c.relative_path.starts_with("shell/")),
        "no third language may join in: {calls:?}"
    );

    // A name unique to one language stays inside it.
    let refs = index.find_references("accumulate_cents").unwrap();
    assert!(!refs.is_empty());
    assert!(refs.iter().all(|r| r.language == "cpp"), "{refs:?}");
}

#[test]
fn list_symbols_partitions_the_fixture_cleanly_by_directory_and_by_language() {
    let index = open_indexed();

    // Directory-prefix form: `jvm/` holds both Java and Kotlin.
    let jvm = index.list_symbols("jvm", None, None).unwrap();
    let mut langs: Vec<&str> = jvm.iter().map(|e| e.language.as_str()).collect();
    langs.sort_unstable();
    langs.dedup();
    assert_eq!(langs, vec!["java", "kotlin"]);

    // ...narrowed by language, the Kotlin half drops out.
    let java_only = index.list_symbols("jvm", None, Some("java")).unwrap();
    assert!(
        java_only.iter().all(|e| e.language == "java"),
        "{java_only:?}"
    );
    assert!(java_only.iter().any(|e| e.name == "addItem"));
    assert!(!java_only.iter().any(|e| e.name == "buildRepository"));

    // Exact-file form on a language with no functions at all.
    let css = index.list_symbols("ui/styles.css", None, None).unwrap();
    assert!(css.iter().all(|e| e.relative_path == "ui/styles.css"));
    assert!(
        css.iter().any(|e| e.kind == "rule" && e.name == ".ledger"),
        "{css:?}"
    );

    // `kind` + `language` combined across the whole fixture's markup half.
    let elements = index
        .list_symbols("config", Some("element"), Some("xml"))
        .unwrap();
    assert_eq!(elements.len(), 3, "{elements:?}");
    assert!(elements
        .iter()
        .all(|e| e.kind == "element" && e.language == "xml"));
}

#[test]
fn every_symbol_row_carries_a_usable_line_range() {
    let index = open_indexed();
    for (language, _, _) in EXPECTED_COVERAGE {
        let entries = index.list_symbols("", None, Some(language)).unwrap();
        // `list_symbols("")` is a prefix match on `/%` and matches nothing —
        // go through the real directories instead.
        assert!(
            entries.is_empty(),
            "empty path is not a wildcard: {entries:?}"
        );
    }

    for dir in [
        "backend",
        "frontend",
        "pyscripts",
        "jvm",
        "dotnet",
        "native",
        "web",
        "rustcore",
        "shell",
        "pwsh",
        "ui",
        "config",
        "docs",
    ] {
        let entries = index.list_symbols(dir, None, None).unwrap();
        assert!(!entries.is_empty(), "`{dir}` indexed nothing");
        for entry in entries {
            assert!(entry.line >= 1, "{entry:?}");
            let end = entry
                .end_line
                .unwrap_or_else(|| panic!("no end_line on {entry:?}"));
            assert!(end >= entry.line, "end_line before line: {entry:?}");
        }
    }
}

// ------------------------------------------------- reindex over real source

/// Copies the fixture into a fresh temp directory so a test can edit it.
/// Never touches the versioned fixture, and never the repo's own index.
fn temp_copy_of_fixture(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
        + COUNTER.fetch_add(1, Ordering::Relaxed);
    let dest = std::env::temp_dir().join(format!("mct-omni-{tag}-{unique}"));
    copy_dir(&fixture_root(), &dest);
    dest
}

fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}

#[test]
fn an_incremental_reindex_of_a_real_polyglot_tree_picks_up_one_edit_and_drops_its_stale_symbols() {
    let root = temp_copy_of_fixture("incremental");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let first = index.reindex(&registry, false).unwrap();
    assert_eq!(first.files_parsed, 28);

    assert_eq!(index.find_symbol("Describe").unwrap().len(), 1);
    assert!(index
        .find_calls("HandlePost")
        .unwrap()
        .iter()
        .any(|c| c.to_name == "Describe"));

    // Rename the Go helper and drop the call to it.
    std::fs::write(
        root.join("backend/server.go"),
        "package backend\n\n\
         import \"fmt\"\n\n\
         func HandlePost(account string, amount int) string {\n\
         \tledger := NewLedger(account)\n\
         \ttotal := ledger.Post(amount)\n\
         \treturn fmt.Sprintf(\"%s=%d\", account, total)\n\
         }\n",
    )
    .unwrap();

    let second = index.reindex(&registry, false).unwrap();
    assert_eq!(second.files_parsed, 1, "only the edited file re-parses");
    assert_eq!(second.files_unchanged, 27);
    assert_eq!(second.files_removed, 0);

    assert!(
        index.find_symbol("Describe").unwrap().is_empty(),
        "the deleted Go function must be gone from the index"
    );
    assert!(
        !index
            .find_calls("HandlePost")
            .unwrap()
            .iter()
            .any(|c| c.to_name == "Describe"),
        "its call relation must be gone with it"
    );
    // Every other language is untouched by a Go-only edit.
    let status = index.status().unwrap();
    assert_eq!(status.languages.len(), EXPECTED_COVERAGE.len());
    for (language, files, symbols) in EXPECTED_COVERAGE.iter().filter(|(l, _, _)| *l != "go") {
        let coverage = status
            .languages
            .iter()
            .find(|l| l.language == *language)
            .unwrap();
        assert_eq!(
            (coverage.file_count, coverage.symbol_count),
            (*files, *symbols),
            "{language}"
        );
    }

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn deleting_a_file_from_a_real_polyglot_tree_removes_only_that_languages_rows() {
    let root = temp_copy_of_fixture("delete");
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();

    std::fs::remove_file(root.join("ui/styles.css")).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert_eq!(report.files_removed, 1);
    assert_eq!(report.files_parsed, 0);

    let status = index.status().unwrap();
    assert!(
        !status.languages.iter().any(|l| l.language == "css"),
        "css was a single-file language; its row must disappear entirely: {:?}",
        status.languages
    );
    assert_eq!(status.languages.len(), EXPECTED_COVERAGE.len() - 1);
    assert!(index
        .list_symbols("ui/styles.css", None, None)
        .unwrap()
        .is_empty());
    // The HTML sibling in the same directory is untouched.
    assert!(!index
        .list_symbols("ui/index.html", None, None)
        .unwrap()
        .is_empty());

    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn manifests_from_three_ecosystems_coexist_in_one_index() {
    let index = open_indexed();
    let status = index.status().unwrap();

    let by_path = |path: &str| {
        status
            .dependencies
            .iter()
            .find(|m| m.manifest_path == path)
            .unwrap_or_else(|| panic!("no manifest row for {path}: {:?}", status.dependencies))
    };

    let npm = by_path("package.json");
    assert_eq!(npm.language, "javascript_typescript");
    assert!(npm.dependencies.iter().any(|d| d.name == "left-pad"));
    assert!(npm.dependencies.iter().any(|d| d.name == "typescript"));

    let go = by_path("go.mod");
    assert_eq!(go.language, "go");
    assert_eq!(go.dependencies.len(), 1);
    assert_eq!(go.dependencies[0].name, "github.com/google/uuid");
    assert_eq!(go.dependencies[0].version.as_deref(), Some("v1.6.0"));

    let py = by_path("requirements.txt");
    assert_eq!(py.language, "python");
    assert!(py.dependencies.iter().any(|d| d.name == "requests"));

    // Manifests are dependency rows, never `files` rows — they must not
    // inflate the per-language file counts asserted above.
    assert_eq!(status.dependencies.len(), 3, "{:?}", status.dependencies);
}
