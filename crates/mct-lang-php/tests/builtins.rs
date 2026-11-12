//! D9 for PHP: a call that can only reach one of PHP's core functions is
//! external when core functions are enabled. An enabled global core function
//! can't be declared again ("Cannot redeclare"), so `\strlen()`, and a bare
//! `strlen()` in a file with no namespace, never reach a repository function.
//! A bare call inside a
//! namespace may reach a namespaced function of that name, an imported
//! function is the import's, and a file that declares the name itself (a
//! polyfill) proves nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use mct_lang_php::PhpParser;

#[test]
fn a_call_that_can_only_reach_a_core_function_is_external() {
    let root = std::env::temp_dir().join(format!("mct-php-builtins-{}", std::process::id()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(
        root.join("src/script.php"),
        "<?php\nfunction slug($s) {\n    return strtolower(trim($s)) . count([]);\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/Service.php"),
        "<?php\nnamespace App;\n\nuse function Lib\\count;\n\nfunction run($s) {\n    return \\strlen($s) + trim($s) + count($s) + \\App\\strlen($s);\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/polyfill.php"),
        "<?php\nif (!function_exists('trim')) {\n    function trim($s) { return $s; }\n}\nfunction pad($s) {\n    return trim($s);\n}\n",
    )
    .unwrap();
    // A repository function of each name, so a call that isn't external
    // finds a candidate.
    std::fs::write(
        root.join("src/helpers.php"),
        "<?php\nnamespace App;\n\nfunction strlen($s) { return 0; }\nfunction trim($s) { return $s; }\nfunction strtolower($s) { return $s; }\n",
    )
    .unwrap();
    std::fs::write(
        root.join("src/lib.php"),
        "<?php\nnamespace Lib;\n\nfunction count($s) { return 0; }\n",
    )
    .unwrap();
    let index = mct_corpus::open_fixture_indexed(&root, Arc::new(PhpParser));
    let mut hits: Vec<String> = ["strtolower", "trim", "count", "strlen"]
        .iter()
        .flat_map(|name| index.find_references(name).unwrap())
        .filter(|h| h.kind == "calls" && h.relative_path != "src/helpers.php")
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
            // `\strlen` is the global one; `\App\strlen` is the repository's.
            "src/Service.php:7 count Resolved",
            "src/Service.php:7 strlen External",
            "src/Service.php:7 strlen Resolved",
            // A bare `trim` in `namespace App` may be `App\trim`.
            "src/Service.php:7 trim Ambiguous",
            // The file declares `trim` (a polyfill): not proven.
            "src/polyfill.php:6 trim Ambiguous",
            // No namespace: each is the global function.
            "src/script.php:3 count External",
            "src/script.php:3 strtolower External",
            "src/script.php:3 trim External",
        ]
    );
}
