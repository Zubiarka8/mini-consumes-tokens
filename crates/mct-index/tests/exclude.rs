//! Confirms the default secret-pattern exclusions actually match, for the
//! ecosystems they claim to cover — added while wiring up C# (Java/C# were
//! the reason `appsettings.*.json`/`*.pfx` are in the base list), since
//! `ExcludeSet` had no test coverage at all before this. Extended while
//! wiring up Go for its Viper-style config-secret conventions.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content (see crates/mct-index/src/ for that policy).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_index::ExcludeSet;

#[path = "common/exclusions.rs"]
mod exclusions;
use exclusions::{assert_default_excludes, assert_default_includes};

#[test]
fn dotnet_secret_patterns_are_excluded() {
    assert_default_excludes(&[
        "appsettings.Development.json",
        "appsettings.Production.json",
        "src/appsettings.Local.json",
        "certs/mykey.pfx",
        "app.snk",
    ]);
}

#[test]
fn dotnet_non_secret_config_is_not_excluded() {
    let set = ExcludeSet::default();
    // The base (non-environment-suffixed) appsettings.json is ordinary
    // config, not a per-environment secret file — must not be swept up by
    // the appsettings.*.json pattern.
    assert!(!set.is_excluded("appsettings.json"));
    assert!(!set.is_excluded("src/Program.cs"));
}

#[test]
fn generic_and_java_ecosystem_patterns_still_work() {
    assert_default_excludes(&[
        ".env",
        ".env.local",
        "id_rsa.pem",
        "gradle.properties",
        "node_modules/left-pad/index.js",
    ]);
}

#[test]
fn php_composer_vendor_directory_is_excluded() {
    let set = ExcludeSet::default();
    assert!(set.is_excluded("vendor/autoload.php"));
    assert!(set.is_excluded("vendor/monolog/monolog/src/Logger.php"));
    assert!(!set.is_excluded("src/Invoice.php"));
}

/// `storage/framework` and `bootstrap/cache` are Laravel's generated
/// directories, but a path alone cannot say that a project is a Laravel
/// application, so no built-in rule hides them: code under those names in
/// another stack, or in a Laravel app below the root, must stay indexed.
#[test]
fn laravel_directory_names_are_not_excluded_by_default() {
    assert_default_includes(&[
        "storage/framework/views/5f1a0c.php",
        "bootstrap/cache/services.php",
        "src/bootstrap/cache/CacheManager.ts",
        "packages/ui/storage/framework/Store.java",
        "apps/shop/storage/framework/views/5f1a0c.php",
        "bootstrap/app.php",
    ]);
}

fn project_with_mctignore(lines: &str) -> (tempfile::TempDir, ExcludeSet) {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join(".mctignore"), lines).unwrap();
    let set = ExcludeSet::for_project(root.path());
    (root, set)
}

/// The documented way to keep a Laravel application's generated files out.
#[test]
fn a_mctignore_excludes_laravel_generated_directories_at_the_root() {
    let (_root, set) = project_with_mctignore("storage/framework/\nbootstrap/cache/\n");
    assert!(set.is_excluded("storage/framework/views/5f1a0c.php"));
    assert!(set.is_excluded("storage/framework/cache/data/ab/entry"));
    assert!(set.is_excluded("bootstrap/cache/services.php"));
    // Source next to them stays indexed.
    assert!(!set.is_excluded("bootstrap/app.php"));
    assert!(!set.is_excluded("bootstrap/providers.php"));
    assert!(!set.is_excluded("storage/app/Report.php"));
    assert!(!set.is_excluded("app/Cache/Repository.php"));
    // Anchored: the same names elsewhere are another project's code.
    assert!(!set.is_excluded("src/bootstrap/cache/CacheManager.ts"));
}

/// A monorepo lists each Laravel application's own directories.
#[test]
fn a_mctignore_excludes_the_generated_directories_of_a_laravel_app_in_a_monorepo() {
    let (_root, set) =
        project_with_mctignore("apps/shop/storage/framework/\napps/shop/bootstrap/cache/\n");
    assert!(set.is_excluded("apps/shop/storage/framework/views/5f1a0c.php"));
    assert!(set.is_excluded("apps/shop/bootstrap/cache/services.php"));
    assert!(!set.is_excluded("apps/shop/bootstrap/app.php"));
    assert!(!set.is_excluded("apps/admin/storage/framework/Store.php"));
}

#[test]
fn go_viper_style_secret_patterns_are_excluded() {
    let set = ExcludeSet::default();
    // `.env.local` (the dotfile) already matches the generic `**/.env.*`
    // pattern above; this one is the distinct, broader Go/Viper convention
    // of a *named* env file suffixed `.env.local` (e.g. `myapp.env.local`),
    // which `**/.env.*` does not cover since it requires the name to start
    // with `.env`.
    assert!(set.is_excluded("myapp.env.local"));
    assert!(set.is_excluded("config/secrets.yaml"));
}

#[test]
fn go_non_secret_config_is_not_excluded() {
    let set = ExcludeSet::default();
    assert!(!set.is_excluded("config/config.yaml"));
    assert!(!set.is_excluded("main.go"));
}

#[test]
fn valid_globs_that_exceed_compiler_limits_do_not_panic() {
    let extra = vec![format!("{}*", "[ab]".repeat(200_000))];
    let exclusions = ExcludeSet::new(&extra);
    assert!(
        exclusions.is_excluded(".env"),
        "built-in secret protection must remain"
    );
    assert!(exclusions.is_excluded("target/debug/app"));
    assert!(!exclusions.is_excluded("src/lib.rs"));
}
