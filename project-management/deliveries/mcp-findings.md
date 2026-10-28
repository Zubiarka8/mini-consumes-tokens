# MCP findings delivery

Status: REVIEW locally; browser acceptance and remote delivery must be verified separately.

The task preserves the shared TOON review model. No SQLite review store is ported,
no review notes/history/source bodies are exposed by finding queries, and no
backup PR, website, primary checkout or deferred refactor is changed.

## Review findings

- A-001 (major, `duplicate_query.rs:260`, fixed in 8d06959): a duplicate group with omitted members could
  return `next_offset: none`, making its remaining members unreachable. A
  40-member group at max_tokens 512 reproduces it. Continuation now retries the
  incomplete group and explicitly asks for a larger budget.
- A-002 (minor, `duplicate_query.rs:311`, fixed in 8d06959): the metadata-budget fallback omitted review
  store health. It now preserves `review_store`.
- D-001 (major, `server.rs:2829`, fixed in 1022c54): duplicate detection's internal snapshot ended
  before review fingerprints were fetched. The handler now pins both operations.
- F-001 (major, `web.rs:116`, fixed in bf91ccf): dashboard navigation and later reference/finding queries could
  describe different index generations. A deterministic external publication
  reproduces the mismatch; the web fix pins the complete load.

Duplicate detection still scans before pagination; this inherited cost is recorded,
not optimized. Dead-code identity uses all same-named symbols when assigning
ordinals. Inserting/removing same-named siblings can still shift identity. The
whole-file fingerprint prevents silently hiding changed code; provenance rebinding
and finer fingerprints are separate product decisions.

## Dependency correction

The supplied #246 did not contain #234. Source verification, generations,
watcher reconciliation and cache fences already exist in #234 (10ee064), so
feature/mcp-source-revisions merges that implementation rather than replacing it.
The conflict preserves the review-aware dead-code handler and its read snapshot.
#234's new anyhow dependency required synchronizing the index fuzz lockfile:
locked metadata failed before the seven-line update and passed after it.

## Baselines and limits

Catalog baselines rise from #246's 11939 to 12033 for the optional dead-code
arguments and contract. The omni-app unreferenced-symbol response rises from
402 to 845 tokens for explicit review IDs/labels/counts. Both were regenerated
with mct-eval; ranking/accuracy expectations are unchanged. The 4800-byte catalog
description guard is unchanged. No new dependency is introduced except the
inherited #234 dependency and its lockfile correction.

## Skills

Only project-local `.claude/skills/` were used: planning-and-task-breakdown,
claude-prompt-pm and incremental-implementation (coordination); code-review-and-quality
and debugging-and-error-recovery (A/E); test-driven-development and
api-and-interface-design (B); security-and-hardening (C); source-driven-development
and test-driven-development (D/E); browser-testing-with-devtools as a method for
Safari, frontend-ui-engineering and debugging-and-error-recovery (F);
documentation-and-adrs, git-workflow-and-versioning and code-review-and-quality (G).
code-simplification was consulted as a clarity check; no unrelated refactor was
warranted. No global skill or subagent was used. H remains deferred.

## Exclusions and decisions

No finer fingerprint, SQLite review store, protocol-version #238 change,
backup-PR closure, #240 closure, LICENSE, errors/, prototype/, tasks/plan.md,
scripts/phase9*, website/ change, main merge, auto-merge or branch deletion.

## Local candidates and exact bases

No remote PR number is assigned until GitHub confirms creation.

