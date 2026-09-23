//! End-to-end exercise of the whole MCP query surface against the broad
//! polyglot fixture (`tests/fixtures/omni-app/`, see `omni_fixture.rs`).
//!
//! `tools.rs` already covers the tool layer against the 2-language
//! `compute-app` fixture; what it does not cover is (a) the two index
//! administration tools — `get_indexing_status` and `reindex` had no test at
//! all — and (b) `depth`/`offset` on `find_references` and
//! `impact_analysis`, and `offset` on `find_calls`. Those, plus every other
//! tool re-run across 16 languages at once, are what this file adds.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::{
    FindCallersArgs, FindCallsArgs, FindReferencesArgs, FindSymbolArgs, GetFileSkeletonArgs,
    GetProjectOverviewArgs, ImpactAnalysisArgs, ListSymbolsArgs, MctServer, ReindexArgs,
};
use rmcp::handler::server::wrapper::Parameters;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/omni-app")
}

fn content_of(result: &rmcp::model::CallToolResult) -> String {
    result
        .content
        .first()
        .and_then(|block| block.as_text())
        .map(|t| t.text.clone())
        .unwrap_or_default()
}

async fn build_server() -> MctServer {
    build_server_at(&fixture_root()).await
}

async fn build_server_at(root: &Path) -> MctServer {
    let registry = mct_mcp_server::registry::build_registry();
    let mut index = Index::open_in_memory(root, ExcludeSet::default()).unwrap();
    index.reindex(&registry, false).unwrap();
    MctServer::new(index, registry)
}

fn list_args(path: &str, kind: Option<&str>, language: Option<&str>) -> ListSymbolsArgs {
    ListSymbolsArgs {
        path: path.to_string(),
        kind: kind.map(str::to_string),
        language: language.map(str::to_string),
        limit: None,
    }
}

// ---------------------------------------------------------------- discovery

#[tokio::test]
async fn list_symbols_exact_file_path_form_works_for_every_language_family() {
    let server = build_server().await;
    // One file per "family": brace-delimited, indentation-delimited, shell,
    // markup and stylesheet — the exact-path branch must not be
    // language-specific.
    for (path, expected) in [
        ("jvm/Invoice.java", "addItem"),
        ("dotnet/PaymentProcessor.cs", "Process"),
        ("native/arithmetic.hpp", "Totals"),
        ("web/repository.php", "RowStore"),
        ("rustcore/tally.rs", "add_cents"),
        ("shell/lib.sh", "RELEASE_CHANNEL"),
        ("pwsh/Common.psm1", "New-Artifact"),
        ("ui/index.html", "ledger-root"),
        ("ui/styles.css", ".ledger-title"),
        ("ui/MainWindow.xaml", "PostButton"),
        ("config/app.xml", "billing"),
        ("docs/architecture.md", "Ledger flow"),
    ] {
        let text = content_of(&server.list_symbols(Parameters(list_args(path, None, None))).await.unwrap());
        assert!(text.contains(expected), "`{path}` did not surface `{expected}`: {text}");
        assert!(text.contains(&format!("under `{path}`")), "got: {text}");
    }
}

#[tokio::test]
async fn list_symbols_directory_prefix_form_spans_two_languages_in_one_directory() {
    let server = build_server().await;
    let text = content_of(&server.list_symbols(Parameters(list_args("jvm", None, None))).await.unwrap());
    // Java and Kotlin share `jvm/`; the prefix form must return both, each
    // entry carrying its own path since the listing spans several files.
    assert!(text.contains("jvm/Invoice.java"), "got: {text}");
    assert!(text.contains("jvm/Repository.kt"), "got: {text}");
    assert!(text.contains("addItem"), "got: {text}");
    assert!(text.contains("buildRepository"), "got: {text}");
    // A sibling directory must not bleed in.
    assert!(!text.contains("dotnet/"), "got: {text}");
}

#[tokio::test]
async fn list_symbols_language_filter_splits_a_shared_directory() {
    let server = build_server().await;
    let kotlin = content_of(
        &server
            .list_symbols(Parameters(list_args("jvm", None, Some("kotlin"))))
            .await
            .unwrap(),
    );
    assert!(kotlin.contains("buildRepository"), "got: {kotlin}");
    assert!(!kotlin.contains("addItem"), "java must be filtered out: {kotlin}");
}

