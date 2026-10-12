# TASK-MD-ALIAS-135 recovery checkpoint

## Status and ownership

REVIEW: implementation complete, independently reviewed; final combined checks
are owned by the Codex coordinator. Recovered the stopped Claude worker's
committed revision `d9ffeec` without altering its worktree or uncommitted probe.

Branch: `codex/recover-alias-135-final`.
Base: `a3e10b0f450a93107061152feb862a1d242820a1`.
Implementation reviewed: `9258d3f` (resolve latest HEAD through Git).

## Approved decisions

The user approved these decisions on 2026-10-12:
- Migration 11 and `symbol_aliases`, documented by ADR-005.
- Ambiguous note names keep all candidates; their heading edge lists matching
  headings, and the separate note edge retains note-level ambiguity.
- Exact, case-sensitive aliases; a title never supplies an alias.

The assertion in `assets.rs` now expects schema 11 and refers to ADR-005.
The original issue's public discussion has not been updated by this recovery;
record the approved schema decision on #135 before publishing the feature PR.

## Completed

- Reviewed parser-declared alias storage, parameterized inserts, foreign-key
  lifecycle, no inference from old relation names, and one-time hash reset.
- Verified duplicate declarations, shared aliases, stem collisions, exact
  paths, titles, case sensitivity and section resolution.
- Verified tests cover unchanged referrers after alias edits, rename and deletion,
  with incremental and full reindex producing the same results.
- Corrected website wording to describe supported link resolution accurately.
- Independent review of the actual base-to-candidate diff: no required findings.

## Checks reproduced by coordinator

On macOS, offline with a separate build directory:
- `scripts/unix/check.sh -p mct-lang-md`: fmt passed, 129 passed / 0 failed /
  1 ignored, CI Clippy passed.
- `cargo test --locked -p mct-index --test assets`: 5 passed / 0 failed.
- `cargo test --locked -p mct-index schema::tests`: 4 passed / 0 failed.
- `git diff --check`: passed.

Prior performance evidence (not independently rerun in this checkpoint):
`relation_scale` debug 41.5 s versus base 33.6 s; release 19.2 s versus base
15.4–21.3 s. The original materialized-view regression was 145.9 s in debug.

## Remaining integration work

The coordinator runs the complete fmt/test/CI-Clippy/eval checks on an isolated
combination of the final docs, alias, CSS and policy commits. A docs conflict in
`limitations.mdx` must preserve both alias resolution and bounded prose search.
Preserve the original worker's temporary `zz_probe.rs`; it is not part of the
candidate and must not be committed or run as a shipped test.

## Publication

No alias push, PR or main merge performed. Candidate is ready for final combined
verification and an issue decision record; do not claim integration into main.
