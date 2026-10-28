# Storage reliability audit

Date: 2026-10-25. Base: `b61cc7ce971fdc46a61259af95af7fac823ae352`,
branch `feature/dead-code-editor`, including pre-existing uncommitted dashboard
work. This is a source-level audit, not a production certification. Source
evidence came from the connected project MCP; file locations may move as the
uncommitted implementation evolves. No source changes preceded this inventory.

Related: [SDD](storage-reliability-spec.md),
[documentation inventory](storage-documentation-inventory.md),
[dashboard lifecycle](web-database-lifecycle.md),
[checkpoint](../../project-management/checkpoints/storage-reliability.md).

## Verified architecture

| Component | Evidence | Verified responsibility |
| --- | --- | --- |
| `mct-core` / `mct-languages` / `mct-lang-*` | workspace manifest and architecture contract | Language-neutral records and extension registry; grammar-specific pure parsers remain in plugins |
| `mct-index/src/lib.rs` | `Index::open`, `open_existing_read_only` | Canonical root, WAL/FK setup, numbered migrations on writable open; read-only open refuses pending/incompatible migrations |
| `mct-index/src/indexer.rs` | `reindex`, `index_file`, `write_parsed_file`, `reindex_paths` | Git blob hashes skip unchanged parsing; path-scoped updates and full reconciliation; file/symbol/relation/literal changes commit together per parsed file |
| `mct-index/src/schema.rs` | migration entry point and schema regression names | Shared language-neutral migrations; parser-related migrations invalidate hashes; compatibility relies on migration version |
| `mct-index/src/queries.rs` | incremental API contract and relation queries | Relations are resolved at query time, avoiding persisted target IDs becoming stale after replacement |
| `mct-index/src/lib.rs` | `navigation_snapshot` | Navigation metadata uses a read transaction; exclusions filter results |
| `mct-mcp-server/src/main.rs` | `main` | Writable index open and initial reindex before MCP handshake; optional watcher startup; watcher startup failure logs and continues |
| `mct-mcp-server/src/background.rs` | `spawn_watcher`, `BatchState` | Native notify/debouncer; coalescing, deduplication, large-batch/rescan/error full-walk fallback; failed work retries with backoff |
| `mct-mcp-server/src/server.rs` | `MctServer`, reindex and tool definitions | In-process `Arc<tokio::sync::Mutex<Index>>`; bounded search/snippets/batch/context packs; this mutex does not coordinate different processes |
| `mct-mcp-server/src/cache.rs` | `RepoTracker::refresh`, `QueryCache::check` | Changes marker plus file hashes; validates result fingerprints and reranks after repository changes; bounded in-process cache |
| `mct-cli/src/web/store.rs` | `Store::open`, `migrate`, schema and test names | Independent dashboard database; WAL, NORMAL synchronization, foreign keys, explicit 5-second busy timeout, four migrations, Unix 0600 |
| Dashboard identity and writes | store revision methods, ADR-019, lifecycle document | Revision checks and mutation in `BEGIN IMMEDIATE`; content fingerprints, retained annotations and conservative association; legacy import remains separate from index rebuild |
| AI-facing review reads | `mct-index/src/review_state.rs`, ADR-019 D12 | Read-only review interpretation shared by CLI/MCP; does not imply MCP owns annotation writes |

The dependency manifests specify rusqlite 0.40.2 with bundled SQLite,
rusqlite_migration 2.6.0, notify 8.2.0, notify-debouncer-full 0.7.0,
tree-sitter 0.25 and Tokio 1.53.1. The optional semantic build uses fastembed
7.1.0. The locked libsqlite3-sys 0.38.2 bundled header reports SQLite 3.53.2;
already running binaries were not inspected. This audit did not establish a
complete dependency vulnerability status.

### Index lifecycle and failure behavior

Full reindex walks without following directory symlinks, filters ignored paths,
checks canonical containment and hashes bytes before parsing. It still reads
unchanged files to compute their hashes. Manifests are reparsed, not hash-skipped.
Read failures retain previous data with issues instead of interpreting an unreadable
subtree as deleted. Syntax failure removes the old file graph and records its
issue in one transaction. A later valid version is parsed again.

Incremental paths handle creations, edits, removals, directory subtrees and
case-only renames. Ignore changes and root events can trigger a full walk.
The existing test `a_moved_and_edited_file_reported_by_its_new_path_only_waits_for_a_full_reindex`
explicitly captures an incomplete-event case requiring later reconciliation.

