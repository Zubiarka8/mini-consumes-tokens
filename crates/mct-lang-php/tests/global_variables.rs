//! PHP global variables: the first `$x = …` at file scope (in a namespace or
//! a top-level block too) is a `variable` of the file, named without `$`, as
//! PowerShell's are. A function, method, closure or class body has its own
//! scope, and a superglobal, a destructuring pattern, a property or an
//! element declares nothing. So that these names don't widen call matches,
//! a bare `f()` can only call a function: `$f()` is the form that calls
//! through a variable.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_core::{LanguageParser, ParsedFile, SourceFile, SymbolKind};
use mct_lang_php::PhpParser;

fn parse(src: &str) -> ParsedFile {
    PhpParser
        .parse(&SourceFile {
            relative_path: "bootstrap.php".to_string(),
            contents: src.to_string(),
        })
        .expect("valid php source should parse")
}

fn variables(parsed: &ParsedFile) -> Vec<(String, u32)> {
    parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Variable)
        .map(|s| {
            assert_eq!(s.parent, None, "{} has a parent", s.name);
            (s.name.clone(), s.location.line)
        })
        .collect()
}

#[test]
fn a_file_scope_assignment_declares_a_variable_once() {
    let parsed = parse(concat!(
        "<?php\n",
        "$app = require __DIR__ . '/app.php';\n",
        "$app = new App();\n",
        "if (PHP_SAPI === 'cli') {\n",
        "    $mode = 'cli';\n",
        "}\n",
        "$count = $total = 0;\n",
    ));
    assert_eq!(
        variables(&parsed),
        [
            ("app".to_string(), 2),
            ("mode".to_string(), 5),
            ("count".to_string(), 7),
            ("total".to_string(), 7),
        ]
    );
}

#[test]
fn a_namespaced_file_scope_assignment_is_a_variable() {
    let parsed = parse("<?php\nnamespace App;\n\n$config = ['debug' => true];\n");
    assert_eq!(variables(&parsed), [("config".to_string(), 4)]);
}

#[test]
fn a_function_method_closure_or_class_scope_declares_none() {
    let parsed = parse(concat!(
        "<?php\n",
        "function boot() { $local = 1; }\n",
        "class Kernel { public function run() { $inner = 2; } }\n",
        "spl_autoload_register(function ($class) { $file = $class . '.php'; });\n",
        "$sum = array_map(fn ($x) => $y = $x, []);\n",
        "$handler = new class { public function handle() { $hidden = 3; } };\n",
        "$cb = function () { $nested = 4; };\n",
    ));
    // `$cb` is the closure's `function` symbol, not a variable.
    assert_eq!(
        variables(&parsed),
        [("sum".to_string(), 5), ("handler".to_string(), 6)]
    );
}

#[test]
fn superglobals_patterns_properties_and_elements_declare_none() {
    let parsed = parse(concat!(
        "<?php\n",
        "$_SESSION = [];\n",
        "$GLOBALS = [];\n",
        "[$a, $b] = [1, 2];\n",
        "list($c) = [3];\n",
        "$obj->name = 'x';\n",
        "$rows[] = 1;\n",
        "Config::$value = 2;\n",
        "$$dynamic = 3;\n",
        "$total += 1;\n",
    ));
    assert_eq!(variables(&parsed), Vec::<(String, u32)>::new());
}

#[test]
fn a_bare_call_never_resolves_to_a_global_variable() {
    let root = std::env::temp_dir().join(format!("mct-php-globals-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/bootstrap.php"),
        "<?php\nnamespace App;\n\n$config = load();\n$handler = config();\n$handler();\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/helpers.php"),
        "<?php\nnamespace App;\n\nfunction config() { return []; }\nfunction load() { return []; }\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    let mut hits: Vec<String> = ["config", "handler", "load"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls")
        .map(|h| {
            format!(
                "{}:{} {} {:?}",
                h.relative_path, h.line, h.to_name, h.resolution
            )
        })
        .collect();
    hits.sort();
    std::fs::remove_dir_all(root).unwrap();
    assert_eq!(
        hits,
        [
            // Only the function is a candidate, not the variable `$config`.
            "src/bootstrap.php:4 load Resolved",
            "src/bootstrap.php:5 config Resolved",
        ]
    );
}
