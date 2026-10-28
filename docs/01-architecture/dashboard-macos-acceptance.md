# macOS dashboard persistence acceptance

Status: **NATIVE BROWSER ACCEPTANCE PASSED in Firefox 158 and Safari 27,
within the build and evidence limits recorded below; coordinator review
pending** (2026-10-27). See
[Native acceptance results](#native-acceptance-results-2026-10-27).
No integration, deployment or full application acceptance is claimed.

## Starting boundary

The examined checkout was `feature/dead-code-editor` at
`b61cc7ce971fdc46a61259af95af7fac823ae352`, with 74 modified tracked files and
34 untracked status entries. All 1,246 files in the supplied inventory matched
their hashes before editing; none was missing. The isolated ten-file Batch 1
patch hash was `adbea661b65a82dff11f1a55449949dd7aa2890097173af0b9decc183cd096c5`.
The inventory manifest hash was
`a73011f3feb8a1d573d68998a89f389caa7bdda17b7d451a5f0b0f742f15c603`.
These are verified local observations, distinct from prior review reports.

### Preservation boundary

1. **Original Batch 1 approval.** The approved Batch 1 change is the ten-file
   patch with the hash above. Two of its files are untracked new files:
   `crates/mct-mcp-server/tests/source_revision.rs` and
   `docs/01-architecture/rust-functional-coverage.md`.
2. **Unchanged approved implementation.** Eight Batch 1 files are
   byte-identical to a reconstruction of the pre-Batch-1 tree plus that patch:
   `crates/mct-cli/src/web/tests_decisions.rs`,
   `crates/mct-index/src/indexer.rs`, `crates/mct-index/src/lib.rs`,
   `crates/mct-index/tests/incremental.rs`,
   `crates/mct-mcp-server/src/server.rs`,
   `crates/mct-mcp-server/tests/query_cache.rs`,
   `crates/mct-mcp-server/tests/source_revision.rs` and
   `docs/01-architecture/rust-functional-coverage.md`.
3. **Later error-register updates.** `errors/REGISTRY.md` and
   `errors/HISTORY.md` are also Batch 1 files, but they no longer match that
   reconstruction. The only difference is a later 2026-10-27 ERR-022
   recurrence record: one sentence added to the open ERR-022 row and one new
   history section. It records an MCP startup problem and is separate from the
   approved functional fixes and from this dashboard work. The ten Batch 1
   files as a whole are therefore **not** byte-identical to the original
   approval.
4. **Dashboard task changes.** This task changed four prior files:
   `crates/mct-cli/src/web/spa/spa.js`, `crates/mct-cli/src/web/tests.rs`,
   `crates/mct-cli/tests/dashboard_spa.cjs` and
   `docs/01-architecture/web-database-lifecycle.md`. It added
   `crates/mct-cli/src/web/tests_dashboard_lifecycle.rs` and this document.
5. **Scope and limits of the comparisons.** A later check compared the
   working tree with a 1,244-entry pre-Batch-1 hash list kept outside the
   checkout. No listed file is missing. Exactly 12 listed files differ: the
   six tracked Batch 1 code and test files, the two error registers and the
   four dashboard files above. Every other listed file is unchanged, including
   unrelated uncommitted work. The two untracked Batch 1 files are not in that
   list and were compared only with the patch reconstruction. The 1,246-entry
   manifest cited above was not available to that later check, so it was not
   repeated in full. Byte equality shows that content is preserved. It is not
   evidence of behaviour, which the validation sections cover.

Branch and HEAD remain unchanged. Work is uncommitted; nothing was pushed or
integrated.

## Confirmed gap and correction

**DASH-001 (minor): delayed pending-queue reconciliation after a decision save.**
`spa.js::saveReview` saved through the API and refreshed finding detail, but did
not reload the filtered list. A dismissed item could remain in Pending until
the next two-second poll; the visible count could also remain stale. This was
a presentation delay, not loss of the SQLite decision.

The successful-save path now reloads the bounded filtered page before refreshing
detail. Pending suppression, restoration and filtered counts therefore update
without waiting for polling. The selected dismissed finding remains inspectable
in its editor. Existing draft generation, optimistic revision and error handling
remain in force. This adds one bounded list request for the selected finding's
successful save; it adds no detector, storage service or per-file source scan.

Six Node regressions cover dead/duplicate × false_positive/ignored/restore.
With the new refresh omitted in memory, all six fail on the missing queue request;
with the shipped correction, all six pass. The baseline experiment changes no
repository file. The existing Fetch helpers are reused; HTTP response checking
and asynchronous response handling follow the
[Fetch API reference](https://developer.mozilla.org/en-US/docs/Web/API/Fetch_API/Using_Fetch#handling_the_response).
The component uses the existing Alpine CSP 3.14.8 build.

A new Rust test, `web::tests::dashboard_lifecycle::
json_decisions_remain_reviewable_through_reopen_repeated_reindex_and_restore`,
drives the actual HTTP parser and JSON handlers with disposable SQLite stores.
For each of the four dismissals it checks pending exclusion, dismissed listing,
detail visibility, four alternating incremental/forced reindexes, unchanged
decision records/history, an unrelated pending symbol, restoration and reopening.
It submits the exact fingerprint and revision exposed by detail, rather than
letting the harness fill those fields. Reopening Site/Store is not a real server
process restart or browser reload.

## Existing persistence and re-review contract

No schema, dependency, identity, fingerprint or API change was needed. No new
backend persistence defect was reproduced in these checks. The existing store
keeps user decisions in `dashboard.sqlite3`, separately from derived analysis in
`index.sqlite3`. Reindexing does not recreate the user store. A current decision
is an upsert; restoration retains history while removing active suppression.

Stable identities exclude line positions and do not match by name alone.
Fingerprints verify complete reviewed source ranges. Moving lines or editing
outside those ranges preserves a valid dismissal. Changed/unverifiable reviewed
source requires re-review; old state remains inspectable. Renames or changed
duplicate membership create new identities; old decisions remain orphaned instead
of being assigned to unrelated findings. Ambiguous identities stay actionable.
Copy labels/notes remain independent of group decisions: a rejected or ignored
copy label does not dismiss the entire duplicate group.

The existing tests freshly rerun here cover line movement, homonyms and identical
twins, membership growth/orphans, changed body and exact-content restoration,
cache invalidation, concurrent review/reindex writes, publication rollback and
recovery, and revision-safe source previews. This is bounded regression evidence,
not a proof of all races or compiler-proven dead-code detection. See the
[lifecycle contract](web-database-lifecycle.md) and [Batch 1 scope](rust-functional-coverage.md).

## Fresh automated validation

All commands ran on the identified checkout plus the scoped uncommitted changes.
Run these commands from the checkout root. Logs were retained outside the checkout
under `/private/tmp/mct-dashboard-*.log`. The results below were newly executed
in the first session, before DASH-002 and DASH-003. For later reviews they are
historical.

| Command | Outcome |
| --- | --- |
| `node --test crates/mct-cli/tests/dashboard_spa.cjs crates/mct-cli/tests/dashboard_copy_labels.cjs` | 49 passed, 0 failed/skipped |
| `cargo test --offline --locked -p mct-cli --bin mct-cli` | 234 passed, 0 failed, 2 ignored |
| `cargo test --offline --locked -p mct-cli --test review_decisions --test writer_coordination` | 13 passed, 0 failed, 1 ignored |
| `cargo test --offline --locked -p mct-mcp-server --test source_revision --test query_cache --test context_pack --test pack_budget --test search_budget --test toon_output` | 68 passed, 0 failed/ignored |
| `cargo test --offline --locked -p mct-index --test incremental --test index_generations` | 29 passed, 0 failed/ignored |
| `cargo clippy --locked -p mct-index -p mct-cli -p mct-mcp-server --all-targets -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic` | PASS, no warnings |
| `cargo fmt --all -- --check` | PASS after correcting formatting in the new test |
| `node --check crates/mct-cli/src/web/spa/spa.js` | PASS |
| `cargo build --offline --locked -p mct-cli --bin mct-cli` | PASS, current JavaScript embedded |
| `git diff --check` | PASS |

Totals for the nonoverlapping Rust commands above: **344 passed, 0 failed,
3 ignored**. Earlier focused executions overlap these totals and are not added
again. Ignored tests are `coordination_fixture_benchmark`,
`duplicate_mcp_workflow_benchmark` and `benchmark_language_aware_dead_candidates`;
all are explicitly manual measurements. The 98 previously approved Batch 1
regressions are included: 68 MCP + 29 index + D8 in the CLI suite.

`scripts/unix/build-dashboard-assets.sh` failed in npm before generation, both
normally and elevated. npm's diagnostic records `E403`; stdout only showed the
unknown `http-proxy` environment configuration warning. Generated CSS, Alpine and
sprite bytes remain unchanged. The correction adds no CSS classes or template
bindings; JavaScript is embedded directly in the freshly built CLI. Full asset
regeneration remained unverified in that session; it was verified later the same
day (see [Native acceptance results](#native-acceptance-results-2026-10-27)).

Not executed: full workspace suite, optional-feature matrix, D3's known failing
Rust macro test, cross-platform acceptance, controlled performance benchmarks.
D3 remains unresolved; green dashboard suites do not establish Rust completeness.
No historical full-suite totals are used as fresh evidence.

## Native execution blocker (first session, since resolved)

This section records the first session's blocker. A later native session on the
same host and checkout did not reproduce it; see
[Native acceptance results](#native-acceptance-results-2026-10-27).

The host reports Darwin. Disposable fixture initialization succeeds using the
project CLI. The real web command fails both within the sandbox and elevated,
including after rebuilding the current executable:

```text
Error: cannot bind the localhost web port

Caused by:
    Operation not permitted (os error 1)
```

The command used a disposable fixture root and `web --no-obsidian --port 18787`.
No server started. Native Safari and Firefox were inventoried as available, but
no dashboard browser workflow or screenshot was produced. Chrome was not used.
No existing listener was stopped, production database opened, or installed MCP
binary modified. This environmental failure is not evidence of a persistence
defect, and it prevents declaring this milestone complete.

## Exact remaining browser acceptance procedure

Use a macOS terminal that permits loopback sockets and Safari or Firefox. From
the checkout root, build the candidate and create a NEW disposable project:

```sh
cargo build --offline --locked -p mct-cli --bin mct-cli
fixture="$(mktemp -d /private/tmp/mct-dashboard-native.XXXXXX)"
mkdir -p "$fixture/src"
python3 - "$fixture" <<'PY'
import pathlib, sys
root = pathlib.Path(sys.argv[1])
body = '''(value: i64) -> i64 {
 let mut total = value;
 total += 1;
 total *= 2;
 total -= 4;
 if total > 12 { total -= 3; }
 total += 8;
 total
}
'''
for group, repeats in [('a', 0), ('b', 1), ('c', 2)]:
    variant = body.replace('total += 8;', 'total *= 3;\n' * repeats + 'total += 8;')
    for copy in range(3):
        (root / 'src' / f'{group}{copy}.rs').write_text(f'pub fn {group}{copy}{variant}')
print(root)
PY
target/debug/mct-cli --root "$fixture" init --no-obsidian
target/debug/mct-cli --root "$fixture" web --no-obsidian --port 18787
```

Keep this terminal running. Open its printed private URL; do not publish its
capability token in reports. Copy the fixture path to another terminal and set
`fixture` there to that exact path. Perform the following, saving screenshots of
the pending/dismissed/detail states and redacted browser network responses:

1. Confirm dashboard load, local assets and no console errors. Confirm requests
   target `127.0.0.1:18787`. Open Dead code, select Pending, inspect `a0`'s name,
   path, ranges, source and heuristic warnings.
2. Set Finding decision to False positive and press Save decision. Verify the
   successful `POST api/review` carries displayed identity/fingerprint/revision;
   confirm `api/dead?status=pending` excludes it and the False positive filter
   and `api/finding` retain the saved decision. Reload the browser and recheck.
3. Mark `a1` Ignored; repeat the same checks. Keep `c0` pending as a control.
4. In Duplicates inspect group A's occurrences and source panes; save the GROUP
   decision False positive. Save group B as Ignored. Do not substitute per-copy
   working labels for these decisions. Confirm group C remains pending.
5. In the second terminal, run each command, waiting for snapshot refresh after
   each, then reload the browser and check all four decisions and pending lists:

   ```sh
   target/debug/mct-cli --root "$fixture" reindex --no-obsidian
   target/debug/mct-cli --root "$fixture" reindex --force --no-obsidian
   target/debug/mct-cli --root "$fixture" reindex --no-obsidian
   ```

6. Stop only this fixture server with Ctrl-C. Start the exact same web command
   again, without rotating its token. Reload its private URL; all four decisions
   must still be present and absent from Pending.
7. Restore `a0` and group A to Pending through Save decision. Check their pending
   presence immediately, after reload, reindex and a second server restart.
   Leave `a1` and group B Ignored and check they remain dismissed throughout.
8. Check actual persisted SQLite using the project's backup commands (no external
   index SQL tool). Create a backup in a new private destination, verify the
   completed backup directory printed by the first command, and retain its TOON
   validation output. Confirm API finding history after restart contains only
   deliberate saves/restores, with no entries added by reads or reindexing:

   ```sh
   target/debug/mct-cli --root "$fixture" dashboard-backup
   # Paste the completed backup directory printed above, then press Return.
   read -r backup
   target/debug/mct-cli --root "$fixture" dashboard-backup-verify --backup "$backup"
   ```

Do not mark any unchecked step PASS. Capture browser/version, candidate SHA,
fixture path, exact commands and counts. The automated lifecycle test checks
exact persisted rows and unchanged history; the native procedure must also prove
real process restart, browser reload and rendering. Close the fixture server
when finished; retain essential evidence before any separately authorized cleanup.

## Native acceptance results (2026-10-27)

A second session ran the procedure above natively on the same macOS 27.0 host,
checkout `feature/dead-code-editor` at
`b61cc7ce971fdc46a61259af95af7fac823ae352` with the same uncommitted changes.
The CLI was rebuilt with `cargo build --offline --locked -p mct-cli --bin mct-cli`
(build B1; see [Builds and run chronology](#builds-and-run-chronology)).
Each full-procedure browser run used a NEW disposable fixture created exactly as
above, outside the checkout. No repository source file, index or installed binary was changed.

### Blocker diagnosis

**EPERM was not reproduced.** `web.rs` binds `TcpListener::bind((127.0.0.1, port))`,
which is correct. In the second session a minimal Python loopback bind and the
real `mct-cli web --no-obsidian --port 18787` both succeeded with the same binary;
`lsof` showed no prior listener. The first session's failure (including its
"elevated" attempt) is therefore attributed to that execution environment, most
likely its sandbox. That cause is inferred, not proven: the first environment
could not be inspected. No application change was made.

**npm E403 was not reproduced, and regeneration is not required.** The second
session's npm uses the default `registry.npmjs.org` with no proxy variables, and
all three pinned packages resolve. The first session's warning about an unknown
`http-proxy` configuration suggests a proxy setting in that environment, a likely
but unproven source of the 403. The exact `build-dashboard-assets.sh` pipeline,
run into a scratch directory without touching the checkout, produced
byte-identical outputs:

| Asset | SHA-256 (generated = committed) |
| --- | --- |
| `spa/spa.css` | `077dbb448d7f50e92248c9353b8931ba6a6ca60b3af8499d6e44f11e966e210b` |
| `spa/vendor/alpine.js` | `c98f46291087f0f93c9cdefb682d120fe1d2b3dcb649de83cfc1ad5b1fac58e4` |
| `spa/sprite.svg` | `2a3e9ea8d060812b8ac9a0f3f2dde5be8541afb33b04b28fc6597da919967bd4` |

### Method

Both browsers were driven by scripts with real pointer input on the SPA
(`<token>/app`), not by a human hand and not by calling the API instead of the UI.
Saves went through the visible decision buttons and Save decision. Checks combined
the rendered lists (Pending and the dismissed filter), `api/dead`,
`api/duplicates`, `api/finding` (state, revision, history) and browser network
and console data. Each checkpoint reloaded the browser. Restarts sent SIGINT to
the fixture server and started the identical command; the private URL was
unchanged both times. After each reindex the server re-analysed the new index
generation (`api/version` reported `stale: false`).

- **Firefox 158.0.1**: headful window with a throwaway profile, WebDriver BiDi
  (`--remote-debugging-port`), network and console events from the browser.
- **Safari 27.0** (WebKit 605.1.15): `safaridriver --mcp` (Safari's built-in MCP
  server: Inspector network and console, page interactions, screenshots), after
  remote automation was enabled with `safaridriver --enable`. The classic
  `safaridriver -p` endpoint timed out connecting to a Safari process started
  before automation was enabled; the MCP endpoint was not affected. Safari ran
  with a Spanish UI; checks used locale-independent selectors.

### Builds and run chronology

Times are UTC on 2026-10-27, from the second session's command log and the
timestamps of its scratch files. No binary hash was recorded. Each run is
matched to a build by the order of the source edits, the `cargo build` calls
and the run itself. Matching a run to a binary by hash is **NOT VERIFIED**.
Between B1 and B3 the second session edited only `spa.js` (embedded in the
CLI), the Node test file and this document.

| Build | Built at | Frontend change included | Browser runs on this build |
| --- | --- | --- | --- |
| B1 | 19:23 (`cargo build --offline --locked -p mct-cli --bin mct-cli`) | DASH-001 | Firefox full procedure, 19:28–19:37; Safari CSRF probe, 19:54; Safari full procedure (two main runs and the supplementary network run), 19:56–20:12 |
| B2 | 20:44 (`cargo build --offline --locked -p mct-cli --bin mct-cli`) | DASH-001, DASH-002 (`spa.js` edited 20:43) | Safari DASH-002 script, 20:46 and 20:47; Firefox DASH-002 script, 20:48 |
| B3 (final) | 20:54 (`cargo build -p mct-cli`) | DASH-001, DASH-002, DASH-003 (`spa.js` edited 20:53) | Safari DASH-003 script, four invocations, 20:54–20:57 |

- The [Results](#results) table, including reload, reindex, restart, restore,
  history and backup, comes from build B1 only.
- Firefox never ran build B3. Its last run was the DASH-002 script on B2.
- Repeated on B3: the Safari DASH-003 script, plus the commands listed in
  [the DASH-003 recheck](#dash-003-recheck).
- Not repeated on B3: any Firefox run, the full procedure in either browser,
  the DASH-002 browser checks, clippy, fmt, and the MCP and index suites. Only
  `spa.js` changed after B1, so the persistence behaviour is expected to be
  unchanged. That is an inference, not a browser result.
- A later read-only review on 2026-10-27 reran `node --check`, the Node tests
  (51 passed), `cargo test --offline --locked -p mct-cli --bin mct-cli`
  (234 passed, 2 ignored), `cargo fmt --all -- --check` and `git diff --check`
  on the unchanged final sources. It ran no browser.

### Results

| Procedure step | Firefox 158 | Safari 27 |
| --- | --- | --- |
| 1. Load, local assets, all requests to `127.0.0.1:18787`; a0 name/path/range/source/heuristic warning shown | PASS | PASS |
| 2-4. a0 False positive, a1 Ignored, group A False positive, group B Ignored (group decisions); c0 and C stay pending | PASS | PASS |
| `POST api/review` carries the displayed identity, fingerprint and revision; 200 | PASS | PASS (see note) |
| DASH-001: the filtered list request follows the POST and precedes `api/finding` | PASS | PASS (see note) |
| Browser reload after the saves | PASS | PASS |
| 5. Three reindexes (incremental, forced, incremental) | PASS | PASS |
| 6. Real server restart, same private URL | PASS | PASS |
| 7. Restore a0 and group A: immediately, after reload, forced reindex and a second restart; a1 and B stay Ignored | PASS | PASS |
| History contains only deliberate saves (`false_positive`, then `pending` after restore); none from reads or reindexes | PASS | PASS |
| 8. `dashboard-backup` + `dashboard-backup-verify` | PASS | PASS |
| Console | See DASH-003/004 | See DASH-003/004 |

Note on Safari: the main run passed 100 checks and failed 12. All 12 were the
network checks on the editor tab, which returned no requests because Safari's
Inspector does not record a tab opened by `window.open` until a network query
targets that tab. That is a harness defect: the same run showed the expected
saved state, revision and history after each save. A supplementary Safari run
armed recording on the editor tab before saving (c0 False positive, group C
Ignored, then both restored) and passed 8 of 8. The Inspector order and the
page's own resource timing matched, for example `POST api/review 200`, then
`GET api/dead?…&status=pending 200`, then `GET api/finding?… 200`.

Screenshots, redacted logs (capability token replaced by `<token>`) and driver
scripts were retained outside the checkout in the session scratch directory.

### Minor findings

- **DASH-002 (cosmetic) — FIXED.** The editor header read "#1 of 0", or "#51 of
  52" for a finding that had just left the filtered queue. `findingNumber` in
  `spa.js` clamped a missing index (`findIndex` = -1) to position 1 while the
  total came from the filtered queue. It predates DASH-001, which made it visible
  right after every save. Data was unaffected.
  Correction: `findingNumber` returns 0 when the finding is not on the
  currently loaded page of the filtered list (at most 50 rows). A new
  `queuePosition` helper, shared by the dead-code and duplicate footers, shows
  "#n of total" only for a finding on that page. Here n is the page offset
  plus the finding's place on the page, and total is the filtered total. A
  finding not on the loaded page gets no invented ordinal. The footer then
  shows "(not in this list · n left)" or "(not in this list · list empty)",
  in Spanish "(fuera de esta lista · quedan n)" or "(… · lista vacía)". The
  message means "not on the loaded page". It does not prove that the finding
  is absent from the full filtered result set; see
  [Known DASH-002 limitations](#known-dash-002-limitations).
  Selection, the DASH-001 queue refresh and request handling are unchanged.
  Regressions in `crates/mct-cli/tests/dashboard_spa.cjs`: the six DASH-001
  save tests now also assert the footer (empty queue after a dismissal, "#1 of 1"
  after a restoration), and a new test covers a kept selection at an offset
  ("#52 of 53") and a removed selection in a non-empty queue, in both languages.
  The new assertions failed before the correction ("Dead code #1 of 0",
  "Duplicate #51 of 52") and pass after it.
- **DASH-003 (console noise) — FIXED.** Alpine's CSP build warned
  `"row.marked"` once per dead-code row. The shared row template in the
  uncommitted `index.html` change uses `x-show="row.marked"`, but `rowOf` set
  `marked` only for duplicate rows. Correction: dead-code rows now carry
  `marked: false` (bookmarks remain a duplicate-only feature). Regression in
  `crates/mct-cli/tests/dashboard_spa.cjs`: every `row.*` field the list
  template reads must exist on both duplicate and dead-code rows. It failed
  before the correction (`dead row: ['marked']`) and passes after it. See
  [the DASH-003 recheck](#dash-003-recheck).
- **DASH-004 (console noise) — ACCEPTED EXCEPTION, no change.** Both browsers
  report the automatic `/favicon.ico` request blocked by `img-src 'none'`.
  The block happens in the browser before any request: a test server with the
  dashboard's exact CSP header never received `/favicon.ico`. Declaring
  `<link rel="icon" href="data:,">` only replaces the report in Safari 27
  (`Refused to load data:, because it does not appear in the img-src
  directive`), because every favicon load is an image load. Silencing it would
  require relaxing `img-src`, which this cosmetic diagnostic does not justify.
  This matches the earlier decision in
  [storage-reliability-verification.md](storage-reliability-verification.md).
  The message is harmless and carries no token or request.

### Known DASH-002 limitations

These are known UX limitations, left for a separate optional improvement. No
JavaScript was changed for them.

- **Page-scoped position.** The ordinal comes from the loaded page only. A
  finding that is still in the filtered queue, but on another page, is shown
  as outside the list. Example: page one is loaded (offset 0, 50 rows) and
  there are 51 pending duplicates. The 51st reads "Duplicate (not in this
  list · 51 left)" instead of "Duplicate #51 of 51". One way to reach this is
  to dismiss the only finding on the last page, which makes the list fall back
  to offset 0, and then restore it. A snapshot restart (HTTP 409) also reloads
  from offset 0. Before DASH-002, the same case showed an invented "#1 of 51".
  This was reproduced only in a scratch Node probe of the shipped `spa.js`,
  not in a real browser. The native fixtures had 9 dead-code findings and 3
  duplicate groups, so no browser run crossed the 50-item page boundary.
- **Spanish singular.** With one finding left, the Spanish footer reads
  "quedan 1" instead of "queda 1".
- **Unloaded list.** `pageTotal` starts at 0 and stays 0 when the list request
  fails. A failed or not-yet-loaded list is therefore described as "list
  empty".

### Validation after the DASH-002 correction

Commands were run from any directory with
`--manifest-path=<checkout>/Cargo.toml` (shown here without it):

| Command | Outcome |
| --- | --- |
| `node --test crates/mct-cli/tests/dashboard_spa.cjs crates/mct-cli/tests/dashboard_copy_labels.cjs` | 50 passed, 0 failed (49 previous + 1 new) |
| `node --check crates/mct-cli/src/web/spa/spa.js` | PASS |
| `cargo build --offline --locked -p mct-cli --bin mct-cli` | PASS |
| `cargo test --offline --locked -p mct-cli --bin mct-cli` | 234 passed, 0 failed, 2 ignored |
| `cargo test --offline --locked -p mct-cli --test review_decisions --test writer_coordination` | 13 passed, 0 failed, 1 ignored |
| `cargo test --offline --locked -p mct-mcp-server --test source_revision --test query_cache --test context_pack --test pack_budget --test search_budget --test toon_output` | 68 passed (Batch 1) |
| `cargo test --offline --locked -p mct-index --test incremental --test index_generations` | 29 passed (Batch 1) |
| `cargo clippy --offline --locked -p mct-index -p mct-cli -p mct-mcp-server --all-targets -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic` | PASS, no warnings |
| `cargo fmt --all --check` | PASS |
| `git diff --check` | PASS |

Rust totals are unchanged: **344 passed, 0 failed, 3 ignored**. The
`cargo build` in this table produced build B2.

### DASH-002 browser check (build B2)

Build B2 was checked in both browsers on one new fixture, Safari first, with
the same scripted real input. Each run restored what it dismissed. For a0 and for group A: open from Pending, dismiss as
False positive, then restore in the same editor tab.

| Check | Firefox 158 | Safari 27 |
| --- | --- | --- |
| Before: a0 "#1 of 9", group A "#3 of 3" | PASS | PASS |
| After dismissal: no ordinal; a0 "(not in this list · 8 left)", A "(… · 2 left)" | PASS | PASS |
| After restoration: same ordinal as before | PASS | PASS |
| No console errors in the editor (favicon excepted, DASH-004) | PASS | PASS |

The first Safari invocation reported 6 passed and 2 failed. The script
expected group A at "#1", but group A is "#3 of 3" in the pending list. After
the expectation was changed to "same ordinal as before", the second invocation
passed 8 of 8. Firefox passed 8 of 8 in one invocation. These checks were not
repeated on build B3. The empty-queue state ("list empty") is covered by the
Node regressions only.

### DASH-003 recheck

After the DASH-003 correction (build B3): `node --test` on the same two files gave
51 passed, 0 failed (50 previous + 1 new); `node --check`, `cargo build -p
mct-cli`, `cargo test -p mct-cli --bin mct-cli` (234 passed, 2 ignored),
`review_decisions` + `writer_coordination` (13 passed, 1 ignored) and
`git diff --check` all passed. Only `spa.js` and the Node test changed, so the
other Rust suites, clippy and fmt were not rerun.

Safari 27 was rechecked on build B3 with scripted real input, reusing the
DASH-002 fixture. The dead-code list loaded and filtered (Pending, All) with
no bookmark mark and no console warning. In the duplicate view, bookmarking a
group in its editor tab showed the mark on its list row, and removing the
bookmark cleared it, with clean consoles in both tabs.

There were four invocations:
1. The first stopped on a script error after four passing checks.
2. The second passed 8 checks and failed 1. Safari logged `Fetch API cannot
   load …/api/version due to access control checks` on the dead-code list,
   after the script had forced a page reload. Its cause was not established.
   A polling request interrupted by that reload is a possible explanation,
   but nothing tested it.
3. The third and fourth passed 9 of 9 each, and the message did not recur.

Firefox was not run on build B3.

### CSRF rejection

One CSRF probe ran in Safari 27 on build B1 at 19:54 UTC. It ran during
harness setup, before the full Safari procedure, against a disposable probe
fixture. From the dashboard page, Safari's `evaluate_javascript` (through
`safaridriver --mcp`) sent a same-origin `fetch('api/review')` POST with
`Content-Type: application/json`, the body `{}` and no `X-CSRF-Token` header.
Safari's Inspector recorded status 403 with the body `Request rejected /
Solicitud rechazada (403).`

The evidence that nothing was written comes from the source only. In
`crates/mct-cli/src/web/http.rs`, an `api/` POST whose header token does not
match returns 403 before `api::write` is called. No database, history or API
read followed the probe, and that fixture was recreated before the full Safari
run. Independent confirmation that nothing was written is **NOT VERIFIED**.
The empty body would not have been a valid decision even with a token.
Firefox ran no CSRF probe.

## False-positive persistence across reindex and code changes (2026-10-27)

An independent read-only check asked whether dead-code and duplicate findings
marked `false_positive` keep that status after reindexing, and what happens when
their code changes. No project source, schema, detector, fingerprint or review
logic was modified. Two Python (standard library) harnesses built disposable
fixtures outside the repository and drove the checked-out
`target/debug/mct-cli` (reindex, `web`, `call`, `dead-code`, `duplicates`).
Decisions were written only through `POST api/review` (Origin plus
`X-CSRF-Token`). Results were read through the API lists, the status filters,
`api/finding`, `api/reviews?status=orphan`, a read-only SQLite connection to the
fixture's own `dashboard.sqlite3`, the CLI reports (with and without
`--include-reviewed`) and the MCP `find_dead_code`. The repository's own
databases were never opened. Scripts and logs are scratch files, not committed.

Run 1 (four groups, each with one dead-code finding and one two-member duplicate
group, all eight marked `false_positive`): **1076 checks, 0 failed.**

| Treatment | Dead code | Duplicate |
| --- | --- | --- |
| None, after incremental ×2, forced ×2, restart, forced reindex while stopped, mtime-only touch, shifted lines | stays `false_positive` | stays `false_positive` |
| `restore` through the API | `pending`, back in the queue; stays so after forced reindex and restart | same |
| Body edited | `needs_reevaluation`, in neither the `pending` nor the `false_positive` filter, listed again by the CLI; back to `false_positive` when reverted | same |
| Name changed / third member added | new `pending` finding; old decision orphaned with its SQLite row unchanged; back when reverted | new group is `pending`; same orphan behavior |
| Homonym in another file / same file with a different body | separate `pending` finding; original keeps its decision | n/a |
| Identical twins in one file | both `pending`, empty fingerprint, no decision inherited; the decision returns when the twin is removed | n/a |

Run 2 (the cases first only inferred from the code; nine decisions including a
three-member group): **256 checks, 0 failed.** All identities and states below
were observed, and the forced reindex plus restart left them unchanged.

| Change | Result |
| --- | --- |
| Dead: comment added inside the body | identity kept, `needs_reevaluation` |
| Dead: whitespace-only change inside the body | identity kept, `needs_reevaluation` |
| Dead: function moved to another file | new `pending` finding; old decision orphaned |
| Dead: function deleted | no current finding; decision orphaned |
| Duplicate: member rewritten so the group no longer exists | decision orphaned |
| Duplicate: member moved to another path / member function renamed | new `pending` group; old decision orphaned |
| Duplicate: member removed from a three-member group | the remaining pair is a new `pending` group; the triple is orphaned with its row intact |
| Identical dead-code twins | both `pending`; `POST api/review` `false_positive` returned **409** for each, so they cannot be dismissed |
| Every change reverted | all nine decisions back to `false_positive` with the saved fingerprints |

Policy observed: the identity says which finding it is (duplicate groups: language
plus the path, name and kind of each member; dead code: language, path, kind,
parent and name, with an ordinal and an exact-content match for same-name
siblings). The fingerprint hashes the exact text of the reviewed range. A
decision applies only when both match. The same identity with a different
fingerprint needs re-review. A different identity is a new `pending` finding and
the old decision stays orphaned and restorable. Unreadable identity (identical
twins) fails closed.

Operational note, not a persistence defect: the read-only CLI reports
(`duplicates --no-cache`, `dead-code`) touch the WAL/SHM mtimes of
`index.sqlite3`, which left a running dashboard stale in 10 of 10 rounds.
`POST api/review` is then refused with 409 (the `index_stamp` check) until the
dashboard refreshes on its next analysis read. The first harness run failed for
this reason before it waited for the refresh.

Limits: the harnesses are self-written and were run once each. They use one
fixture language (Rust). The remaining checks are in the three subsections
below.

### Rust suites, clippy and JS tests (same session)

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 1978 passed, 0 failed, 26 ignored (74 tests have review, identity, false_positive or dead_identity in the name) |
| `cargo clippy` | clean |
| JS tests (`dashboard_spa.cjs`) | 51 of 51 |

The 26 ignored tests were not run.

### Safari 27 UI pass on disposable Rust fixtures

Two fixtures: the first with 4 groups, the second with 4 duplicate groups plus
64 dead-code findings. Dead-code clicks were real pointer input through
`page_interactions`. The duplicate group-decision buttons were triggered with
JavaScript `.click()` because their uids were not in the page tree.

| Check | Result |
| --- | --- |
| Dead code: mark false positive in the editor, forced reindex, reload | persists |
| Dead code: edit the function | "Reevaluar (1)" |
| Pagination with 64 findings | "1-50 / 64" then "51-64 / 64" |
| Finding 51 (page 2): mark, forced reindex | persists (1 false_positive, 63 pending) |
| Dead code: "Volver a pendiente" | 64 pending, 0 false_positive |
| Duplicate group: false_positive via "Decision sobre el hallazgo", forced reindex | persists (badge 3 pending) |
| Duplicate: edit a member, reindex | `needs_reevaluation` |
| Duplicate: "Volver a pendiente" | pending (checked through the API) |
| Per-copy "Rechazar" | group stays pending; matches the UI text |

Notes: "Marcar duplicado" is a bookmark, not a decision. The first load after a
reindex showed "Actualizando..." and a second reload showed the right state. A
per-copy status went back to "Sin revisar" after the copy's code changed (seen
once, cause not verified). The browser console was not read in this pass.

### Concurrency harness (Rust fixture, 27 of 27 checks)

Script `fp_concurrent.py`, threads against a real server, one run.

| Case | Result |
| --- | --- |
| 12 simultaneous different decisions on one finding, same revision | 1 accepted, 11 rejected with 409; one SQLite row equal to the API state |
| 8 different findings decided at once | 8 x 200; 8 rows false_positive |
| Stale tab saves after another tab restored | 409; state is what the other tab left |
| 6 rounds of decisions during `reindex --force` | 6 x 200, 12 x 409, no other code; settled decisions survive a further reindex |
| Code edited and a post with the old fingerprint during reindex | 409; finding pending, never a clean false_positive |
| Second server on the same project | coexisted and the first kept serving; concurrent writes from both were not tested |

The 409s during reindex are the `index_stamp` refusal: no data is lost, but a
user who marks at that moment has to retry.

## Acceptance matrix and follow-up

| Requirement | Result |
| --- | --- |
| Identified starting state; unrelated work preserved | PASS; Batch 1 code and docs unchanged, error registers later updated for ERR-022 (see [Preservation boundary](#preservation-boundary)) |
| API dead/duplicate review, filtering and restoration | PASS (automated) |
| SQLite false_positive/ignored persistence and repeated reindex | PASS (automated); PASS (native, Firefox and Safari) |
| Reopened Site/Store persistence | PASS (automated) |
| Homonym isolation, membership changes and conservative re-review | PASS (automated regressions) |
| `false_positive` survives reindex; code-change policy (edit, move, rename, delete, membership) | PASS (fixture harnesses, 1076 + 256 checks, Rust suites, Safari 27 UI; see [the section](#false-positive-persistence-across-reindex-and-code-changes-2026-10-27)) |
| Concurrent review, failed publication and recovery | PASS (automated regressions); PASS (thread harness, 27 checks, see [the section](#concurrency-harness-rust-fixture-27-of-27-checks)) |
| Revision-safe source rendering and Batch 1 protection | PASS (automated regressions) |
| Immediate filtered queue reconciliation | PASS (Node); PASS (Firefox 158, Safari 27; build B1) |
| macOS real localhost startup | PASS (first session's EPERM was environmental) |
| Real-browser interactions, reload and screenshots | PASS (Firefox 158, Safari 27; scripted real input; build B1) |
| Real dashboard process restart persistence | PASS (two SIGINT restarts per browser run; build B1) |
| Full asset regeneration | PASS (byte-identical; first session's npm E403 was environmental) |
| Editor position within the loaded page (DASH-002) | PASS (fixed; Node; Firefox 158 and Safari 27 on build B2); page-scoped, see [Known DASH-002 limitations](#known-dash-002-limitations) |
| API write rejected without the CSRF token | PASS (403; one Safari 27 probe on build B1); no write confirmed by source only |
| Console free of errors | PASS with exceptions: DASH-003 fixed (Node; Safari 27 on build B3); DASH-004 accepted; one unexplained Safari `Fetch API … access control checks` message in one of four B3 invocations; Firefox console not checked on B3 |
| Complete macOS dashboard milestone | PASS, pending coordinator review |

Remaining risks:
- Browser runs were scripted, not manual. They used one small Rust fixture
  that never crossed the 50-item page boundary.
- Firefox never ran the final build B3, and the full persistence procedure
  ran on build B1 only.
- DASH-002's position is page-scoped (see
  [Known DASH-002 limitations](#known-dash-002-limitations)).
- The empty-queue footer is verified only by Node tests.
- The DASH-003 fix was rechecked in Safari only.
- One Safari console message is unexplained.
- The CSRF no-write result rests on source inspection.
- The false-positive persistence and concurrency harnesses are self-written,
  ran once each on Rust fixtures (at most 64 dead-code findings) and are not
  kept in the repository, so no repository test guards the edit, move, delete
  and membership cases.
- Not tested: other languages (Python has duplicate token sources), a real
  large project, manual multi-day use, Firefox for this feature, two real
  browsers writing at once, and the 26 ignored Rust tests.
- The EPERM and E403 causes are inferred, not proven.

ERR-022 (MCP startup timeout) is independent and remains OPEN in
[the error registry](../../errors/REGISTRY.md). An MCP connection timeout is
not evidence about the localhost dashboard.

The next step is coordinator review of these results.
No production cutover is authorized. D3 stays separate and follows this P0 milestone unless a
specific acceptance dependency is demonstrated and implementation authorized.
