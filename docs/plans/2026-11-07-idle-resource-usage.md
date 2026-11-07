# Implementation plan: idle resource usage

Status: **Implemented on `perf/idle-resource-usage`, uncommitted (2026-11-07)**, except Task 0 (operations, left to the maintainer) and Task 3 (deferred, see below). Open questions were resolved with the recommendations: Q1 (a), Q2 lease, Q3 opt-in, Q4 keep, Q5 default `off`. Branch at planning time: `feat/web-launcher-script` (`ce13a34`).

## Outcome

| Task | State | Notes |
|---|---|---|
| 0 | Not done | Killing processes and closing tabs is an operator action. |
| 1 | Done | `total_changes` decides; idempotent manifest and issue writes; hook once per process, and always for the explicit `reindex` tool (`Index::reindex_requested`). |
| 2 | Done | Stamp = database file identity + `published_generation`. |
| 3 | Deferred | After each published change the page reloads the overview, which needs the duplicate, dead-code and pending-review counts (`web/api/overview.rs`). Lazy sections would not save that work. Tasks 1–2 already remove the reloads that changed nothing. |
| 4 | Done | |
| 5 | Done | Nested `.gitignore` discovered on full reindex and on `.gitignore` events only; `.next` built in. This repository has no `.mctignore`, so only `.next` applies here. |
| 6–7 | Done | ADR-028; `mct_mcp_server::maintainer`. |
| 8 | Done | `WatcherHandle::stop`; the callback no longer blocks on a full channel. |
| 9 | Done | ADR-029. Deviation from D5: a sleeping owner **releases** the lease, so an active follower takes over; a wake failure fails that call with the reason, instead of a stale warning. |

Checkpoint A, short form: debug server on a copy of this repository, `--reconcile-interval-seconds 60`, 150 s idle: WAL and database mtime and size unchanged; 0.72 s CPU. The 15-minute run with a dashboard, and Checkpoints B–D on live sessions, remain for the maintainer: installed binaries must be updated first, because old binaries still publish no-ops and do not know the lease.

## Overview

Several MCT processes serve one project at once. On 2026-11-07 there were five `mct-mcp-server` sessions (Claude and Codex) and one `mct-cli web` dashboard. Each one keeps its own watcher and periodic full reconciliation. Lifetime CPU averages were 4–18 % of a core per MCP process and 19 % for the dashboard. Short idle windows look almost free (0.65 % over 5 min), so the cost arrives in bursts.

This plan removes the mechanisms that were proven to cost CPU, in order of benefit per risk. It then adds the structural fixes: one maintenance owner per index and an explicit idle sleep. The process stays alive because Claude Code and Codex do not reliably relaunch a stdio server that exits.

## Evidence (summary)

Full records are in two scratchpad reports from 2026-11-07, `mct-idle-investigation-2026-11-07.md` and `mct-idle-investigation-part2-2026-11-07.md`. Those files are not in the repository. Proven mechanisms:

| ID | Mechanism | Evidence |
|---|---|---|
| E1 | Every reindex, including a no-op reconciliation (`parsed=0 removed=0`), writes `index_meta` and increments `published_generation` (`touch_last_indexed_at`, `crates/mct-index/src/indexer.rs:1413`). | WAL mtime moved by exactly the reconciliation interval (60 s) with no source change. |
| E2 | `published_generation` keys the MCP response cache (`crates/mct-mcp-server/src/cache.rs:680`, `:709`), duplicate queries (`duplicate_query.rs:177`), `hybrid_search_like` (`server.rs:1981`) and the CLI duplicate cache (`crates/mct-cli/src/duplicates.rs:399`). Every no-op reconciliation by any process therefore invalidates the caches of all processes. | Source inspection; the cost was not measured separately. |
| E3 | The dashboard reloads its whole snapshot whenever the mtime or size of the database or WAL changes (`IndexStamp::of`, `crates/mct-cli/src/web.rs:48`; `Site::follow_reindex`, `:257`). With a tab polling (`main.js:472`), each WAL write costs about 4.5 s of CPU at 100 % (debug build). | `sample`: 2904/2904 samples in `follow_reindex → Site::load → load_snapshot`. Reproduced 3 times. |
| E4 | `mct-cli web` is an extra maintainer: its own watcher with a 2 s debounce, plus a startup reindex (`crates/mct-cli/src/web/autoreindex.rs:28`). Its `--help` says "without reindexing". | Source; `--help` text. |
| E5 | Independent sessions on one index each run their own full scans. | Two-server probe: one scan each at its own 60 s deadline. |
| E6 | Git-ignored generated output (`website/.next`, `website/out`, 659 files) is indexed. Parsing it costs more than twice as much as all of `crates/*.rs` (1.46 s vs 0.62 s user time). | `get_indexing_status`; `mct-cli probe` timing. |
| E7 | Closing the watcher channel schedules a final full reconciliation (`background.rs:352`). Sleep therefore needs its own stop path. | Source. |
| E8 | Claude Code and Codex do not reliably respawn an exited stdio server. | Public issue trackers (see the part-2 report). |

