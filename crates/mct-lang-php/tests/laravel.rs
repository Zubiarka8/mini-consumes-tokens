//! Laravel support, end to end: a Laravel 12 application skeleton written
//! from the official documentation (routing, controllers, Blade, events,
//! migrations, service providers) goes through the real indexer, and each
//! test asks the index the question a user would ask a tool.
//!
//! The fixture is documentation-derived, not a clone of a real project.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-php/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, RelationHit, Resolution};
use mct_lang_php::PhpParser;

fn open_indexed() -> Index {
    open_indexed_with(ExcludeSet::default())
}

fn open_indexed_with(exclude: ExcludeSet) -> Index {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/laravel-app");
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PhpParser));
    let mut index = Index::open_in_memory(&root, exclude).unwrap();
    let report = index.reindex(&registry, false).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    index
}

/// `(file the relation is in, file it resolves to)` for the relations
/// matching `keep`, resolved ones only.
fn resolved_to(hits: &[RelationHit], keep: impl Fn(&RelationHit) -> bool) -> Vec<(String, String)> {
    let mut pairs: Vec<_> = hits
        .iter()
        .filter(|h| keep(h) && h.resolution == Resolution::Resolved)
        .filter_map(|h| {
            h.candidates
                .first()
                .map(|c| (h.relative_path.clone(), c.relative_path.clone()))
        })
        .collect();
    pairs.sort();
    pairs.dedup();
    pairs
}

fn pair(from: &str, to: &str) -> (String, String) {
    (from.to_string(), to.to_string())
}

fn indexed_paths(index: &Index) -> Vec<String> {
    index
        .indexed_files()
        .unwrap()
        .into_iter()
        .map(|f| f.relative_path)
        .collect()
}

/// Nothing hides a directory just for its name: `storage/framework` is
/// Laravel's output only when the project is a Laravel application, which a
/// path cannot tell, so by default a compiled view is indexed like any file.
#[test]
fn generated_laravel_directories_are_indexed_unless_the_project_excludes_them() {
    let files = indexed_paths(&open_indexed());
    assert!(
        files.contains(&"storage/framework/views/0a1b2c.php".to_string()),
        "{files:?}"
    );
}

/// What the `.mctignore` lines `storage/framework/` and `bootstrap/cache/`
/// (the documented way) turn into; the rest of `bootstrap/` stays indexed.
#[test]
fn a_mctignore_keeps_the_generated_laravel_directories_out() {
    let rules = [
        "storage/framework".to_string(),
        "storage/framework/**".to_string(),
        "bootstrap/cache".to_string(),
        "bootstrap/cache/**".to_string(),
    ];
    let files = indexed_paths(&open_indexed_with(ExcludeSet::new(&rules)));
    assert!(
        !files.iter().any(|f| f.starts_with("storage/framework")),
        "{files:?}"
    );
    assert!(files.contains(&"bootstrap/app.php".to_string()));
    assert!(files.contains(&"bootstrap/providers.php".to_string()));
}

#[test]
fn route_actions_resolve_to_the_controller_methods() {
    let index = open_indexed();
    let callers = |name: &str| index.find_callers(name).unwrap();
    let from_routes = |hits: &[RelationHit]| {
        resolved_to(hits, |h| h.relative_path == "routes/web.php")
            .into_iter()
            .map(|(_, to)| to)
            .collect::<Vec<_>>()
    };

    // `[OrderController::class, 'index']`, a resource and a controller group.
    assert_eq!(
        from_routes(&callers("index")),
        [
            "app/Http/Controllers/OrderController.php",
            "app/Http/Controllers/PhotoController.php",
            "app/Http/Controllers/ReportController.php",
        ]
    );
    // An invokable controller is reached through `__invoke`.
    assert_eq!(
        from_routes(&callers("__invoke")),
        ["app/Http/Controllers/HomeController.php"]
    );
    assert_eq!(
        from_routes(&callers("store")),
        ["app/Http/Controllers/OrderController.php"]
    );
}

