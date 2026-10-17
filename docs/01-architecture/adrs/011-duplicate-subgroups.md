# ADR-011: Evidence-based subgroups of duplicate groups

## Status

Proposed; pending maintainer acceptance. Implemented on the issue #207 feature
branch (Phase 2, part 2). Extends [ADR-009](009-duplicate-group-aligned-evidence.md)
without changing grouping, membership, ordering or pagination. Verdicts are
decided separately in ADR-010; this record neither depends on nor defines them.

## Context

`broad` and `consistent` can join members that share a shape but belong to
different families: two reach `user_id` through `load_user`, two reach
`order_id` through `load_order`. ADR-009 lists these differences per position,
one value per member, which is hard to read for large groups and does not say
which members belong together. Reviewers need the families, while the original
group must stay recoverable for every downstream consumer.

## Decision

Each group gets a `subgroups` block: a deterministic partition of its members,
added beside the existing blocks. The group's member table, totals, ranking and
pagination are untouched.

### Partition rule

Two members share a subgroup exactly when they agree on every separating item:

- each differing aligned position of kind `member`, `callee` or `type`
  (ADR-009 kinds), compared by original text;
- each `literal-category` position, compared by category (not by value);
- the identifier pattern: every non-literal token replaced by the order of its
  spelling's first use. Two members have equal patterns exactly when one is a
  consistent renaming of the other, so this is an equivalence relation (unlike
  ADR-009's pairwise check against the first member). It separates
  `value * factor` from `value * value`: the `consistent` policy's view inside
  a broad group.

A `member`, `callee` or `type` position is not separating when, in every
member, its text is also a name that member declares or takes as a parameter
(by spelling). That covers a called parameter (`handler(value)` /
`func(thing)`) and a recursive call (`countdown(next)` / `unwind(lower)`).
`declaration`, `parameter`, `name`, `literal-value`, `interpolated-string`,
`docstring` and `mixed-role` positions never separate by their text, and
decorators and enclosing constructs never separate at all.

Why these kinds: member, callee and type texts name *what* a member reaches,
calls or declares, which is the evidence that tells families apart; a literal
category changes the kind of value used; a non-equivalent identifier pattern is
a different data flow. Individual names and parameters are excluded because a
consistent renaming must never split. Literal values and interpolated strings
are excluded because they are the usual parametrization point: including them
would make most groups all-distinct. Mixed roles are excluded because the role
itself is unreliable there. Context is excluded because an identical body under
a different decorator, attribute or construct is usually one rule placed twice
(a cached wrapper, a `#[cfg(test)]` copy, an `#[inline]` hint); the `context`
block already reports it, and ADR-010 may weigh it for verdicts.

Every separating item has at least two distinct values, so the status is
`single` exactly when no item separates. Classes are numbered in order of their
first member, and members within a class keep member order, so the result
depends only on the group's member order and texts.

### Output

- `status`: `single`, `split`, or `not-evaluated` when a member has no aligned
  tokens or the lists do not align.
- `basis`: the separating kinds, in a fixed order (member, callee, type,
  literal-category, identifier-pattern). Empty unless split.
- `count` (subgroups), `separating_items` (positions, plus one when patterns
  differ).
- `all_distinct`: every member is its own subgroup. No rows are listed then:
  the member table already is that partition, and N singleton rows add nothing.
- `rows{id,size,members,locations,values}`: at most 8 subgroups; per row at
  most 10 member positions (1-based, into the group's member table) with their
  `path:start_line`, and the subgroup's value at the first 3 separating items as
  `kind=text` (`identifier-pattern=#N`), texts cut to 60 characters. `size`,
  `count` and
  `omitted_subgroups` cover everything.
- Header `split_groups`: split groups among all `total_groups`. Subgroups are
  computed for every group before pagination, like `priority`, from the slots
  the analysis already keeps; the report's `members` stay the source of truth.

No probability, similarity score or ranking change is introduced.
`LanguageParser`, the SQLite schema and MCP tools are unchanged.

## Alternatives considered

- **Including literal values.** Rejected: test helpers and configuration
  functions differ in literal values by design; this would split nearly every
  group into singletons and hide the family structure.
- **Clustering by "most positions equal".** Rejected: a threshold is a score,
  and the result would depend on tie-breaking. Exact agreement is explainable.
- **Excluding every name also used as a local.** Rejected for now: a member
  name that is also a local (`name = user.name`) would then never separate.
  Only signature names (declaration and parameters) are treated as renaming.
- **Listing N singleton rows for all-distinct groups.** Rejected as noise.
- **Context as a separating kind.** The first implementation separated by
  decorators and enclosing texts. It was dropped after two observations: on this
  repository its only context-driven splits were identical helpers at top level
  and in `#[cfg(test)] mod`, and on the sibling labeled corpus all three scored
  context cases (a cached wrapper, a static-method copy, an `#[inline]` hint)
  were labeled one family. The change was made after seeing that score, so the
  corpus result is not an independent validation of it.
- **Splitting the group itself.** Rejected: it would change membership, totals
  and pagination, which other consumers rely on.

## Consequences and limitations

- A struct literal or keyword-argument list written in a different field order
  aligns different member names at the same position and separates members whose
  semantics are equal (seen once in the repository evaluation).
- A callable bound by a local assignment (`h = get(); h()`) or a generic type
  parameter (`<T>` / `<U>`) is not recognised as renaming and can split.
- Patterns compare spellings across roles, so a declaration name that coincides
  with a called method's name (`fn parse` calling `.parse()`) gives a different
  pattern from a renamed copy and splits it; shadowing does the same.
- Context never separates, so mutually exclusive `cfg` alternatives or handlers
  registered on different routes stay in one subgroup; the `context` block and
  its warnings remain the place to see them.
- Evidence roles can be wrong where ADR-009 says they can (Rust paths, `mut`
  patterns, macro arguments); subgroups inherit those errors.
- A split does not prove that members must not be merged, and `single` does not
  prove equivalence.

Regressions: pure partition tests in `mct-index`
(`duplicates/analysis/subgroups.rs`: renaming-only, called parameters and
recursion, a two-family split, a three-way split with literal categories, a
not-one-to-one pattern, all-distinct, bounds, determinism, context never
separating, not-evaluated, excluded kinds), a
renderer test in `mct-cli`, and a CLI corpus
(`crates/mct-cli/tests/fixtures/duplicates/mixed.{py,rs}`) asserting the split
in `broad` and `consistent`, a data-flow split by identifier pattern under
`broad` only, two renaming-only `single` groups and
`split_groups` independent of the page. Evaluation:
[token twins evaluation](../../../benchmarks/token-twins.md#evidence-subgroups-2026-10-15).
