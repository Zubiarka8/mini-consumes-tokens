# ADR-019: Dashboard state in a separate SQLite database, JSON API and embedded SPA

## Status

Accepted (2026-10-24). Milestones M1 to M6 (data layer, store wired into the pages, read API with background refresh, JSON write API, embedded SPA, documentation) are implemented on `feature/web-dashboard`. Supersedes the persistence part of ADR-018 (D4: `reviews.toon`). Everything else in ADR-014, ADR-015 and ADR-018 stays in force unless a section below says otherwise.

## 1. Context

ADR-018 keeps human review decisions in `.mct-index/reviews.toon`, written with a lock file, a temporary file and an atomic rename. That works for two tables and one writer at a time, but it does not scale to the features the dashboard now needs (comments, per-finding history, live refresh, editing from a richer UI): every write rewrites the whole file, there is no query language, and concurrent writers fail instead of waiting.

Verified facts about the current code that drive this decision:

| Fact | Where |
|---|---|
| `index.sqlite3` runs in WAL mode; only the indexer writes it (the MCP background watcher or `mct-cli reindex`). | `Index::open`, `crates/mct-index/src/lib.rs` |
| The dashboard opens it read-only (`SQLITE_OPEN_READ_ONLY`), never migrates it and never changes its journal mode. | `Index::open_existing_read_only` |
| The index is derived data: schema migrations, rebuilds and `ON DELETE CASCADE` can remove or renumber rows at any reindex. | `crates/mct-index/src/schema.rs`, `rules.md` |
| `Index::change_marker` already exposes `PRAGMA data_version`, a near-free "did another connection commit?" probe. | `crates/mct-index/src/lib.rs` |
| The repository forbids touching `index.sqlite3` with anything but this project's own code (no `sqlite3` CLI). | `rules.md`, enforcement hook |
| SQLite allows one writer at a time; in WAL mode readers never block the writer and the writer never blocks readers. | SQLite documentation |

## 2. Decisions

### D1. Two databases with strictly different roles

- `.mct-index/index.sqlite3` remains **derived, read-only for the dashboard**. The dashboard only reads it through the public `mct-index` APIs. No table is added to it and no migration is made (`rules.md` requires an issue first for any index-schema change; none is needed).
- `.mct-index/dashboard.sqlite3` is **user state**: review decisions, their history and comments. It is owned by `mct-cli` (`crates/mct-cli/src/web/store.rs`), has its own migrations (`rusqlite_migration`, version in `PRAGMA user_version`) and is never rebuilt from source.

Writing user state into the index database was rejected: it would put the web into the indexer's single-writer queue, break the read-only open that protects the index, and lose data whenever the index is rebuilt.

### D2. No foreign keys to the index

User rows are keyed by the stable ADR-018 pair **(kind, identity)** plus the **fingerprint** at decision time. They never store index row ids, so reindexing, schema rebuilds and `rm index.sqlite3` cannot orphan or corrupt them. Cross-database queries are not needed: the server joins in memory (identity is computed from the index snapshot).

### D3. Concurrency and latency without damaging either database

`dashboard.sqlite3` is opened with `journal_mode=WAL`, `synchronous=NORMAL`, `foreign_keys=ON` and `busy_timeout=5000`. Every write is one short `BEGIN IMMEDIATE` transaction (takes the write lock up front, so a reader-to-writer upgrade can never deadlock), validated before it starts. Several processes (two dashboards, the CLI) can write concurrently; they queue for at most the busy timeout and never see a half-written state. Reads use their own snapshot and never wait.

Live refresh target: **1 to 5 seconds**, by polling, no push channel:

- `GET /api/version` returns a cheap token: the modification time of `index.sqlite3` and `index.sqlite3-wal` (as `index_stamp` already does) plus the dashboard store's `PRAGMA data_version`. It costs two `stat` calls and one pragma.
- The browser polls it every 2 s. When the token moves it refetches only the views on screen.
- If the index stamp moved, the server rebuilds its read-only analysis snapshot on the next data request, at most once every 2 s and never while a rebuild is running. **Measured in M3** (release build, `--no-obsidian`, this repository: 992 files, 192 duplicate groups, 193 dead candidates): a full rebuild took 3.2 s on a quiet machine and 5.2 to 8.7 s while the indexer and a compiler were running. That exceeds the 5 s target, so the rebuild runs on a worker thread (`Site::refresh_if_stale`): the server keeps answering from the previous snapshot, `api/version` reports `stale` and `rebuilding`, and a later data read swaps the new snapshot in. A manual reload (`POST reload`) stays synchronous. The index stamp is taken right after the read-only index is opened, because opening can create the `-wal` file; a commit after that only makes the snapshot look stale, never fresher. The version token is `current_index_stamp.loaded_index_stamp.snapshot_at.data_version.writes`, where `writes` counts this process's own commits (`PRAGMA data_version` only moves for other connections). Not measured: a rebuild with the Obsidian vault enabled.

### D4. Migration from `reviews.toon`

On first open, if `reviews.toon` exists and the meta key `reviews_toon_imported` is absent, its current decisions and history are copied in one transaction and the key is set. The import is idempotent (`INSERT OR IGNORE` inside `BEGIN IMMEDIATE`, so two processes cannot import twice). A corrupt file aborts the import and the store does not open (fail closed). `reviews.toon` and `duplicate-reviews.toon` are **never modified or deleted**; the latter is still read as legacy (ADR-018 D4), mapped 1:1.

### D5. Data model (store version 1)

- `reviews(kind, identity, fingerprint, status, at, reason)`, primary key `(kind, identity)`: the current decision.
- `review_history(id, kind, identity, fingerprint, status, at, reason)`: append-only, bounded to 50,000 rows (oldest dropped and reported, as in ADR-018). Restoring writes a `pending` row, as before.
- `comments(id, kind, identity, body, created_at, updated_at)`: free text, multi-line, at most 2,000 bytes each and 500 per finding. Create, read, update and delete are all supported.
- `meta(key, value)`: import flags.
- `CHECK` constraints repeat the Rust validation (kind, status, 64-hex keys, length bounds) so a bug cannot persist an invalid row.

Timestamps are Unix seconds written by the server, never taken from a request.

### D6. HTTP API and security

A JSON API under the existing secret path (`/<token>/api/...`): reads (`version`, `stats`, `duplicates`, `dead`, `obsidian`, `notes`, `reviews`) and writes (`review`, `comments`). JSON is used because it is the browser's native `fetch` format, an external protocol requirement under `rules.md`; the agent-facing exports stay TOON. All ADR-014 and ADR-018 guarantees apply unchanged to the API: loopback only, capability path, Host/Origin/Fetch-Metadata checks, CSRF token on every write (header, not query string), bounded bodies (the 4 KiB form limit is raised to 8 KiB for JSON writes only), one request per connection, no path ever taken from a request. Responses carry `Cache-Control: no-store`.

### D7. Embedded SPA

The interface prototype (`prototype/codeinsight/index.html`, Tailwind + Alpine + Lucide) is a mock-up that loads its libraries from a CDN. The shipped interface cannot do that: ADR-018 D1 sets a CSP of `script-src 'self'; style-src 'self'` with no external origin. Therefore:

- Tailwind is **compiled ahead of time** into one static CSS file (the standalone Tailwind CLI, run by a documented script, output committed), not the in-browser runtime.
- Alpine uses its **CSP build** (no `new Function`, no `unsafe-eval`); Lucide and Alpine are vendored as static files.
- All assets are embedded with `include_str!`/`include_bytes!` and served from fixed routes. No Node toolchain is needed to build `mct-cli`.
- The server-rendered pages of ADR-018 stay available during the migration; they are removed only when the SPA covers every view (decision at M6).

### D8. Per-copy working labels (store versions 2 and 4)

A duplicate group can contain several copies in one file. Each copy has its own
working status and note, independent of the group decision. Neither a label save
nor its informational code-change warning changes the group verdict or moves it
to `needs_reevaluation`.