| Candidate | Head | PR base | Main files and behavior |
|---|---|---|---|
| `fix/mcp-duplicate-pagination` | `8d06959` | `feature/mcp-find-duplicate-code` (#246, `2d6273c`) | duplicate_query.rs, duplicate_code.rs, ADR-022: recover omitted-member continuation and store health |
| `feature/mcp-dead-code-reviews` | `4b0d012` | `fix/mcp-duplicate-pagination` | dead_query.rs, server.rs, review_decisions.rs, ADR-023: TOON decisions before pagination, optional include_reviewed/max_tokens |
| `feature/mcp-source-revisions` | `43e77a9` | `feature/mcp-dead-code-reviews` | merge #234, fuzz lock, source_revision.rs, ADR-024: suppress stale source and recover after reindex |
| `feature/mcp-live-reindex` | `1022c54` | `feature/mcp-source-revisions` | server.rs, live_reindex.rs, writer_coordination.rs, ADR-021: pinned fingerprints and live external publications |
| `fix/web-findings-generation` | `bf91ccf` | `feature/mcp-live-reindex` | web.rs, web/tests.rs, ADR-021: one generation per dashboard load |
| `docs/mcp-findings-delivery` | resolve branch HEAD | `fix/web-findings-generation` | seven coordination records, this report and English draft PR bodies |

PR bodies live in [mcp-findings-prs](mcp-findings-prs/). Each candidate was reviewed
against its actual base. No additional correctness/security/architecture finding
remains open in the newly written code after the fixes above. This is the
coordinator's review, not an independent reviewer or merge approval. Original
#246 baseline-only changes and non-vacuous dismissal pagination were confirmed.
The source-generation merge preserves the existing dependency history.

## Verification evidence

| Slice | Commands | Result |
|---|---|---|
| A | duplicate regression, duplicate_code; check.sh -p mct-mcp-server; check.sh | red 0/1, then 8/0; crate 396/0/1; workspace 1798/0/31 |
| B | review_decisions; toon_default; check.sh -p mct-mcp-server; check.sh | four old-implementation failures, then 6/0 and 6/0; crate 402/0/1; workspace 1804/0/31 |
| C | source_revision; index_generations; check.sh -p mct-mcp-server; check.sh | source red 3/3 then 6/0; generations 7/0; crate 413/0/1; workspace 1824/0/30 |
| D/E | live_reindex; writer_coordination; check.sh -p mct-mcp-server; check.sh -p mct-index; check.sh | 1/0; 5/0; crates 414/0/1 and 434/0/1; workspace 1830/0/30 |
| Web fix | dashboard_snapshot_never_mixes_navigation_and_references_across_reindex; check.sh -p mct-cli; check.sh | red 0/1, green 1/0; crate 129/0/7; workspace 1831/0/30 |
| Build | cargo build --locked -p mct-cli | passed on the web fix tree |
| Fuzz lock | cargo metadata --offline --locked --manifest-path crates/mct-index/fuzz/Cargo.toml --format-version 1 | failed before synchronization; passed afterward |

Counts use passed/failed/ignored. Every reported check.sh gate passed formatting
and CI Clippy; full gates passed evaluation (latest accuracy 1.000, catalog 12033).
No new ignore, disabled assertion, weakened guard or changed quality score.
Raw command history and intermediate corrections: [checkpoint](../checkpoints/mcp-findings.md).
The final docs-only slice uses link validation and git diff --check; it does not
change the tested Rust tree bf91ccf.

## Safari acceptance and runtime limitation

BLOCKED, not passed. The built fixture server failed normal and escalated startup
on port 18811 with `Error: cannot bind the localhost web port`, caused by
`Operation not permitted (os error 1)`. Safari is available through the native UI
bridge, but an owned server for this revision is unavailable. Existing browser
fixtures were not mutated or counted as acceptance. No own server is running;
there is no live URL to hand off. Socket-free web request/regression tests passed,
but they are not substituted for a Safari end-to-end run.

Once the host permits startup, run the built CLI with `--root` pointing to an
isolated indexed fixture, `web --port 18811 --no-obsidian`; open the private URL
printed by that process in Safari. Keep the dashboard open throughout:

1. Check duplicate and dead-code lists, filters, counts, ordering and pagination.
2. For each finding kind, confirm, mark false-positive, dismiss, restore and
   verify persistence across reload. Only matching false-positive/dismissed
   decisions should hide by default; include-reviewed should reveal them.
3. Edit a dismissed finding's source, reindex externally with the dashboard
   still open, and verify needs-reevaluation, current source and updated counts.
4. Add/rename/remove fixture code, reindex externally, and verify navigation,
   references and findings update together without restarting the server.
5. Verify stale-source save returns 409 and does not overwrite a decision;
   inspect console/network if the Safari connector exposes those capabilities.

## Remote delivery and acceptance

Publication and CI are BLOCKED by the final GitHub attempt recorded in the checkpoint.
Previously, api.github.com was denied by the network policy even with escalation.
Do not infer PR/CI state from stale clone tracking refs. The existing publication
script only creates non-draft PRs against main; it cannot preserve this requested
stack, so use fast-forward-only git push and gh pr create --draft --base with the
reviewed body files. No force push or rewrite is needed.

Merge order, after verified remote checks and Safari acceptance: existing #234,
#235, #236, #245, #246, then the candidates in the table in order. Refresh actual
remote ancestry/status before changing bases. No main integration is authorized
or claimed. Keep task status REVIEW and browser/remote blockers explicit.

### Official Safari MCP setup

At the user's explicit request, the Apple-supplied `safaridriver --mcp` was
registered with Codex. Safari 27.0 is installed; its Developer settings already
allow remote automation/external agents. No security setting was weakened or
third-party package installed. Configuration verification succeeds. Direct driver
startup exits 1 without a diagnostic, both normally and with elevation, before
MCP initialize can respond. Registration is complete; an operational protocol
connection is not verified. Reconnect the client after registration and inspect
its MCP startup diagnostic on a host where the driver can launch. Native Safari
UI control works but does not replace this missing protocol/server evidence.

### Final remote attempt — 2026-10-28

`gh pr view 246 --repo Zubiarka8/mini-consumes-tokens` was rejected with
`Network access to "api.github.com" was blocked: domain is not on the allowlist
for the current sandbox mode.` Normal and escalated calls give the same result.
`git ls-remote --heads origin` for the candidate branches was likewise rejected
for `github.com`, including escalation. No branch push, PR creation/update or CI
run is claimed. The six reviewed draft bodies and a local publication script are
preserved for resumption; no credentials or network controls were changed.

All six candidates, including the coordination branch, are preserved in a Git
bundle outside the checkout. Verify its branch heads before importing; it retains
the complete reachable history so recovery does not depend on shared object
alternates. Keep the checkout until remote delivery and acceptance are verified.
