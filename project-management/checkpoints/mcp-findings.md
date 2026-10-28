# MCP findings delivery

Owner: Codex, single writer, isolated checkout `mct-plan5`.
Base: 2d6273ca01ead94f9db39169fed58ee2eaf91f66 (#246).
Status: IN_PROGRESS. No main integration authorized.

## Reviewed plan and acceptance

1. Repair duplicate pagination: omitted members must provide a retry offset and an instruction to raise the budget; degraded-store metadata must survive the fallback. Review filtering precedes pagination. Keep the 4800-byte catalog limit and unchanged fuzz lockfile.
2. Add review-aware dead-code findings via scan_dead_code and the TOON domain. Preserve text/TOON formats, existing defaults, optional new arguments; resolve identities from all scoped symbols. Tests cover states, edits, degradation and pagination.
3. Verify source bytes before applying indexed ranges in all MCP source consumers. Omit stale snippets/signatures, reject stale skeletons, retain metadata and recover after reindex.
4. Bring in the existing #234 dependency (10ee064), resolve integration conflicts, pin the duplicate query snapshot, and prove live-server visibility and cache invalidation after external reindex. Do not duplicate existing generation/watcher code.
5. Compare legacy identity recovery with the TOON model; port only demonstrated missing behavior without importing SQLite review stores.
6. Verify the dashboard workflow with Safari where available. Keep website/ excluded.
7. Update docs per slice; focused and full check.sh gates before commits/publication.

## Initial evidence and limitations

- Supplied worktree is clean at the reported #246 revision. The primary checkout has unrelated changes and is untouched.
- #246 does NOT contain #234: source verification and snapshot APIs cannot be assumed present.
- Existing scan_dead_code already supplies the complete symbol set required by dead-id-v2.
- Duplicate partial-group pagination currently advances past omitted members, with no recoverable continuation. Metadata-budget fallback drops review_store.
- GitHub access was denied by the network policy, including an escalated retry. CI/publication remain unverified.
- The original repository refuses Git metadata writes even with escalation; development uses an isolated local clone with its own Git metadata.
- No worker dispatched. Phase H refactoring, SQLite review stores, backup PR closure, #238, LICENSE, errors/, prototype/, tasks/plan.md and website/ are excluded.

## Next action

Phase A regression reproduced: 0 passed / 1 failed, then duplicate_code 8/8 passed.
Crate check: 396 passed / 0 failed / 1 ignored, fmt and CI Clippy passed.
Full workspace tests: 1798 passed / 0 failed / 31 ignored; Clippy/evaluation pending.
The original dismissal test selects the first .rs member row and is non-vacuous.
Both original baseline diffs change only catalog_tokens (10727 to 11939).
The fuzz lockfile has no diff. Detector analysis remains unbounded before paging;
this is an acknowledged cost, not changed in this slice.

Next: finish the full gate, commit the duplicate correction, then branch for
review-aware dead-code findings.

## Phase B progress

Phase A full gate passed: 1798 passed / 0 failed / 31 ignored, fmt, CI
Clippy and evaluation passed (catalog 11939). Commit: 8d06959.
Branch: feature/mcp-dead-code-reviews; base: 8d06959.
Four initial tests failed on the old implementation, then passed with TOON
resolution. Two further tests cover restoration/all-suppressed and budgeting.
The compatibility gate exposed TOON leaking into text output; fixed and both
review_decisions (6) and toon_default (6) passed. Final crate gate: 402 passed,
0 failed, 1 ignored; fmt and CI Clippy passed. Full gate in progress.

Baselines regenerated using mct-eval: catalog 11939 -> 12033 in both suites;
omni-app unreferenced-symbol response 402 -> 845 tokens, reflecting explicit
identities and review metadata. No quality score was changed.

Plan correction: #234 already implements MCP source-byte verification as well
as generations/watcher reconciliation. Port the source tests before integrating
that dependency, then adapt the new finding handlers to its snapshots rather
than duplicating production helpers. Inspect the fuzz lock after integration:
#234 adds anyhow to mct-index but does not update its separate fuzz lockfile.

UI limitation: Safari is available; existing dashboard read-only observation
works, but starting the owned fixture server on 18811 was denied (Operation not
permitted), including the approved escalated retry. No existing review data was
modified. Full Safari acceptance remains pending. The existing 18810 service
was not stopped.

## Phase C integration in progress

Phase B full gate passed: 1804 passed / 0 failed / 31 ignored, fmt, CI
Clippy and evaluation passed (catalog 12033). Commit: 4b0d012.
Current branch: feature/mcp-source-revisions. Pending merge of dependency
10ee064f45903552f0e662f6891f34ec3ba06a6b (#234), with conflict resolved by
retaining the review-aware dead-code handler and pinning its read snapshot.
The source helper, generation/cache and watcher implementation are inherited
from that identified dependency, not newly duplicated here.

Source regressions: before dependency integration, 3 passed / 3 failed
(stale snippets, related signatures and cached source). After integration,
source_revision: 6 passed. Final MCP crate gate: 413 passed, 0 failed, 1 ignored;
fmt and CI Clippy passed. index_generations: 7 passed. Full gate in progress.

The fuzz lock failed cargo metadata --offline --locked after #234 because
anyhow was missing. Cargo metadata --offline added only anyhow 1.0.104 and
its mct-index dependency entry; the identical --locked check then passed.
The #246-only diff remains unchanged for that lockfile.

Remaining: complete C full gate and record the dependency merge separately;
add the new duplicate handler's outer snapshot and live external-reindex test.
#234 has no writer_coordination.rs: add focused tests for writer exclusion,
reader/maintenance barriers, release/recovery and linked lockfile refusal.
Retain the existing TOON whole-file identity contract; the backup provenance
rebinding depends on an incompatible store and is not transplanted.