- Migration 4 rebuilds `file_reviews` with primary key `(identity, path,
  member_key)` and adds `member_base` and nullable `code_hash`. All version-2/3
  rows, statuses, notes and timestamps survive with empty member keys and null
  hashes. These legacy path labels apply to each copy until a concrete override
  is saved; an explicit pending/empty override masks the inherited label. A
  pending/empty concrete row without inheritance is deleted. Retained legacy
  and unmatched copy rows remain available through `retained_files`.
- A server-generated opaque key hashes the member's path/name/kind/indexed
  parent plus an ordinal for same-base siblings. Line numbers locate indexed
  parents but never enter persistent keys. Unique verified content matches
  preserve sibling keys through moves; a single-member family retains its key
  after a body edit. Indistinguishable siblings retain visible slots with an
  uncertainty warning. Unmatched ambiguous labels remain retained instead of
  silently attaching to another body. No labels transfer between group identities.
- The server computes `code_hash` from the complete indexed range after verifying
  current source against the indexed file. The browser cannot supply a hash.
  A differing saved hash produces `code_changed`; unavailable or ambiguous
  association produces `code_unavailable`. Legacy null hashes never claim a
  code change. Restoring the original body clears the change warning.
- `GET api/duplicates` provides each member's `member_key`. `GET api/finding`
  provides member descriptors (including revisions for unlabeled/cleared copies)
  and effective `files` rows with key, status, note, revision, inheritance and
  warning flags. `POST api/file-review` accepts the key with path and
  `expected_revision`, verifies membership (404), and refuses unverifiable code
  (409). The legacy path-only write is supported only for a unique path and
  cannot overwrite concrete copy labels (409).
- Copy revisions are checked and advanced inside `BEGIN IMMEDIATE`, including
  delete/recreate, independently for each copy; inherited copies initially use
  their legacy path revision. The unchanged 20,000-row bound includes retained
  and override rows and is checked atomically. Older binaries refuse schema 4
  rather than downgrading it. Parent lookup is built once per snapshot; effective
  labels and revisions are fetched once per group and joined in memory.
- Status remains one of `pending`, `accepted`, `rejected`, `in_review`, `ignored`;
  notes retain the 2,000-byte limit and SQL constraints. The SPA uses copy keys
  for labels, drafts and saves, and shows “Código cambiado, revisar” (English:
  “Code changed, review needed”) in Files, the right panel, all-copy status and the AI
  prompt. Finding history and existing group identity/orphan behavior are unchanged.

### D9. Editor ergonomics (M8)

The editor tab keeps comments and history in the API. D12 restores the actual
finding-decision controls alongside the separate file working labels. Files close with ✕ and reopen from the list;
panes have a minimum width and the side panels are collapsible. A right panel
generates an AI prompt (locations with line ranges plus the duplicate/dead
task) with a copy button. Alpine CSP note: an `x-for` variable must never be
named like a component method (`t` shadowed `t()` and broke the close handler).

### D10. Content fingerprints and conservative migration (store version 3)

The stable ADR-018 identities stay unchanged. The index has no per-symbol hash
(`SymbolRecord` and `SymbolListEntry` carry locations, not source hashes).
On snapshot load the dashboard reads each candidate/member file once with the
source-preview path guards and 2 MiB bound, requires complete UTF-8, and verifies
its Git blob hash against the navigation snapshot before applying indexed ranges.
The file hash is a freshness check only; it is absent from the new fingerprint.

- Dead code: `dead-fp-v2`, hashing the complete symbol line range as text.
- Duplicates: `duplicate-fp-v2`, normalization, language, and the sorted members'
  path/name/kind and hashes of their complete block text. Neither member line
  numbers nor whole-file hashes participate. Member ordering does not matter.
- Text is exact, including whitespace, comments and line endings. Moving a range
  or editing outside it preserves a decision; changing its text requires review.
- Missing/invalid ranges, unknown end lines, unreadable/oversized/non-UTF-8 files,
  mismatched on-disk revisions, and ambiguous duplicate identities cannot produce
  a usable fingerprint. Existing decisions require reevaluation; saving an
  unverifiable decision is refused. No truncated source is ever fingerprinted.

