# Session Log

Record significant events only. Link to task, review, and decision details; omit transcripts, credentials, private machine paths, and routine command output.

## 2026-10-01

- Confirmed the primary checkout was clean on main at `2488ec1`; the documentation worktree was clean at `25673a5` before this change.
- Confirmed PR #96 was OPEN on `codex/rules-navigation`.
- Created TASK-PM-001 and the requested project-management records.
- Documented single-writer shared state, isolated worker checkpoints, and provider outage recovery.
- Recorded other observed worktrees as unverified inventory; none were reused or removed.
- No Claude worker was dispatched, no source files were modified, and no runtime recovery test was performed.
- Relative-link checks and `git diff --check` passed; the documentation self-review result is APPROVED.
- Task remains REVIEW until maintainer acceptance and verified integration.

## 2026-10-10

- PR #170 merged as `a94d101`; #167 and #168 closed as duplicates it carries. ERR-010 re-verified through a live MCP call after a forced reindex.
- The user stopped the Codex workers. A Claude Code session resumed from the coordinator's handoff, confirmed no worker test process was still running and preserved all uncommitted diffs.
- PR #169 updated to `6b1bede`: reconciled landing plus a merge of `main`. Website checks passed locally (types, format, resources, prefixed build, export links, 18 WebKit tests).
- Shared records reconciled with merged PRs: TASK-PM-001, TASK-LANG-001 and TASK-LIB-001 marked DONE; TASK-WEB-169 added.

## 2026-10-12 — Four-task recovery

- Recovered the stopped alias worker's committed implementation and preserved its
  temporary probe. Confirmed policy commit ancestry and docs tracking configuration.
- Used separate branches/worktrees and independent alias, CSS and policy reviewers.
- Finalized approved alias decisions, schema gate and ADR005; corrected two additional
  CSS nesting defects with regression tests and one scoped snapshot update.
- Retired Dogfooding in active original and recovery checkouts, including cached calls.
- Combined candidate checks passed fmt, CI Clippy, evaluation and all website checks;
  full tests 1260/3/24, with the same watcher failures reproduced on the base.
- Updated shared recovery records; preserved original branches and prepared local
  recoverable refs for import. No new publication or main integration.
- Recovery bundle verified and imported locally; original aliases and CSS branches
  advanced by fast-forward to the reviewed candidates. No main integration.
- Reproduced the ignored relation_scale release measurement on base and candidate:
  both pass, 15.85 s versus 15.75 s; final alias checkpoint records direct evidence.

## 2026-10-15 — Issue #207 dispatch

- User confirmed on-demand scope, Rust + Python, local-only work and three agents.
- Coordinator committed the interface contract `5efc8a7` and error records `e36b890`;
  dispatched TASK-207-A/B/C in separate worktrees (PM-DEC-207).
- Workers A, B and C completed; coordinator reviewed and merged them without conflicts,
  scrubbed a local-account leak from worker A's unpublished history (REPORT R207-1), added
  real-token end-to-end tests, ran Stage 8 on this repository and pydantic-core, finished
  ADR-007 and docs. check.sh on `3ca021a`: all checks passed (1476/0/24).