#[test]
fn resource_only_lists_leave_the_other_methods_unrouted() {
    let index = open_indexed();
    // `->only(['index', 'show'])`: `destroy` is not routed.
    assert!(index.find_callers("destroy").unwrap().is_empty());
}

#[test]
fn a_legacy_string_action_is_not_guessed() {
    let index = open_indexed();
    // `'LegacyController@index'` depends on a namespace set at run time.
    let callers = index.find_callers("index").unwrap();
    assert_eq!(
        callers
            .iter()
            .filter(|h| h.relative_path == "routes/web.php")
            .count(),
        3
    );
}

#[test]
fn bootstrap_includes_the_route_files_it_names() {
    let index = open_indexed();
    let edges = index.direct_file_imports().unwrap();
    let from_bootstrap: Vec<_> = edges
        .iter()
        .filter(|e| e.from == "bootstrap/app.php")
        .map(|e| e.to.as_str())
        .collect();
    assert!(from_bootstrap.contains(&"routes/web.php"), "{edges:?}");
    assert!(from_bootstrap.contains(&"routes/console.php"), "{edges:?}");
}

#[test]
fn views_are_file_dependencies_of_whatever_renders_them() {
    let index = open_indexed();
    let edges: Vec<_> = index
        .direct_file_imports()
        .unwrap()
        .into_iter()
        .map(|e| (e.from, e.to))
        .collect();
    let has = |from: &str, to: &str| edges.contains(&pair(from, to));

    // `view('orders.index')` in a controller, `view('welcome')` and
    // `Route::view` elsewhere.
    assert!(has(
        "app/Http/Controllers/OrderController.php",
        "resources/views/orders/index.blade.php"
    ));
    assert!(has(
        "app/Http/Controllers/HomeController.php",
        "resources/views/welcome.blade.php"
    ));
    assert!(has("routes/web.php", "resources/views/welcome.blade.php"));
    // Blade's own directives.
    assert!(has(
        "resources/views/orders/index.blade.php",
        "resources/views/layouts/app.blade.php"
    ));
    assert!(has(
        "resources/views/orders/index.blade.php",
        "resources/views/orders/_row.blade.php"
    ));
    // A directive inside a Blade comment is not a dependency.
    assert!(!edges.iter().any(|(_, to)| to.contains("ignored")));
}

#[test]
fn a_view_that_does_not_exist_stays_unresolved() {
    let index = open_indexed();
    let missing = index.find_references("missing.blade").unwrap();
    assert!(!missing.is_empty());
    assert!(
        missing
            .iter()
            .all(|h| h.resolution == Resolution::Unresolved),
        "{missing:?}"
    );
}

#[test]
fn blade_components_reach_their_class_or_their_template() {
    let index = open_indexed();
    let badge = index.find_references("Badge").unwrap();
    assert!(
        badge.iter().any(
            |h| h.relative_path == "resources/views/orders/index.blade.php"
                && h.resolution == Resolution::Resolved
                && h.candidates.first().map(|c| c.relative_path.as_str())
                    == Some("app/View/Components/Badge.php")
        ),
        "<x-badge /> renders the class App\\View\\Components\\Badge: {badge:?}"
    );
    let alert = index.find_references("alert.blade").unwrap();
    assert!(
        alert
            .iter()
            .any(|h| h.relative_path == "resources/views/welcome.blade.php"
                && h.resolution == Resolution::Resolved),
        "<x-alert> renders the anonymous template: {alert:?}"
    );
    // A package-namespaced component names no local file.
    assert!(index.find_references("thing").unwrap().is_empty());
}

