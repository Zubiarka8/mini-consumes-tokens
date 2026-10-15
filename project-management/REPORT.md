# Review Report

## Four-task recovery review — 2026-10-12

Base: a3e10b0f450a93107061152feb862a1d242820a1. Combined code tree: ddbc17b.
Candidate refs are listed in TASKS.md. Original worktrees and partial probe preserved.

- Aliases: independent diff review APPROVED at 9258d3f; later d0fe58f and 6bdbbcd change
  only its checkpoint, and f3617fd and 7d085f3 add only documentation
  (scale evidence). ADR005/migration11 and exact aliases verified. Coordinator:
  Markdown 129 pass/0 fail/1 ignore; assets 5/0; schema migration tests 4/0.
  Result: APPROVED (independent) at 9258d3f; later commits do not change reviewed code.
- CSS: two required defects (R-001, R-002 below) corrected in a361b23.
  AST offsets and capacity checks retained. Focused crate 71/0/1; focused CI Clippy,
  fmt and snapshot passed. Coordinator reviewed the follow-up diff.
  Result: CHANGES_REQUIRED resolved; APPROVED by the coordinator at a361b23 (not independent).
- PR policy: independent review APPROVED at 14a2586, including upstream-safe
  publish-branch.sh. 3ca8f70 is an ancestor. GitHub protection refresh failed DNS;
  date-qualified previous observations are not asserted as freshly checked.
  Result: APPROVED (independent) at 14a2586.
- Docs: combined limitations conflict resolved preserving both alias behavior and
  prose bounds. Published candidate 62c677f is prior reported evidence; remote
  refresh failed DNS. No recovery push.
  Result: no independent review recorded; conflict resolution checked by the coordinator only.
- Hook: 6a0daf3 removes registration and replaces stale entry point with a no-op.
  JSON validation, direct invocation and diff check passed. Original worktrees'
  active settings were updated with explicit user authorization.
  Result: no independent review recorded; checks above passed. Not claimed as APPROVED.

Findings (template format). The original review called both defects "required", so
severity major is assigned here; file/line and per-finding checks were not recorded.
- R-001: major. A relative selector containing `&` needs its implicit leading parent.
  Required check: focused CSS crate tests (71/0/1). Status: fixed in a361b23.
- R-002: major. Complex parents need individual grouping.
  Required check: focused CSS crate tests (71/0/1). Status: fixed in a361b23.

Direct performance verification after the user's follow-up:
`cargo test --locked -p mct-index --release --test relation_scale -- --ignored --nocapture`
passed on base a3e10b0 and the combined candidate. Existing synthetic fixtures:
10,000 and 100,000 symbols, resolved and ambiguous, four query scenarios per case.
Base test 15.85 s; candidate 15.75 s. At 100,000 resolved symbols, page limit 50:
313.08 ms / 315.98 ms; full list: 4.61294 s / 4.61014 s; depth 3 limit 50:
13.88 ms / 12.99 ms (base / candidate). One matched run; no alias-heavy workload
or peak-RSS claim. Raw logs: relation-scale-base.log, relation-scale-candidate.log.

Commands/results on the combined tree:
- scripts/unix/check.sh (offline): fmt passed, CI Clippy passed,
  mct-eval no regressions (accuracy 1.000, catalog approximately 8545 tokens).
  Default test step stopped on watcher failures after 1085 pass/3 fail/23 ignore.
- cargo test --locked --workspace --no-fail-fast (escalated attempt):
  1260 passed / 3 failed / 24 ignored. Only background_watcher target failed.
- cargo test --locked -p mct-mcp-server --test background_watcher, retry:
  8 passed / 3 failed. The identical tests on a3e10b0 also gave 8/3.
  Failure names: watcher_reindexes_after_a_file_change_settles,
  watcher_drops_a_deleted_file_and_follows_a_rename_incrementally,
  watcher_applies_ignore_file_edits_without_a_restart.
  These are reproduced baseline failures in this environment, not introduced by
  the four tasks. Event delivery cause remains unverified. No tests relaxed/skipped.
- npm run types:check; npm run build: passed.
- npm run test:export: 29 HTML exports / 1101 local targets passed.
- npm run test:resources: 177 matching translation keys passed.
  Build emitted dependency cache/CSS optimizer warnings; no build errors.

Logs retained locally under target/script-logs in the integration worktree:
check-summary.md, check-test.log, check-clippy.log, check-eval.log,
background-watcher-retry.log, workspace-all.log. Website checks were also captured.

Recovery refs imported through a verified local bundle; original aliases and CSS
branches fast-forwarded without rewriting history. Local hook modifications and
the interrupted worker probe preserved. Primary checkout HEAD unchanged.

Result: code and policy review accepted; combined verification has a reproduced
baseline watcher limitation. Main integration, live GitHub verification and the
public schema issue decision record remain pending. No new dependencies.