Migration 3 adds `reviews.fingerprint_version`, defaulting existing/imported rows
to 1. New explicit saves write version 2. At snapshot load a short transaction
upgrades only version-1 rows whose **identity and old fingerprint both match** the
current indexed finding, and whose new content fingerprint is available. The
original status, timestamp, reason and history remain intact. Subsequent loads
cannot silently upgrade that row again. Rows whose old fingerprint cannot be
verified require reevaluation until an explicit save replaces them.

Accepting a legacy row on identity alone was rejected: it would silently bless
body changes made before upgrading, violating the fail-closed rule. If a file
was already moved/edited before migration, the old format cannot prove its old
symbol text; one reevaluation is therefore unavoidable. ADR-015 fingerprints
remain byte-identical for legacy lookup, with an in-memory alias from verified
new fingerprints; legacy files are never rewritten.

### D11. Orphan decisions and private database permissions

Both the SPA's **Orphans** view and the server-rendered **No current finding**
view list stored decisions whose identities are absent from the current snapshot.
Reindexing/reloading never deletes them. An explicit Delete decision button posts
to `api/review/delete` (JSON) or `review/delete` (form), through the existing
Origin/Fetch-Metadata/CSRF gates. Only absent identities can be deleted; active
findings and unknown decisions are rejected. A stale index snapshot blocks deletion
with HTTP 409 until analysis is refreshed. Deletion removes the current review
row only; history, comments and per-file annotations remain.

On Unix, `dashboard.sqlite3` is created with mode `0600` before SQLite opens it.
Every reopen enforces `0600` again before migrations, imports or WAL setup. A
permission failure aborts opening. Existing symlink/hard-link guards stay in force;
platforms without Unix mode bits retain their native permission behavior.

### D12. Effective decisions shared with AI-facing reports

`mct-index::review_keys` owns the language-neutral identities, legacy keys and
verified content fingerprints used by both the dashboard and reports.
`mct-index::review_state::ReviewSnapshot` reads existing dashboard/legacy state
without creating a database, migrating it, or writing either database. MCP
`find_dead_code`, CLI `dead-code` and CLI `duplicates` resolve effective decisions
before output pagination. Only unchanged `false_positive` and `ignored` findings
are suppressed. Confirmed, pending, changed, ambiguous or unverifiable findings
remain actionable. Read errors are explicit errors, never proof of a dismissal.
The published MCP input schema is unchanged. CLI `--include-reviewed` provides
raw diagnostic output. Rendered duplicate caches are bypassed while saved review
state exists, preventing an old cached report from undoing a later dismissal.
Active and retained Pending records have separate 100,000-row reader bounds;
exceeding either reports the relevant category instead of truncating decisions.

Copied AI prompts include the effective finding state, its previous decision
when reevaluation is needed, reason, content fingerprint and task scope. Per-file
working labels are clearly separate from the actual finding decision, which is
again editable in the SPA.

### D13. Stale editor conflicts and retained drafts

Finding writes, including server-rendered forms, carry `fingerprint` and
`expected_revision`; file working-label writes carry `expected_revision`.
Missing version fields are rejected. A changed fingerprint, stale analysis, a
source read that no longer matches the index, or an intervening saved revision
returns HTTP 409 without replacing the decision. Revisions are checked atomically
inside the same `BEGIN IMMEDIATE` transaction as the mutation, and increase
monotonically across delete/recreate. Comments and file-label changes also advance
the finding revision so an old orphan-delete request cannot archive newer notes.
Every dashboard write (decisions, file labels, orphan deletions and comments) is
refused with 409 while the served snapshot is older than the index on disk; the
check runs once for every JSON write route.

Per-file duplicate labels are working notes. `api/finding` `files[]` entries and
`api/file-review` responses carry `scope: "file_label"`, and the latter also
returns the unchanged `finding_state`; a group is dismissed only by a finding
decision. `api/file-review` accepts an optional `fingerprint` (the group the
editor saw): a mismatch or unverifiable source returns 409. The fingerprint is
checked but not stored, and file labels still have no history. After its own
label save the SPA adopts the returned `finding_revision` for a pending decision
draft only when it is exactly one step ahead, so a foreign write still conflicts.