Measured per-event costs (release binary, copy of the repository, 1029 files): full initial index ~3 s CPU; no-op full reconciliation ≤1 s; one-file edit <1 s; 30 s of heavy churn in `target/` ~2 s.

## Architecture decisions

- **D1 — A reindex that changes nothing publishes nothing.** If no files were parsed or removed, no manifests changed and no exclusion sweep ran, skip the `index_meta` writes, the generation increment and the post-reindex hook (Obsidian). Freshness claims ("last checked at") become process-local status, not shared database writes. Rationale: E1–E3. A no-op must not invalidate other processes.
- **D2 — The dashboard follows `published_generation`, not file metadata.** One indexed read per request replaces the mtime/size comparison. WAL checkpoints, metadata writes and readers then stop triggering reloads.
- **D3 — The dashboard computes expensive sections on demand.** Duplicates and dead code load on the first request that needs them for the current generation, not on every reload.
- **D4 — One maintenance owner per canonical index.** A lifetime OS lock (`.mct-index/index.sqlite3.maintainer.lock`, `flock`/`LockFileEx`) separate from the per-write lock. The holder runs the watcher and reconciliation. Followers serve queries and keep explicit `reindex`. A crash releases the lock. Followers retry acquisition at a bounded interval (not busy). The dashboard takes part as a follower or owner.
- **D5 — Idle sleep is opt-in and keeps the process alive.** After N minutes with no index-using requests, the owner stops its watcher and reconciliation through an explicit cancel path that does not schedule the final rescan of E7. The next index-using request restarts the watcher first, runs one coalesced reconciliation, and only then answers. Exiting the process is out of scope (E8).
- **D6 — Generated output is excluded at the source.** Honour nested `.gitignore` files when `@import-gitignore` is active, and add `.next/` to the built-in exclusions. A generic `out/` stays opt-in because it can be a real source directory.

Every decision above that changes public behaviour (D1 status fields, D4, D5) gets an ADR in `docs/01-architecture/adrs/` before implementation.

## Task list

### Phase 0 — Operational mitigation (no code, maintainer action)

- [ ] **Task 0: Reduce current load.** Close the dashboard tab when it is not in use, or run the dashboard from a release build. After confirming they are unused, stop the orphaned `mct-cli web` processes (PPID 1: 21108, 61199 and six on deleted temporary roots). Add a local `.mctignore` for `website/.next/` and `website/out/` until Task 5 lands, if wanted. Close stale MCP sessions.
  - Acceptance: `ps` shows one dashboard at most; `get_indexing_status` no longer lists `website/.next` if the local `.mctignore` was added.
  - Dependencies: none. Scope: none (operations).

### Phase 1 — Stop publishing no-ops (highest benefit per risk)

- [ ] **Task 1: No-op reindex writes nothing.**
  - Description: In `reindex_inner` and `reindex_paths_inner`, call `touch_last_indexed_at` only when the run changed the index. Skip `run_post_reindex_hook` on a no-op. Audit `symbol_changes::begin/finish`, `record_walk_failure` and the parse-issue rewrites so that a no-op run commits no rows. Keep reporting the reconciliation in `ReindexReport` and in process-local status.
  - Acceptance:
    - With `--reconcile-interval-seconds 60` and no source change, the WAL mtime and `published_generation` do not change across two intervals.
    - A real change still increments the generation exactly once and runs the hook.
    - Exclusion changes (privacy sweep) still publish.
  - Verification: new tests in `crates/mct-index/tests/` (no-op leaves the generation unchanged; change increments it) and in `crates/mct-mcp-server/tests/background_watcher.rs` (an idle reconciliation without change does not publish; a change made before watch start is still found, as the existing test at `:152` requires). Run `cargo test -p mct-index -p mct-mcp-server`, then clippy.
  - Files: `crates/mct-index/src/indexer.rs`, `crates/mct-index/src/lib.rs`, `crates/mct-index/src/symbol_changes.rs`, the tests above. Scope: M.
  - Dependencies: none. Open question Q1 (status semantics) must be answered first.
