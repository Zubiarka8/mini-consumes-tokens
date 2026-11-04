//! `find_dead_code` and Laravel listeners (issue #273): a class that only
//! event discovery could register stays a candidate and carries a
//! "possible entry point" note; one named in the source is not a candidate;
//! a project that is not a Laravel application gets no note and no new column.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-mcp-server/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use mct_index::{ExcludeSet, Index};
use mct_mcp_server::server::MctServer;
use serde_json::json;

struct Project(PathBuf);

impl Project {
    fn new(files: &[(&str, &str)]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "mct-possible-entry-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(root.join(".mct-index")).unwrap();
        for (path, contents) in files {
            write(&root, path, contents);
        }
        Self(root.canonicalize().unwrap())
    }

    fn server(&self) -> MctServer {
        let mut index = Index::open_in_memory(&self.0, ExcludeSet::default()).unwrap();
        index
            .reindex(&mct_languages::build_registry(), true)
            .unwrap();
        MctServer::new(index, mct_languages::build_registry())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn write(root: &Path, rel: &str, contents: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, contents).unwrap();
}

const LISTENER: &str = "<?php\nnamespace App\\Listeners;\nuse App\\Events\\OrderPlaced;\n\
class NotifyWarehouse\n{\n    public function handle(OrderPlaced $event): void {}\n}\n";
const NAMED_LISTENER: &str = "<?php\nnamespace App\\Listeners;\nuse App\\Events\\OrderPlaced;\n\
class AuditOrder\n{\n    public function handle(OrderPlaced $event): void {}\n}\n";
const PROVIDER: &str = "<?php\nnamespace App\\Providers;\nuse App\\Events\\OrderPlaced;\n\
use App\\Listeners\\AuditOrder;\nuse Illuminate\\Support\\Facades\\Event;\n\
class AppServiceProvider\n{\n    public function boot(): void\n    {\n        \
Event::listen(OrderPlaced::class, AuditOrder::class);\n    }\n}\n";
const EVENT: &str = "<?php\nnamespace App\\Events;\nclass OrderPlaced {}\n";
const UNUSED: &str = "<?php\nnamespace App\\Support;\nclass Unused {}\n";
const BOOT: &str = "<?php\nuse App\\Providers\\AppServiceProvider;\nnew AppServiceProvider();\n";

fn laravel(composer: &str) -> Project {
    Project::new(&[
        ("composer.json", composer),
        ("app/Listeners/NotifyWarehouse.php", LISTENER),
        ("app/Listeners/AuditOrder.php", NAMED_LISTENER),
        ("app/Providers/AppServiceProvider.php", PROVIDER),
        ("app/Events/OrderPlaced.php", EVENT),
        ("app/Support/Unused.php", UNUSED),
        ("bootstrap/app.php", BOOT),
    ])
}

async fn dead_code(server: &MctServer, format: &str) -> String {
    server
        .call_tool_by_name(
            "find_dead_code",
            json!({"format": format}).as_object().cloned(),
        )
        .await
        .unwrap()
        .content[0]
        .as_text()
        .unwrap()
        .text
        .clone()
}

const LARAVEL_12: &str = r#"{ "require": { "laravel/framework": "^12.0" } }"#;

#[tokio::test]
async fn a_discovered_listener_stays_a_candidate_with_a_possible_entry_point_note() {
    let project = laravel(LARAVEL_12);
    let server = project.server();
    for format in ["toon", "text"] {
        let out = dead_code(&server, format).await;
        assert!(out.contains("NotifyWarehouse"), "{format}: {out}");
        assert!(
            out.contains("possible entry point (laravel, event-listener-discovery)"),
            "{format}: {out}"
        );
        assert!(out.contains("Not proven"), "{format}: {out}");
        assert!(
            out.contains("handle(OrderPlaced) in app/Listeners"),
            "{format}: {out}"
        );
        // Named in `Event::listen`: referenced, so not a candidate at all.
        assert!(!out.contains("AuditOrder"), "{format}: {out}");
        // Nothing claims that the framework loads it.
        let lower = out.to_lowercase();
        for claim in [
            "will load",
            "is loaded",
            "is registered",
            "will be registered",
        ] {
            assert!(!lower.contains(claim), "{format}: {claim:?} in {out}");
        }
    }
}

#[tokio::test]
async fn only_the_candidate_with_a_hint_carries_a_note() {
    let project = laravel(LARAVEL_12);
    let out = dead_code(&project.server(), "toon").await;
    let rows: Vec<_> = out.lines().filter(|l| l.contains("class,")).collect();
    assert_eq!(rows.len(), 2, "{out}");
    let note = |name: &str| {
        rows.iter()
            .find(|row| row.contains(&format!("class,{name},")))
            .unwrap()
            .contains("possible entry point")
    };
    assert!(note("NotifyWarehouse"));
    assert!(!note("Unused"));
}

#[tokio::test]
async fn the_note_does_not_change_the_candidate_identity_or_review_state() {
    let with_note = dead_code(&laravel(LARAVEL_12).server(), "toon").await;
    let without = dead_code(
        &laravel(r#"{ "require": { "symfony/console": "^7.0" } }"#).server(),
        "toon",
    )
    .await;
    let id = |out: &str| {
        out.lines()
            .find(|l| l.contains("class,NotifyWarehouse,"))
            .unwrap()
            .split(',')
            .nth(5)
            .unwrap()
            .to_string()
    };
    assert_eq!(id(&with_note), id(&without));
    assert!(with_note.contains("unreviewed,true"), "{with_note}");
}

#[tokio::test]
async fn a_project_that_is_not_laravel_gets_no_note_and_no_new_column() {
    for composer in [
        r#"{ "require": { "symfony/console": "^7.0" } }"#,
        r#"{ "require": { "laravel/framework": "^10.0" } }"#,
    ] {
        let out = dead_code(&laravel(composer).server(), "toon").await;
        assert!(out.contains("NotifyWarehouse"), "{out}");
        assert!(!out.contains("possible entry point"), "{out}");
        assert!(
            out.contains("candidates[2]{kind,name,path,line,end_line,id,review,actionable}:"),
            "today's columns, unchanged: {out}"
        );
    }
    let no_manifest = Project::new(&[("app/Listeners/NotifyWarehouse.php", LISTENER)]);
    let out = dead_code(&no_manifest.server(), "toon").await;
    assert!(out.contains("NotifyWarehouse"), "{out}");
    assert!(!out.contains("possible entry point"), "{out}");
}

#[tokio::test]
async fn each_application_of_a_monorepo_is_judged_by_its_own_manifest() {
    let project = Project::new(&[
        ("apps/shop/composer.json", LARAVEL_12),
        (
            "apps/shop/app/Listeners/ShopListener.php",
            &LISTENER.replace("NotifyWarehouse", "ShopListener"),
        ),
        (
            "apps/plain/composer.json",
            r#"{ "require": { "monolog/monolog": "^3.0" } }"#,
        ),
        (
            "apps/plain/app/Listeners/PlainListener.php",
            &LISTENER.replace("NotifyWarehouse", "PlainListener"),
        ),
        (
            "tools/app/Listeners/ToolListener.php",
            &LISTENER.replace("NotifyWarehouse", "ToolListener"),
        ),
    ]);
    let out = dead_code(&project.server(), "toon").await;
    let row = |name: &str| {
        out.lines()
            .find(|l| l.contains(&format!("class,{name},")))
            .unwrap_or_else(|| panic!("{name} is not a candidate: {out}"))
            .to_string()
    };
    assert!(
        row("ShopListener").contains("possible entry point"),
        "{out}"
    );
    assert!(
        !row("PlainListener").contains("possible entry point"),
        "{out}"
    );
    assert!(
        !row("ToolListener").contains("possible entry point"),
        "{out}"
    );
}