Version and deletion metadata use the existing `meta` table, requiring no index
or dashboard schema change. Keys include `review_revision:<kind>:<identity>`,
`file_revision:<identity>:<path>`, `review_deleted:<kind>:<identity>` and
`review_archived:<kind>:<identity>`. Explicit deletion persistently suppresses
legacy fallback while retaining history/annotations. New annotations can make
retained state discoverable again without reinstating an old legacy verdict.
Restored Pending rows do not consume the active-decision quota; active and
restored rows are each bounded at 100,000, matching the AI-facing reader, and
the bound is checked only when a row moves into a category.

The SPA retains file and finding drafts by identity/path. Saving a captured draft
clears only that draft generation; typing during its request survives. Navigation
and polling retain drafts and their original revision/fingerprint. A conflict
requires an explicit review of the latest version before rebasing the draft.
Closing a tab with drafts warns rather than silently discarding them. Drafts are
in-memory for that tab, not a cross-device or crash-recovery store.

### D14. Bounded pages and honest source previews

Finding and orphan lists use server-side `q`/`status` filtering and bounded
`offset`/`limit` pages (SPA page size 50; API maximum 200). Responses include counts
and a `snapshot` token that pins analysis plus user-state generation. A stale
requested token returns 409; the client restarts the page instead of appending
another generation and tells the user the results restarted. The browser loads
the visible page rather than preloading all findings after every version change.
Full-source chunk reads (`duplicates?id=...&full=1`) are pinned only to the
analysis generation (`<index stamp>.<generated>`): a review write cannot change
indexed source, so it does not invalidate a chunk being read. Copying a partial
chunk says that only the visible chunk was copied.

`api/duplicates?id=...&full=1` returns a bounded source chunk, not a promise of
unbounded source: `source_offset` is relative to each member, `source_limit` is
at most 400 lines, and each member reports `excerpt_first`, `excerpt_available`,
`excerpt_truncated`, `source_total` and `source_next_offset`. The SPA provides
previous/next source controls. Exhausted shorter members return an empty available
chunk; unreadable/stale content is explicitly unavailable. Source chunks are read
from the same verified indexed bytes, never a new revision under old line ranges.

Orphan enumeration includes restored decisions and annotation/history/file-only
identities. Their retained details can be inspected on demand; explicit archive
hides them without deleting their history. Archived identities do not immediately
reappear in the orphan list.

### D15. Same-name dead-symbol provenance

Reviewed dead symbols retain `dead_base:<identity>` and
`dead_fingerprint:<identity>` metadata. A base is language/path/kind/parent/name.
After a same-name sibling moves or disappears, only a unique current-to-saved
base/content match may retain a valid decision. A unique single-symbol body edit
keeps identity and triggers fingerprint reevaluation. Reserved saved identities
cannot be stolen by newly inserted siblings. Identical/ambiguous siblings are
unverifiable and cannot inherit a valid dismissal.

Pre-provenance decisions use bounded ordinal recovery (256 ordinals per base,
65,536 total recovery lookups); outside that bound they remain conservative new
or reevaluation work rather than receiving an unproven dismissal. Bindings are
written only when their fingerprint still matches the saved decision.
Identities are resolved only by `mct_index::dead_identity::resolve_dead_identities`,
fed by the same `ReviewSnapshot` inputs for the dashboard and for MCP/CLI:
identity bindings plus every retained dead identity (decision rows, else the
newest history fingerprint, else annotation-only identities, minus archived or
deleted-and-not-reopened ones). Each same-name family is resolved in source
order, so a pre-provenance decision is recovered identically before any
dashboard load, and ambiguous matches stay uncertain everywhere.

Adding a member to a duplicate group still creates a different group identity.
A verdict on A+B must not automatically dismiss the new A+C/B+C relationships.
This remains an intentional conservative limitation rather than a claim that
all group-membership changes preserve a verdict.