- [ ] **Task 2: Dashboard stamp = published generation.**
  - Description: Replace `IndexStamp` (mtime/size) with `published_generation` read through the snapshot's index. Keep the existing guarantee that navigation and references are never mixed across a reindex (`dashboard_snapshot_never_mixes_navigation_and_references_across_reindex`).
  - Acceptance: a WAL write without a generation change (checkpoint or no-op) causes no reload; a published change is picked up on the next request.
  - Verification: tests in `crates/mct-cli/src/web/tests.rs`; `cargo test -p mct-cli`. Manual check: dashboard open, run `mct-cli call --tool get_indexing_status`; the dashboard's `ps time` does not grow by seconds.
  - Files: `crates/mct-cli/src/web.rs`, `crates/mct-cli/src/web/tests.rs`. Scope: S.
  - Dependencies: Task 1 (otherwise `mct-cli call` and reconciliations still increment the generation).

#### Checkpoint A (after Tasks 1–2)

- [ ] Workspace tests and clippy pass.
- [ ] Re-measure: dashboard plus 2 MCP servers idle for 15 min with a tab polling. Dashboard CPU stays below 1 s; no WAL mtime change.
- [ ] Maintainer reviews before Phase 2.

### Phase 2 — Dashboard cost and correctness

- [ ] **Task 3: Load expensive dashboard sections on demand.**
  - Description: Split `Site::load_snapshot` so that duplicates and dead-code data are computed the first time a page needs them for the current generation, then cached. Navigation and symbols stay in the base snapshot.
  - Acceptance: reloading after a published change costs only the base snapshot until a duplicates or dead-code page is requested; responses stay generation-consistent.
  - Verification: `cargo test -p mct-cli`; the dashboard harness tests under `crates/mct-cli/tests/dashboard/`; record the `Timings` before and after.
  - Files: `crates/mct-cli/src/web.rs`, `crates/mct-cli/src/web/api/*.rs`, tests. Scope: M.
  - Dependencies: Task 2.
- [ ] **Task 4: Correct the `mct-cli web` help text.** State that the dashboard watches and reindexes the project (until Task 7 changes that). Scope: XS. Dependencies: none.

### Phase 3 — Generated output

- [ ] **Task 5: Exclude generated output.**
  - Description: When `@import-gitignore` is active, apply nested `.gitignore` files relative to their directory. Add `.next/` to the built-in exclusions. Document the change in the indexing docs and the CHANGELOG. Existing indexes drop the newly excluded rows through the exclusion sweep.
  - Acceptance: in this repository, `website/.next/**` and `website/out/**` (with `@import-gitignore`) are not indexed; `.mctignore` negations still re-include paths; privacy exclusion tests still pass.
  - Verification: `crates/mct-index/tests/exclude*.rs`, `crates/mct-mcp-server/tests/exclusion_privacy.rs`; `cargo test -p mct-index -p mct-mcp-server`.
  - Files: `crates/mct-index/src/exclude.rs`, `crates/mct-index/src/indexer.rs` (walk), tests, docs. Scope: M.
  - Dependencies: none (independent of Phases 1–2). Open question Q3.

#### Checkpoint B (after Tasks 3–5)

- [ ] Tests and clippy pass; CHANGELOG updated.
- [ ] Re-measure the cost of a full reconciliation and of a website build.

### Phase 4 — One maintenance owner per index (ADR first)

- [ ] **Task 6: ADR and maintainer lease primitive.**
  - Description: Write an ADR for D4. In `crates/mct-index/src/coordination.rs`, add `MaintainerLease::try_acquire(index_path)`: a non-blocking exclusive lock held for the lease lifetime and released on drop or process death. Key it by the canonical index path, and record schema and configuration compatibility in the lock file for diagnostics only.
  - Acceptance: two processes, exactly one holder; killing the holder (SIGKILL) lets the other acquire; no PID-only marker.
  - Verification: unit and multi-process tests in `crates/mct-index/tests/`. Run on Linux, macOS and Windows in CI.
  - Files: `coordination.rs`, `lib.rs`, a new test, an ADR. Scope: M.
  - Dependencies: Checkpoint A.
- [ ] **Task 7: Only the owner watches and reconciles.**
  - Description: In `startup_index` (`crates/mct-mcp-server/src/main.rs:208`) and `autoreindex::watch`, start the watcher and reconciliation only while holding the lease. Followers retry `try_acquire` with a bounded interval (for example 30 s plus jitter) and take over when it succeeds. Explicit `reindex` stays available to all and is coordinated by the writer lock.
  - Acceptance: two MCP sessions plus a dashboard on one project produce exactly one full scan per interval; after the owner exits, another process takes over within one retry interval; a follower's queries see the owner's publications (generation-validated caches).
  - Verification: extend the two-session probe scenario into an integration test in `crates/mct-mcp-server/tests/`; `cargo test -p mct-mcp-server -p mct-cli`.
  - Files: `main.rs`, `background.rs`, `web/autoreindex.rs`, tests. Scope: M.
  - Dependencies: Task 6.

