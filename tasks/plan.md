[ROL]

# Web review database lifecycle and section controls

Detailed current-state and improvement reference:
[Web dashboard database and review lifecycle](../docs/01-architecture/web-database-lifecycle.md).

The coordinator documents current persistence and reindex behavior and plans
verification and review controls. Planning only: no product changes, database
mutation, browser control, delegation or publication is authorized by this plan.
The owner reviews proposed behavior before implementation.

[CONTEXTO]

Date: 2026-10-25. Branch: feature/dead-code-editor.
Base: b61cc7ce971fdc46a61259af95af7fac823ae352, with existing uncommitted work.
Follow rules.md and PROJECT_MANAGER.md. The prior copy-label implementation is
in REVIEW; its automated results were 228 Rust and 33 Node tests passing, with
format, strict Clippy and mct-eval passing. Those checks were not rerun for this
planning task. Real-browser acceptance remains blocked by localhost binding
restrictions and WebKit launch failures. Do not use Chrome without explicit
current-task authorization.

## Verified storage boundaries

- .mct-index/index.sqlite3 contains derived analysis: files, symbols and relations.
  Reindexing updates derived data. The dashboard reads it through project APIs.
- .mct-index/dashboard.sqlite3 contains user work and is not reconstructed by
  reindexing. Never treat the whole .mct-index directory as disposable: it also
  contains this user database.
- reviews stores the current finding decision and reviewed content fingerprint;
  review_history stores decision transitions, with a bounded 50,000-row history.
- file_reviews stores each duplicate copy's working status and note, keyed by
  group identity/path/member_key, with a server-verified code hash. It has no
  append-only note/status history. Editing replaces that copy's current values.
- comments contains finding comments. Comments can be created, edited and deleted;
  their edits do not constitute a complete append-only audit log.
- meta contains migration/import markers, revisions and retention/deletion state.
- Unsaved drafts live in the tab's memory. Navigation/polling preserve drafts,
  but closing/crashing the tab is not a guaranteed recovery mechanism.

## Save lifecycle

1. A user edits a note/status. A copy-label draft is local until Save is submitted;
   comments and finding decisions have their own submission actions.
2. The browser submits the target identity and, for copy labels, its member key
   and expected revision; a reviewed fingerprint is required for finding decisions
   and optionally checked for copy labels. The browser never supplies code_hash.
3. The server validates session/CSRF, input, membership and analysis freshness.
   It verifies indexed source when a write requires reviewed code.
4. A short BEGIN IMMEDIATE transaction checks the revision, writes all related
   rows and metadata, then commits. Failure rolls back; stale revisions/analysis
   return 409 instead of silently replacing newer work. Annotation writes also
   advance the finding revision without changing its verdict.
5. After success the editor refreshes saved details. Version polling refreshes
   visible data in other tabs. The database uses WAL and a 5-second busy timeout;
   this is not a promise of zero latency or guaranteed saving during failures.

## Reindex and visibility matrix

| Scenario | Saved copy label/note | Finding decision and visibility |
| --- | --- | --- |
| Set a copy to Reject duplicate | Saved as rejected, independent of sibling labels | Does not dismiss the finding; it can remain in the list immediately and after reindex |
| Reindex unchanged code with the same group identity | Retained | Decision remains valid; web visibility depends on selected status filter |
| Move lines or edit outside reviewed ranges | Retained when identity and verified range text remain the same | No content reevaluation for unchanged reviewed ranges |
| Edit a reviewed copy body, keeping its group identity | Retained for uniquely identifiable copies, with code_changed warning | An existing nonpending group decision requires reevaluation; an undecided group stays pending |
| Rename a symbol/path or add/remove a group member | No automatic transfer to the new identity; old rows retained | If still detected, new identity is pending; old identity is retained as an orphan |
| Remove the duplication entirely | Rows retained, not erased by reindex | No current duplicate candidate; retained review details remain inspectable |
| Mark the whole group false_positive or ignored, unchanged verified content | Copy labels remain independent | MCP/CLI suppress the valid dismissal; the web All view can still show it |
| Change the content of a dismissed group | Copy labels warn where association is proven | The dismissal no longer suppresses changed work; reevaluation is needed |

