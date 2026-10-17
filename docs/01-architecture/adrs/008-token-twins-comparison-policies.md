# ADR-008: Precise token twins with explicit comparison policies

## Status

Implemented on the issue #207 feature branch; maintainer acceptance pending.
Extends [ADR-007](007-duplicate-function-report.md) and supersedes its line-only
candidate selection and single-policy limitations. Its historical evaluation is
retained and does not describe the new policies.

## Context

Line-based selection absorbs neighboring declarations and enclosing punctuation.
Replacing every identifier with one token loses repetition patterns; collapsing
Python f-strings loses executable interpolation structure. The original report's
`complete` flag describes indexed candidates, not files that failed indexing.

## Decision

- Rust and Python token sources return exact callable byte spans from their
  existing AST walk. Match them to the index by name and inclusive line range;
  an ambiguous or missing match is `invalid-range`, never a guessed range.
  Callable nodes exclude preceding decorators and attributes. Nested callable
  contents remain part of the outer callable. Legacy token sources can return
  no span metadata and retain line-based selection.
- Keep `broad` as the default: identifiers and literals are independently erased,
  preserving the original comparison policy. Boundary corrections still apply.
- Offer `consistent`: assign each distinct identifier spelling a deterministic
  per-candidate number. Seed the declaration name separately so recursive calls
  refer to the declaration's number. Preserve repeated-use patterns, but do not
  claim binding or data-flow analysis. Member names, type names and local names
  share the same spelling map; shadowing is not resolved.
- Offer `exact`: preserve all emitted identifier and literal text, including the
  declaration name. This finds token-identical copies across scopes/files; it
  still ignores trivia, including Rust visibility, and is not byte equality.
- Both opt-in policies use `TokenSource::tokenize_structural`, whose default
  implementation delegates to `tokenize`. Python expands strings containing
  interpolation nodes and retains expressions, conversions and format syntax;
  ordinary strings stay atomic. Language tables supply all grammar kinds.
  Literal fragments are normalized in consistent mode and retained in exact
  mode. Broad mode retains whole-string literal compatibility.
- Filter candidates by exact language and repository-relative file/directory
  before file reads. Normalize `.` and trailing separators; reject paths that
  escape the root. Directory matching respects component boundaries and SQL
  LIKE escaping. Deleted indexed files remain visible to the analysis.
- Report `status` for candidate analysis separately from whole-index diagnostics:
  `index_coverage` is `partial`, `unknown`, or `no-recorded-errors`. The last value
  does not claim discovery of new, unindexed files. Diagnostics are whole-index
  even when candidates are filtered.
- Keep line-based ranking by default. Offer `--ranking tokens`, using source
  tokens times additional members as an estimate of repeated volume. This is
  neither an equivalence score nor an estimate of safely removable code.
- `--limit` and `--offset` select ranked groups; `total_groups` precedes pagination
  and `omitted_groups` includes both skipped pages and the remaining tail.
  `--no-excerpts` omits source text. These are output controls, not early exits
  from analysis: totals and grouping still cover the whole filtered scope.
- Open the existing database using SQLite read-only flags. Reject both older and
  newer schemas instead of migrating. Do not change journal settings or indexed
  data. SQLite still manages its own read locks/WAL coordination; this is not a
  guarantee of immutable filesystem metadata under concurrent index writers.
- Retrieve callable candidates in one query instead of one per indexed file.
  Retain normalized sequences with full equality, never hash-only matching.
  Store no excerpts for singleton buckets. When a bucket becomes a group, reuse
  the loaded file; earlier members in other files are read lazily with the same
  content-hash check. A failed late read removes affected members, recomputes
  groups/order and marks the report partial before pagination.

No LanguageParser change, schema migration, grammar dependency in mct-index,
new language, MCP tool, persistence or near-miss matching is introduced.

## Consequences and validation

The modes intentionally return different groups. Reported policy and evidence
must agree; no mode proves semantic equivalence or that consolidation is useful.
Literal types remain erased in consistent/broad modes. Exact mode includes
function names, and quote/prefix differences in interpolated strings can keep
structurally similar expressions apart. Same-named callables sharing an indexed
line range are conservatively skipped as ambiguous.

Real CLI regressions cover boundary contamination, identifier patterns,
interpolation changes, exact literal equality, nested Python blocks, visibility,
filters, coverage, pagination, deterministic output and database preservation.
Grammar tests cover format expressions, nested strings, escaped braces and
malformed interpolation. Synthetic index tests cover ranking, skips, exclusions
and bounded excerpts. Schema tests verify no creation/migration for missing,
old and future schemas. [Evaluation notes](../../../benchmarks/token-twins.md)
describe the quality sample and reproducible scale measurements.
