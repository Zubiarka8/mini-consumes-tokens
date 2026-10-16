# ADR-009: Aligned evidence for duplicate groups

## Status

Evidence increments implemented on the issue #207 feature branch;
maintainer acceptance pending. Extends [ADR-008](008-token-twins-comparison-policies.md)
without changing its grouping policies.

## Context

Grouping answers one question: do members share a normalized token sequence?
Reviewers also need to know what that normalization hid. Under `broad` and
`consistent`, groups can join functions that reach different attributes, call
different functions, declare different types, use literals of different
categories, or (under `broad`) contain different f-string expressions. The
report gave no evidence of any of these.

## Decision

- Separate the two questions. Grouping is untouched. For each reported group,
  compare the members' original identifier and literal tokens. Equal normalized
  sequences guarantee that those tokens align position by position in every
  policy.
- Token sources attach a syntactic `TokenRole` to each token: declaration,
  parameter, member, callee, type, or a literal category (string, interpolated
  string, docstring, number, boolean, null, character, other). Language crates
  supply the grammar tables (`literal_categories`, `interpolated_literal_kinds`,
  `statement_literal_parents`, `kind_roles`, `type_kinds`, `parent_roles`). The
  shared walker resolves them from the node kind, its parent kind and field, and
  an enclosing type position. An ancestor-kind stack keeps the walk iterative.
  Nothing is resolved: no binding, import, alias or type. Unclassified means
  unknown, not "ordinary variable".
- A `role_wrappers` table names kinds the parent lookup looks through. An
  identifier whose parent is a wrapper is matched against its grandparent, and
  only by `parent_roles` entries without a field, since the wrapper's own field
  is not recorded. Python lists `list_splat_pattern` and
  `dictionary_splat_pattern`: `*args` and `**kwargs` under `parameters`,
  `lambda_parameters` or `typed_parameter` are parameters, while `head, *rest =
  items` and `for *init, last in pairs` keep unclassified targets. Splat
  arguments of a call (`f(*args)`) are a different kind and stay references.
  Rust lists no wrapper.
- Each position whose text differs gets a kind: the first member's role, or
  `mixed-role` when the members' roles disagree. A literal-category difference
  takes precedence over a value difference. Positions with the same kind and
  per-member texts form one distinct difference with an occurrence count. At
  most 12 are listed, texts are cut to 60 characters, and counts cover all of
  them.
- The identifier pattern is checked in both directions between the first member
  and each other member: `identical`, `one-to-one` or `not-one-to-one` with an
  example. It compares spellings, so shadowing can produce `not-one-to-one`.
- Warnings are explanations, not verdicts:
  - Member, callee and type-name differences: no domain is inferred from names.
  - Type compatibility is never claimed: aliases, unions, generics and imports
    are unresolved.
  - Literal value and category changes: whether a value is essential or could
    become a parameter is not evaluated.
  - Interpolated strings that `broad` compares whole.
  - A non-one-to-one identifier pattern.

  Docstring and declaration-name differences are listed without a warning.
- `broad` keeps whole-string literal compatibility. Its hidden f-string
  differences become visible as evidence instead of changing its grouping.
- Original texts are interned once per analysis and kept per candidate as
  compact slots. Evidence is computed only for the reported page, after
  pagination and after late excerpt failures have removed members.
- Callable context is compared apart from the aligned tokens (second slice):
  - Language crates list `decorator_kinds` (Python `decorator`, Rust outer
    `attribute_item`) and `context_kinds` (kind to label: `class`, `function`,
    `if`/`elif`/`else`, `try`/`except`/`finally`, `with`, loops and `match`
    for Python; `mod`, `extern`, `impl`, `trait`, `fn`, `closure` and control
    expressions for Rust).
  - The shared walker attaches consecutive decorator siblings, across trivia,
    to the next node. A callable's become `CallableSpan::decorators`. An
    enclosing construct's prefix its label, so a test module is
    `#[cfg(test)] mod` and a decorated class `@dataclass class`.
    `CallableSpan::enclosing` lists labels outermost first. Decorator text drops
    its comments and collapses whitespace.
  - Each group gets a `context` with a status (`same`, `differs`,
    `not-evaluated`) and one row per aspect: the shared value, or one value per
    member. A differing aspect adds `decorators-differ` or
    `enclosing-context-differs`; a member without callable spans adds
    `context-unavailable`. These are warnings: a difference does not split or
    discard a group.
  - Only labels and decorator texts are compared. Conditions, context managers,
    impl targets and class names are not part of a label, and no decorator is
    resolved or given an effect.