Ambiguous same-name copies cannot inherit an unproven association. Unmatched
labels remain retained. Legacy labels without code hashes do not claim a code
change. Restoring original content can restore the earlier valid decision/warning
state when identity and association match.

## Source evidence checked through the project MCP

- web/store.rs: set_member_review (1024), state (512), set_inner (612),
  add_comment (747). Atomic saves, labels, history, revisions, state evaluation.
- web/api.rs: duplicates (227), matches_status (130). The web list filters by
  finding state; per-copy rejected is not a dismissal filter.
- web/spa/spa.js: saveFile (1132). Draft capture, copy key, conflict preservation.
- mct-index/src/review_keys.rs: group_identities (117). Identity uses language
  and sorted member path/name/kind, excluding line numbers and content.
- mct-index/src/review_state.rs: suppresses (234), duplicate_visibility (277).
  Only valid unchanged false_positive/ignored decisions suppress findings.
- ADR-019 D1-D3, D8, D10-D15: separate stores, migrations, warning semantics,
  provenance, history and retention.

MCP health: 1,031 files, 17,987 symbols, index timestamp1792917470. Eighteen known
parse diagnostics persist: 16 deliberate malformed fixtures (ERR-003),
review-check.ps1 line29 and generated spa.css line1 (ERR-020). Context queries
succeeded; no new parse fix is claimed.

[TAREA]

## Ordered work and acceptance

### DB-1 — Document and confirm persistence behavior (completed planning)

Scope: this plan and private task records. Provide the two-store boundary, save
lifecycle and reindex matrix, explicitly distinguishing a rejected copy from a
dismissed group. Verification: source context packs and ADR cross-checks above.
Do not change the existing decision policy or transfer labels across identities.

### DB-2 — Validate the complete save/reindex lifecycle (proposed)

Depends on DB-1. Scope: focused tests in web/tests_copy_labels.rs,
web/tests_decisions.rs and web/tests_identity.rs, adding a separate test module
if changes would make ownership unclear. Preserve all existing assertions.
Acceptance: notes and statuses survive save/reopen/unchanged reindex; rejected
copy remains a candidate; group dismissals and changed-group reappearance follow
the matrix; stale writes retain drafts. Reuse current coverage where sufficient.
Verification: scripts/unix/check.sh -p mct-cli, separately --only eval, and the
Node editor suites. Record actual results rather than treating prior runs as new.

### UI-1 — Open/close all complete review sections (new owner requirement)

Depends on DB-1; clarify the exact existing section boundaries through MCP before
implementation. Applies to complete expandable review sections, including the
section described by the owner as “Un editor de código”. Add two visible controls
with Spanish labels “Abrir todos” and “Cerrar todos”, plus English translations.
Owner-confirmed placement from the supplied screenshot: group both buttons in
the top toolbar immediately to the right of the search field, on the same row
when space allows. Keep them outside the collapsible review content so both
remain reachable after Cerrar todos. At narrow widths, allow the toolbar to
wrap without overlapping the search field or existing toolbar actions.
These controls expand/collapse sections; they do not change a copy's status,
delete review data, close browser tabs or submit forms.

Likely files: web/spa/index.html and spa.js; generated spa.css if new classes are
introduced; one focused Node test file. No database migration is needed for
presentation-only open/closed state.

Acceptance:
- Abrir todos opens every section in its stated review scope; Cerrar todos
  collapses every section in the same scope, including complete code sections.
- Individual sections can still be toggled; repeated bulk actions are idempotent
  and do not reset the active finding, saved notes/statuses or unsaved drafts.
- Controls are keyboard accessible and do not issue save/delete requests. Keep
  usable access to the bulk controls after all content is collapsed.
- Both buttons appear together beside the top search field as requested in the
  owner's screenshot; responsive wrapping preserves access to search and actions.

