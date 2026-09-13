//! Confirms the default secret-pattern exclusions actually match, for the
//! ecosystems they claim to cover — added while wiring up C# (Java/C# were
//! the reason `appsettings.*.json`/`*.pfx` are in the base list), since
//! `ExcludeSet` had no test coverage at all before this. Extended while
//! wiring up Go for its Viper-style config-secret conventions.

use ccm_index::ExcludeSet;

#[test]
fn dotnet_secret_patterns_are_excluded() {
    let set = ExcludeSet::default();
    assert!(set.is_excluded("appsettings.Development.json"));
    assert!(set.is_excluded("appsettings.Production.json"));
    assert!(set.is_excluded("src/appsettings.Local.json"));
    assert!(set.is_excluded("certs/mykey.pfx"));
    assert!(set.is_excluded("app.snk"));
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
    let set = ExcludeSet::default();
    assert!(set.is_excluded(".env"));
    assert!(set.is_excluded(".env.local"));
    assert!(set.is_excluded("id_rsa.pem"));
    assert!(set.is_excluded("gradle.properties"));
    assert!(set.is_excluded("node_modules/left-pad/index.js"));
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
