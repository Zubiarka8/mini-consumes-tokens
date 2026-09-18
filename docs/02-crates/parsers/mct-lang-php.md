# mct-lang-php

The crate CI's fuzzer first caught the unbounded-recursion class of bug in — see [[sec-001-php-stack-overflow]].

## Special behaviors
- `self::`/`parent::`/`static::`/`Class::foo()` scoped calls: the scope is ignored, only `name` is registered as `Calls`.
- Constructor property promotion (`property_promotion_parameter`, PHP 8.0+) indexed as `Field`.
- `use TraitName;` (trait composition) mapped to `RelationKind::Implements` — `mct-core` has no "mixin" relation kind, documented as a deliberate reuse rather than a new variant.
- `SymbolKind::Trait` reused from Rust with different semantics (documented limitation).

## Related
[[overview]] · [[sec-001-php-stack-overflow]]

#security #crate
