# TASK-STORAGE-RELIABILITY

Owner: Codex, single implementation agent. No delegation.
Date: 2026-10-25. Status: REVIEW for the manual slice; overall PARTIALLY DONE.
Branch: `feature/dead-code-editor`; primary checkout.
Base: `b61cc7ce971fdc46a61259af95af7fac823ae352` plus pre-existing dirty work.

## Scope and preservation

Audit first, then an SDD specification and a bounded backup/verification slice.
Existing dashboard, parser, documentation and prototype changes belong to the
previous work and must survive. Original tracked diff and hashes were recorded
in a private temporary evidence directory before editing. No commit, push,
production migration, service shutdown, restoration or browser automation.

Creating `codex/storage-reliability-foundation` failed both normally and with
outside-sandbox execution requested: `fatal: cannot lock ref
'refs/heads/codex/storage-reliability-foundation': unable to create directory
for .git/refs/heads/codex/storage-reliability-foundation`. Continue as the single
writer in the existing checkout; no second agent or concurrent branch edits.
Branch isolation remains an integration prerequisite, not an invented success.

## Initial audit evidence

Read rules.md, PROJECT_MANAGER.md, README.md, architecture/lifecycle documents,
ADR-001/019, performance/cache notes, task/state/handoff records and manifests.
Source inspected through project MCP context packs. MCP status: 1,036 files,
18,081 symbols, index timestamp 1792929587. Eighteen parse diagnostics: sixteen
intentional malformed fixtures, the existing PowerShell line-29 rejection and
ERR-020 generated CSS line-1 rejection. No parser correction claimed.

Process listing `ps -axo pid,comm` was denied with `operation not permitted`
even after requesting outside-sandbox execution. Current process count,
loaded executable versions and live database ownership remain unverified.
No process arguments, access URLs or production databases were read.

## Acceptance and next action

Write the verified inventory and proposed SDD/ADRs before source changes.
Implement only a bounded consistent backup and isolated verification slice;
reuse dashboard migrations for compatibility checks without running migrations
on the backup source. Run focused isolated tests and repository checks with a
dedicated build target. Record actual results, remaining milestones and exact
continuation here before closure. Whole-system acceptance is still pending.

## Implementation checkpoint

Audit, SDD, ADR-020/021 and 147-entry Markdown document inventory are written.
The CLI now has dashboard-backup and dashboard-backup-verify. The implementation
uses Online Backup with a pinned read-only source, current-schema preflight,
256 MiB bound, stepped-copy deadline, integrity/FK/schema validation, streaming
SHA-256 and an atomic TOON completion manifest. Unique private bundles cannot
replace existing backups. Verification restores into a private disposable DB;
there is no production replacement command. Existing migrations are unchanged.

Actual new results: final backup module tests 17 passed, 0 failed, 0 ignored
(0.34 seconds test time). CLI help/absent-source checks passed. Formatting,
local documentation links and git diff --check passed. Unrelated pre-existing
dirty files retained their original SHA-256 hashes; intended overlaps are only
CHANGELOG, web module registration/store preflight, lifecycle follow-up pointer
and appended error observations.

Official check.sh with dedicated target/storage-reliability: 1,887 workspace
tests passed, 0 failed, 24 ignored; all-target/all-feature strict Clippy passed;
mct-eval reported no regressions. Evaluation fixture: omni-app, 33 cases,
five timed iterations each, accuracy 1.000, MRR 0.917, success 100%, p50/p95/max
2.49/11.98/20.11 ms, index build 147.4 ms, approximately 6,843 response tokens
and 11,433 catalog tokens. These are fixture observations, not storage-performance
improvement or production guarantees. No RSS/disk-write baseline was measured.

A final full-workspace test rerun after the last backup-only hardening completed
through check.sh --isolated-target --no-fail-fast --only test: 1,887 passed,
0 failed, 24 ignored; exit 0. Logs are in target/storage-reliability-final-tests.
No source edits followed this run. No worker or verification command remains
running on this task.

## Remaining roadmap and exact continuation

