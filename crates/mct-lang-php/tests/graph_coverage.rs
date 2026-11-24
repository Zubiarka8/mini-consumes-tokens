//! Regression tests through the Rust index and its relation resolver.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_index::{ExcludeSet, Index, Resolution};
use mct_lang_php::PhpParser;

fn indexed(files: &[(&str, &str)]) -> Index {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let unique = NEXT.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("mct-php-graph-{}-{unique}", std::process::id()));
    for (path, source) in files {
        let file = root.join(path);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(file, source).unwrap();
    }
    let mut registry = LanguageRegistry::new();
    registry.register(Arc::new(PhpParser));
    let mut index = Index::open_in_memory(&root, ExcludeSet::default()).unwrap();
    let report = index.reindex(&registry, true).unwrap();
    assert!(report.issues.is_empty(), "{:?}", report.issues);
    fs::remove_dir_all(root).unwrap();
    index
}

#[test]
fn acquiring_a_callable_is_a_reference_and_unpacking_is_a_call() {
    let index = indexed(&[(
        "Example.php",
        r"<?php
function prepare(...$args) {}
const PREPARE_CALLBACK = prepare(...);
class Task {
    static function start() {}
    function capture() { return [prepare(/* callback */ ...), self::start(...), $this->capture(...)]; }
    function invoke(array $args) { prepare(...$args); self::start(); }
}",
    )]);
    for name in ["prepare", "start"] {
        let calls = index.find_callers(name).unwrap();
        assert_eq!(calls.len(), 1, "{name}: {calls:?}");
        let refs = index.find_references(name).unwrap();
        assert!(
            refs.iter()
                .any(|hit| hit.kind == "references" && hit.resolution == Resolution::Resolved),
            "{name}: {refs:?}"
        );
    }
    assert!(index.find_callers("capture").unwrap().is_empty());
    let refs = index.find_references("capture").unwrap();
    assert!(refs
        .iter()
        .any(|hit| hit.kind == "references" && hit.resolution == Resolution::Resolved));
}

#[test]
fn trait_adaptations_link_calls_to_the_selected_trait() {
    let index = indexed(&[
        (
            "Traits.php",
            r"<?php
namespace Parts;
trait Cached { public function load() {} }
trait Fresh { public function load() {} }",
        ),
        (
            "Report.php",
            r"<?php
namespace App;
use Parts\{Cached, Fresh as Live};
class Report {
    use Cached, Live { Cached::load insteadof Live; Live::load as reload; }
    public function refresh() { $this->reload(); $this->load(); }
}
class Single {
    use Cached { load as singleAlias; load as replacedAlias; }
    public function replacedAlias() {}
}",
        ),
        (
            "Consumer.php",
            r"<?php
function update(\App\Report $report) { $report->reload(); $report->load(); }",
        ),
    ]);
    for (method, trait_name) in [("reload", "Fresh"), ("load", "Cached")] {
        let calls = index.find_callers(method).unwrap();
        assert_eq!(calls.len(), 2);
        assert!(
            calls
                .iter()
                .all(|hit| hit.resolution == Resolution::Resolved),
            "{method}: {calls:?}"
        );
        let refs = index.find_references("load").unwrap();
        assert!(
            refs.iter().any(|hit| hit.resolution == Resolution::Resolved
                && hit.from_symbol == method
                && hit
                    .candidates
                    .iter()
                    .any(|candidate| candidate.parent.as_deref() == Some(trait_name))),
            "{refs:?}"
        );
    }
    let single = index.find_symbol("singleAlias").unwrap();
    assert_eq!(single.len(), 1);
    assert_eq!(single[0].parent.as_deref(), Some("Single"));
    let replaced = index.find_symbol("replacedAlias").unwrap();
    assert_eq!(replaced.len(), 1);
    assert!(replaced[0].line > single[0].line);
}