#[tokio::test]
async fn list_symbols_kind_filter_reaches_non_code_kinds_too() {
    let server = build_server().await;
    let rules = content_of(
        &server
            .list_symbols(Parameters(list_args("ui", Some("rule"), None)))
            .await
            .unwrap(),
    );
    assert!(rules.contains("Rules:"), "got: {rules}");
    assert!(rules.contains(".ledger"), "got: {rules}");
    // `element` entries from the HTML/XAML siblings under `ui/` are excluded.
    assert!(!rules.contains("PostButton"), "got: {rules}");
}

// ------------------------------------------------------------------- lookup

#[tokio::test]
async fn find_symbol_returns_every_language_holding_that_name() {
    let server = build_server().await;
    let text = content_of(
        &server
            .find_symbol(Parameters(FindSymbolArgs {
                name: "deploy".to_string(),
                match_mode: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("3 definition(s) of `deploy`"), "got: {text}");
    assert!(text.contains("[python] function deploy"), "got: {text}");
    assert!(text.contains("[bash] module deploy"), "got: {text}");
}

#[tokio::test]
async fn find_symbol_on_an_unknown_name_says_so_rather_than_erroring() {
    let server = build_server().await;
    let text = content_of(
        &server
            .find_symbol(Parameters(FindSymbolArgs {
                name: "no_such_symbol_anywhere".to_string(),
                match_mode: None,
                limit: None,
            }))
            .await
            .unwrap(),
    );
    assert_eq!(text, "No symbol named `no_such_symbol_anywhere` found in the index.");
}

// --------------------------------------------------------------- traversal

/// The PHP chain `describe_request -> handle_request -> load_rows` is the
/// fixture's canonical two-hop path, used by every depth test below.
#[tokio::test]
async fn find_calls_depth_two_reaches_the_second_hop_and_tags_it() {
    let server = build_server().await;
    let direct = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                function: "describe_request".to_string(),
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(direct.contains("handle_request"), "got: {direct}");
    assert!(!direct.contains("load_rows"), "depth 1 must stop at the direct hits: {direct}");

    let deep = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                function: "describe_request".to_string(),
                limit: None,
                depth: Some(2),
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(deep.contains("load_rows"), "got: {deep}");
    assert!(deep.contains("[depth 2]"), "second-hop hits must be tagged: {deep}");
}

#[tokio::test]
async fn find_calls_offset_pages_through_a_multi_callee_function() {
    let server = build_server().await;
    let all = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                function: "HandlePost".to_string(),
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(all.contains("3 call(s) made by this function"), "got: {all}");

    let page = content_of(
        &server
            .find_calls(Parameters(FindCallsArgs {
                function: "HandlePost".to_string(),
                limit: Some(1),
                depth: None,
                offset: Some(2),
            }))
            .await
            .unwrap(),
    );
    assert!(page.contains("showing 1 starting at offset 2"), "got: {page}");
    assert!(page.contains("0 more available"), "got: {page}");
    // The first page's callee must not reappear on the last page.
    assert!(page.contains("Describe"), "got: {page}");
    assert!(!page.contains("--calls--> NewLedger"), "got: {page}");
}

#[tokio::test]
async fn find_callers_depth_two_walks_the_call_graph_backwards() {
    let server = build_server().await;
    let deep = content_of(
        &server
            .find_callers(Parameters(FindCallersArgs {
                function: "load_rows".to_string(),
                limit: None,
                depth: Some(2),
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(deep.contains("handle_request"), "direct caller: {deep}");
    assert!(deep.contains("describe_request"), "caller-of-caller: {deep}");
    assert!(deep.contains("[depth 2]"), "got: {deep}");
}

#[tokio::test]
async fn find_references_honours_depth_beyond_the_direct_hits() {
    let server = build_server().await;
    let direct = content_of(
        &server
            .find_references(Parameters(FindReferencesArgs {
                symbol: "load_rows".to_string(),
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(direct.contains("handle_request"), "got: {direct}");
    assert!(!direct.contains("describe_request"), "depth 1 stops here: {direct}");

    let deep = content_of(
        &server
            .find_references(Parameters(FindReferencesArgs {
                symbol: "load_rows".to_string(),
                limit: None,
                depth: Some(2),
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(deep.contains("describe_request"), "got: {deep}");
    assert!(deep.contains("[depth 2]"), "got: {deep}");
}

#[tokio::test]
async fn find_references_offset_pages_past_the_first_hit() {
    let server = build_server().await;
    let all = content_of(
        &server
            .find_references(Parameters(FindReferencesArgs {
                symbol: "formatAmount".to_string(),
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(all.contains("3 reference(s)"), "got: {all}");

    let page = content_of(
        &server
            .find_references(Parameters(FindReferencesArgs {
                symbol: "formatAmount".to_string(),
                limit: Some(2),
                depth: None,
                offset: Some(1),
            }))
            .await
            .unwrap(),
    );
    assert!(page.contains("showing 2 starting at offset 1"), "got: {page}");
    assert!(page.contains("0 more available"), "got: {page}");
}

#[tokio::test]
async fn impact_analysis_combines_callers_and_references_across_a_two_hop_radius() {
    let server = build_server().await;
    let shallow = content_of(
        &server
            .impact_analysis(Parameters(ImpactAnalysisArgs {
                symbol: "load_rows".to_string(),
                limit: None,
                depth: None,
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(shallow.contains("handle_request"), "got: {shallow}");
    assert!(!shallow.contains("describe_request"), "got: {shallow}");

    let deep = content_of(
        &server
            .impact_analysis(Parameters(ImpactAnalysisArgs {
                symbol: "load_rows".to_string(),
                limit: None,
                depth: Some(3),
                offset: None,
            }))
            .await
            .unwrap(),
    );
    assert!(deep.contains("describe_request"), "blast radius must widen with depth: {deep}");
}

#[tokio::test]
async fn impact_analysis_offset_applies_to_its_caller_and_reference_sections() {
    let server = build_server().await;
    let page = content_of(
        &server
            .impact_analysis(Parameters(ImpactAnalysisArgs {
                symbol: "formatAmount".to_string(),
                limit: Some(1),
                depth: None,
                offset: Some(1),
            }))
            .await
            .unwrap(),
    );
    assert!(page.contains("starting at offset 1"), "got: {page}");
}

// --------------------------------------------------------------- rendering

#[tokio::test]
async fn get_file_skeleton_collapses_bodies_for_a_brace_language_other_than_rust() {
    let server = build_server().await;
    let text = content_of(
        &server
            .get_file_skeleton(Parameters(GetFileSkeletonArgs {
                path: "web/repository.php".to_string(),
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("function load_rows($account)"), "got: {text}");
    assert!(text.contains("// ..."), "bodies must be elided: {text}");
    // The body's actual statement must be gone.
    assert!(!text.contains("return [$account => 0];"), "got: {text}");
    // The synthetic whole-file `module` entry is never rendered as a block.
    assert!(!text.contains("2 top-level symbol(s)") || text.contains("class RowStore"), "got: {text}");
}

#[tokio::test]
async fn get_file_skeleton_falls_back_to_declaration_lines_for_a_non_brace_language() {
    let server = build_server().await;
    let text = content_of(
        &server
            .get_file_skeleton(Parameters(GetFileSkeletonArgs {
                path: "pyscripts/notify.py".to_string(),
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("Skeleton of `pyscripts/notify.py`"), "got: {text}");
    assert!(text.contains("def send_alert(channel, message):"), "got: {text}");
    assert!(!text.contains("return f\"{channel}"), "the body must not be printed: {text}");
}

/// Languages whose parsers give every top-level declaration a `parent` (the
/// package, namespace or synthetic file-module symbol) rather than `None`.
const PARENTED_TOP_LEVEL_LANGUAGES: &[(&str, &str)] = &[
    ("backend/server.go", "go"),
    ("dotnet/PaymentProcessor.cs", "csharp"),
    ("shell/lib.sh", "bash"),
    ("pwsh/Common.psm1", "powershell"),
];

#[tokio::test]
#[ignore = "product bug: get_file_skeleton returns nothing for Go/C#/Bash/PowerShell"]
async fn get_file_skeleton_renders_declarations_for_every_registered_language() {
    // EXPECTED: a skeleton listing the file's declarations, as the tool's own
    // description promises ("Brace-delimited languages (Rust, Go, Java, C++,
    // C#, PHP, JS/TS, Kotlin) get precise body elision; other languages (e.g.
    // Python, Lua, Bash, PowerShell) get a best-effort declaration-line-only
    // rendering").
    //
    // OBSERVED: `No top-level symbols found in <path> to build a skeleton
    // from.` for all four files below — 4 of the 16 registered languages,
    // two of them (Go, C#) named in that very description.
    //
    // ROOT CAUSE: `MctServer::get_file_skeleton` filters entries with
    // `entry.parent.is_none() && entry.kind != "module"` as its definition of
    // "top-level declaration". For these four parsers, a top-level function
    // *does* carry a parent — the Go package (`backend`), the C# namespace
    // (`Omni.Payments`), or the synthetic file-module (`lib`, `Common`) — so
    // every real declaration is filtered out and only the `module` entries
    // remain, which the same filter then also drops. Not fixed here: this
    // task is test-only.
    let server = build_server().await;
    for (path, language) in PARENTED_TOP_LEVEL_LANGUAGES {
        let text = content_of(
            &server
                .get_file_skeleton(Parameters(GetFileSkeletonArgs {
                    path: path.to_string(),
                }))
                .await
                .unwrap(),
        );
        assert!(
            text.starts_with(&format!("Skeleton of `{path}`")),
            "{language}: {text}"
        );
    }
}

#[tokio::test]
async fn get_file_skeleton_is_currently_empty_wherever_declarations_carry_a_parent() {
    // Pins the *observed* behaviour of the bug documented by the ignored test
    // above, so its blast radius cannot silently grow to a fifth language
    // without a test going red.
    let server = build_server().await;
    for (path, language) in PARENTED_TOP_LEVEL_LANGUAGES {
        let text = content_of(
            &server
                .get_file_skeleton(Parameters(GetFileSkeletonArgs {
                    path: path.to_string(),
                }))
                .await
                .unwrap(),
        );
        assert_eq!(
            text,
            format!("No top-level symbols found in `{path}` to build a skeleton from."),
            "{language}"
        );
    }

    // Every other registered language does render a skeleton.
    for path in [
        "jvm/Invoice.java",
        "jvm/Main.kt",
        "rustcore/tally.rs",
        "web/api.php",
        "native/arithmetic.cpp",
        "frontend/format.ts",
        "pyscripts/notify.py",
        "ui/index.html",
        "ui/styles.css",
        "ui/MainWindow.xaml",
        "config/app.xml",
        "docs/architecture.md",
    ] {
        let text = content_of(
            &server
                .get_file_skeleton(Parameters(GetFileSkeletonArgs {
                    path: path.to_string(),
                }))
                .await
                .unwrap(),
        );
        assert!(text.starts_with(&format!("Skeleton of `{path}`")), "{path}: {text}");
    }
}

#[tokio::test]
async fn get_project_overview_of_the_whole_fixture_spans_every_language() {
    let server = build_server().await;
    let text = content_of(
        &server
            .get_project_overview(Parameters(GetProjectOverviewArgs {
                path: None,
                language: None,
                max_symbols_per_module: None,
                include_relations: Some(false),
            }))
            .await
            .unwrap(),
    );
    for path in [
        "backend/server.go",
        "frontend/format.ts",
        "pyscripts/deploy.py",
        "jvm/Invoice.java",
        "jvm/Repository.kt",
        "dotnet/PaymentProcessor.cs",
        "native/arithmetic.cpp",
        "web/api.php",
        "rustcore/tally.rs",
        "shell/lib.sh",
        "pwsh/Common.psm1",
    ] {
        assert!(text.contains(path), "`{path}` missing from the project overview: {text}");
    }
}

#[tokio::test]
async fn get_project_overview_language_filter_narrows_the_digest_to_one_language() {
    let server = build_server().await;
    let text = content_of(
        &server
            .get_project_overview(Parameters(GetProjectOverviewArgs {
                path: None,
                language: Some("php".to_string()),
                max_symbols_per_module: None,
                include_relations: Some(true),
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("web/repository.php"), "got: {text}");
    assert!(text.contains("load_rows"), "got: {text}");
    assert!(text.contains("RowStore"), "got: {text}");
    assert!(!text.contains("backend/server.go"), "got: {text}");
    // `include_relations: true` must attach each symbol's top callers.
    assert!(text.contains("handle_request"), "caller sub-line missing: {text}");
}

#[tokio::test]
#[ignore = "product bug: get_project_overview shows only the file-module stub for Go/C#/Bash/PowerShell"]
async fn get_project_overview_surfaces_real_declarations_for_every_registered_language() {
    // Same root cause as `get_file_skeleton_renders_declarations_for_every_
    // registered_language` above: `get_project_overview` builds its per-module
    // candidate list with `e.parent.is_none() && OVERVIEW_KIND_ALLOWLIST
    // .contains(&e.kind)`, so for the four languages whose parsers parent
    // top-level declarations, the only surviving candidate is the synthetic
    // whole-file `module` entry.
    //
    // EXPECTED: `Write-Line`/`New-Artifact` (and the Go/C# equivalents)
    // listed under their module.
    // OBSERVED: `pwsh/Common.psm1:` followed only by `[module] Common L1-L9`.
    let server = build_server().await;
    let text = content_of(
        &server
            .get_project_overview(Parameters(GetProjectOverviewArgs {
                path: None,
                language: Some("powershell".to_string()),
                max_symbols_per_module: None,
                include_relations: Some(true),
            }))
            .await
            .unwrap(),
    );
    assert!(text.contains("Write-Line"), "got: {text}");
    assert!(text.contains("New-Artifact"), "got: {text}");
}

#[tokio::test]
async fn get_project_overview_currently_degrades_to_module_stubs_for_parented_languages() {
    // Pins the observed behaviour of the bug above.
    let server = build_server().await;
    for (language, module) in [
        ("powershell", "pwsh/Common.psm1"),
        ("go", "backend/server.go"),
        ("csharp", "dotnet/PaymentProcessor.cs"),
        ("bash", "shell/lib.sh"),
    ] {
        let text = content_of(
            &server
                .get_project_overview(Parameters(GetProjectOverviewArgs {
                    path: None,
                    language: Some(language.to_string()),
                    max_symbols_per_module: None,
                    include_relations: Some(false),
                }))
                .await
                .unwrap(),
        );
        assert!(text.contains(module), "{language}: {text}");
        assert!(
            text.lines()
                .filter(|l| l.trim_start().starts_with('['))
                .all(|l| l.trim_start().starts_with("[module]")),
            "{language}: only module stubs are expected today: {text}"
        );
    }
}

// --------------------------------------------------------- administration

#[tokio::test]
async fn get_indexing_status_reports_every_language_and_the_detected_manifests() {
    // `get_indexing_status` had no test of its own before this one.
    let server = build_server().await;
    let text = content_of(&server.get_indexing_status().await.unwrap());

    assert!(text.contains("28 files indexed, 100 symbols total."), "got: {text}");
    assert!(text.contains("Coverage by language:"), "got: {text}");
    for language in [
        "bash", "cpp", "csharp", "css", "go", "html", "java", "javascript_typescript",
        "kotlin", "markdown", "php", "powershell", "python", "rust", "xaml", "xml",
    ] {
        assert!(text.contains(&format!("  {language}: ")), "`{language}` missing: {text}");
    }
    assert!(!text.contains("failed to parse"), "got: {text}");
    assert!(!text.contains("not yet supported"), "got: {text}");

    assert!(text.contains("Dependencies detected:"), "got: {text}");
    assert!(text.contains("package.json (javascript_typescript, 2 dep(s)):"), "got: {text}");
    assert!(text.contains("go.mod (go, 1 dep(s)):"), "got: {text}");
    assert!(text.contains("requirements.txt (python, 2 dep(s)):"), "got: {text}");
}

#[tokio::test]
async fn reindex_tool_reports_an_incremental_no_op_then_a_full_reparse_when_forced() {
    // The `reindex` tool had no test of its own before this one.
    let server = build_server().await;

    let incremental = content_of(
        &server
            .reindex(Parameters(ReindexArgs { force: false }))
            .await
            .unwrap(),
    );
    assert!(
        incremental.starts_with("Reindex complete: 0 parsed, 28 unchanged, 0 removed, 0 symbols written."),
        "nothing changed on disk, so nothing should be re-parsed: {incremental}"
    );

    let forced = content_of(
        &server
            .reindex(Parameters(ReindexArgs { force: true }))
            .await
            .unwrap(),
    );
    assert!(
        forced.starts_with("Reindex complete: 28 parsed, 0 unchanged, 0 removed, 100 symbols written."),
        "force must bypass the hash check for every file: {forced}"
    );

    // The index is still correct after a forced rewrite — no duplicated rows.
    let status = content_of(&server.get_indexing_status().await.unwrap());
    assert!(status.contains("28 files indexed, 100 symbols total."), "got: {status}");
}