Status remains PARTIALLY DONE for the whole request; local implementation is
awaiting review, not integrated/deployed. First inspect the new verification
record and final test summary. Review phase 2a separately. Next implement phase
2b's configurable scheduler/retention/total-space failure handling, with isolated
retention/interruption/disk-full tests. Proposed index generations, periodic
reconciliation and writer ownership remain phases 3–4 and require the prior
compatibility/schema issue process where applicable. No issue publication,
commit or production action is authorized. Resolve Git branch isolation before
integration; obtain operator evidence of loaded binaries/ownership before any
deployment. Windows/Linux, native production FSEvents, actual process crash and
production restore/cutover acceptance remain outstanding.

Full changed-path and verification report:
[storage-reliability-verification.md](../../docs/01-architecture/storage-reliability-verification.md).

## Phase 4 indexing checkpoint — 2026-10-25

The owner authorized the Phase 4 implementation in the active dirty checkout.
Branch/HEAD remain `feature/dead-code-editor` /
`b61cc7ce971fdc46a61259af95af7fac823ae352`; all prior dirty files and fixtures
were retained. No commit, push, merge, production database access, service stop,
or deploy was performed.

### Verified code changes

- `mct-index` wraps full and path reindex operations in one SQLite
  `BEGIN IMMEDIATE` transaction, rolls back on errors/panics/process exit, and
  increments `published_generation` in the existing `index_meta` table at
  commit. No migration or dashboard database change was made.
- The parser path rereads/hashes each parsed file before writing rows and
  aborts if source bytes changed during parsing. Unreadable paths retain last
  good rows and prevent advancing `last_full_reconciled_at`.
- MCP read handlers pin a SQLite snapshot; cache generation checks reject a
  decision spanning publication. Hybrid search fences cache validation by
  generation before and after it and checks the candidate against the final
  response snapshot after async model work.
- Watcher event delivery is bounded to one pending batch, path sets cap at 256,
  continuous batches cap at 30 seconds, and a five-minute idle full hash
  reconciliation can be changed via CLI/environment (minimum 60 seconds).
  Notify errors/overflow and failed updates request/retry a full walk.
- Docs: Phase 4 SDD and ADR-021 decision/limits, COMMANDS, README, plan and this
  checkpoint. Separate pagination requests may span generations; the existing
  per-process mutex serializes MCP tools with indexing. Dashboard data stays
  independent. Legacy/external writers remain outside advisory-lock guarantees.

### Phase 4 validation status

New isolated tests: `mct-index/tests/index_generations.rs` passed 6/6,
including subprocess termination during an uncommitted run, old/new reader
visibility, parser-time source mutation and reuse of a caller-pinned navigation
snapshot. Watcher integration suite passed
12/12 including idle missed-event reconciliation. MCP library tests passed
107/107 after status formatting/cache edits; the final cache integration suite
passed 22/22. `cargo fmt --all -- --check` and `git diff --check` pass.

The focused `cargo test -p mct-index` run was interrupted while its exhaustive
`relation_scale::paged_walks_match_the_full_walk_at_every_depth` test remained
active; it is not claimed as a pass. The first MCP library run exposed an
outdated expected status string; the test expectation was updated, then all 107
library tests passed. A later cache snapshot edit was compile/test validated by
the 107-pass run. Full official `scripts/unix/check.sh --isolated-target
--no-fail-fast` initially reported 1,918 passed, 7 failed, 25 ignored. Failures
identified nested SQLite read transactions when MCP dead-code filtering called
`navigation_snapshot` from within a tool snapshot. The method now reuses an
existing read transaction. Post-fix reproduction tests passed `review_decisions`
2/2, CLI identity tests 8/8, and generation tests 6/6. The final official
workspace test run passed **1,927 tests, 0 failed, 25 ignored**. Strict
workspace Clippy passed without warnings; formatting and `git diff --check`
passed. Final MCP evaluation had no regressions (accuracy 1.000; catalog
approximately 11,433 tokens). An intermediate cache design caused six cache
integration failures; the final generation-fence approach was simplified and
the cache suite passed 22/22. No comparable indexing/reconciliation/RSS/disk/MCP
latency benchmark was run. Only macOS arm64 is available locally; Linux/Windows,
native deployment, legacy executable inventory, and production acceptance
remain pending.
