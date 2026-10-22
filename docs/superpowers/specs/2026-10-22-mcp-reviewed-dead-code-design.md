# Design: `reviewed` argument for MCP `find_dead_code` (P3)

Date: 2026-10-22

Status: specification for review. **Implementation is blocked** until the owner
authorizes P3 explicitly. Nothing in `crates/mct-mcp-server/src` changes with
this document.

Inputs: GitHub issue #228 (an input, not proof; its performance figures are
stale), `2026-10-22-shared-review-decisions-design.md` (§7.4, §7.8, §7.11, D3),
`docs/01-architecture/adrs/019-shared-review-decisions.md`. Every behavior below
that depends on the integrated tree must be re-measured there before P3 starts.

## 1. Problem

People mark dead-code candidates `false-positive` or `dismissed` in the
dashboard and the CLI. `find_dead_code` over MCP ignores those decisions, so an
agent re-reviews rejected findings and spends context on them.

## 2. Functional requirements

- FR-1 One optional argument, `reviewed`: `show`, `hide` or `only`.
- FR-2 Omitted: the response is byte-identical to today's, and no review file
  is opened or statted.
- FR-3 Review data is read through `mct_index::reviews`; the MCP crate gets no
  second parser.
- FR-4 Filtering happens before pagination: `offset` and `limit` apply to the
  filtered list and the totals describe it.
- FR-5 Read only. The MCP server never writes review state. No new tool.
- FR-6 Notes are never returned.
- FR-7 `batch` forwards the argument. Verified: `batch_params` deserializes the
  sub-query `args` into `FindDeadCodeArgs`, so adding the field is enough.

## 3. Non-functional requirements

- Latency and memory with `reviewed` set: at most +5 ms at 1,000 review rows,
  +50 ms and +64 MiB at 100,000 rows. Same datasets and method as
  `benchmarks/review-decisions.md`.
- Default call: p50 overhead at most 2%, identical SHA-256 of the response.
- Catalog: one extra optional property. The `tools/list` delta is measured and
  reported apart from the per-response cost (issue #228 estimates +94 tokens;
  not verified).

## 4. Existing contract

`FindDeadCodeArgs` (`crates/mct-mcp-server/src/server.rs`) has `path`,
`language`, `limit` (default 50), `offset` (default 0) and `format` (`toon`
default, `text`). All stay as they are. `tools.ttc` says `EFFECTS read; index
only`, which becomes false when `reviewed` is set and must change (§17).

## 5. The optional parameter

| `reviewed` | Rows | Header | `review` column |
|---|---|---|---|
| omitted | all | unchanged | no |
| `show` | all | `... (hidden: 0)` | yes |
| `hide` | actionable only | `... hidden by review: k (false-positive a, dismissed b); reviewed=only lists them` | yes |
| `only` | suppressed only | `k hidden by review ...` | yes |

An unknown value is an explicit validation error, in the style the server already
uses for enum-like arguments (`parse_match_mode`, `parse_execution_sort`):
`McpError::invalid_params("reviewed must be one of show, hide, only — got `x`")`.
It is never ignored and never treated as `show`. `null` counts as omitted, as for
`format`. The error is checked before any file is read.

## 6. Resolution semantics

Unchanged and shared with web and CLI: latest decision time (Unix ms), then the
outcome that keeps the finding visible, then the newer format (C3 > B2 > A2 >
A1). Only `false-positive` and `dismissed` whose identity and exact code
fingerprint both match hide a finding.

## 7. Legacy formats

A1/A2 (`duplicate-reviews.toon`, seconds) and B2 (`reviews.toon`, seconds) are
read through the same resolver, never rewritten, and a legacy key that matches
more than one finding decides nothing.

## 8. Missing-hash and ambiguity rules

A missing or empty content hash never hides a finding (regression test on the
branch since `cccb522`). Changed code is `needs-reevaluation` and visible.
Ambiguous keys leave the finding visible with no state.

## 9. Read versus write

Read only. Writes stay in the dashboard (`POST api/review`, CSRF nonce,
`revision`, HTTP 409 on conflict). The MCP process takes no lock and writes no
file, so it cannot block or corrupt a dashboard save.