## Records reconciliation — 2026-10-10

Reviewed tree: documentation-only changes on `docs/reconcile-records-worktree-audit`, base `eee98e9ed0f1467dac32fe172cb90eb846628f83`.

- ERR-011: required `symbol` confirmed by schema; incorrect `name` reproduced the exact missing-field error; corrected call succeeded. Historical original response is unavailable and not presented as newly captured evidence.
- TASK-WEB-169: merged #169/#171/#173/#174 ancestry checked; Docs run 38083267842 passed build and deployment; current website/workflow tree matches its tested revision. The four intentional manual edits in #174 are recorded explicitly. Earlier local/browser evidence is distinguished from current CI and was not rerun.
- Worktrees: 85 inspected through Git; 68 clean, 15 dirty, two prunable/unusable, seven detached, 57 HEADs reachable from main. Git status failed for the two invalid administrative links. Ownership and ignored data prevent unconditional safe-delete claims. No cleanup or GitHub mutation performed.
- Verification: documentation diff review, relative file links and `git diff --check`; runtime tests are not applicable. Review the prepared local diff before any separately authorized publication.

## TASK-PM-001

Branch: `codex/rules-navigation`
Revision: Documentation working tree based on `25673a5`; resolve the publication commit from Git
Reviewer: Codex coordinator (self-review; maintainer acceptance remains pending)
Date: 2026-10-01

### Summary

Documentation defines the requested coordination process, durable state, isolation, recovery, and review. It retains one technical/model-policy source and optional provider coordination. No source-code changes are included.

### Critical, major, and minor issues

No confirmed finding from the documentation consistency review. This is not an independent code review or runtime recovery test.

### Verification

Relative file links checked across all 11 changed/new documents: no missing file targets. `git diff --check` passed. Manual documentation consistency review completed. No code tests were run (documentation-only scope).

### Missing tests and documentation

Runtime tests are not applicable to the documentation-only change. A future implementation of a scheduler, lock mechanism, or automated failover would require its own task and checks.

### Potential regressions and unverified assumptions

- The process relies on agents following documented write ownership and checkpoint discipline.
- Existing worktrees and the project's wider backlog have not been audited.
- MCP/index health and actual Claude/Codex outage recovery have not been exercised.
- Maintainer acceptance and integration are pending.

### Result

APPROVED for documentation consistency after self-review. Maintainer acceptance and verified integration remain pending; no runtime or independent review approval is claimed.

## Finding template

```text
Finding ID: R-001
Task / reviewed commit / reviewer / date:
Severity: critical | major | minor
File and lines or symbol:
Problem and evidence:
Why it matters:
Expected fix:
Required check:
Status and fix commit:
```

Results: APPROVED, CHANGES_REQUIRED, BLOCKED. APPROVED identifies a reviewed revision; it does not imply integration or user acceptance.

## TASK-207 review (2026-10-15, coordinator)

Revision: `feat/207-token-twins` at `3ca021a`, base `2705d64` (`origin/main`). Result: APPROVED
by the coordinator for maintainer review; not integrated into main.

- R207-1 (major, fixed): worker A's commit `a324710` embedded the output of the shell `id`
  command (local account and group names) in an assertion message in
  `crates/mct-languages/src/lib.rs`. Replaced by `{id}` and squashed into the same commit
  before integration (now `d690c7b`); `git log -p bfbf4b3..HEAD` on the branch has no match.
  The branch was never pushed. Worker A's checkpoint lists the pre-rewrite hashes.
- R207-2 (minor, fixed): the coordinator's first real-grouping test expected two groups; a
  Python docstring is a literal statement, so only one group is correct. Expectation fixed.
- Checks on `3ca021a`: `scripts/unix/check.sh` fmt ok; tests 1476 passed, 0 failed, 24 ignored;
  CI clippy (unwrap/expect/panic denied) no warnings; mct-eval no regressions.
- Scope audit: no change to `schema.rs`, `indexer.rs`, `mct-mcp-server` or `LanguageParser`;
  no TODO/FIXME/placeholder in the new code; no Cargo.toml/Cargo.lock change.
- Stage 8 and the 20-group manual review: ADR-007 "Evaluation (Stage 8)".
- R207-3 (major, fixed in `99f7058`): black-box testing found that two Python functions
  differing only in whether a statement sits inside an `if` were grouped as clones, because
  Python's indent/dedent tokens are hidden in the tree. Fixed with synthetic block markers,
  restricted to the candidate's own blocks. Prove-it tests were written first and failed
  before the fix. An intermediate version that kept every marker in the candidate's lines
  lost two real pydantic-core groups; the re-measurement caught it, and a regression test
  now covers it. Final reports are identical to Stage 8 on both projects. ADR-007 updated in
  `bfb7ee7`, which also documents three further limitations found by the same testing.
  check.sh on `bfb7ee7`: all checks passed (1488/0/24).