Verification: focused Node tests for bulk/individual toggles and preserved
review/draft state; regenerate assets if required. Perform a real-browser check
when the environment supports it, without Chrome unless explicitly authorized.
Top-toolbar placement beside search is confirmed by the owner. Exact section
boundaries and implemented browser behavior remain to be verified.

### DB-3 — Make review scope understandable in the interface (proposed)

Depends on DB-1. Scope: SPA labels/help text and their focused tests. Explain
that Reject duplicate labels one copy and that finding decisions control the
group. Show saved/conflict and changed-code information without changing verdicts.
Acceptance: user can distinguish saved labels from group decisions and local
unsaved drafts; no silent auto-dismissal is introduced. Verification: Node tests
and the real-browser lifecycle when available.

### DB-4 — Retention and recovery review (proposed operational design)

Depends on DB-2. Document what is overwritten versus historized, the history/row
limits and protection of dashboard.sqlite3 during cleanup and reindex. Define a
consistent SQLite backup/restore procedure before implementing any backup command;
do not copy only the main database while ignoring a live WAL. A full audit log of
copy-note/status edits would be a separate product decision and schema design.
Acceptance: a recovery exercise restores saved decisions, labels and comments;
no automatic pruning/deletion or new migration is introduced by this plan.
Verification: isolated temporary-store recovery tests after a concrete design is
approved, plus documentation review.

## Checkpoints and constraints

After DB-2/UI-1, compare the scenario matrix against automated and browser
results. Record blocked checks explicitly. After DB-3/DB-4, review terminology
and the recovery proposal. Preserve existing dirty work, prototype/ and
crates/mct-index/fuzz/Cargo.lock. No commits, pushes, PRs, new dependencies,
reindex of the primary checkout or direct index database reads in this task.

[ENTREGA]

Deliver this written plan and the concise explanation to the owner. Keep the
implementation task in REVIEW until real-browser acceptance is possible. Track
follow-up tasks in the project's canonical private TASKS.md rather than creating
a second competing todo file. Product implementation is not part of this planning
turn. Unverified: real browser behavior, responsive placement/section boundaries for bulk
section controls, backup/restore procedure, and any new lifecycle test executions.


## UI-1 implementation update — 2026-10-25

The owner subsequently requested actual visible buttons at the supplied editor
URL, authorizing UI-1 implementation. This supersedes the planning-only boundary
for UI-1 alone; DB-2, DB-3 and DB-4 remain proposed.

Implemented together immediately after search in the top toolbar, Spanish
“Abrir todos” / “Cerrar todos”, using existing translated actions to open/close
complete duplicate code panes. Controls remain visible when every pane is closed.
Notes, labels, selection and drafts are preserved, with no save/delete requests.
Assets regenerated with pinned cached dependencies; executable rebuilt.

Verification: Node35 passed, Rust228 passed, fmt/strict Clippy passed, JS syntax
and git diff --check passed. No existing test changed. No Chrome/browser used.
Live delivery remains BLOCKED: curl to the supplied localhost URL could not
connect both normally and with escalation; starting the rebuilt server on8787
returned `cannot bind the localhost web port`, caused by
`Operation not permitted (os error 1)` both normally and with escalation.
No server process started. Restart the rebuilt target/debug/mct-cli from a
working native terminal, then refresh the editor to verify layout visually.
Implementation remains REVIEW; no commit or publication.


## Authorized reliability specification — 2026-10-25

The current owner request authorizes DB-2, DB-3, DB-4 and UI-1 investigation,
minimal fixes and isolated validation, superseding the earlier planning-only
boundary. One writer continues the existing dirty branch; preserve all prior work.

### Problem and requirements

Embedded assets may differ from a running process. Review annotations must survive
reindexing and remain associated only with verifiable copies. Recovery must include
committed WAL data. Documented behavior requires fresh evidence.

Functional acceptance: the scenario matrix above is exercised through real store,
indexer and HTTP handlers; unchanged reindex/reopen retains both independent labels;
line movement retains proven associations; edits warn and restoration clears the
warning; rename, membership change and disappearance retain old annotations without
transferring them. Unavailable source cannot claim unchanged code. Concurrent stale
writes return 409 without modifying data; in-tab navigation and data refresh retain
copy/group drafts. Bulk controls preserve all draft generations and issue no writes.
Review scope, local drafts, save success and unverifiable code must be explained.

