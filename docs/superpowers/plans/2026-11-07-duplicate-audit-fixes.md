# Duplicate audit fixes implementation plan

**Goal:** Correct callable coverage and make duplicate evidence, membership,
pagination and review validity inspectable without changing saved decisions.

**Architecture:** Language adapters own callable syntax; the index owns grouping
and the complete subgroup partition; MCP and CLI own bounded presentation.
Reuse the existing review resolver and conservative identities/fingerprints.

**Contract reference:** Existing issue #233 and ADR-022 cover the read-only MCP
query, explicit omissions, generation and recoverable pagination. Extend inputs
additively for focused member continuation and snapshot validation. No trait or
database migration, new dependency, production reindex or review write.

## Acceptance and slices

- [x] Cover Rust bound closures and macros, Lua table functions, nested functions,
  methods and declarations sharing a line. Add real-parser regressions before
  implementation; retain exact byte boundaries and reject ambiguous stale spans.
- [x] Retain a complete subgroup membership map while keeping summary caps.
  Expose explicit omitted rows/members and stable 1-based references in CLI/MCP.
- [x] Preserve verdicts, reasons, type/context/call differences and priority in
  MCP under the estimated budget; disclose every presentation omission.
- [x] Page oversized groups by identity/member offset, validate continuation
  against a digest of effective options, index and review results, and test
  complete retrieval at the maximum budget without repeating or losing members.
- [x] Report effective filters, project/version/generation, full/page counters,
  candidate coverage separately from whole-index errors, and review validity.
- [x] Exercise review lifecycle changes on temporary projects: unrelated edits,
  line insertion, rename/move, new/removed copies, split/merge and normalization.
  Preserve history; never inherit suppression across changed fingerprints.
- [x] Run focused regressions, workspace tests, strict Clippy, formatter,
  evaluator and complete diff review. Document behavior and remaining limits.

## Review focus

Small budgets must retain an explicit continuation; huge groups must terminate
when paged. Hidden groups must remain filtered before pagination. Snapshot tokens
must change after source/review/filter changes. Unknown type compatibility and
unresolved callers must never become a claim that consolidation is safe. Preserve
the prior-work checkpoint commit and leave new fixes uncommitted for review.

## Verification record

Prior work was committed as `6acddc6` before implementation. The final
`scripts/unix/check.sh --no-fail-fast` passed: formatting, 2105 tests (zero
failures, 30 existing ignored tests), strict all-feature Clippy and the evaluator.
The duplicate evaluation baseline was regenerated for the richer response;
scores remain 1.0 and thresholds remain unchanged. ADR-022 records its token cost.

The real checkout was queried twice per normalization using the freshly built
CLI, `--no-cache --no-excerpts`, six token-bearing lines and forty tokens,
without path/language filtering or partial blocks. Index generation 4266:

| Normalization | Groups | Analyzed | Below threshold | Skipped |
| --- | ---: | ---: | ---: | ---: |
| broad | 176 | 6257 | 1952 | 0 |
| consistent | 171 | 6257 | 1944 | 0 |
| exact | 35 | 6257 | 1944 | 0 |

Repeated outputs match (generation metadata excluded from comparison). All
invalid-range omissions are gone in this checkpoint. Twenty pre-existing global
index syntax diagnostics remain; unsupported languages and unindexed files
still limit whole-project coverage. Source and index contents changed since the
reference audit, so group-count differences are not attributed solely to the fix.
No manual production reindex, review migration or saved-decision edit was used.

Review identity/fingerprint policy remains deliberately conservative. New or
changed membership never inherits suppression automatically; whole-file edits
and line shifts can require reevaluation. Closure body shape and semantic type
compatibility remain unknown, and matching tokens never authorize extraction.
