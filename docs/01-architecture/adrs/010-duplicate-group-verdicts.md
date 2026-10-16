# ADR-010: Explainable verdicts for duplicate groups

## Status

Proposed on the issue #207 feature branch, pending maintainer acceptance.
Builds on [ADR-009](009-duplicate-group-aligned-evidence.md) without changing
grouping, membership, ranking, demotion or pagination.

## Context

ADR-009 gives every reported group evidence: aligned differences by syntactic
role, an identifier pattern, primitive type spellings, callable context and a
call shape. A reviewer still has to combine those blocks to decide which
groups to read first. The goal is one verdict per group that says which
evidence led to it. The verdict must never claim more than that evidence
supports.

## Decision

- Each reported group gets a `verdict` with `level` (`REAL`, `SUSPICIOUS` or
  `FALSE_POSITIVE`) and `reasons` (kebab-case codes). The report header gets
  `verdict_scope` and `verdict_counts` (`real`, `suspicious`,
  `false_positive`). These names form a shared output contract.
- The verdict is a pure function of the group's existing evidence and priority
  (`crates/mct-index/src/duplicates/analysis/verdict.rs`). It reads no source,
  resolves no name, and uses no score, weight, threshold or probability. Each
  reason is a rule that fired, and each rule maps to a field of the evidence
  that is already printed.
- Verdicts are computed after evidence, only for the reported page, like
  evidence. `verdict_counts` cover the reported groups, not `total_groups`.
  A verdict never removes, splits, reorders or filters a group. Demotion is
  decided earlier and is unchanged.
- `alignment_not_evaluated` adds "behavioral equivalence (verdicts are
  heuristic explanations and prove nothing)".

### Rule table

Rules are applied in this order, and every rule that fires is listed. If any
rule fires, the level is `SUSPICIOUS`:

| Evidence | Reason | Why the detector cannot judge it |
|---|---|---|
| identifier pattern `not-evaluated`, or an `alignment-unavailable` warning | `alignment-unavailable` | nothing was compared |
| identifier pattern `not-one-to-one` | `identifier-pattern-differs` | data flow may differ; shadowing can also cause it |
| `undeclared_name_positions > 0` | `undeclared-names-differ` | see below |
| `mixed-role` differences | `mixed-role-differences` | members use a position differently |
| `member` differences | `member-names-differ` | different fields or methods are reached |
| `callee` differences | `callee-names-differ` | different functions are called |
| `type` differences | `type-names-differ` | compatibility is unresolved |
| primitive spelling conflicts | `primitive-type-spellings-differ` | spellings can be shadowed, so this is not proof |
| `literal-category` differences | `literal-categories-differ` | values of different literal types |
| `literal-value` differences | `literal-values-differ` | whether a value is essential is not evaluated |
| `interpolated-string` differences | `interpolated-strings-differ` | embedded expressions may differ under `broad` |
| context `not-evaluated`, or `differs` with no differing row | `context-unavailable` | context unknown |
| decorators differ | `decorators-differ` | effects are not evaluated |
| enclosing constructs differ | `enclosing-context-differs` | conditions, receivers or handlers may differ |
| call shape `call-only` (under either ranking policy) | `call-only-bodies` | the body's meaning lies in calls whose effects are not evaluated |
| call shape `mixed` | `call-shape-mixed` | members' bodies are shaped differently |
| call shape `not-evaluated` | `body-shape-not-evaluated` | shape unknown |

If no rule fires, the level is `REAL`. Its reasons are `identical-tokens` (every
aligned identifier is equal) or `consistent-renaming`, then `same-context`, and
`docstrings-differ` when docstrings differ. `REAL` therefore means the
evidence shows no difference beyond these:
- the declaration name;
- a one-to-one renaming of the members' own parameters and their uses;
- docstrings;
in the same decorators and enclosing constructs, with bodies that are not only
flat calls.

`REAL` is not proof of equal behavior: bindings, imports, types and call
effects are unresolved.

### Choices and their reasons