Nonfunctional acceptance: no new dependencies or schema changes; existing input
limits, source path guards, authentication and atomic revisions stay enforced.
Tests use temporary roots and databases only. Never delete .mct-index or rotate
an active token. No unowned process may be stopped.

### Design, edge cases and security

Reuse store transactions, member resolver, content hashes and revision contracts.
Add focused regressions where the existing matrix lacks evidence; fix only
reproduced failures. Test identical/ambiguous siblings, same-path copies, missing
source, retired groups, concurrent connections and migration/rollback bounds.
Validate a consistent VACUUM INTO snapshot against a live WAL with autocheckpoint
disabled; reopen the restored database, compare all user tables and revisions,
and run integrity checks. This is a recovery design/test, not an automatic backup
service. Preserve legacy files and metadata; never overwrite a live database.

### Validation and Definition of Done

Run Node dashboard suites, asset regeneration when needed, executable build,
cargo fmt --all -- --check, workspace tests and workspace all-target Clippy;
use scripts/unix/check.sh for its stricter CI lint and evaluation as available.
Record exact results and existing failures. Verify embedded assets through real
HTTP handlers; native listener/browser checks require a working permitted host.
Chrome remains unauthorized. DONE requires the complete acceptance matrix and
real responsive/keyboard browser behavior, not compilation alone. If host access
prevents those checks, report PARTIALLY DONE with the remaining steps. No commits,
pushes, merges or releases are authorized.


## Authorized delivery follow-up — 2026-10-25

Owner request authorizes environment diagnostics, official asset generation,
recompilation, isolated startup and acceptance validation. Existing processes,
active stores/tokens and previous dirty work remain protected; no publication.
Single Codex writer continues feature/dead-code-editor at b61cc7ce971fdc46a61259af95af7fac823ae352.

- [x] Reconcile current listeners, mapped executable inodes and database roots.
- [x] Regenerate through the official pinned npm script; compare generated bytes.
- [x] Build with --locked and verify complete embedded HTML/CSS/JS/SVG/vendor bytes.
- [x] Rerun CLI checks (232 pass, fmt/strict Clippy) and Node suites (36 pass).
- [x] Verify primary dashboard main/WAL/SHM and prior dirty product file hashes.
- [ ] Start an updated native isolated instance: loopback bind EPERM even elevated.
- [ ] Complete real responsive/keyboard/two-tab acceptance: WebKit aborts134 even elevated.
- [ ] Cut over intended owned process after explicit authorization and backup.

Result: PARTIALLY DONE. Asset blocker resolved; native delivery remains unverified.
See lifecycle section13 for exact pipeline, evidence, all12 criteria, environment
limits and executable manual acceptance/cutover procedure. No source changes.

### Native acceptance recheck and operator handoff — 2026-10-25

Owner selected an operator handoff; no alternate execution host was supplied.
Remaining execution status: BLOCKED. Reuse lifecycle section 13's fixture
procedure; section 14 records the recheck and production gates.

- [x] Reverify all seven listeners and existing mapped executable/database evidence.
- [x] Reverify candidate hash and all five complete embedded assets; no rebuild needed.
- [x] Confirm independent bind/connect EPERM normally and outside the sandbox.
- [x] Verify child-process/temp-file support and native Safari availability.
- [x] Rerun only the live-WAL snapshot/restore regression: 1 passed, 0 failed.
- [x] Preserve previous dirty product bytes and primary dashboard main/WAL/SHM.
- [ ] Operator completes the existing real-browser matrix on an isolated native root.
- [ ] Owner selects the intended service and verifies private startup configuration.
- [ ] Operator validates a production snapshot/restore and rollback compatibility.
- [ ] Explicitly authorized cutover and actual authenticated runtime acceptance.

No new product source, dependencies, commits, publication, token access or process
termination. Earlier broad test counts remain historical evidence.

### Runtime restriction and hook investigation — 2026-10-25

