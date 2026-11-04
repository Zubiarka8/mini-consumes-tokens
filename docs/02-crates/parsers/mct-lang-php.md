# mct-lang-php

The crate's CI fuzzer first caught an unbounded-recursion bug; see [[sec-001-php-stack-overflow]].

## Special behaviors
- Namespaces are evidence, not symbols: the file `Module` carries its namespaces (`\App\Models`, `\` for global) and declared type names as aliases, and a class reference carries the namespace it resolves to under PHP's `namespace`/`use`/alias rules (`names.rs`). The index resolves a class to the file that declares that namespace, independently of Composer's PSR-4 map, and tells apart classes with the same name in different namespaces.
- `Class::foo()`, `self::`, `static::`, `$this->` and `parent::` calls are qualified by the class they denote; any other receiver (`$o->m()`, `$class::m()`, a trait's or anonymous class's `$this`) is a `member` call whose candidates are kept, never resolved. Type hints, `catch`, `instanceof`, `X::class`, `X::CONST` and `X::$prop` are `References`.
- A `use X as Y` import is recorded under the real class name `X`, never the file-local alias.
- `*.blade.php` files skip the PHP grammar and are read by the Laravel Blade extractor (`libraries/laravel/blade.rs`). Laravel routes, view names, anonymous migrations and possible listener entry points ([ADR-027](../../01-architecture/adrs/027-possible-entry-points.md)) are extracted by `libraries/laravel/`; see [library support](../../01-architecture/library-support.md#laravel).
- Constructor property promotion (`property_promotion_parameter`, PHP 8.0+) indexed as `Field`.
- `use TraitName;` (trait composition) mapped to `RelationKind::Implements` — `mct-core` has no "mixin" relation kind, documented as a deliberate reuse rather than a new variant.
- `SymbolKind::Trait` reused from Rust with different semantics (documented limitation).

## Related
[[overview]] · [[sec-001-php-stack-overflow]]

#security #crate
