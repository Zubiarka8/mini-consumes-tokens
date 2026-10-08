//! Long, multi-file fixture corpus (issue #74): the operations tooling of a
//! warehouse in PowerShell 7 — shared helpers, an HTTP API client module, an
//! inventory module with classes and an enum, a deployment script and the
//! nightly jobs — each file 300–600 lines and importing the others. The
//! shared checks (size, line ranges, golden snapshot, index round trip,
//! malformed input) come from `mct-corpus`; the tests below pin what the
//! PowerShell parser is expected to extract and its documented limits.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-powershell/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{RelationKind, SymbolKind};
use mct_lang_powershell::PowerShellParser;

mct_corpus::standard_tests!(PowerShellParser);

use RelationKind::{Calls, Imports};
use SymbolKind::{Function, Variable};

fn parent<'a>(path: &str, name: &str, kind: SymbolKind) -> Option<&'a str> {
    corpus().symbol(path, name, kind).parent.as_deref()
}

#[test]
fn functions_belong_to_their_script_or_module() {
    assert_eq!(
        parent("Warehouse.Common.psm1", "Write-WarehouseLog", Function),
        Some("Warehouse.Common")
    );
    assert_eq!(
        parent("Warehouse.Api.psm1", "Invoke-WarehouseApi", Function),
        Some("Warehouse.Api")
    );
    assert_eq!(
        parent("Warehouse.Inventory.psm1", "Import-CycleCount", Function),
        Some("Warehouse.Inventory")
    );
    assert_eq!(
        parent("Deploy-Warehouse.ps1", "Watch-Canary", Function),
        Some("Deploy-Warehouse")
    );
    assert_eq!(
        parent("Invoke-NightlyJobs.ps1", "Invoke-Job", Function),
        Some("Invoke-NightlyJobs")
    );
}

#[test]
fn script_level_variables_are_symbols_with_scope_stripped() {
    assert_eq!(
        parent("Warehouse.Common.psm1", "ModuleRoot", Variable),
        Some("Warehouse.Common")
    );
    assert_eq!(
        parent("Warehouse.Common.psm1", "LevelOrder", Variable),
        Some("Warehouse.Common")
    );
    assert_eq!(
        parent("Invoke-NightlyJobs.ps1", "ReorderPoints", Variable),
        Some("Invoke-NightlyJobs")
    );
    // `$Global:` scope is stripped too.
    corpus().symbol("Warehouse.Common.psm1", "WarehouseCorrelationId", Variable);
    // Variables assigned inside a function are not symbols.
    assert!(corpus().symbols_named("attempt").is_empty());
}

#[test]
fn commands_are_calls() {
    let c = corpus();
    c.relation(
        "Warehouse.Common.psm1",
        "Invoke-WithRetry",
        Calls,
        "Test-TransientError",
    );
    c.relation(
        "Warehouse.Common.psm1",
        "Invoke-WithRetry",
        Calls,
        "Start-Sleep",
    );
    c.relation(
        "Warehouse.Common.psm1",
        "Get-WarehouseConfig",
        Calls,
        "Get-WarehouseSecret",
    );
    // Commands in pipelines, in `try`/`catch`, and inside script blocks.
    c.relation(
        "Warehouse.Common.psm1",
        "Format-Table2",
        Calls,
        "Measure-Object",
    );
    c.relation(
        "Warehouse.Api.psm1",
        "Invoke-WarehouseApi",
        Calls,
        "ConvertFrom-ProblemError",
    );
    c.relation(
        "Invoke-NightlyJobs.ps1",
        "Invoke-NightlyJobs",
        Calls,
        "Test-ApiHealth",
    );
}

#[test]
fn only_a_literal_import_target_is_an_import() {
    let c = corpus();
    c.relation(
        "Deploy-Warehouse.ps1",
        "Deploy-Warehouse",
        Imports,
        "Microsoft.PowerShell.SecretManagement",
    );
    // Known limit: a computed path (`Import-Module (Join-Path $PSScriptRoot
    // 'X.psm1')`, `. "$PSScriptRoot/lib/Mail.ps1"`) is not evaluated, so it
    // is no import at all — never its source text (see the crate docs).
    let imports: Vec<_> = c
        .relations()
        .into_iter()
        .filter(|r| r.kind == Imports)
        .map(|r| r.to)
        .collect();
    assert_eq!(imports, ["Microsoft.PowerShell.SecretManagement"]);
}

#[test]
fn cross_file_calls_are_extracted() {
    let c = corpus();
    c.relation(
        "Warehouse.Api.psm1",
        "Connect-WarehouseApi",
        Calls,
        "Get-WarehouseConfig",
    );
    c.relation(
        "Warehouse.Api.psm1",
        "New-WarehouseOrder",
        Calls,
        "Format-Money",
    );
    c.relation(
        "Warehouse.Inventory.psm1",
        "Move-WarehouseStock",
        Calls,
        "Get-WarehouseStock",
    );
    c.relation(
        "Deploy-Warehouse.ps1",
        "Test-Preflight",
        Calls,
        "Invoke-WarehouseApi",
    );
    c.relation(
        "Invoke-NightlyJobs.ps1",
        "Write-ReorderReport",
        Calls,
        "Get-ReorderReport",
    );
    assert!(c.name_matched_relation_count() >= 50);
}

#[test]
fn calls_in_begin_process_end_blocks_are_extracted() {
    // An advanced function's body can be `begin { } process { } end { }`
    // (a named block list) instead of a plain statement list.
    let c = corpus();
    for (from, to) in [
        ("Get-WarehouseOrder", "Invoke-WarehouseApi"),
        ("Stop-WarehouseOrder", "Write-WarehouseLog"),
        ("Split-CycleCount", "Write-WarehouseLog"),
        ("Get-WarehouseStock", "Split-Batch"),
        // `param()` too: `[ValidateScript({ … })]` and default values.
        ("Get-WarehouseOrder", "Test-OrderNumber"),
        ("Export-DailyMetric", "Get-Date"),
    ] {
        assert!(c.has_relation(from, Calls, to), "{from} -> {to}");
    }
}

#[test]
fn classes_and_enums_have_no_symbol() {
    // Known limit: `class Location { … }`, its methods and `enum
    // StorageZone` are out of the plugin's scope (see the crate docs).
    let c = corpus();
    for name in [
        "Location",
        "CycleCount",
        "StorageZone",
        "Difference",
        "NeedsReview",
    ] {
        assert!(c.symbols_named(name).is_empty(), "{name} became a symbol");
    }
}

#[test]
fn index_answers_cross_file_queries() {
    let index = corpus().index();
    let callers = index.find_callers("Invoke-WarehouseApi").unwrap();
    let mut files: Vec<_> = callers.iter().map(|h| h.relative_path.as_str()).collect();
    files.sort();
    files.dedup();
    for file in [
        "Deploy-Warehouse.ps1",
        "Invoke-NightlyJobs.ps1",
        "Warehouse.Api.psm1",
        "Warehouse.Inventory.psm1",
    ] {
        assert!(
            files.contains(&file),
            "{file} calls Invoke-WarehouseApi: {files:?}"
        );
    }
}