**Atomicity is per file, not per reindex run.** Other processes can observe a
partially advanced collection of individually valid files. An in-process lock
does not establish cross-process query-generation consistency. Navigation uses
a read transaction, but that does not prove every multi-query MCP tool pins one
snapshot. A process crash loses the current transaction, while earlier file
commits remain. No durable run-completion generation was verified.

The watcher blocks on `rx.recv()` when idle and no failed rescan is pending.
Therefore a silently lost event is not repaired by a periodic timer in this
implementation. Startup reindex/manual full reconciliation remain fallbacks.
The event channel is unbounded, despite the coalesced path-set cap. A continuous
stream can extend the quiet deadline. Native backend behavior requires platform
acceptance; test-only macOS kqueue evidence is not production FSEvents evidence.

### Persistent user data

`dashboard.sqlite3` owns `reviews`, `review_history`, `comments`, `file_reviews`
and `meta`. Current copy labels use identity/path/member key, not index row IDs.
Schema 4 retains legacy path labels. Revisions survive delete/recreate; source
verification and stale-analysis checks protect saves. Migration failure is
reported after bounded retries. Future schemas are refused by the migration
library rather than downgraded.

Decision history is bounded to 50,000 rows. Copy notes/status changes and edited
or deleted comments are not complete append-only histories. Browser drafts are
not crash-durable. Reindex cannot recreate any of these user records. A saved
backup supplies a snapshot, not arbitrary point-in-time recovery.

## Confirmed risks and evidence limits

| ID | Severity | Finding / impact | Required response |
| --- | --- | --- | --- |
| SR-01 | High | No shipped automated, verified backup/production cutover was present at audit start; persistent annotations cannot be rebuilt | Add consistent snapshots, validation, isolated restoration and explicit operator cutover |
| SR-02 | High | Dashboard explicitly uses `synchronous=NORMAL`; committed WAL transactions may be lost after power failure even though process-crash atomicity is retained | Evaluate FULL for user saves; measure latency and document durability contract |
| SR-03 | High | Initial scan and updates occur in every MCP writer process; no project-scoped ownership gate in the inspected open/reindex paths | OS-backed ownership gate shared by CLI/MCP; fail old/noncooperating binaries at operational boundary |
| SR-04 | High | Per-file commits and no verified completed generation permit mixed repository states across processes | Explicit generation/run state and read-snapshot contract; interrupted-run reconciliation |
| SR-05 | Medium | No idle periodic reconciliation; incomplete rename events can leave old rows | Timer-driven content reconciliation independent of events |
| SR-06 | Medium | Unbounded event channel and quiet-period postponement | Bound transport, overflow-to-dirty flag, maximum batch age |
| SR-07 | Medium | Writable index open migrates automatically; CLI `status` also reaches writable open | Preflight compatible readers and backup-before-user-migration; clarify read-only CLI operations |
| SR-08 | Medium | Watcher debug logs include full events; startup logs include root and update logs include removed symbols | Redact paths/symbols by default in operational diagnostics |
| SR-09 | Medium | Existing lifecycle evidence records a dashboard `/reload` 403 and incomplete reload acceptance | Separate UI/security investigation; do not relax CSRF based on this audit |
| SR-10 | Unverified | Active process count, executable versions, current production ownership, corruption and disk state | `ps` denied even after outside-sandbox request; obtain operator evidence before cutover |

No live corruption or malicious overwrite was observed. Multi-process risk is
an architectural finding, not a claim that current user data is already damaged.
Existing symlink/hard-link guards are useful but path checks followed by open
have a race against a hostile local directory writer. Private, owner-controlled
directories remain a precondition; network/shared filesystems are excluded.

## Test inventory at audit start

Existing index suites cover incremental hashes, edits, additions/deletions,
renames, exclusions, permissions/read failures, relations, read-only opens,
change markers and schema hash invalidation. MCP suites cover rescan/large
batches, retry state, timing, query-cache invalidation, budgets and quality.
Dashboard suites cover persistence, concurrent writers, revisions, migration
preservation, legacy imports, link guards and reindex-with-reviews.

Named tests were inventoried, **not yet executed by this audit**. Historical
228-Rust/33-Node and earlier workspace results in the lifecycle/project records
are prior evidence only. No complete subprocess kill-at-each-stage, disk-full,
failed production migration or verified production restoration campaign was
established. See the checkpoint for new executions after the audit.

## Runtime observation

MCP returned 1,036 indexed files and 18,081 symbols, timestamp 1792929587.
Eighteen parse diagnostics include sixteen deliberate malformed fixtures,
`scripts/windows/review-check.ps1: line 29: syntax error` and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (existing ERR-020).
Queries succeeded; no reindex or primary-database inspection was initiated.
Process enumeration was denied. Dirty checkout state, stale project board entries
and historical production/browser observations must not be confused with current
deployment verification.


