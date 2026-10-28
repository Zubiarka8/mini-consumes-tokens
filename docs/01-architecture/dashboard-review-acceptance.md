# Dashboard review decisions: acceptance record

Status: **accepted for the local dashboard, within the limits listed under
[What it does not do](#what-it-does-not-do) and
[Not covered](#not-covered)** (2026-10-28). This records what was run and what
it showed. It makes no claim about releases, deployment or other platforms.

It covers the duplicate-code and dead-code review pages of `mct-cli web` and the
decision store behind them, described in
[ADR-019](adrs/019-shared-review-decisions.md). It replaces the earlier acceptance
notes written for a SQLite decision store; that store does not exist on
`origin/main`.

## Design under test

| Item | Behaviour |
| --- | --- |
| Store | `.mct-index/review-decisions.toon`, format version 3, with `reviews[]{kind,identity,fingerprint,state,updated,note}` and `history[]{kind,identity,fingerprint,from,to,at,note}` tables. |
| Side files | `review-decisions.toon.prev` (backup of the previous version) and `review-decisions.lock` (writer lock). |
| Code | `mct_index::reviews` (format, identity, resolve, store); the dashboard adapter is `web/reviews.rs`. |
| Matching | A decision applies only when both the finding **identity** and the exact code **fingerprint** match. |
| Changed code | A decision whose fingerprint no longer matches shows as `needs-reevaluation` and keeps `previousState`. |
| Still detected | A `resolved` finding that is still detected shows as `resolved-but-detected`. |
| States | `unreviewed`, `needs-reevaluation`, `in-review`, `confirmed`, `resolved-but-detected`, `false-positive`, `resolved`, `dismissed`. |
| Default list | Hides `false-positive`, `resolved` and `dismissed`. Omitting `state` in the API returns every state. |
| Writes | `POST api/review` with a CSRF token, an `Origin` check, an optimistic `revision` (409 when stale) and notes of at most 500 characters. |
| Snapshot | The server reads the index once at start. After a reindex it must be restarted (`Restart after reindexing`). |

## Build under test

- Worktree on `origin/main` plus ADR-021 (draft PR #234), commit `10ee064`.
- Debug binary `target/debug/mct-cli` built from that worktree.
- Automated suite on that commit: `cargo fmt --check` OK, `cargo clippy` clean,
  1,803 passed, 0 failed, 30 ignored; no merge conflicts.

## Evidence

All scripts live outside the repository, in the session scratchpad. They start
their own server, drive the HTTP API with the real CSRF token and `Origin`
header, and stop the server afterwards. Safari 27 was used for the
browser checks. Chrome was not used.

| Run | Scope | Result |
| --- | --- | --- |
| `main_matrix.py` | Persistence, re-review, restore, concurrency, filters on a 4+4 finding fixture and 60 extra dead functions | 57 passed, 2 failed |
| `main_toon.py` and `main_t6.py` | Corrupt, truncated, deleted and unwritable store | 9 passed, 1 failed in the combined run; the failed check passes when run alone |
| `main_big.py` | 332 Rust files (about 101,000 lines), 277 findings | 7 passed, 0 failed |
| Safari, small project | Inspector and Revisiones flows with real clicks | See [Browser checks](#browser-checks) |
| Safari, large project | Page render, pagination, one real decision | See [Large project](#large-project) |

### The three recorded failures are expectation errors

- Two matrix checks expected `resolved`. A `resolved` decision on code that is
  still detected is shown as `resolved-but-detected`, as designed.
- One store check posted `confirmed` on a finding that was already `confirmed`.
  That is a no-op (200, nothing written). Repeated with a different state on a
  read-only directory it returns 500 with `No changes were applied`, and the
  file keeps the same hash and inode.

No product defect was reproduced in any run.

## What it does

### Persistence

- Decisions are written to the TOON file with history and notes.
- They survive an incremental reindex, `reindex --force` and a server restart,
  for dead code and for duplicate groups, for the five states a user can set
  (`in-review`, `confirmed`, `false-positive`, `resolved`, `dismissed`).
  `needs-reevaluation` and `resolved-but-detected` are derived, not stored.
- Notes are kept. A reopened finding returns to `unreviewed` and stays so across
  reindexing.

### Re-review

- Editing a decided function gives `needs-reevaluation` with `previousState`.
  It counts as actionable and is listed by the default filter.
- Reverting the edit restores the original decision (the fingerprint matches
  again).
- Renaming a duplicate member does not carry the old decision to the new group.
  Changing one member so the group disappears hides it; reverting brings the
  decision back.
- A deleted function loses its finding. Its decision stays in the file and
  returns if the code returns.

### Writes and safety

- A stale `revision` gets 409. With nine simultaneous writers on one finding
  exactly one is accepted and the rest get 409; none gets 5xx.
- Six parallel decisions on distinct findings are all accepted with no
  duplicate rows. Decisions posted while `reindex` runs produce no 5xx.
- A bad CSRF token, a foreign `Origin`, malformed JSON, an unknown key and an
  invalid state are all rejected with 4xx.

### Damaged or missing store

| Store state | Server | Reads | Writes |
| --- | --- | --- | --- |
| Garbage or unsupported format | Starts | Everything shown as unreviewed | 503, file left untouched |
| Truncated mid-table | Starts | Everything shown as unreviewed | Refused, same as above |
| Deleted | Starts clean | No decisions | Accepted; the file is recreated |
| Unwritable directory | Starts | Decisions shown | 500 with `No changes were applied`, file untouched |
| Good file put back | Starts | All decisions return | Accepted |

### Filters, search and lists

- State facets add up to the unfiltered total. Pagination pages are disjoint
  and the last page is partial.
- Search matches symbol and path, ignores case, treats `%` literally and
  returns 0 hits (status 200) for an injection-style string.
- Every sort key and direction the script tried returns a full page.
- In the browser, checkboxes combine as OR inside a group and AND across
  groups. Facet options with no matches disappear. `Limpiar` restores the full
  list. The four predefined saved filters return the expected sets (0, 61, 3
  and 62 on the fixture). The page search sets `q=` in the URL.

### Browser checks

Real clicks in Safari on the small project:

- Inspector `Ignorar` changes the state, adds a history entry and updates the
  counters at once.
- Changing a decision from the selector asks for confirmation. Cancel changes
  nothing; confirm applies it. The transitions appear in the TOON file and the
  decision is still there after a page reload.
- `Volver a «Sin revisar» (reabrir)` asks for confirmation and works.
- `Guardar filtros actuales` opens a dialog (stored in the browser, reason
  required). It was cancelled, not completed.

### Large project

A copy of this repository's crates: 332 Rust files, 13,204 symbols, 114
dead-code candidates and 163 duplicate groups.

- Index: `init` 5.2 s; a no-change `reindex` 0.2 s. Dashboard start 2.6 s at
  about 57 MB resident.
- API latency: 1 to 10 ms per list or detail call (median of five).
- 200 sequential decisions all succeed (26 ms each on average); 60 parallel
  decisions all succeed. The store grows to 93 KB and memory stays near 60 MB.
- All 200 false-positive decisions are visible, then survive a server restart
  and `reindex --force` plus restart.
- Browser: duplicates render in 31 ms, dead code in 27 ms and Revisiones in
  54 ms after navigation. Pagination reaches the last page (a single row on
  page 4 of 76). One real `Ignorar` click was written to the file.

## What it does not do

- **Not live.** The dashboard does not follow the index. Reindexing needs a
  server restart; `api/meta` reports `staleFiles`. The dashboard never runs the
  indexer.
- **Exact-code fingerprint.** A parameter rename or a comment-only edit also
  gives `needs-reevaluation`.
- **Orphans.** Decisions for deleted code stay in the file.
- **Default filter after clearing.** `Limpiar todos los filtros` sets
  `state=all`; it does not return to the default that hides false-positive,
  resolved and dismissed.
- **Row actions.** Rows offer only Comparar, Ver and Ignorar. Other decisions
  are in the inspector. A row-level `Ignorar` applies without a confirmation
  dialog; changing a decision in the inspector asks for one.
- **Unknown state values.** The API answers 200 with an empty list for a state
  value it does not know, such as `state=all`, instead of a 4xx.
- **No data that does not exist.** No similarity percentage or confidence for
  duplicates, no severity, no reviewer or assignee.
- **No source editor.** `main` has none.
- **MCP.** `find_dead_code` and `find_duplicate_code` ignore review decisions.
- **Saved filters** are stored in the browser (local storage), not in the
  project.

## Not covered

- Two browser tabs open at once.
- English UI, dark theme and the mobile layout.
- Completing and re-applying a user-saved filter.
- Projects larger than the 101,000-line copy above, and non-Rust projects for
  the decision flow.
- Other operating systems and browsers (only macOS and Safari 27).
- A real crash during a write (power loss). The `.prev` backup and the writer
  lock exist but a kill during `rename` was not exercised.

## Reproduce

Run from the worktree root, using absolute paths and a disposable project:

```sh
cargo build -p mct-cli
target/debug/mct-cli --root <project> init
target/debug/mct-cli --root <project> web --no-obsidian --port <port>
```

The dashboard prints `Local project web:` with a one-time URL; the token in it
changes on every start. After `reindex`, stop and start the server again.