Status: BLOCKED. Single Codex investigator; same branch/base and preserved dirty
tree. Scope: read-only environment/hook diagnostics, isolated recovery regression
and existing documentation only. See lifecycle section 15 for fresh evidence,
per-criterion status and exact native operator HTTP prerequisite.

- [x] Reverify all seven listener mappings and both primary database users.
- [x] Reverify candidate checksum/embedding and baseline preservation.
- [x] Reproduce independent socket EPERM, including one outside-sandbox attempt.
- [x] Observe effective Seatbelt network denial and clean-environment EPERM.
- [x] Inspect hook registrations; active SQLite guard allows the socket command.
- [x] Rerun isolated live-WAL recovery: 1 passed, 0 failed.
- [ ] Identify the exact inherited profile/launcher responsible for restriction.
- [ ] Native operator runs independent HTTP/Safari prerequisite; Terminal UI denied.
- [ ] Execute real-browser matrix; all browser criteria remain BLOCKED.
- [ ] Resolve section 14 production ownership/configuration/backup/rollback gates.
- [ ] Obtain explicit cutover authorization and verify production after deployment.

No hooks or security controls disabled; no application or production changes.

### Restart preparation — 2026-10-25

Owner requested a reversible local-network permission helper and restart handoff.
See lifecycle section 15's final checkpoint for exact commands and scope.

- [x] Confirm project permission profile and missing explicit network/proxy options.
- [x] Prepare operator helper at /private/tmp/mct-enable-local-network.py.
- [x] Dry-run current TOML transformation and preservation checks: PASS.
- [ ] Operator applies helper, retains backup/rollback command and restarts tab.
- [ ] Reverify effective permissions; one independent HTTP probe must pass.
- [ ] Complete isolated Safari matrix and separate production deployment gates.

The helper has not been applied. No live permissions, hooks, product source,
production services or databases were changed; status remains BLOCKED.

## Phase 3 — Backup automation follow-up

The subsequent owner request authorizes local Phase3 implementation and isolated
tests, superseding planning-only scope for that storage slice. Existing UI work
and production gates remain separate. [SDD](../docs/01-architecture/storage-reliability-spec.md)
and [ADR-020](../docs/01-architecture/adrs/020-user-state-backup.md) are authoritative.

| Milestone | Scope/files | Dependency | Owner/check |
| --- | --- | --- | --- |
| A | Existing audit/spec/ADR020 and lifecycle | Existing backup + OS coordination | Single implementation owner, source verification |
| B | web/backup_manager.rs, main.rs, web.rs | A | Explicit OS-scheduler tick, state, contention/restart subprocess tests |
| C | backup.rs, backup_manager.rs/tests | B | Verify-before-publish, protected retention/quota, storage/corruption/kill tests |
| D | CLI status/isolated restore, COMMANDS/lifecycle | C | Table equivalence, no-clobber, recovery procedure |
| E | Workspace/eval and fixture evidence | B–D | Official checks; native OS/production scheduler acceptance stays operator-owned |

Phase 4 indexing work is now implemented locally: idle periodic hash
reconciliation, bounded event coalescing, transaction-level generation
publication, source-mutation detection and MCP read snapshots. Current-session
validation and remaining platform/performance evidence are recorded in
`docs/01-architecture/storage-reliability-verification.md`. The next phase is
operational acceptance: compatible executable inventory, native Linux/Windows
and filesystem validation, authorized production backup/restore rehearsal, and
larger reproducible performance fixtures. No production database replacement
is automatic.


## Phase 5 — Detection and review stability

Owner: single Codex implementation agent, same preserved dirty checkout.
Design: storage reliability SDD Phase 5 and ADR-019 D16. Scope: language-aware
reference evidence, coherent dashboard/CLI analysis, review lifecycle regressions.
No schema/identity migration or new detection engine. Existing review, copy-label,
backup and generation mechanisms remain authoritative.

- [x] Verify actual analyzer, fingerprints, persistence and suppression contract.
- [x] Reproduce unrelated cross-language homonym hiding dead candidates.
- [x] Respect explicit target languages and pin composite analysis snapshots.
- [x] Add unchanged full/incremental, block-change, rename/reappearance and API
  comment/copy-note regression fixtures using isolated databases.
