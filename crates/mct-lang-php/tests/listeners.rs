//! The "possible entry point" hint for classes Laravel's event discovery may
//! register (`libraries/laravel/listeners.rs`): what is marked, what is left
//! alone, and what the hint claims.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-lang-php/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{EntryPointHint, LanguageParser, SourceFile};
use mct_lang_php::PhpParser;

fn hints(path: &str, src: &str) -> Vec<EntryPointHint> {
    PhpParser
        .parse(&SourceFile {
            relative_path: path.to_string(),
            contents: src.to_string(),
        })
        .expect("valid php source should parse")
        .entry_points
}

fn listener(body: &str) -> String {
    format!("<?php\nnamespace App\\Listeners;\nclass Ship\n{{\n{body}\n}}\n")
}

fn one(path: &str, src: &str) -> EntryPointHint {
    let mut found = hints(path, src);
    assert_eq!(found.len(), 1, "{found:?}");
    found.remove(0)
}

#[test]
fn a_handle_method_with_an_event_type_hint_is_a_possible_entry_point() {
    let hint = one(
        "app/Listeners/Ship.php",
        &listener("public function handle(OrderPlaced $event): void {}"),
    );
    assert_eq!(hint.framework, "laravel");
    assert_eq!(hint.rule, "event-listener-discovery");
    assert!(
        hint.evidence
            .contains("handle(OrderPlaced) in app/Listeners"),
        "{}",
        hint.evidence
    );
    let requires = hint.requires.expect("the hint depends on the project");
    assert_eq!(requires.manifest_path, "composer.json");
    assert_eq!(requires.package, "laravel/framework");
    assert_eq!(requires.min_major, 11);
}

#[test]
fn the_requirement_follows_the_application_root_in_a_monorepo() {
    let hint = one(
        "apps/shop/app/Listeners/Domain/Ship.php",
        &listener("public function handle(OrderPlaced $event) {}"),
    );
    assert_eq!(
        hint.requires.unwrap().manifest_path,
        "apps/shop/composer.json"
    );
}

#[test]
fn handler_names_beginning_with_handle_and_invoke_are_both_discovered() {
    // The documented convention: a method whose name *begins with* `handle`
    // or `__invoke`, which also covers a subscriber's `handleUserLogin`.
    let hint = one(
        "app/Listeners/Sub.php",
        &listener(
            "public function handleUserLogin(Login $e) {}\n\
             public function __invoke(Logout $e) {}\n\
             public function subscribe($events) {}",
        ),
    );
    assert!(
        hint.evidence.contains("handleUserLogin(Login)"),
        "{}",
        hint.evidence
    );
    assert!(
        hint.evidence.contains("__invoke(Logout)"),
        "{}",
        hint.evidence
    );
}

#[test]
fn union_and_nullable_class_types_are_event_types() {
    let union = one(
        "app/Listeners/Ship.php",
        &listener("public function handle(PodcastProcessed|PodcastPublished $e) {}"),
    );
    assert!(
        union
            .evidence
            .contains("handle(PodcastProcessed|PodcastPublished)"),
        "{}",
        union.evidence
    );
    let nullable = one(
        "app/Listeners/Ship.php",
        &listener("public function handle(?OrderPlaced $e) {}"),
    );
    assert!(
        nullable.evidence.contains("handle(OrderPlaced)"),
        "{}",
        nullable.evidence
    );
}

#[test]
fn a_qualified_event_type_is_kept_as_written() {
    let hint = one(
        "app/Listeners/Ship.php",
        &listener("public function handle(\\App\\Events\\OrderPlaced $e) {}"),
    );
    assert!(
        hint.evidence.contains("handle(\\App\\Events\\OrderPlaced)"),
        "{}",
        hint.evidence
    );
}

#[test]
fn shapes_discovery_does_not_read_get_no_hint() {
    let path = "app/Listeners/Ship.php";
    for body in [
        // no type hint, or only builtin types: not an event
        "public function handle($event) {}",
        "public function handle(array $payload) {}",
        "public function handle(string|int $x) {}",
        "public function handle(self $x) {}",
        // not a handler name
        "public function process(OrderPlaced $e) {}",
        "public function onHandle(OrderPlaced $e) {}",
        // not a plain public method
        "private function handle(OrderPlaced $e) {}",
        "protected function handle(OrderPlaced $e) {}",
        "public static function handle(OrderPlaced $e) {}",
        // no parameters
        "public function handle() {}",
        // the event is not the first parameter
        "public function handle(int $n, OrderPlaced $e) {}",
    ] {
        assert!(
            hints(path, &listener(body)).is_empty(),
            "marked, but should not be: {body}"
        );
    }
}

#[test]
fn abstract_classes_interfaces_traits_and_enums_get_no_hint() {
    let path = "app/Listeners/Ship.php";
    let method = "public function handle(OrderPlaced $e) {}";
    for src in [
        format!("<?php\nabstract class Ship {{ {method} }}\n"),
        format!("<?php\ntrait Ship {{ {method} }}\n"),
        "<?php\ninterface Ship { public function handle(OrderPlaced $e); }\n".to_string(),
        format!("<?php\nenum Ship {{ {method} }}\n"),
    ] {
        assert!(
            hints(path, &src).is_empty(),
            "marked, but should not be: {src}"
        );
    }
}

#[test]
fn only_the_default_listeners_directory_is_read() {
    let src = listener("public function handle(OrderPlaced $e) {}");
    for path in [
        "app/Handlers/Ship.php",
        "src/Listeners/Ship.php",
        "app/Domain/Orders/Listeners/Ship.php",
        "tests/app/Listenersx/Ship.php",
        "myapp/Listeners/Ship.php",
    ] {
        assert!(
            hints(path, &src).is_empty(),
            "marked, but should not be: {path}"
        );
    }
}

#[test]
fn a_listener_that_can_opt_out_says_so() {
    let hint = one(
        "app/Listeners/Ship.php",
        &listener(
            "public function handle(OrderPlaced $e) {}\n\
             public static function shouldBeDiscovered(): bool { return false; }",
        ),
    );
    assert!(
        hint.evidence.contains("shouldBeDiscovered()"),
        "{}",
        hint.evidence
    );
}

#[test]
fn the_evidence_never_claims_that_laravel_loads_the_class() {
    let hint = one(
        "app/Listeners/Ship.php",
        &listener("public function handle(OrderPlaced $e) {}"),
    );
    let text = hint.evidence.to_lowercase();
    assert!(text.contains("depends on"), "{text}");
    for claim in ["will be", "is registered", "is loaded", "will load"] {
        assert!(!text.contains(claim), "{claim:?} in {text}");
    }
}