#### Checkpoint C (after Tasks 6–7)

- [ ] Cross-platform CI green.
- [ ] Live check: open Claude and Codex on this repository; `log stream` or server logs show one watcher owner.

### Phase 5 — Idle maintenance sleep (ADR first, opt-in)

- [ ] **Task 8: Explicit stop path for the background worker.**
  - Description: Make `spawn_watcher_with_reconciliation` return a handle that owns the debouncer and the worker `JoinHandle` and exposes `stop()`. `stop()` sends an explicit stop message, takes priority over the bounded channel (it does not block on a full `sync_channel(1)`), never schedules the final rescan, waits for an in-flight publication to finish rather than aborting it, and joins the worker. Disconnection keeps its current final-rescan behaviour.
  - Acceptance: `stop()` returns within a bounded time even with a saturated channel; no reindex runs after `stop()`; drop-without-stop behaves as today.
  - Verification: tests in `crates/mct-mcp-server/tests/background_watcher.rs`.
  - Files: `background.rs`, `main.rs`, tests. Scope: M.
  - Dependencies: Task 7.
- [ ] **Task 9: Idle sleep and wake-before-answer.**
  - Description: Write an ADR for D5. Add `--maintenance-idle <duration|off>` (and an environment variable), default `off`. Track the last completed index-using request; catalog and discovery calls do not count. On timeout the owner calls `stop()` and keeps its lease. On the next index-using request: restart the watcher, run one coalesced reconciliation (concurrent requests wait on the same refresh), then answer. If the refresh fails, answer with an explicit stale warning.
  - Acceptance:
    - With idle set to 2 min, there are no scans after the timeout.
    - An edit, add, delete, rename or ignore-file edit made while asleep is visible in the first query after wake, including the privacy exclusions.
    - N concurrent wake requests produce one refresh.
  - Verification: integration tests driven over stdio, as in the existing protocol tests; `cargo test -p mct-mcp-server`.
  - Files: `main.rs`, `server.rs` or `session_compat.rs` (request hook), `background.rs`, tests, ADR, CLI docs. Scope: M–L. If it grows past five files, split request tracking and wake into separate tasks.
  - Dependencies: Task 8.

#### Checkpoint D (complete)

- [ ] All acceptance criteria met; workspace tests and clippy green on all three operating systems.
- [ ] Repeat the lifetime CPU table from the part-2 report after about 4 h of normal Claude and Codex use, and compare.
- [ ] Update `docs/01-architecture/` (watcher and dashboard sections) and `errors/` records if any defect is reclassified.

## Risks and mitigations

| Risk | Impact | Mitigation |
|---|---|---|
| Something relies on the generation increasing on every reconciliation (cache warm-up, status "last indexed at", tests such as `writer_coordination.rs:46`). | Medium | List every `published_generation` and `last_indexed_at` reader before Task 1 (references above). Q1 decides the status semantics. Run the full test suite. |
| A no-op detection misses a real change (for example, manifest-only edits or parse-issue changes) and stale data is served. | High | Define "changed" as any row written; prefer counting writes in the transaction over inferring from the report. Add a test per change kind. |
| Lease semantics differ by operating system (network filesystems, Windows). | Medium | Reuse the existing `coordination.rs` locking approach; CI on three operating systems; fall back to today's behaviour (everyone maintains) if the lock cannot be created. |
| Wake latency makes the first query after sleep slow (≤1 s measured for a full reconciliation of this repository). | Low | Opt-in default; document it; coalesce concurrent wakes. |
| A suspended watcher misses changes. | High | Restart the watcher before the wake reconciliation (D5); test every change kind while asleep. |
| Honouring nested `.gitignore` excludes source someone wanted indexed. | Medium | Only under `@import-gitignore`; `.mctignore` negation still re-includes; CHANGELOG note. |

## Open questions

- **Q1.** After Task 1, what should `get_indexing_status` report for "last indexed at"? Option (a): the last publication that changed the index, plus a process-local "last verified at". Option (b): keep the current wall-clock update by writing it at most every N minutes. Recommendation: (a).
- **Q2.** Should the dashboard keep a watcher at all once Task 7 exists, or only follow publications and become the owner only when no MCP server holds the lease? Recommendation: participate in the lease (D4).
- **Q3.** Should nested `.gitignore` support be on without `@import-gitignore`? Recommendation: no; keep the current opt-in contract.
- **Q4.** Should `mct-cli call` keep its incremental reindex before each call? After Task 1 it is nearly free when nothing changed; leaving it as is is recommended.
- **Q5.** What default for `--maintenance-idle` after a period of opt-in use? Decide from Checkpoint D data.