- Trivial calls are ranked, never removed (third slice):
  - Language crates list `expression_statement_kinds`, `call_kinds`,
    `non_flat_kinds` and `delimited_argument_kinds`. The walker reports a
    `BodyShape` per callable: direct body statements (docstrings, comments and
    attributes excluded, a Rust tail expression included) and how many are a
    flat call: one call or macro invocation whose subtree holds no other call,
    closure, comprehension, conditional, block, `await` or `?`, and no macro
    `token_tree` nested in another.
  - A group is `call-only` when every member's body is non-empty and only flat
    calls; `mixed` when only some are; `other` when none are; `not-evaluated`
    when a member has no shape. Callee names are never read, so a `print` or
    `log` call is treated like any other.
  - `--trivial-calls demote` (default) ranks call-only groups after all other
    groups, keeping their relative order; `keep` leaves the ranking order.
    Every group gets a `priority` block with a level, the call shape and a
    reason; the header reports `demoted_groups`. Totals and pagination include
    demoted groups, so nothing is lost.
- The report header lists criteria not evaluated: binding and scope resolution,
  type compatibility, effects of decorators and enclosing conditions, effects
  of calls in call-only bodies, and partial-block repetition.

`Token` and `CallableSpan` gain fields and `TokenClasses` gains tables. All are
new on this branch; `LanguageParser`, the SQLite schema and MCP tools are
unchanged.

## Consequences and validation

### Primitive spelling evidence (F1.3)

Type compatibility remains `unknown`, including when primitive spellings differ.
Both Python builtins and Rust primitive names can be shadowed (`type u32 =
String` is valid Rust). No import, alias or binding is resolved. Calling these
differences incompatible would require an assumption the detector cannot verify.
The maintainer selected spelling conflicts with unknown compatibility on
2026-10-15 rather than conditional incompatibility claims.

Language adapters identify primitive spellings only when the token is a whole,
direct parameter, return or local-variable annotation. Compound types, qualified
paths, generic arguments, unions, Optional, forward-reference strings and custom
names stay unknown. The shared walker applies the tables without language tests.
Aligned type positions are counted even when their texts are equal. A position
has a primitive spelling conflict when at least two members carry recognized,
different primitive spellings; another member with an unknown annotation does
not hide that evidence. Mixed roles remain unknown. The report lists bounded
per-member values and lines for conflicts, with counts over all positions.
Neither equal spellings nor a spelling conflict establishes compatibility or
incompatibility. Warnings never remove, split, reorder or classify groups.

The header retains type compatibility in `not_evaluated`: only syntactic spelling
evidence has been added. All three normalization policies remain unchanged.

### Evidence limitations and regression coverage

The TOON report carries `alignment` (including `types`), `context` and `priority`
blocks per group. Evidence can be wrong where roles are: Rust `mut` patterns,
Rust path segments and locals, and format placeholders inside Rust string
literals are not analyzed. Python splat parameters are classified only through
the wrapper tables; the wrapper lookup goes one level up, so a wrapper nested in
another wrapper leaves its identifier unclassified. Context labels name
kinds only, so two methods of different classes, or functions under different
`if` conditions, have the same context; inner attributes (`#![..]`) are not
collected. The call-only shape is syntactic: operators and attribute chains in
arguments still count as flat, `await f()`, `return f()` and `x = f()` are not
flat calls, and a Rust macro's arguments are unparsed, so only nested
delimiters, not calls, are seen inside them. Demotion changes order, not
membership, and says nothing about whether the calls matter. A difference
does not prove that a group is not a duplicate, and no difference does not
prove equivalence.

Regressions: walker role mechanics (`mct-tree-sitter`), Python and Rust role and
category tables, pure alignment logic (`mct-index`), and a real CLI corpus
(`crates/mct-cli/tests/fixtures/duplicates/evidence.{py,rs}`). The corpus covers
consistent renaming, non-one-to-one patterns, member/callee/type differences,
equal and absent annotations, literal value and category changes, f-strings,
docstrings and shadowing. A second corpus
(`crates/mct-cli/tests/fixtures/duplicates/contexts.{py,rs}`) covers differing
and shared decorators, `try`/`except` branches, a `#[cfg(test)]` module and an
outer attribute; walker, adapter and comparison logic have unit tests of their
own. A third corpus (`crates/mct-cli/tests/fixtures/duplicates/trivial.{py,rs}`)
covers call-only logging and recording bodies, real logic around `print` and
`log` calls, and calls with nested calls, under both ranking policies.
`crates/mct-cli/tests/fixtures/duplicates/splat.py` covers renamed `*` and `**`
parameters next to renamed unpacking targets and later reads, in `broad` and
`consistent`; the wrapper lookup has its own walker test.

Paired measurements on the same synthetic workloads appear in
[token twins evaluation](../../../benchmarks/token-twins.md#aligned-evidence-cost).
