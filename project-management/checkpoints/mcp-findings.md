# MCP findings delivery

Owner: Codex, single writer, isolated checkout `mct-plan5`.
Base: 2d6273ca01ead94f9db39169fed58ee2eaf91f66 (#246).
Status: REVIEW locally. Safari and remote acceptance blocked. No main integration authorized.

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

## Phase D/E verification in progress

Phase C full gate: 1824 passed / 0 failed / 30 ignored; fmt, CI Clippy and eval
passed (catalog 12033). Merge f806746 preserves #234 history; 63771fc fixes the
fuzz lock; 43e77a9 adds source regression coverage and documentation.
Current branch: feature/mcp-live-reindex, base 43e77a9.

The live external-reindex test passed on the inherited #234 implementation:
MCP sees updated symbols/source and duplicate groups without restart; dismissed
duplicate/dead-code findings reappear as needs-reevaluation after an edit, and
cached rankings do not keep the old name after a second publication. This is
coverage of existing behavior, not an artificially failing new feature test.
An outer snapshot now extends duplicate detection's internal snapshot through
its review-hash query, closing the multi-query generation boundary.
writer_coordination: 5 passed. The source revisions and generation tests use
real indexes and stores, not mocks. No new tests were disabled or ignored.

Identity comparison: dead-id-v2 already uses all same-named symbols, not only
candidates, to assign ordinals; existing tests cover reference changes and line
insertion. The backup resolver depends on retained provenance in a SQLite-only
store. No such store or fingerprint-policy change was introduced. Inserting or
removing same-named siblings can still change ordinals under the existing TOON
contract; whole-file fingerprints prevent silent suppression after those edits.

Next: complete focused crate and full workspace gates, review the final diff,
then preserve the delivery report and branch bundle. Safari and GitHub remain
blocked by the recorded environment restrictions.

## Context checkpoint requested by user

Current checkout: mct-plan5 (isolated local clone); primary checkout source is
untouched. Current branch: feature/mcp-live-reindex. HEAD: 43e77a9.
No remote publication or main integration. No worker dispatched.

Completed locally:
- 8d06959: duplicate partial-member continuation and fallback review_store fix.
- 4b0d012: review-aware dead-code MCP query, six tests, ADR-023, docs and baselines.
- f806746: merge #234 (10ee064) with review-aware dead-code snapshot preserved.
- 63771fc: fuzz lockfile synchronized with #234's anyhow dependency.
- 43e77a9: six source revision regressions and ADR-024/docs.

Uncommitted D/E slice: duplicate handler outer read snapshot; new live_reindex
integration test (external writer, changed dismissals -> needs-reevaluation,
new symbols/source and stale ranking invalidation); five writer_coordination
tests; ADR-021 integration notes, protocol table and changelog/checkpoint.
Focused gates passed: mct-mcp-server 414 passed / 0 failed / 1 ignored;
mct-index 434 passed / 0 failed / 1 ignored; both fmt and CI Clippy passed.
The live test assertions were then strengthened (actual source and typed symbol
row rather than query-name presence); live_reindex passed again. Full check.sh
is currently running; do not mutate source until it completes. Expected next
step: collect its result, commit the D/E slice on the current branch.

Additional confirmed review concern to investigate next (not implemented yet):
Site::load in crates/mct-cli/src/web.rs loads navigation and subsequent queries
without one outer read snapshot, so an external publication can mix generations
in the cached dashboard state. Add a deterministic regression with a publication
between navigation and reference queries; only then pin that load's SQL reads
using #234's read_snapshot. Keep this in a separate fix/web-findings-generation
branch after the current D/E commit. Existing web wire tests need no sockets.
A private load helper with a no-op production callback can provide deterministic
interleaving without sleeps; a draft test/patch script exists only as local
scratch, and must be reviewed before use. Browser verification remains separate.

Environment blockers:
- GitHub api.github.com denied by network policy, including escalation.
- Original .git metadata writes denied including escalation: use this clone.
- Owned fixture server bind on 18811 denied (Operation not permitted), including
  approved escalation. Safari is available and existing pages were read, but
  the requested end-to-end browser flow has NOT been verified. Existing services
  and review data were preserved. No UI/dashboard asset changes yet.

Remaining delivery: finish D/E gate+commit; investigate/fix dashboard load race
with regression and focused/full checks; update seven coordination records and
final English report/PR drafts; preserve branches in a Git bundle. Publication,
remote CI and Safari acceptance remain pending access. No backup PR closure,
SQLite review store port, website/, LICENSE, errors/, tasks/plan.md or phase H.

All test counts are actual script output; no new test was ignored or disabled.
Last completed full gate (C): 1824 passed / 0 failed / 30 ignored, fmt/Clippy/eval
passed. Both catalog baselines: 12033 tokens; omni dead-code response baseline
402 -> 845 for explicit review metadata. Catalog limit remains 4800 bytes.

### Latest verified state before compaction

The D/E full check has completed: 1830 passed, 0 failed, 30 ignored; fmt,
CI Clippy and evaluation passed (catalog 12033). No check process remains
pending. The D/E slice is being committed on feature/mcp-live-reindex; resolve
its latest HEAD through Git. Next action is the separate dashboard load race
regression described above, followed by delivery records and report.

### Dashboard race closure — 2026-10-28

Branch `fix/web-findings-generation`, base `1022c54`. A deterministic external
publication between navigation and reference queries reproduced generation mixing
(0 passed / 1 failed: alpha unexpectedly acquired a reference). Holding
`Index::read_snapshot` across the load fixes it (1 passed / 0 failed). The same
test confirms next-request visibility after the guard drops. No UI asset change
or unrelated refactor. Focused and full gates are pending below.

Dashboard gate results: `scripts/unix/check.sh -p mct-cli`: 129 passed / 0 failed /
7 ignored, fmt and CI Clippy passed. `scripts/unix/check.sh`: 1831 passed /
0 failed / 30 ignored, fmt/CI Clippy/evaluation passed, accuracy 1.000, catalog
12033. `cargo build --locked -p mct-cli` passed. No test disabled or weakened.

Safari acceptance remains BLOCKED: the newly built owned fixture server on port
18811 failed both normal and escalated startup with `cannot bind the localhost
web port: Operation not permitted (os error 1)`. Safari was available, but no
server for this revision could be reached. Other fixtures were preserved and
are not counted as acceptance. No own server is running.

### Delivery preparation — 2026-10-28

Code candidate bf91ccf; coordination branch docs/mcp-findings-delivery. See
../deliveries/mcp-findings.md for the complete base/head mapping, checks and
Safari acceptance procedure. Seven shared records now reference this delivery.
English PR bodies are prepared, with draft stacked bases. Publication/remote CI
will be attempted next; no remote result is claimed before confirmation.

### Official Safari MCP registration

User explicitly requested adding the Safari connector. Registered the built-in
Apple safaridriver --mcp with Codex; configuration reports enabled stdio. Safari
27.0 installed, remote automation/external agents already enabled (observed in
Safari Developer settings); no permission checkbox changed. Driver exits 1
without output before initialize, with and without elevation. Thus registered
but runtime protocol connection is not verified. No third-party package or
Chrome used. Native app control remains available.

### Final remote delivery attempt — 2026-10-28

GitHub CLI rejected api.github.com by allowlist policy, normal and escalated.
Git ls-remote rejected github.com by the same policy, normal and escalated.
No remote branch, PR number or CI result can be verified; no upload performed.
Draft bodies and publication mapping are complete. A syntax-checked local
publication script preserves draft bases and fast-forward-only pushes for the
next permitted host/session. Overall task remains REVIEW/BLOCKED for acceptance,
not DONE. Code bf91ccf unchanged; final docs validated separately.