#[test]
fn eloquent_attribute_keys_are_searchable_and_their_reads_link_to_the_model() {
    let index = indexed(&[(
        "app/Models/User.php",
        r"<?php
namespace App\Models;
use Illuminate\Database\Eloquent\Model;
use Illuminate\Foundation\Auth\User as Authenticatable;
use Illuminate\Database\Eloquent\Casts\Attribute as CastAttribute;
class User extends Authenticatable {
    protected $casts = ['archived_at' => 'datetime'];
    protected function firstName(): CastAttribute { return CastAttribute::make(get: fn ($v) => $v); }
    protected function casts(): array { return ['verified_at' => 'datetime', 'first_name' => 'string']; }
    public function read() { return [$this->first_name, $this->verified_at]; }
}
class Invoice extends Model {
    protected function casts(): array { return ['paid_at' => 'datetime']; }
}
class Other {
    protected function casts(): array { return ['bogus' => 'datetime']; }
}",
    )]);
    for name in ["first_name", "verified_at", "archived_at", "paid_at"] {
        let hits = index.find_symbol(name).unwrap();
        assert_eq!(hits.len(), 1, "{name}: {hits:?}");
    }
    for name in ["first_name", "verified_at"] {
        let refs = index.find_references(name).unwrap();
        assert!(
            refs.iter()
                .any(|hit| hit.resolution == Resolution::Resolved),
            "{name}: {refs:?}"
        );
    }
    for field in ["first_name", "casts"] {
        let query = format!(r"App\Models\User::${field}");
        let hits = index.search_symbols(&query, Default::default()).unwrap();
        assert_eq!(hits.len(), 1, "{query}: {hits:?}");
    }
    assert!(index.find_symbol("bogus").unwrap().is_empty());
}

#[test]
fn literal_framework_macros_link_only_to_their_receiver_and_application() {
    let index = indexed(&[
        (
            "one/app/Providers/Macros.php",
            r"<?php
use Illuminate\Support\Collection as Rows;
use Illuminate\Support\Str;
use Illuminate\Database\Query\Builder;
function uppercaseValue(string $value): string { return strtoupper($value); }
Rows::macro('toUpper', fn (string $value): string => uppercaseValue($value));
Str::macro('decorate', fn (string $value) => $value);
Builder::macro('forReports', fn () => $this);
class Fake { public static function macro($name, $value) {} }
Fake::macro('bogus', fn () => []);
",
        ),
        (
            "one/app/UseMacro.php",
            r"<?php
use Illuminate\Support\Collection;
use Illuminate\Support\Str;
use Illuminate\Database\Query\Builder;
function useMacro(Collection $rows) { return $rows->toUpper('word'); }
function wrap(Builder $query) { return Str::decorate($query->forReports()); }",
        ),
        (
            "two/app/UseMacro.php",
            r"<?php
use Illuminate\Support\Collection;
function otherApp(Collection $rows) { return $rows->toUpper('word'); }",
        ),
    ]);
    for name in ["toUpper", "decorate", "forReports"] {
        assert_eq!(index.find_symbol(name).unwrap().len(), 1);
        let calls = index.find_callers(name).unwrap();
        assert!(
            calls
                .iter()
                .any(|hit| hit.resolution == Resolution::Resolved),
            "{name}: {calls:?}"
        );
    }
    assert!(index.find_symbol("bogus").unwrap().is_empty());
    assert_eq!(
        index.find_calls("toUpper").unwrap()[0].to_name,
        "uppercaseValue"
    );
    let calls = index.find_callers("toUpper").unwrap();
    assert!(
        calls
            .iter()
            .any(|hit| hit.relative_path == "one/app/UseMacro.php"
                && hit.resolution == Resolution::Resolved),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|hit| hit.relative_path == "two/app/UseMacro.php"
                && hit.resolution != Resolution::Resolved),
        "{calls:?}"
    );
}

#[test]
fn literal_morph_aliases_link_model_and_lookup() {
    let index = indexed(&[
        (
            "app/Models/User.php",
            r"<?php namespace App\Models; class User {}",
        ),
        (
            "app/Providers/Morphs.php",
            r"<?php
use Illuminate\Database\Eloquent\Relations\Relation as Mapping;
use App\Models\User;
Mapping::enforceMorphMap(['author' => User::class, 'editor' => 'App\Models\User', 'dynamic' => $model]);
Mapping::getMorphedModel('author');
Mapping::getMorphedModel('editor');
Mapping::morphMap(['invalid' => 'not a class']);
",
        ),
    ]);
    for name in ["author", "editor"] {
        assert_eq!(index.find_symbol(name).unwrap().len(), 1, "{name}");
        let refs = index.find_references(name).unwrap();
        assert!(
            refs.iter()
                .any(|hit| hit.resolution == Resolution::Resolved),
            "{name}: {refs:?}"
        );
    }
    assert!(index.find_symbol("dynamic").unwrap().is_empty());
    assert!(index.find_symbol("invalid").unwrap().is_empty());
    let refs = index.find_references("User").unwrap();
    assert_eq!(
        refs.iter()
            .filter(|hit| hit.resolution == Resolution::Resolved
                && matches!(hit.from_symbol.as_str(), "author" | "editor"))
            .count(),
        2,
        "{refs:?}"
    );
}