## Writer-coordination audit — 2026-10-25

Reverified base/branch and preserved dirty work match the earlier inventory.
Current MCP observation:1,045 files/18,183 symbols, timestamp1792931650; same18
known diagnostics. Process enumeration `ps -axo pid,comm` again returned
`operation not permitted`, including elevated execution. Actual loaded versions,
root identities and ownership remain unknown; no service was stopped.

| Database / entry | Actual operation before this slice | Coordination boundary added |
| --- | --- | --- |
| Index::open | Writable/create, WAL/FK configuration, migrations | Shared connection access, initializer writer gate, exclusive migration barrier |
| CLI init/reindex | Opens writable and runs full index; force bypasses hashes | Whole index-run writer gate |
| CLI call | Writable open/initial index then MCP tool execution | Same central Index boundaries |
| CLI status/dead-code/obsidian | Writable open even when subsequent query/report is read-only | Open still guarded; query methods take no writer lock; command classification retained |
| MCP startup | Writable open and initial scan in every process | Same open/run gates |
| MCP manual reindex/watcher | Full/path scans; ignore edits can cause scan during query preparation | reindex, reindex_paths, apply_exclusion_changes gate before first persistent mutation |
| Semantic maintenance | refresh_embeddings, commit_embedding_batch, clear_embeddings | Independent guarded mutation; snapshot/embed-query is read-only |
| Symbol-change tracking | Connection-local TEMP trigger state | No persistent writer gate needed |
| Dashboard Store::open | Create/private permissions, WAL/settings, migrate, one-time TOON import | Shared lifetime barrier, exclusive migration access, separate initializer/import writer gate |
| Dashboard saves | set/set_checked, delete_decision variants, add/update/delete_comment, set_file_review variants, set_member_review | Existing SQLite IMMEDIATE/revision transactions; shared connection access |
| Dashboard maintenance | migrate_fingerprints, remember_dead_bindings, bounded history/metadata updates and imports | Same Store access barrier and native SQL transactions; initialization/import has writer gate |
| Dashboard analysis/duplicates CLI | Index::open_existing_read_only | Shared connection access, no exclusive writer gate; refuses pending/future index schema |
| MCP/CLI effective review reads | ReviewSnapshot::load | Shared access for snapshot; future dashboard schemas refused |
| dashboard-backup | Read-only pinned Online Backup source, private destination | Shared source barrier only; no writer exclusion or migration |
| dashboard-backup-verify | Private disposable restored candidate | No production gate required |
| Production restore | No command shipped | Still operator-only/offline; exclusive maintenance guard is prerequisite for cooperating cutover, not a restore implementation |

Confirmed gaps retained: nonparticipating legacy binaries/direct SQLite tools can
bypass advisory ownership; external file replacement/unlinking lock files is
unsupported; private trusted local directories and a controlled rollout are
required. Per-file transactions are unchanged and still do not create a whole
repository generation. Index contention now fails before mutation, permitting
watcher retry and keeping process-local MCP mutexes from sleeping on another
writer. Multiple compatible dashboard stores remain permitted; SQLite owns save
serialization. Network filesystems, legacy rollout and native Windows behavior
have no new acceptance evidence here.

## Phase 3 verified starting inventory (2026-10-25)

Git branch/HEAD remain feature/dead-code-editor/b61cc7ce971fdc46a61259af95af7fac823ae352,
with earlier dirty work preserved. Source inspection verified the existing
backup.rs engine: bounded stepped Online Backup, pinned read snapshot, schema4
preflight, SHA-256, eight-field TOON manifest, private directories, integrity/FK
checks and disposable restore. Seventeen existing backup tests were present.
DatabaseAccess implements shared lifetime access and exclusive writer/maintenance
OS guards; backups use shared source access. This is cooperative, not protection
against legacy tools or hostile same-owner path replacement.

CLI dispatches backup commands before opening the derived index. Web is a
synchronous local server and MCP owns its separate watcher/runtime; neither
provides a reusable persistent backup scheduler. Configuration currently uses
explicit Clap flags (for example backup destination and reindex delay). Selected
integration: a one-shot explicit maintenance command suitable for existing OS
schedulers, no new worker in web/MCP and no dependency/schema change. Shutdown
cannot strand a daemon/task; forced process termination leaves recoverable output.

