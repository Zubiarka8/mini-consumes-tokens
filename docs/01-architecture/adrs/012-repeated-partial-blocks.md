# ADR-012: Repeated partial blocks

## Status

Proposed, pending maintainer acceptance (issue #207, Phase 3). Extends
[ADR-008](008-token-twins-comparison-policies.md) and
[ADR-009](009-duplicate-group-aligned-evidence.md) without changing their
groups. ADR-010 (verdicts) and ADR-011 (subgroups) are written in parallel;
this decision depends on neither.

## Context

Whole-function grouping misses the most common copy-and-paste shape: two
functions that share a run of statements but differ before or after it. The
report listed `partial-block repetition` under `alignment_not_evaluated` for
that reason. A search for shared runs inside every function is potentially
quadratic and can flood a report, so it needs explicit limits, and it must not
repeat what whole-function groups already say.

## Decision

### Scope and shape

- Opt-in only: `mct-cli duplicates --partial-blocks`. Without the flag the
  analysis collects nothing extra and the report is byte-identical to the
  report without the feature.
- A block is a repeated token sequence in two or more **different**
  functions of one language. Repetition inside one function, including between
  a nested function and the function around it, is not reported: periodic
  runs (a list of similar calls) would otherwise match themselves shifted, and
  the request was for repetition across functions. When a block occurs more
  than once in one function, only its first occurrence there is listed.
- Wording: the report calls a block a "repeated token sequence" and states in
  its header that extraction feasibility (values flowing in and out, control
  flow, side effects) is not evaluated. Nothing claims that a block is
  extractable or should be refactored. With the flag,
  `alignment_not_evaluated` replaces `partial-block repetition` with
  `extractability of repeated partial blocks (...)`.
- Output contract (TOON), only when the flag is set:
  - header `partial_blocks_scope`, `partial_blocks_warning`,
    `partial_blocks_budget` (`min_tokens`, `min_lines`, `max_windows`,
    `max_comparisons`, `max_blocks`, `functions_searched`, `windows_used`,
    `comparisons_used`, `budget_exhausted`) and `partial_block_count`;
    `budget_exhausted` is the TOON boolean `false` or the name of the first
    budget hit (`max-windows`, `max-comparisons`, `max-blocks`);
  - a top-level `partial_blocks[N]` list after `groups`, each item with `id`,
    `language`, `tokens`, `lines` and an
    `occurrences[M]{path,start_line,end_line,function}` table.
  `function` is the innermost analyzed function or method around the
  occurrence. Blocks are ordered by tokens, then occurrence count (both
  descending), then the first occurrence's path and line; `id` follows that
  order.

### Token streams

- Reuse the candidates' token streams and the selected policy. One stream per
  outermost analyzed function (byte extents when the token source reports
  callable spans, otherwise lines): a nested function's tokens are already in
  its enclosing function, and a second stream would only match itself.
- Same selection as whole functions: exact callable bytes and only the
  candidate's own paired block markers.
- `broad`: identifiers and literals erased, as for groups. `exact`: all text
  kept. `consistent`: the stream erases identifiers like `broad` but keeps each
  spelling; once a block is found, its occurrences are split by identifier
  pattern renumbered from the block's first token, and only sets of two or more
  are kept. Renumbering from the function start (as whole functions do) would
  make every block depend on what precedes it.

### Algorithm

1. Seeds: hash every window of `w = min(block_min_tokens, 64)` stream
   positions with a polynomial rolling hash (wrapping 64-bit arithmetic over
   splitmix-mixed ids). Any block of at least `block_min_tokens` tokens starts
   with such a window, so the seeds are complete; the cap only bounds the cost
   of verifying one window.
2. Verification: windows are sorted by language and hash; every equal-hash run
   is split into classes by comparing the windows token by token. A hash
   collision therefore never joins different sequences (unit-tested by forcing
   every window into one bucket).
3. Extension: each class is walked like a suffix-tree node. All positions are
   extended while their next tokens agree; when they stop agreeing, the set is
   a maximal repeat for that length and is split by the next token, and the
   children continue. A set whose positions all share their preceding token is
   skipped, because the class one token to the left reaches the same positions
   with a longer extent. This visits each class once instead of every pair.
4. Boundaries, chosen for readability: statement boundaries are not available
   without changing every token source's tables, so blocks are aligned to
   **whole lines with balanced delimiters**. The start moves forward to a
   token that is the first source token of its line in every occurrence; the
   end moves back to a token that is the last source token of its line
   (markers right after it stay, so a block that ends a Python suite keeps its
   end marker). Then the longest piece with balanced `()`, `[]`, `{}` and block
   markers is kept, and the two steps repeat until stable. Thresholds apply to
   the trimmed block: `block_min_tokens` source tokens (markers excluded) and
   `block_min_lines` token-bearing lines in every occurrence.
5. Selection, in report order:
   - a block whose every occurrence lies (by path and line range) inside
     members of one reported whole-function group is dropped: the group
     already reports it. All groups count, before `--limit`/`--offset`;
   - a block is kept only if at least one of its occurrences does not overlap
     an occurrence of a larger kept block in the same function. A shorter
     block therefore appears only when it adds a location, and then with all
     its occurrences. Overlap rather than containment: the first evaluation
     showed a periodic test table reported twice, shifted by one row.

### Budgets

Every budget is a deterministic count with a flag and a default, so the same
tree and options stop at the same point; a run is never cut by wall-clock time.

| Flag | Default | Bounds | When exhausted |
|---|---|---|---|
| `--block-min-tokens` | 40 | block size (and window length, capped at 64) | - |
| `--block-min-lines` | 6 | block size, in every occurrence | - |
| `--block-max-windows` | 2,000,000 | memory: the window table is 16 bytes per window (about 32 MB) | functions after the cut-off are not searched |
| `--block-max-comparisons` | 20,000,000 | time: token comparisons spent extending classes | classes after the cut-off are not extended |
| `--block-max-blocks` | 200 | report size | the remaining blocks are not listed |

The size defaults match the whole-function defaults. Window verification costs
at most `w` comparisons per window and is bounded by the window budget, not
counted in `max_comparisons`. Streams themselves (about 32 bytes per token) are
held only while the flag is set, and only for outermost functions that meet
both block thresholds. Each `--block-*`
flag requires `--partial-blocks`.

## Consequences

- The report can name shared runs between functions that no group joins, with
  exact line ranges, without changing groups, ranking, pagination or evidence.
- Line alignment is a proxy for statements. A statement spanning several lines
  can still be cut at a line inside it if that line ends and starts cleanly in
  every occurrence, and two statements on one line are never split.
- The `consistent` split covers the found extent only; a shorter consistently
  renamed sub-block of a broad block whose occurrences disagree is not searched.
- A block contained in one group's members but also present elsewhere is
  reported with every occurrence, including the group members.
- Group coverage compares line ranges, so a one-line callable sharing a line
  with other code can be counted as covering a block on that line.
- Budget exhaustion makes the block list partial but leaves groups complete;
  the header names the budget.
- Cost when the flag is off: none beyond one `Option` check per candidate.

## Alternatives considered

- **Pairwise extension of every equal window pair.** Simple, but quadratic in
  the size of a class: 1,000 similar test functions sharing a setup prefix give
  half a million pairs per window. The class walk does the same work once per
  class.
- **A suffix array over all streams.** Finds the same maximal repeats in
  `O(n log n)`, but enumerating repeat sets from LCP intervals can still be
  quadratic in output, and it is more code to get right for the same result.
- **AST statement boundaries.** More readable blocks, but every token source
  would need statement tables and the shared walker a new field; deferred until
  line alignment proves too coarse.
- **Wall-clock budget.** Bounds time directly but makes the report depend on
  machine load; rejected for determinism.
- **Reporting same-function repetition.** Deferred: it needs overlap handling
  for periodic runs and is a different review question.

## Validation

Unit tests in `crates/mct-index/src/duplicates/analysis/blocks.rs`: minimum
tokens and lines, maximal extension, line and delimiter trimming, block-marker
handling, nested and overlapping blocks, same-function repetition, language
separation, group exclusion, each budget's exhaustion, determinism, forced hash
collisions and the consistent split. CLI regressions in
`crates/mct-cli/tests/duplicates.rs` use
`crates/mct-cli/tests/fixtures/duplicates/blocks.{py,rs}`: two functions per
language sharing one run of statements but differing elsewhere, plus a
whole-function pair whose lines must not appear as a block. Evaluation on this
repository and cost measurements are in
[token twins evaluation](../../../benchmarks/token-twins.md#repeated-partial-blocks).