## 10. Errors and security

- Same path safety as the index: links and hard-linked files refused, 32 MiB cap
  before parsing, fail closed.
- Only the three files in `.mct-index/` are read.
- Degraded store (any file unreadable, corrupt or over the cap): nothing is
  hidden, the response behaves as `show` without states and carries exactly one
  warning line naming the file, never row contents or notes. An omitted
  `reviewed` with the same bad file produces no warning, because no file is read.
- Review state is data from the project directory; it is never executed or
  interpolated into anything except the `review` column and the header counts.

## 11. Compact formats

With `reviewed` set, TOON gets one extra column `review` and text format one extra
field. The value is the canonical label from `Resolution::label()` — verified
names: `unreviewed`, `in-review`, `confirmed`, `false-positive`, `dismissed`,
`resolved`, plus the derived `needs-reevaluation` and `resolved-but-detected`.
Full names, not abbreviations, so the three consumers share one vocabulary. The
column is absent when `reviewed` is omitted.

## 12. Progressive retrieval and token budget

`limit` 50 stays the default. `hide` saves tokens by dropping rejected rows;
`only` audits what was hidden. No new budget mechanism. Report three numbers
apart: catalog delta, default-call cost (must be zero) and the per-row cost of the
`review` column.

## 13. Cache consistency

`find_dead_code` does not go through `QueryCache`. P3 adds no parse cache: every
call that sets `reviewed` reads the sources again, so a decision saved in the
dashboard between two calls in one server process is visible in the second. A
cache is out of scope; it needs a new decision, with an invalidation key over the
mtime, size and length of all three files, and only if the 100,000-row gate fails.

## 14. Pagination

After filtering, in the tool's current order. An offset past the filtered total
returns an empty page with correct totals.

## 15. Performance criteria

The §3 gates, measured on the integrated branch and not carried over from the
pre-integration run. The default response is hashed before and after.

## 16. Tests and fixtures

1. Default call byte-identical to a stored snapshot (`tests/toon_output.rs`,
   `tests/tools.rs`); a test that no review path is touched.
2. `show`, `hide`, `only`: headers, columns and counts in TOON and text.
3. Pagination across filtered pages; offset past the end.
4. Unknown `reviewed`: `invalid_params`, message names the allowed values.
5. Unreadable file: nothing hidden, one warning; the default call with the same
   file has no warning.
6. A decision written between two calls is visible in the second.
7. Changed code gives `needs-reevaluation`, not hidden.
8. Empty and missing content hash are not hidden.
9. A1, A2, B2 and C3 fixtures, an ambiguous key, conflicting rows.
10. A fixture with a distinctive note string never appears in any response.
11. `batch` forwards `reviewed` and rejects an unknown value with the sub-query's name.
12. Catalog size delta recorded.

## 17. Risks and rollback

| Risk | Mitigation |
|---|---|
| Default contract drift | additive argument, snapshot and hash test |
| Hiding something a person did not reject | hash and fingerprint match required; degraded store hides nothing |
| Skew between web, CLI and MCP | one resolver in `mct_index::reviews`, shared fixtures |
| Cost on a long-running server | gates; cache deferred (§13) |

Rollback: remove the field, the column and the `tools.ttc` text. Clients that
never sent `reviewed` are unaffected.

## 18. Definition of done

- Every test in §16 passes on the integrated branch, exit statuses observed.
- `scripts/unix/check.sh` exits 0.
- The §3 gates are met with the same method and datasets.
- `tools.ttc` (`EFFECTS: read; index, plus .mct-index review files when reviewed
  is set`), the `find_dead_code` row of `rules.md`, `README.md` and
  `docs/01-architecture/mcp-protocol-spec.md` are updated.
- The `crates/mct-mcp-server/src` diff is limited to `server.rs`, `format.rs` and
  `tools.ttc`, and `reviewed` omitted leaves every other tool unchanged.
- The owner has authorized P3 in writing.

## Boundaries

- Always: opt-in, read only, fail closed.
- Ask first: changing the default to `hide`, a duplicates MCP tool, exposing
  notes, a parse cache.
- Never: write review state from MCP; hide a finding without an identity and
  fingerprint match.