### D16. Phase 5 coherent analysis and review stability

One SQLite read snapshot covers each dashboard analysis build: navigation,
status, duplicate candidates and dead candidates use the same committed index
generation. Release it before fingerprint upgrades, provenance writes and note
loading. CLI dead/duplicate reports pin their analysis and suppression inputs
similarly; MCP already pins each operation. The analysis APIs also pin a read
when no caller transaction exists. WAL permits independent indexing commits;
separate requests/pages still follow their documented snapshot-token rules.

Regression acceptance explicitly repeats forced full, unchanged full and
path-scoped reindexing after real dashboard decisions. Valid false positives
stay suppressed in CLI/MCP and retain dashboard decisions/history/comments.
Whitespace/comments inside reviewed blocks require reevaluation; movement and
unrelated text do not. Renames/membership changes cannot inherit old decisions.
Restored original content can reactivate a still-current stored decision; this
is not an arbitrary historical undo mechanism. Copy rejection and its notes stay
independent of group suppression. No migration, new identity namespace or new
suppression store is introduced. See the Phase 5 SDD and verification record.

## 3. Consequences

Positive: transactional writes with no whole-file rewrite, safe concurrent writers, queryable history and comments, no change to the index or its read-only guarantee, a path to 1 to 5 s live updates, and the reviews survive any reindex.

Negative: a new dependency pair in `mct-cli` (`rusqlite` with the bundled engine and `rusqlite_migration`, both already built for `mct-index`), a second database file to document, and an ADR-018 test group that must be rewritten (the ones that assert on `reviews.toon` bytes, locks and symlinks). Polling costs a request every 2 s per open tab.

## 4. Milestones

- **M1 Data layer** (done): `store.rs` with migrations, the `reviews.toon` import, review CRUD, comment CRUD, `change_marker`, tests for persistence, concurrent writers, import, bounds and path guards. Not yet used by the pages.
- **M2** (done): `Site` uses the store; the dependent ADR-018 tests were rewritten.
- **M3** (done): read API (`api/version`, `summary`, `duplicates`, `dead`, `reviews`, `finding`), version token, background refresh, rebuild measured. `obsidian` and `notes` reads are not in the API yet; they come with the SPA views in M5.
- **M4** (done): JSON writes `POST api/review`, `api/comment`, `api/comment/update`, `api/comment/delete`. Same gate as the forms (exact `Origin`, no cross-site `Sec-Fetch-Site`, host check) plus `Content-Type: application/json` (forces a CORS preflight for other sites), the CSRF token in the `X-CSRF-Token` header, a body of at most 8 KiB (413 above), a closed set of keys per route, and 404 for unknown findings or comments. Form routes are unchanged and still refuse JSON. Tests cover each rejection and that rejected requests change nothing.
- **M5** (done): precompiled assets (`scripts/unix/build-dashboard-assets.sh` builds Tailwind 3.4.17, the Alpine 3.14.8 CSP build and an inline SVG sprite from lucide-static 0.468.0 into `crates/mct-cli/src/web/spa/`) and the SPA served at `<token>/app` with duplicates, dead code, Obsidian (stats, note list, note detail, canvas graph), review with reason and history, and comment create/edit/delete. Read API gained `notes`, `note` and `graph`. Verified in the built-in browser against this repository (192 duplicate groups, 231 dead candidates, 1,440 notes): no console errors, compare panes, review, comments, restore, graph and language/theme toggles. The Alpine CSP build cannot evaluate calls with arguments or `!`, so the templates only read properties and every derived string or flag is precomputed in `spa.js`.
- **M6** (done): README, `COMMANDS.md` and the changelog describe the dashboard, the API and the two databases. **Decision: the ADR-018 server-rendered pages are kept**, not removed. They are the only interface that works without JavaScript, the SPA has not been compared page by page with them, and ADR-018's tests exercise them. Removing them needs its own ADR once the SPA covers every view. The website (`website/`) is intentionally not updated or pushed here.
- **M7** (done): per-file review state and notes (D8), the side-by-side N-file compare view and the file list in the SPA.