#[test]
fn homonymous_models_are_told_apart_by_namespace() {
    let index = open_indexed();
    let refs = index.find_references("Customer").unwrap();
    let target = |from: &str| {
        refs.iter()
            .filter(|h| h.relative_path == from)
            .flat_map(|h| h.candidates.iter().map(|c| c.relative_path.clone()))
            .collect::<std::collections::BTreeSet<_>>()
    };
    // `use App\Billing\Customer as Payer` and a `Payer $payer` parameter.
    assert_eq!(
        target("app/Http/Controllers/OrderController.php")
            .into_iter()
            .collect::<Vec<_>>(),
        ["app/Billing/Customer.php"]
    );
    // `Customer::class` in `App\Models\Order`.
    assert_eq!(
        target("app/Models/Order.php")
            .into_iter()
            .collect::<Vec<_>>(),
        ["app/Models/Customer.php"]
    );
}

/// Issue #272: `Order` is both the file module and the class of
/// `app/Models/Order.php`; a parameter type or `Order::class` names the class.
#[test]
fn a_model_type_reference_is_resolved_not_ambiguous_with_its_file_module() {
    let index = open_indexed();
    let refs = index.find_references("Order").unwrap();
    assert!(refs.len() >= 5, "{refs:?}");
    for hit in &refs {
        assert_eq!(hit.resolution, Resolution::Resolved, "{hit:?}");
    }
}

#[test]
fn a_method_called_on_an_unknown_receiver_is_never_resolved() {
    let index = open_indexed();
    // `$payer->balance()`: the receiver's class is a run-time value.
    let calls = index.find_callers("balance").unwrap();
    assert_eq!(calls.len(), 1);
    assert_eq!(calls[0].resolution, Resolution::Ambiguous);
}

#[test]
fn service_providers_listed_in_bootstrap_are_not_dead_code() {
    let index = open_indexed();
    let dead: Vec<_> = mct_index::find_dead_code_candidates(&index, None, None)
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        !dead.contains(&"AppServiceProvider".to_string()),
        "{dead:?}"
    );
    assert!(!dead.contains(&"Customer".to_string()), "{dead:?}");
    // Provable from the source: the provider names the class.
    assert!(!dead.contains(&"AuditOrder".to_string()), "{dead:?}");
    // Only possible, never proven: event discovery would pick this listener
    // up from its `handle(OrderPlaced)` signature and its folder, but
    // nothing in the source names it, and discovery can be switched off or
    // pointed elsewhere. It stays a candidate rather than being declared
    // alive because it fits a convention.
    assert!(dead.contains(&"NotifyWarehouse".to_string()), "{dead:?}");
}

#[test]
fn a_discovered_listener_is_a_candidate_with_a_possible_entry_point_note() {
    let index = open_indexed();
    let scan = mct_index::scan_dead_code(&index, None, None).unwrap();
    let note_of = |name: &str| {
        scan.candidates
            .iter()
            .find(|&&id| scan.symbols[id].name == name)
            .map(|&id| scan.possible_entry_points(id).to_vec())
    };
    let notes = note_of("NotifyWarehouse").expect("still a candidate");
    assert_eq!(notes.len(), 1, "{notes:?}");
    assert_eq!(notes[0].framework, "laravel");
    assert_eq!(notes[0].rule, "event-listener-discovery");
    let text = notes[0].describe();
    assert!(text.contains("possible entry point"), "{text}");
    assert!(text.contains("Not proven"), "{text}");
    // Named in `Event::listen`, so not a candidate at all.
    assert!(note_of("AuditOrder").is_none());
    // A candidate no convention could load carries no note.
    for &id in &scan.candidates {
        if scan.symbols[id].name != "NotifyWarehouse" {
            assert!(
                scan.possible_entry_points(id).is_empty(),
                "{}",
                scan.symbols[id].name
            );
        }
    }
}

#[test]
fn anonymous_migrations_expose_up_and_down() {
    let index = open_indexed();
    let up = index.find_symbol("up").unwrap();
    assert_eq!(up.len(), 1);
    assert_eq!(
        up[0].relative_path,
        "database/migrations/2024_01_01_000000_create_orders_table.php"
    );
    assert_eq!(up[0].kind, "method");
    assert!(!index.find_symbol("down").unwrap().is_empty());
}