Documentation update ownership: SDD defines requirements/acceptance; ADR-020 owns
backup decisions, ADR-021 remains source writer/generation scope; this audit owns
verified boundaries; verification.md owns new results; COMMANDS.md is the operator
reference; web-database-lifecycle.md and tasks/plan.md link these changes without
copying policy. Existing documentation inventory remains the audience/update map.
CI runs workspace checks on Linux/macOS/Windows, but only the current native host
provides execution evidence in this session. No production schedule installed,
service stopped, database migrated, restored or inspected through external tools.


## Phase 5 verified detection/review inventory (2026-10-25)

Starting branch/HEAD remained feature/dead-code-editor at
b61cc7ce971fdc46a61259af95af7fac823ae352, with earlier uncommitted work.
The connected MCP still reports the older status format; current source and
isolated tests, not that executable, establish the new behavior.

| Step | Current code and responsibility | Verified behavior / limit |
| --- | --- | --- |
| Source → index | Registry language parsers, `indexer.rs` | Hash skipping, verified bytes, Phase 4 atomic generation; no user-data writes |
| Index → dead candidates | `dead_code.rs`, `queries.rs` | Allowlisted kinds, entry/test exclusions, zero references by target language; heuristic, no reachability proof |
| Index/source → duplicate groups | `duplicates/analysis.rs`, `analysis/fingerprint.rs`, `analysis/file.rs` | Verified source, token normalization, deterministic sorted members/ranking, thresholds and skipped diagnostics; semantic equivalence is not proven |
| Finding → identity | `review_keys.rs`, `dead_identity.rs` | Versioned digests independent of row IDs/line shifts, conservative same-name recovery; ambiguous content is unverified |
| Finding → content fingerprint | `review_keys::Fingerprints` | One bounded verified read per file; exact complete block text, including whitespace/comments; stale source cannot suppress |
| Save → persistent user data | `web/store.rs`, `web/api.rs` | Independent dashboard DB, revisions/409, comments/history, source verification; copy statuses independent from group decisions |
| Saved → effective decision | `review_state.rs`, `web/store.rs` | Current identity/content required; only false_positive/ignored suppress normal AI/CLI results; history alone never suppresses |
| Effective → outputs | MCP server, `main::dead_code_report`, CLI `duplicates::run`, `web::Site::load` | Same shared identity/suppression contract; CLI raw flag and dashboard All retain underlying findings; there is no MCP duplicates tool |

Confirmed starting defects: relation names from an unrelated language could hide
an unused same-name candidate (Rust/Python regression reproduced before the
fix); dashboard/CLI composite reads could query different generations after an
external commit. Fixes partition names by explicit parser target language or
source language and pin analysis snapshots. Existing explicit cross-language
evidence is retained; no new parser language cases or database schema changes.
The same-language name heuristic still has false negatives from unrelated
homonyms/self recursion and false positives from external/dynamic usage.

The existing normalizations are exact/consistent/broad; broad is the CLI default.
Comments and formatting can normalize away for detection while still invalidating
an exact reviewed-text fingerprint. This is an intentional conservative contract,
not a detector regression. Duplicate copy reassociation includes parent/content
provenance and refuses ambiguous automatic note transfer. Increasing/decreasing
membership changes the group identity; old decisions remain inspectable orphans.


## Phase 6 verified review lifecycle follow-up (2026-10-25)

Same dirty branch/base as Phase5. Source → detector → review_keys stable identity
and exact verified range fingerprint → explicit Store decision/copy mutation →
independent dashboard.sqlite3 → reindex only index.sqlite3 → effective State /
read-only ReviewSnapshot → MCP/CLI suppression and dashboard filters. Schema4,
five copy labels and four finding statuses confirmed in actual Rust code.

Confirmed gaps: finding detail composed state/revision/history/comments/copy
revisions with separate SQLite reads; a concurrent writer could mix versions.
Orphan detail returned pending without a separate current saved row. Dashboard
state reads trusted cached fingerprints even if a file became unreadable before
index refresh. Phase6 adds a coherent read snapshot, additive diagnostics and
verification of applicable decisions. Existing403/409, identity/reassociation,
bounded history, transactions, coordination and backup engines are reused.
No production mutation, schema change or browser acceptance was performed.


### Phase7 verified follow-up

The confirmed same-comment last-writer-wins gap is closed for participating
current binaries: edit/delete requires owning finding aggregate revision and
association, checked inside the write transaction. The SPA now exposes those
existing annotations with separately scoped retained drafts. No schema change.
Native Safari identified a separate stale rendered-source applicability defect
after unchanged reindex; a same-server regression reproduces it and the read/
write freshness separation corrects it locally. Final native correction evidence
and complete checks are maintained in storage-reliability-verification.md.
Legacy binaries ignoring the protocol, limited audit retention, full asset
regeneration and operational/platform acceptance remain outside proven safety.