- **M8** (done): a home page (`Inicio`, "¡Hola mundo!" plus the summary cards and links) and a "code editor" tab for duplicates. Clicking a duplicate in the list calls `window.open('app?view=editor&id=<identity>', '_blank', 'noopener')`; the `app` route ignores the query string, so the same embedded shell serves it and `spa.js` reads `view` and `id` on start. The editor tab has no list: a toolbar (previous/next duplicate, `Todo`/`Coincidentes`/`Distintas` filters, search in code, wrap, font size, copy, close), closable file tabs with a letter tag, base name and parent directories (so the many `lib.rs` can be told apart; the active tab and pane get a strong brand border), and a per-file review panel. Closing a tab removes its pane and the remaining panes share the width; the last open tab cannot be closed and the file list reopens any closed file. The font size is a CSS variable set through the CSSOM, because the CSP (`style-src 'self'`) blocks `style=` attributes.

## 5. Not verified

The WebKit end-to-end run for D10/D11 is blocked in this execution environment: an elevated direct server launch returns `cannot bind the localhost web port: Operation not permitted`, and WebKit itself exits with `Abort trap: 6` when launched through an in-memory HTTP bridge. Browser attempts were stopped after the user reported the failures. Focused in-memory HTTP tests exercise the lifecycle; browser verification remains pending.

Snapshot rebuild time on large repositories and with the Obsidian vault enabled (D3), the SPA at wide viewports and in browsers other than the built-in one (the layout was checked narrow only), and the default `busy_timeout` interplay with a long `mct-cli reindex` (the dashboard database is separate, so a reindex cannot hold its lock, but this has not been exercised end to end).


### D17 — Coherent saved and effective finding details (Phase 6)

Keep schema4 and existing State/ReviewSnapshot authority. Add saved_decision,
effective_decision, review_validity, source_verified and actionable to existing
finding detail responses. Preserve existing status fields for clients. Orphans
retain saved rows without pretending they have a currently applicable verdict.
Reverify currently applicable dashboard decisions against source; cached analysis
alone cannot prove unchanged source before watcher reconciliation.

Use a deferred SQLite WAL read snapshot per detail response rather than a writer
lock or additional persistence system. Pin before the first state read and retain
through revisions, history, comments and copy data. Other writers remain available;
old revision saves conflict. Avoid reconstructing current decisions from historical
rows. Costs: one short read transaction and bounded source verification for
applicable decisions; exact timings and platform gaps belong to the verification
record. Extensive frontend changes and unbounded audit retention are out of scope.


### D18 — Compare-and-swap comment mutations and explicit annotation scope (Phase 7)

Use the existing owning finding revision rather than timestamps or a new
comment-version column. An IMMEDIATE transaction verifies kind/identity/id,
compares the supplied revision, changes the comment and advances the aggregate
revision once. Missing or mismatched association:404; stale token:409; legacy
requests without owner/token:400. Creation is an atomic bounded append. This
intentionally conflicts on unrelated finding annotation changes; per-comment
versions would reduce conflicts but require migration and more public state.
No unsafe edit/delete fallback is exposed to old clients. Legacy executables
that ignore these checks remain an explicit deployment limitation.

Keep comment drafts in tab memory, separately keyed from copy notes and finding
decisions. Polling never rebases a draft; explicit latest-version acknowledgement
can rebase only if the comment still exists. Deleted comments cannot be recreated
by an old edit. Typing during a save survives. Comment creation has no idempotency
key: after a lost response, inspect the list before manually retrying. Never retry
append automatically. No persistent browser-restart drafts or complete immutable
comment audit history is introduced. UI names group comments separately from
individual-copy notes; both use the existing persistent database.

Read applicability must not treat an unrelated index commit as proof that source
changed. Read verification checks identity/content against indexed source hashes;
write verification additionally requires the current index stamp. Reuse the
existing throttled asynchronous refresh for rendered analysis reads, never
rebuild synchronously in the request thread. Stale-generation observability and
conservative rejection of changed/unreadable source remain intact.
