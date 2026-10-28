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