- [x] Complete workspace checks (1,933 passed/0 failed/26 ignored), strict
  Clippy/fmt/eval, Node36, labeled duplicate evaluation and paired query timings.
  See storage-reliability-verification.md for measurements and accuracy limits.
- [ ] Native platform/browser and production acceptance remain operator-owned.
- [ ] Broader labeled dead-code precision/recall and static-analysis coverage.


## Phase 6 — Persistent review state and explicit applicability

Single implementation owner; SDD Phase6 and ADR019 D17. Schema4 is retained.
Existing copy/finding statuses, identity rules, history limits and backup engine
remain authoritative. Task-specific changes do not include Phase7 redesign.

- [x] Audit saved/effective decisions, copy labels, APIs and SQLite/reindex rules.
- [x] Add coherent detail snapshots, saved-state diagnostics and source checks.
- [x] Add six regressions: copy transition matrix, finding transition matrix,
  read/revision coherence, rollback injection, detail lifecycle and cross-consumer
  unchanged reindex visibility. Reuse Phase5 full/incremental/identity fixtures.
- [x] Complete official checks:1939passed/0failed/26ignored, strict Clippy/fmt,
  Node36, MCP33x31 and store32call measurements. Results/limits live in
  storage-reliability-verification.md, separate from historical phase totals.
- [ ] Native browser and Linux/Windows acceptance; authorized production rollout.
- [ ] Decide a compatible revision contract for comment edit/delete (currently
  last-writer-wins on the same comment, though writes advance finding revision).
- [ ] Broader storage/resource and release performance measurements.


## Phase 7 — Dashboard concurrency and native acceptance

Single owner, existing schema/store. SDD Phase7 and ADR019 D18.

- [x] Require aggregate revision and finding association on comment edit/delete.
- [x] Preserve transactional mutation,409 handling, old data and independent copy labels.
- [x] Add shared comment editor, retained drafts, explicit rebase and unavailable state.
- [x] Explain saved/effective decisions and group-comment versus copy-note scope.
- [x] Add independent-connection rollback, concurrent append, real HTTP and Node regressions.
- [x] Reproduce native stale rendered false-positive applicability after unchanged reindex;
  separate source read verification from generation write freshness and schedule SSR refresh.
- [x] Full workspace1944pass/0fail/26ignored; Node42; native correction confirmed;
  initial measurement-test lint corrected and strict lint/fmt/focused check pass.
  Exact sequencing and incomplete acceptance are in the verification record.
- [ ] Official full asset regeneration once the npm registry permits pinned dependencies.
- [ ] Full visual/accessibility/security/large-fixture latency acceptance and Linux/Windows.
- [ ] Authorized operational rollout and compatible legacy process inventory.

Phase6 comment CAS follow-up is implemented here; earlier pending status records
its historical state. No commit, service replacement or production migration.


## Phase 8 — MCP retrieval consistency and token optimization