- **Undeclared names.** The first draft of the rule accepted any one-to-one
  renaming of unclassified `name` positions. On this repository it labeled
  12 Rust groups `REAL` through `consistent-renaming`. Six of them
  rename references to different items, not locals:
  - Rust path segments (`traversal::find_calls_bfs` versus `find_callers_bfs`,
    `queries::find_references_scoped` versus `find_calls_scoped`);
  - enum variants (`SymbolMatchMode::Prefix` versus `Fuzzy`);
  - parser unit structs (`JsTsParser` versus `RustParser`);
  - grammar functions (`tree_sitter_python` versus `tree_sitter_rust`).

  Only parameters carry a binding role in the evidence. Rust `let` patterns
  and Python assignment targets are unclassified, so a renamed local looks the
  same as a renamed reference to an outside item. The evidence now counts
  differing `name` positions whose spelling is not a parameter of its own
  member (`undeclared_name_positions`). A non-zero count is
  `undeclared-names-differ`.
  - Declaration names do not count as declared. A path that repeats the
    member's own name (`queries::find_x` inside `fn find_x`) reaches another
    item. Recursive calls are callees, so they are not affected.
  - Cost: renamed locals are conservatively `SUSPICIOUS` (for example the
    `splat.py` fixture's `head`/`first`).
- **Equal type spellings are not a reason.** 97 Rust groups have type
  positions. Equal spellings carry the same unresolved-binding caveat as every
  equal identifier, which `alignment_not_evaluated` already states. Only
  differing type names and primitive conflicts make a group suspicious.
- **Docstrings do not make a group suspicious.** ADR-009 already lists them
  without a warning, because they change documentation, not computation. They
  remain visible as a `REAL` reason.
- **Call-only bodies are suspicious under both ranking policies.** The verdict
  reads the call shape, not the ranking level, so `--trivial-calls keep`
  changes order but not verdicts.

### Why `FALSE_POSITIVE` is never emitted

A `FALSE_POSITIVE` rule would need evidence that by itself shows the members
are not duplicates. Every candidate fails that test:
- **Not-one-to-one pattern.** Shadowing and reuse of one spelling for a field
  and a local produce it for equivalent code; the `shadow_a`/`shadow_b`
  fixture does.
- **Literal category changes.** Code that differs only in a constant's type is
  often worth parametrizing.
- **Differing context.** ADR-009 says a context difference does not make the
  repetition false.
- **Mixed roles, callee, member or type differences.** These are the cases
  where a parameter or a generic could unify the members, and the detector
  cannot tell those apart from unrelated code.
- **Alignment unavailable.** This is an internal failure, not evidence.

The level stays in the contract so that a later, evidence-backed rule (for
example resolved bindings) can use it without a format change. Exhaustive
unit tests over every combination of the inputs the rules read check that no
combination yields it.

## Consequences

- One additional block per group and four header lines. Output grows by about
  170 bytes per group. The work is a scan over evidence already computed, plus
  one parameter set per member for the reported page.
- `REAL` is rare on code whose copies differ in locals or literals. On this
  repository, 36 of 153 Rust groups under `crates` are `REAL`, 30 of them
  identical copies. All 7 Python groups at `--min-lines 3 --min-tokens 20` are
  `SUSPICIOUS`. This is intended: a wrong `REAL` costs more than an extra look.
- Verdict quality is bounded by role quality (ADR-009 limitations). An
  unclassified reference that is renamed consistently and happens to be a
  parameter spelling elsewhere is not detected. Callee names are never
  interpreted. A `REAL` group can still differ in behavior through unresolved
  shadowing, imports, macros or types.

## Validation

- Unit tests: every rule, rule ordering and deduplication, policy independence
  for call shapes, and an exhaustive check over identifier pattern x
  difference kind x context status x call shape x primitive conflicts x
  undeclared names. `REAL` must come out exactly when no rule fires, and
  `FALSE_POSITIVE` never. The evidence tests cover undeclared-name counting,
  including a path that repeats the member's own name.
- Index integration: a token source without callable spans or body shapes
  never yields `REAL`, and counts follow pagination without changing
  membership.
- CLI integration on the `evidence`, `contexts`, `trivial` and `splat`
  fixtures, plus an identical pair under all three policies. Both reachable
  levels are covered, together with the header counts, `--trivial-calls keep`,
  `--limit` and the consistent policy.
- Real-code evaluation and cost:
  [token twins evaluation](../../../benchmarks/token-twins.md#explainable-verdicts).