Audit, contract, and local result wording slice on the preserved dirty checkout.
Specification and tool gap: [MCP Protocol Spec — Phase 8](../docs/01-architecture/mcp-protocol-spec.md#phase-8--mcp-retrieval-consistency-and-review-aware-context)
and [proposed ADR-022](../docs/01-architecture/adrs/022-mcp-duplicate-findings.md).

- [x] Verify current 22-tool default catalog, generation snapshots, cache rules,
  review filtering, pagination limits, and token budget behavior through MCP.
- [x] Establish current `mct-eval` and `token-report.sh` baselines before edits.
- [x] Distinguish no dead-code candidates from candidates suppressed by valid
  decisions; count suppressions before pagination, without changing tool schema.
- [ ] Add a review-aware duplicate query only after a tracked issue authorizes
  the published MCP tool contract; no issue was opened or published here.
- [ ] Add end-to-end duplicate retrieval quality/recall and whole-task token
  measurements before deciding whether the catalog cost is justified.
- [ ] Cross-platform, production, and multi-request pagination acceptance.

No schema or dependency change, production index mutation, commit or publication.

## Phase 8.1 — Review-aware MCP duplicate retrieval (#233)

Owner approved the contract on 2026-10-26; issue #233 exists. Earlier Phase8
issue-blocker status is historical. Single implementation owner; prior dirty
checkout retained. No schema/dependency or detector-threshold changes.

- [x] Create issue and extend existing SDD/ADR before registering the tool.
- [x] Reuse detector, token sources, stable identities and shared review states.
- [x] Add validated read-only tool, TTC/category/batch registration and compact
  generation-aware output with pre-pagination suppression and bounded budgets.
- [x] Isolated MCP11 tests plus real protocol/catalog tests; dashboard SQLite /
  copy-label / CLI parity; unchanged reindex and changed-fingerprint coverage.
- [x] Add five Rust/Python evaluation cases and reproducible1/10/50group benchmark.
- [x] Final official workspace:1959passed/0failed/27ignored; fmt, strict all-feature Clippy, eval and diff PASS.
- [ ] Linux/Windows execution and authorized operational acceptance.
- [ ] Broad labeled detector quality/recall, resource/cancellation profiling and
  complete agent-task token measurement including expanded-context calls.

The query does not cache; snapshots end with each call. Offset pages report a
new generation and may change between calls. Whole analysis holds the existing
server index mutex, so large scans can delay other tools; resource and token
budgets bound output, not repository scan cost. Scope by path/language when
possible. Production deployment/replacement remains explicitly unauthorized.


## Phase 9 — QA and hardening (2026-10-26)

- [x] Preserve and identify dirty candidate; extend existing SDD/release gates.
- [x] Run initial whole-workspace check, Node, pinned1.96compile/fmt, real stdio
  lifecycle profile at16/128files and isolated native-fixture backup/restore.
- [x] Reproduce Alpine CSP warning initialization defect; add failing regression,
  initialize property and pass43Node tests; rebuild embedded CLI.
- [x] Final combined check:1959passed/0failed/27ignored; fmt/strictClippy/eval PASS.
- [ ] Native correction acceptance on the new embedded CLI.
- [ ] Resolve unexplained intermittent native watcher failure without weakening tests.
- [ ] Fresh dependency audit and official asset regeneration (network blocked).
- [ ] Native Linux/Windows, complete keyboard/screen-reader/network acceptance.
- [ ] Full agent token/resource/concurrency and authorized production acceptance.

Authoritative gate matrix, measurements and failure evidence:
storage-reliability-verification.md Phase9. No production deployment authorized.

## Phase 9.1 — Blocker hardening (2026-10-26)

- [x] Preserve dirty candidate and distinguish new from historical evidence.
- [x] Reproduce/fix expired deadline starvation with deterministic regression.
- [x] Preserve bounded subprocess diagnostics and runtime backend identity.
- [x] Reproduce/fix repeated-clock fixture database collisions;20generation runs pass.
- [x] Native owner FSEvents10rounds+64fileburst pass; agent60s fallback passes.
- [x] Corrected native dashboard persistence, selected keyboard/375px/tablet checks.
- [ ] Close historical QA-002/QA-005 incidents through verified disposition.
- [x] Restore official registry build and verify generated/embedded bytes
  (owner-native build twice byte-identical; patched binary embeds and serves them).
- [x] Fresh RustSec audit; Tokio 1.53.3 patch validated by the isolated full gate
  1962/0/27; yanked yoke-derive moved to 0.8.4 (lockfile-only). paste remains
  an unmaintained compile-time warning for owner risk acceptance.
- [x] Native owner FSEvents acceptance repeated on the patched normal binary.
- [ ] Native Linux/Windows against the actual candidate.
- [ ] Screen-reader/contrast/whole keyboard and HTTP inspection acceptance.
- [ ] Sustained concurrency, physicalIO and whole agent-token measurements.
- [ ] Complete release gate evidence before Phase10 operator rollout.

See storage-reliability-verification.md Phase9.1 for exact outcomes/limitations.
No deployment, service stop, production DB replacement or publication authorized.
