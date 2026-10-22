# Design: shared review decisions for web, CLI and MCP

Date: 2026-10-22

Status: proposal, not implemented. Base: `origin/main` at
`e68aec9c77db0726f0127bf5e0822eff9a1ea13b` (verified after `git fetch`).
Approved: D1, D2, D4, D5, D6, D8. D3 is approved at design level; the GitHub
issue it requires (`rules.md`, Architecture) is drafted but not published.
D7 and D9 were re-validated (§15.1, §15.2) and wait for approval, as do the
decisions added in this revision (D11–D18). Implementation must not start
before the blocking decisions in §15 are approved.

## 1. Goal

A human decision about a finding (a duplicate group or a dead-code candidate)
must mean the same thing in every consumer that shows that finding, survive
reindexing, never hide a finding it does not provably describe, and never be
lost. Agents that use the MCP server can then skip findings a person already
rejected, instead of re-reviewing them and spending context on them.

Priorities, in order:

1. Safe migration between the two incompatible `version: 2` review formats.
2. Preservation of human decisions and review history.
3. Stable finding identity and conservative suppression.
4. Backward compatibility of MCP and CLI contracts.
5. Correct filtering, pagination and counts.
6. Atomic persistence, concurrency and recovery.
7. Reproducible tests and performance benchmarks.

When two rules conflict, the higher priority wins. "Conservative" always means:
when in doubt, show the finding.

## 2. Verified current state

| Fact | Evidence |
|---|---|
| Review logic exists only in the web dashboard. | `crates/mct-cli/src/web/reviews.rs`, `crates/mct-cli/src/web/api/findings.rs` |
| MCP `find_dead_code` ignores reviews; it collects every candidate and paginates in the formatter. | `crates/mct-mcp-server/src/server.rs` (`MctServer::find_dead_code`, `FindDeadCodeArgs`), `crates/mct-mcp-server/src/format.rs` (`find_dead_code_toon`, `paginate`) |
| No MCP tool exposes duplicates. | No duplicate symbol in `crates/mct-mcp-server` |
| CLI `dead-code` (CSV) and `duplicates` (TOON, with `duplicates-cache.toon`) ignore reviews; ADR-015 keeps the CLI report complete and unchanged. | `crates/mct-cli/src/main.rs` (`dead_code_report`), `crates/mct-cli/src/duplicates.rs` (`run`) |
| `mct-obsidian` does not show findings. | No dead-code or duplicate symbols in `crates/mct-obsidian/src` |
| `find_dead_code` is not served from the MCP query cache. | `QueryCache::lookup_exact` is called only by the search tools |
| `Index::file_hashes()` returns every `(relative_path, content_hash)` with one query; `content_hash` is the Git blob id of the file bytes. | `crates/mct-index/src/lib.rs`, `crates/mct-index/src/indexer.rs` |
| No tagged release reads or writes any review file. The latest tag, `v0.2.0` (`7db6290`, 2026-10-09), predates the web dashboard (first commit `5e4a1eb`, 2026-10-21). `CHANGELOG.md` lists the dashboard and store v2 under *Unreleased*. | `git tag --contains 5e4a1eb` is empty |
| Every writer stores Unix **seconds** from `SystemTime::now()` on the server. | `now_unix` in `crates/mct-cli/src/web.rs` (MA and MB) |
| `M0` and `MA` use the same index schema. | `git diff a52c3e4 e68aec9 -- crates/mct-index/src/schema.rs` is empty |
| Dependencies: `mct-index` has no `hmac-sha256`, `form_urlencoded` or `tempfile`, and no dev-dependencies. `hmac-sha256 1.1.15` is already built for `mct-cli`, `mct-mcp-server` and `mct-obsidian`; `form_urlencoded 1.2.2` only for `mct-cli`; `tempfile 3.27.0` for `mct-cli` and `mct-obsidian`. | crate `Cargo.toml` files, `Cargo.lock`, `cargo tree -i` |

### 2.1 Review formats that exist today

Four binaries have written review state. Each format is identified by its
file name, its first line and its exact table header.

| Id | Written by | File | Header | Key | States |
|---|---|---|---|---|---|
| `A1` | ADR-015 binaries (for example `a52c3e4`) | `.mct-index/duplicate-reviews.toon` | `version: 1` / `reviews[n]{key,reason}:` | ADR-015 duplicate key | `false-positive`, `dismissed` |
| `A2` | `origin/main` since PR #226 (`e68aec9`) | same file, rewritten from `A1` on first save | `version: 2` / `reviews[n]{key,state,updated,note}:` / `history[n]{key,from,to,at,note}:` | ADR-015 duplicate key, or `dead_code_key` (path, name, kind, start line, file content hash) | `in-review`, `confirmed`, `false-positive`, `resolved`, `dismissed`; no entry = unreviewed; `none` in history |
| `B2` | unmerged work on `feature/web-dashboard` (ADR-018 draft) | `.mct-index/reviews.toon` | `version: 2` / `reviews[n]{kind,identity,fingerprint,status,at,reason}:` / `history[n]{same}:` | `(kind, identity)` plus a separate fingerprint | `pending` (restore tombstone), `confirmed`, `false_positive`, `ignored`; `needs_reevaluation` is derived |

Verified writer behavior:

- `A2` restore (`Reviews::update(key, None, ..)`) removes the current entry
  and appends a history event with `to = none`.
- `A2` upgrades `A1` rows with `updated = 0`, so time 0 also appears in `A2`.
- `A2` and `B2` lock with `create_new` and remove the lock in `Drop`; a crash
  leaves the lock. `A2` publishes with `tempfile::NamedTempFile::persist`
  after `sync_all`; neither syncs the parent directory.
- `B2` stores `at` as `u64` seconds; decisions read from `A1` have no time.

Incompatibilities observed in the code:

- `A2` and `B2` both say `version: 2` but have different files and schemas.
- `A1` readers (`a52c3e4`) and `B2` readers require `duplicate-reviews.toon` to
  be `version: 1`. Once an `A2` binary saves a decision, they refuse to load
  and their web dashboard does not start. This breakage already exists; this
  design must not add more.
- `A2` keys carry no finding kind. A key can only be classified by matching it
  against the keys of current findings.
- `A2` notes are form-urlencoded. `B2` reasons are stored raw in the last
  column, with control characters removed, so they may contain commas.
- `A2` rejects a save when history is full; `B2` silently drops the oldest
  history rows. The two lock files (`duplicate-reviews.lock`, `reviews.lock`)
  do not exclude each other.
- `B2`'s dead-code identity numbers same-named symbols by their order among
  *candidates*. A sibling that gains a reference shifts the ordinal of the
  other. The fingerprint (line range) still differs, so this degrades into a
  visible re-evaluation, not a wrong suppression.
- `A2`'s web silently skips dead-code candidates that cannot be mapped to the
  navigation snapshot or have no file hash.
- No writer guards against a clock that moves backwards, and no writer detects
  that another process changed a decision after the page was rendered.

The developer checkout of this repository currently holds a `B2` file with one
current decision and two history rows. No `duplicate-reviews.toon` exists there.

## 3. Terminology

- **Finding**: one duplicate group or one dead-code candidate, as produced now by
  `find_duplicate_groups` or `find_dead_code_candidates`.
- **Identity**: a hash that stays the same across edits of the finding's code;
  it answers "is this the same finding?".
- **Fingerprint**: a hash of the finding's exact code state; it answers "is the
  decision still about this exact code?".
- **Source**: one review file (`C3` canonical, or legacy `A1`/`A2`/`B2`).
- **Candidate**: the decision one source holds for one finding (§7.6).
- **Tombstone**: a timed record that a person restored a finding to unreviewed.
- **Resolution**: the review state of one current finding after reading every
  source.
- **Revision**: the `(state, time, source)` of the winning candidate, used by the
  web to detect stale writes (§7.10).
- **Suppressed**: hidden from an actionable list because of a human decision.

## 4. Functional requirements

| Id | Requirement |
|---|---|
| FR-1 | One review domain module (formats, identity, resolution, visibility policy, atomic store) used by the web, the CLI and the MCP server. |
| FR-2 | Every finding has an identity and a fingerprint (§7.2). A decision suppresses a finding only when both match exactly. |
| FR-3 | A decision whose identity matches and whose fingerprint does not becomes `needs-reevaluation`: always visible, never automatically resolved, previous state shown. |
| FR-4 | One state vocabulary and one visibility function (§7.3, §7.4); consumers may differ only in their *default filter*, which is documented. |
| FR-5 | No decision or history row is ever deleted or rewritten by the new code. Legacy files are never written, renamed or deleted. Restoring a finding writes a tombstone row. |
| FR-6 | Filtering happens before pagination. Every filtered response reports the actionable total and the suppressed count per state. |
| FR-7 | Suppressed findings stay reachable on request (MCP `reviewed=only`/`show`, CLI `--reviewed only`/`show`, the web state filter). |
| FR-8 | If any source exists but cannot be read, nothing is suppressed and no review state is shown. Reads report which source failed and why; writes fail without replacing any file (§7.8). |
| FR-9 | All decisions in `A1`, `A2` and `B2` files resolve with their original state, time and note, without modifying those files (§7.6). |
| FR-10 | Decisions survive `reindex --force` and deletion and rebuild of `index.sqlite3`. |
| FR-11 | Each MCP or CLI call that asks for review data reads current sources. The web reflects other processes' writes at its next save or analysis reload, as today. |
| FR-12 | Diagnostics: which sources were read (path, format id), whether the store is degraded, and counts of orphan rows, conflicts, ambiguous legacy keys, inconsistent `A2` keys, future timestamps and leftover temporary files. Notes are never included in MCP output. |
| FR-13 | Resolution is a pure, deterministic function of the source bytes and the current findings: the same inputs give the same output on every platform and run (§7.6). |
| FR-14 | A web save based on a stale view is rejected without writing; the user sees the current state (§7.10). |
| FR-15 | A new decision made in the web always takes effect, even if the clock moved backwards or a legacy row carries a future time (§7.7). |

## 5. Non-functional requirements

| Area | Requirement |
|---|---|
| Compatibility | With `reviewed` omitted, every `find_dead_code` call result is byte-identical to today and no review file is opened. CLI command output is byte-identical without the new flags. Only the tool schema and `--help` text gain the new optional parameter (D18). |
| Performance | §12 thresholds. Lookup is O(1) per finding (hash maps keyed by identity and by legacy key). |
| Memory | Each source is bounded to 32 MiB, 100,000 current rows and 100,000 history rows, checked before parsing. |
| Integrity | Exclusive lock file, temporary file in the same directory, `sync_all`, invariant check, atomic rename, parent-directory sync on Unix. A crash leaves the old or the new complete file. |
| Security | Existing guards kept: symlinked ancestors refused, hard-linked files refused on Unix, 64-hex keys, notes at most 500 characters without control characters. No path in any request reaches the filesystem. |
| Portability | Linux, macOS and Windows. No crate outside the current lockfile. |
| Maintainability | `mct-core` stays unaware of reviews. The domain module knows no HTTP, CLI or MCP types. No traits, generics or plugin layer. |

## 6. Non-goals

Fuzzy or subset matching of findings, per-symbol body hashing, a duplicate MCP
tool, multi-user accounts, a SQLite schema change, live reload of the web,
automatic repair of corrupt files, and deleting or archiving legacy files.
Each needs its own issue if wanted.

## 7. Design

### 7.1 Module placement

`crates/mct-index/src/reviews/`, re-exported from `mct-index`:

| File | Responsibility |
|---|---|
| `mod.rs` | public types: `Kind`, `State`, `Candidate`, `Resolution`, `Revision`, `Visibility`, `Diagnostics`, `ReviewError`, `FindingKeys` |
| `identity.rs` | identity, fingerprint and legacy-key computation for both kinds |
| `format.rs` | `C3` parser and writer; read-only parsers for `A1`, `A2`, `B2` |
| `store.rs` | source discovery, bounded safe reads, lock, atomic write, invariant check |
| `resolve.rs` | candidates, the total order of §7.6, the history union |
| `policy.rs` | `visibility(&Resolution) -> Visibility` and per-state counts |

Errors use `thiserror`, like the rest of `mct-index`. Dependencies follow D7
(§15.1). Labels and translations stay in `crates/mct-cli/src/web`.

### 7.2 Identity, fingerprint and legacy keys

Every hash is SHA-256 over length-prefixed fields (the existing framing), as
64 lowercase hex characters.

| Kind | Identity | Fingerprint | Legacy keys, computed only to read legacy sources |
|---|---|---|---|
| duplicate | `duplicate-id-v2`: language, the sorted `(path, symbol name, symbol kind)` of every member, and the ordinal among current groups with the same members, ordered by their sorted member line ranges | the ADR-015 key: normalization, language, sorted members with line range and file content hash | `A1`/`A2` key = fingerprint; `B2` identity = `duplicate-id-v1` (the same fields without the ordinal) |
| dead code | `dead-id-v2`: language, path, symbol kind, parent, name and the ordinal among **all indexed symbols** of that file with the same `(kind, parent, name)`, ordered by start line | `dead-fp-v1`: file content hash, start line, end line (same as `B2`) | `A2` key = `dead_code_key` (path, name, kind, start line, file content hash); `B2` identity = `dead-id-v1` (ordinal among candidates, recomputed over the same candidate list) |

The ordinal in `duplicate-id-v2` was added during implementation: partial-block
groups (ADR-012) between the same symbols have the same members, so
`duplicate-id-v1` alone can name two current groups. An ordinal shift can only
produce `needs-reevaluation` or no decision, never a suppression, because
suppression still needs the exact fingerprint.

A finding whose file has no content hash in the index gets a fingerprint over
an empty hash (as `B2` does) and is marked as having unknown content: no
decision ever suppresses it, and it has no `A1`/`A2` key.

`dead-id-v2` removes the ordinal shift described in §2.1. Computing it needs
the symbols of the candidates' files. `find_dead_code_candidates` already loads
them through `list_symbols_all`, so the shared function returns both instead of
running a second query. File hashes come from `Index::file_hashes()`, not from
a `NavigationSnapshot`.

A path or language scope always includes whole files, so scoped and unscoped
calls compute the same identities and ordinals.

### 7.3 States and mapping

Canonical stored values (the `A2` spelling, already published on `main`):
`unreviewed` (stored only as a restore tombstone), `in-review`, `confirmed`,
`false-positive`, `dismissed`, `resolved`.
Derived, never stored: `needs-reevaluation` and `resolved-but-detected` (D2).

| Legacy value | Canonical |
|---|---|
| `A1` `false-positive` / `dismissed` | `false-positive` / `dismissed` |
| `A2` state values | identical; `none` in history = `unreviewed` |
| `B2` `pending` / `confirmed` / `false_positive` / `ignored` | `unreviewed` / `confirmed` / `false-positive` / `dismissed` |

### 7.4 Visibility policy

| Resolution | Visibility |
|---|---|
| no decision, `unreviewed`, `in-review`, `confirmed` | actionable |
| `false-positive` or `dismissed`, fingerprint matches | suppressed |
| `resolved`, fingerprint matches | actionable, shown as `resolved-but-detected` (D2) |
| any non-tombstone decision whose fingerprint does not match | actionable, `needs-reevaluation` (shows the previous state) |
| ambiguous legacy key (§7.6.1) | actionable, no decision |
| store degraded (§7.8) | actionable, no state shown |

Only `false-positive` and `dismissed` with a matching fingerprint suppress.

### 7.5 Canonical format `C3`

File `.mct-index/review-decisions.toon` (D1). The new name means no existing
binary ever opens it, so none of them can break on it.

```
version: 3
reviews[n]{kind,identity,fingerprint,state,updated,note}:
  dead-code,<64 hex>,<64 hex>,false-positive,1792700000,generated%2C+kept
history[n]{kind,identity,fingerprint,from,to,at,note}:
  dead-code,<64 hex>,<64 hex>,unreviewed,false-positive,1792700000,generated%2C+kept
```

- `kind` is `duplicate` or `dead-code`; `(kind, identity)` is unique in `reviews`.
- `updated` and `at` are decimal Unix seconds (`u64`), assigned by §7.7, never
  taken from a request.
- Notes are form-urlencoded (as `A2`), so a row never contains a raw comma,
  colon, quote or newline.
- `history` is append-only. When a save would exceed a bound it is rejected
  with an explicit error; nothing is dropped (D6).
- Rows are written sorted by `(kind, identity)` for `reviews` and in append
  order for `history`, so output is deterministic.
- Any other `version` value is unreadable (§7.8). A future format must use a
  new version, and this code never overwrites a file it cannot parse.

### 7.6 Migration: virtual, read-time, non-destructive

Legacy files are never converted. Legacy `A1`/`A2` keys are opaque hashes, so
they can be classified only against current findings. A one-time conversion
would therefore either lose the rows that match nothing now, or have to keep
the legacy files anyway. Instead, every read resolves current findings against
all sources, and new decisions go to `C3` only. Because nothing is converted,
there is no half-migrated state to recover from.

**Step 1: discover and classify sources.** For each path in
(`review-decisions.toon`, `reviews.toon`, `duplicate-reviews.toon`):

1. Absent (`NotFound`): skip it. Any other `metadata` or open error makes the
   source unreadable.
2. Run `safe_path`, read at most 32 MiB + 1 byte, and reject anything larger.
   An empty file is unreadable, not absent.
3. Classify by first line and exact header: `review-decisions.toon` must be
   `C3`; `reviews.toon` must be `B2`; `duplicate-reviews.toon` must be `A1` or
   `A2`. Anything else is unreadable.
4. Parse strictly: row counts must match headers, keys must be valid, no
   duplicate current key, states must be known, times must be decimal `u64`.
   Any failure makes that source unreadable, with the line number and reason.

If any present source is unreadable, the result is `Degraded` (§7.8).

**Step 2: build candidates.** At most one candidate per source and finding.

| Source | Lookup | Candidate | Time | `fp_match` |
|---|---|---|---|---|
| `C3` | `(kind, identity)` | the current row; `unreviewed` = tombstone | `updated` | `row.fingerprint == f.fingerprint` |
| `B2` | `(kind, dead-id-v1 or duplicate-id-v1)` | the current row; `pending` = tombstone | `at` | `row.fingerprint == f.fingerprint` |
| `A2` | key | the current entry; if there is none and the key's last history event has `to = none`, a tombstone | `updated`, or the event's `at` | true (the key includes lines and file hashes) |
| `A1` | key | the current entry | 0 | true |

Time 0 means *unknown time* (`A1`, `A2` rows upgraded from `A1`, `B2` rows read
from `A1`).

**Step 3: resolve each current finding** with the total order of §7.6.2. A
winning tombstone gives *no decision*, whatever its fingerprint. Any other
winner gives `Decided(state)` if `fp_match`, else `NeedsReevaluation(state)`.
Losing candidates never hide a finding; if a loser's outcome differs from the
winner's, the conflict is counted in the diagnostics.

**Step 4: history of one finding** is the union of `C3` rows for
`(kind, identity)`, `B2` rows for its `B2` identity and `A2` events for its
key. It is sorted by `(at, source rank, row position in its file)`, each row is
tagged with its source, and nothing is deduplicated, because rows from
different sources are distinct events.

**Step 5: orphans.** Rows that match no current finding are kept untouched and
counted. If the code returns (for example after a revert), they match again.

#### 7.6.1 Ambiguity and inconsistency

- **Ambiguous legacy key**: if one `A1`/`A2` key or one `B2` identity matches
  more than one current finding (for example two dead-code candidates with the
  same path, name, kind, line and file hash), that source gives no candidate
  for any of them. The count goes to the diagnostics.
- **Inconsistent `A2` key**: history mentions the key, there is no current
  entry, and the last event is not `to = none`. No candidate; counted.
- `C3` cannot be ambiguous: `(kind, identity)` is unique in the file, and both
  `duplicate-id-v2` and `dead-id-v2` include an ordinal, so no two current
  findings share an identity.
- `B2`'s `duplicate-id-v1` has no ordinal: if it names more than one current
  group, it is ambiguous under the rule above.

#### 7.6.2 Deterministic conflict resolution

The candidates of one finding are totally ordered. The first one wins:

1. **Greater time first.** The latest human action wins. Time 0 loses to any
   known time.
2. **On equal time, the less suppressing outcome first** (D11): a tombstone,
   `needs-reevaluation` or any actionable state before `false-positive` or
   `dismissed` with a matching fingerprint. Two decisions made in the same
   second that disagree are treated as doubt, and doubt shows the finding.
3. **Then source rank**: `C3` before `B2` before `A2` before `A1`.

Each source gives at most one candidate per finding, so rule 3 always breaks
the tie. The order uses no hash-map iteration, file timestamps, clock reads or
platform-specific data (FR-13).

Equal times arise only from writes that the deciding process did not see.
Writes made through the new web never tie with what that web showed, because of
§7.7.

#### 7.6.3 Timestamps, precision and clock skew

- **Precision**: seconds, in every format. `C3` keeps seconds so its times
  compare directly with `A2` and `B2`. Within one source there is one current
  row per finding, so no sub-second order is needed there. History order
  within a file is its append order, which is authoritative.
- **Logical time on write** (D12): a new `C3` row gets
  `t = max(now, t_seen + 1)`, where `t_seen` is the greatest time of any
  candidate *or history row* of that finding, in any source. The latest
  action therefore always wins and each finding's history is strictly
  increasing, even if the clock went backwards or a legacy row holds a
  future time. If `t_seen == u64::MAX`, the save is refused with an explicit
  error.
- **Future timestamps**: rows more than 300 s after `now` are accepted as
  they are, because old binaries accept them too. They are counted in the
  diagnostics, and the web shows a warning naming the source.
- **Old binaries** write wall-clock time without this guard. A decision they
  make while the clock is behind can lose to an older `C3` row. That decision
  is still kept in its file and appears in the history union and the conflict
  count. It is a documented limitation of mixing binaries (R13).
- **Reading time 0**: shown as "unknown time" in the web, never as 1970.

#### 7.6.4 Tombstones and restoration

- A tombstone competes in §7.6.2 like any decision. If it wins, the finding
  has no decision; a fingerprint mismatch does not turn a tombstone into
  `needs-reevaluation`.
- Restoring in the web writes a `C3` row with `state = unreviewed` and the
  current fingerprint, plus a history row `from = previous resolved state`,
  `to = unreviewed`. The tombstone remains as the current `C3` row; it is never
  removed.
- A `C3` tombstone overrides every older legacy decision without editing the
  legacy file. A legacy decision made *later* by an old binary wins over it
  again, because it is the latest action.
- Restoring a finding that has no decision is a no-op and writes nothing.
- A new decision after a restore replaces the tombstone row and appends history.
- `A2` restores (entry removed, `to = none`) and `B2` `pending` rows are
  tombstones with their own times. `A1` has no restore information: a removed
  `A1` row is simply absent.

### 7.7 Write protocol (web only)

1. Validate the kind, 64-hex identity and fingerprint, the state, the note
   (at most 500 characters, no control characters), and the expected revision
   sent by the page (§7.10).
2. Create `.mct-index/review-decisions.lock` with `create_new`, recording the
   process id and start time. On failure, report the path and the lock's age.
   Never remove someone else's lock automatically (R14).
3. Under the lock, re-run Step 1 for every source and resolve the finding. If
   any source is unreadable, abort (D14).
4. If the resolved revision differs from the expected revision, abort with a
   conflict that carries the current resolution (D13). Nothing is written.
5. If the requested state equals the resolved state, make no change and write
   no history row.
6. Compute `t` (§7.6.3). Upsert the `C3` current row and append one history row
   `(from = resolved state, to = new, at = t)`. A restore writes
   `state = unreviewed`.
7. Check the bounds; if exceeded, abort with an explicit error (D6).
8. Render the new file, then **verify the invariant** by parsing those bytes
   with the `C3` parser. The new history must start with the old history
   exactly, with one row appended. Every old current row must be unchanged,
   except the target row, which must equal the request. If not, abort.
9. If D15 is approved, publish the previous `C3` bytes to
   `review-decisions.toon.prev` (temporary file, `sync_all`, rename).
10. Write the new bytes to a temporary file
    `review-decisions.toon.<pid>.<random>.tmp` created with `create_new`.
    Call `sync_all`, re-check `safe_path`, atomically rename it over
    `review-decisions.toon`, sync the parent directory on Unix, then update
    the in-memory view. A guard removes this process's temporary file on every
    error path.
11. Release the lock in `Drop`.

The web stops writing `duplicate-reviews.toon` (D9). Legacy files are read
under this lock only for consistency of this process's own view: old binaries
do not take `review-decisions.lock`, so they may change their files at any
time, and each such change is atomic (§2.1).

### 7.8 Degraded mode: one source corrupt, others valid

An unreadable source may hold the newest decision or tombstone for any finding,
so no other source can be trusted to suppress on its own. The rule is global
and fail-closed:

| Situation | Read result | Web | MCP / CLI with `reviewed` | Writes |
|---|---|---|---|---|
| all present sources readable | normal resolution | normal | normal | allowed |
| one legacy source unreadable, `C3` readable | `Degraded` | starts read-only: banner naming the file, the line and the reason, plus manual recovery steps; no states shown; actions disabled (D5) | one warning line; every finding listed; no `review` column values; CLI warning on stderr, exit code per D17 | refused (D14) |
| `C3` unreadable, legacy readable | `Degraded` | same; if a valid `review-decisions.toon.prev` exists, the banner says so | same | refused |
| several unreadable | `Degraded` listing each | same | same | refused |
| a source is a symlink, a hard link or oversized | `Degraded` | same | same | refused |
| `C3` has an unknown version (for example a newer binary's format) | `Degraded` | banner: "written by a newer version" | same | refused; the file is never overwritten |

Recovery is always manual and documented: the user fixes the file, moves it
aside, or restores `.prev`. Moving a legacy file aside removes its decisions
from the resolution; that is the user's explicit choice. The code never
repairs, renames or deletes a review file.

On Windows, Rust's standard library opens files with read, write and delete
sharing, so a reader does not block the writer's rename. A reader that still
gets a sharing violation retries once after 50 ms before reporting the
source unreadable.

### 7.9 Read protocol

No lock. Atomic rename guarantees that a reader sees a complete old or new
file. MCP and CLI read sources on each call that requests review data. Sources
are read one after another, so a concurrent old-binary save may fall between
two reads. The result is still a valid resolution of complete files, and the
next call sees the save. A parse cache keyed by file length, modification
time and file id is added only if §12 shows reads exceed the threshold.

### 7.10 Stale-view protection

The web sends, with each save, the revision it displayed: `(state, time,
source)` of the winning candidate, or `none`. Under the lock the server
resolves again (§7.7 step 4). If another N process changed the decision in the
meantime, or an old binary wrote a newer legacy decision, the save returns
HTTP 409 with the current resolution, and the page shows it without saving.
Old binaries do not perform this check; their concurrent decisions are ordered
by §7.6.2 and remain visible in the history union.

`POST api/review` belongs to the embedded front end, which is built from the same
commit as the server, and is not a published contract. It gains the revision
field.

### 7.11 Consumers

**Web** (`origin/main` code). `web/reviews.rs` becomes a thin adapter over
`mct_index::reviews`. `api/findings.rs` keeps filtering before paging and its
facets. The state facet gains `needs-reevaluation` and `resolved-but-detected`.
`app/page-findings.js` adds both to the default visible states and to its
action table, and sends the revision. A degraded store starts the dashboard
read-only (D5).

**MCP `find_dead_code`.** A new optional `reviewed` argument (D3, issue draft
pending authorization):

| `reviewed` | Rows | Header | `review` column |
|---|---|---|---|
| omitted | all candidates, today's output byte for byte; no file opened | unchanged | no |
| `show` | all candidates | `N candidate(s) … (hidden: 0)` | yes |
| `hide` | actionable only | `N actionable … hidden by review: k (false-positive a, dismissed b); reviewed=only lists them` | yes |
| `only` | suppressed only | `k hidden by review …` | yes |

`offset`/`limit` apply after filtering, and totals describe the filtered list.
The `review` column holds the state only, never notes. A degraded store adds
one warning line and behaves as `show` without states. Update `tools.ttc`
(`POST`/`EFFECTS: read; index and .mct-index review files when reviewed is
set`) and the `rules.md` and `README.md` tool tables. `batch` forwards the
argument unchanged.

**CLI.** `dead-code --reviewed show|hide|only` adds a `review_state` CSV column
only when the flag is given. `duplicates --reviewed show|hide|only` filters
groups the same way. Without the flag, behavior and the report cache key are
unchanged. With the flag, the cache key also includes the mode and the length
and SHA-256 of each present source, so a new decision invalidates the cached
filtered report (D4).

## 8. Compatibility

### 8.1 Matrix

Binaries: **M0** ADR-015 (`a52c3e4` and earlier, `A1` only); **MA** current
`origin/main` (`e68aec9`, reads `A1`/`A2`, writes `A2`); **MB** unmerged
`feature/web-dashboard` working tree (reads `B2`, plus `A1` only, writes `B2`);
**R** released `v0.2.0` (no web, no review files); **N** this design. Old MCP
and CLI binaries read no review file.

| Files present → | none | `A1` | `A2` | `B2` | `C3` (+ any legacy) | corrupt file |
|---|---|---|---|---|---|---|
| R (any command) | ok | unaffected | unaffected | unaffected | unaffected | unaffected |
| M0 web | ok | ok | **does not start** (pre-existing, caused by MA) | ignores | ignores `C3`; legacy as left | does not start |
| MA web | ok | ok; **rewrites to `A2` on first save** (pre-existing) | ok | ignores | ignores `C3` | does not start |
| MB web | ok | ok | **does not start** (pre-existing) | ok | ignores `C3` | does not start |
| N web | ok | resolves, never writes it | resolves, never writes it | resolves, never writes it | reads and writes `C3` | starts read-only with a banner (D5) |
| N MCP, `reviewed` omitted | today's output | same | same | same | same | same (nothing read) |
| N MCP, `reviewed` set | no states | resolves | resolves | resolves | resolves | warning, nothing hidden |
| N CLI without flags | today's output | same | same | same | same | same |
| Old MCP / CLI | unaffected | unaffected | unaffected | unaffected | unaffected | unaffected |

Transitions:

- **Upgrade to N**: every existing decision resolves immediately. No file
  changes until the first save, and that save writes only `C3`.
- **Rollback from N to MA or MB**: decisions made in N are invisible to the
  old binary; nothing is lost, and no file N wrote breaks it. Upgrading again
  shows everything, ordered by §7.6.2. Decisions made with the old binary
  during the rollback win if they are later.
- **N and MA (or MB) running together**: they write different files under
  different locks, so no update is lost. N sees both sides; old binaries see
  only their own.
- **N never creates the pre-existing `A2` breakage of M0 and MB**, because it
  never writes `duplicate-reviews.toon`.
- **Partial rollout** (new web, old MCP or CLI, or the reverse): each binary
  behaves as in its own row; no combination changes another binary's output.

### 8.2 Compatibility and rollback test matrix

"Fixture" tests use golden files produced by the real old writers (§11.1).
"Live" tests run the real old binaries through the harness of §11.2.

| Id | Scenario | Start files | Steps | Expected | Kind |
|---|---|---|---|---|---|
| CR-1 | upgrade M0 → N | `A1` | N reads | every `A1` decision resolves (time unknown); file hash unchanged | fixture |
| CR-2 | upgrade MA → N | `A2` with decisions, restores, notes with commas, rows at time 0 | N reads | states, times, notes and tombstones match the `A2` fixture; hash unchanged | fixture |
| CR-3 | upgrade MB → N | `B2` with a tombstone and a stale fingerprint | N reads | `needs-reevaluation` where the fingerprint differs; hash unchanged | fixture |
| CR-4 | developer used both branches | `A2` + `B2` for the same findings | N reads | §7.6.2 outcome per finding; conflicts counted | fixture |
| CR-5 | rollback N → MA | `A2` + `C3` | N saves 3 decisions; start MA | MA starts, shows only its `A2` state; MA save works; `C3` hash unchanged by MA | live |
| CR-6 | re-upgrade after CR-5 | as left by CR-5 plus MA's new save | start N | MA's later decision wins; N's earlier ones remain for other findings | live |
| CR-7 | rollback N → MB | `B2` + `C3` | as CR-5 with MB | as CR-5 | live (D16) |
| CR-8 | rollback N → M0 | `A1` + `C3` | N saves; start M0 | M0 starts; `A1` hash unchanged by N | live |
| CR-9 | rollback N → M0 with `A2` present | `A2` + `C3` | start M0 | M0 fails exactly as it does without N (pre-existing); N did not change `A2` | live |
| CR-10 | rollback to R | any | run R's MCP and CLI | output identical to a run with no review files | live |
| CR-11 | MCP default output | each column of §8.1, plus a source made unreadable by permissions | N `find_dead_code` without `reviewed` | bytes identical to MA's output; the unreadable file causes no warning (proves it is not opened) | live |
| CR-12 | CLI default output | same as CR-11 | N `dead-code`, `duplicates` without flags | bytes identical to MA's | live |
| CR-13 | one source corrupt at a time | each source truncated in turn, the others valid | N web, MCP `hide`, CLI `--reviewed hide` | degraded per §7.8; nothing hidden; no file changed | fixture |
| CR-14 | future `C3` version | `version: 4` file | N reads and attempts a save | degraded; save refused; file byte-identical | fixture |
| CR-15 | index rebuild | `C3` + legacy | delete `index.sqlite3`, `init` with N | identical resolutions before and after | fixture |
| CR-16 | crash during N save | `C3` | fail point after the temporary file is synced, before the rename | old `C3` intact and readable by N; MA and MB unaffected; the next save succeeds | live |
| CR-17 | `A1` rewritten by MA while N runs | `A1` | N resolves; MA saves an unrelated finding (rewrites to `A2`); N resolves again | outcomes for the original findings are identical | live |

## 9. Identity scenarios

| Scenario | Result |
|---|---|
| Lines inserted elsewhere in the file, function moved, body changed | same identity, new fingerprint → `needs-reevaluation`, visible |
| File renamed, symbol renamed | new identity → no decision; the old row becomes an orphan |
| Duplicate group gains or loses a member | new identity → no decision (no subset matching) |
| A same-named sibling gains a reference | `dead-id-v2` unchanged for the remaining symbol |
| Two candidates share one legacy key | that legacy source gives no decision for either (§7.6.1) |
| Finding disappears | decision kept, counted as orphan, not as suppressed |
| Finding reappears with identical code | decision applies again |
| Index deleted and rebuilt | identities and fingerprints unchanged (`content_hash` is content-derived) |
| Content-hash algorithm changes in a future version | every fingerprint changes → everything visible (safe direction) |
| Review file restored from backup | that backup's decisions apply; later decisions are absent |

## 10. Risk register

P = probability, I = impact (L/M/H).

| Id | Risk | P | I | Mitigation | Residual | Test |
|---|---|---|---|---|---|---|
| R2 | wrong suppression | M | H | suppress only on exact identity and fingerprint; `dead-id-v2`; legacy keys are fingerprints; ambiguous keys give no decision | low | homonyms; sibling gains a reference; identical files at two paths; shared legacy key |
| R5 | legacy loss or breakage | M (no release has review files) | H | virtual migration; legacy files never written; byte-identical checks | low | CR-1…CR-9 |
| R4 | corrupt store hides findings | L | H | global degraded mode; nothing hidden; writes refused | low | CR-13, CR-14 |
| R16 | one corrupt legacy file blocks all review writes | M | M | banner with file, line, reason and recovery steps; manual recovery only (D14) | medium (accepted availability cost) | CR-13 |
| R17 | lost update between two N processes on one finding | M | M | revision check under the lock, HTTP 409 (D13) | low | CW-5 |
| R18 | old binary and N decide the same finding concurrently | L | M | §7.6.2 order; history union keeps both; conflicts counted | low | CW-2, CW-6 |
| R13 | clock skew or equal timestamps reorder decisions | L | M | logical time on N writes (D12); conservative tie (D11); future-time warning | low for N, medium when old binaries keep writing | §11 timestamp tests, CW-3 |
| R19 | a writer bug truncates history or rewrites rows | L | H | parse-back invariant check before rename; `.prev` generation (D15) | low | invariant tests with injected faulty renderers |
| R7 | MCP or CLI contract break | M | M | opt-in argument and flags; default call output byte-identical; tool schema and `--help` change additively (D18) | low | CR-11, CR-12; context-budget delta |
| R9 | inconsistent interpretation between consumers | H today | M | one `visibility()` function | low | cross-consumer test |
| R11 | pagination or counts wrong | M | M | filter before `paginate` | low | 60 candidates, 15 suppressed, page through all |
| R1 | identity drift noise | H | M | conservative by design; documented | medium | insert-line fixture → `needs-reevaluation` |
| R12 | stale state in a long-running MCP | M | M | read per call | low | write between two calls in one process |
| R3 | concurrent read and write | M | L | atomic rename; readers take no lock; Windows retry | low | writer and reader threads, 1,000 iterations; CW-6 |
| R20 | inconsistent `A2` history | L | L | no decision, counted | low | fixture |
| R6 | latency or memory | L | M | bounds, O(1) lookups, no file read by default | low | §12 |
| R10 | path tricks, oversized notes | L | H | existing guards plus tests | low | symlink, hard link, control characters, 501-character note |
| R14 | stale lock or temporary file after a crash | L | M | the error shows lock age and pid; locks are never removed automatically; leftover temporary files are reported (D15) | medium | CR-16 |
| R21 | compatibility harness slow or flaky | M | L | `#[ignore]` by default, dedicated script, port 0, bounded timeouts, deterministic oracle, no retries that hide failures | low | harness runs twice with identical results |
| R22 | MB has no commit to build from | H | L | build a read-only copy of the working tree (D16); otherwise fixtures only | low | fixture README records the source digest |
| R8 | conflicting concurrent branches | H | H | dedicated worktree from `origin/main`; `feature/web-dashboard` untouched | medium | review the diff against base before integration |

## 11. Test plan

All tests use temporary directories and never touch a developer's
`.mct-index`. Required checks: `scripts/unix/check.sh` (fmt, workspace tests,
CI clippy with `unwrap_used`/`expect_used`/`panic` denied, `mct-eval`).

### 11.1 Fixtures

Golden fixtures live in `crates/mct-index/tests/fixtures/reviews/`. They come
from running the real M0 (`a52c3e4`), MA (`e68aec9`) and MB writers against a
temporary project. Their README records the commits, the MB source digest
(D16) and the commands used. Hand-written fixtures cover malformed inputs.

### 11.2 Live harness for old and new binaries

- `scripts/unix/review-compat.sh` builds M0 and MA with `cargo build --release
  -p mct-cli` in detached temporary worktrees under the system temporary
  directory, and MB per D16. It never builds in, or writes to, the primary
  checkout, and it checks that all binaries use the same index schema before
  running.
- `crates/mct-cli/tests/review_compat.rs` holds `#[ignore]` tests that read the
  binary paths from `MCT_COMPAT_M0`, `MCT_COMPAT_MA` and `MCT_COMPAT_MB`. A
  missing variable makes the test fail with a clear message when run with
  `--ignored`; it does not silently pass.
- Each test copies a small fixture repository with known duplicates and dead
  code to a temporary directory, indexes it once, and starts each web binary
  with `--port 0 --no-obsidian`. It reads the URL printed on stdout and drives
  saves over HTTP: MA and N read the CSRF value from `api/overview` and call
  `POST api/review`; M0 uses the ADR-015 form with its page nonce.
- N's crash fail point exists only behind a test-only cargo feature
  (`review-failpoints`) and never in release builds.

### 11.3 Simultaneous writes by old and new binaries

| Id | Setup | Action | Expected |
|---|---|---|---|
| CW-1 | N and MA running on one project | 200 saves alternating between them, on different findings | every acknowledged save resolves in N; MA, restarted, shows its 100; `A2` changes only on MA saves |
| CW-2 | N and MA running | MA dismisses a finding; N, after reload, restores it | N shows unreviewed with both events in history; MA still shows dismissed (documented divergence) |
| CW-3 | fixtures with equal times in `C3` and `A2` | resolve | §7.6.2 rule 2, then rule 3; identical result over 20 runs |
| CW-4 | N and MB running | as CW-1 and CW-2 with `B2` | as CW-1 and CW-2 |
| CW-5 | two N processes | both save the same finding from the same displayed revision | exactly one succeeds; the other gets 409; history grows by one |
| CW-6 | N and MA in tight save loops for 10 s on overlapping findings, plus N readers | stop; compare with an oracle built from both processes' acknowledged saves | no partial file was ever read; every acknowledged save is in its file; N's resolution equals the oracle |
| CW-7 | N holds `review-decisions.lock`; MA holds `duplicate-reviews.lock` | the other binary saves | each lock blocks only its own writer |
| CW-8 | N crashes at the fail point while MA saves | restart N | `C3` old or new and complete; `A2` contains MA's save |

### 11.4 Unit and integration tests

- Formats: each legacy fixture parses to the expected candidates; `C3` round
  trip; count mismatch, unknown version, duplicate key, invalid key, unknown
  state, non-decimal time, empty file, oversized file → unreadable with line
  and reason.
- Conflict resolution: every pair and triple of sources; time order; time 0;
  equal-time rules 2 and 3; property test over random candidate sets checking
  that the winner does not depend on input order.
- Timestamps: logical time when `now < t_seen`; strictly increasing history
  per finding; `t_seen == u64::MAX` refused; future-time warning threshold;
  time 0 displayed as unknown.
- Tombstones: `C3` tombstone beats an older `A2` decision; a later `A2`
  decision beats an older `C3` tombstone; an `A2` `to = none` tombstone; a
  `B2` `pending` tombstone; a tombstone with a different fingerprint gives no
  decision; restoring with no decision writes nothing; deciding after a
  restore.
- Ambiguity and inconsistency: a shared legacy key gives no decision for both
  findings; inconsistent `A2` history gives no decision; both counted.
- Degraded mode: every row of §7.8, for web, MCP and CLI.
- Store: concurrent N writers on different findings both persist; a lock
  present gives an error with its age; the revision check; the invariant check
  rejects a faulty renderer (dropped history row, changed unrelated row,
  reordered history); bounds rejected with nothing dropped; leftover temporary
  file reported; `.prev` written when D15 is approved.
- Identity: every row in §9, including `dead-id-v2` with C++/TypeScript-style
  same-name overloads and identical files at two paths.
- Policy: every row in §7.4.
- MCP: default output byte-identical (snapshot); `show`/`hide`/`only` headers,
  columns and counts; pagination across filtered pages; degraded warning;
  a write between two calls is visible; `batch` passes the argument through.
- CLI: default CSV and TOON byte-identical; flag outputs; the duplicates cache
  key changes after a decision; degraded exit code per D17.
- Web: the existing `cargo test -p mct-cli web` suite stays green; new tests
  for the new facets, read-only degraded mode, 409 on a stale revision and
  `C3`-only writes.
- Cross-consumer: one fixture store gives the same actionable set in web, MCP
  and CLI.

## 12. Benchmark plan

Example `crates/mct-mcp-server/examples/review_overhead.rs`, run with
`cargo run --release -p mct-mcp-server --example review_overhead -- --root <tmp> --rows <n> --iterations 50`.

- Dataset: a temporary copy of this repository at the base commit, indexed by
  `mct-cli --root <tmp> init`. It currently gives 998 indexed files and 192
  dead-code candidates; record the actual counts in the report. Synthetic
  stores (deterministic seed) of 0, 1,000, 10,000 and 100,000 rows, about 10%
  matching real candidates, in each of `C3`, `A2` and `B2`, and one run with all
  three present.
- Measure: `find_dead_code` p50/p95 for `reviewed` omitted, `show` and `hide`;
  source parse time; web save latency including the invariant check; peak RSS
  (`/usr/bin/time -l` on macOS, `-v` on Linux); response bytes and estimated
  tokens with the project's `EstimatedCounter`
  (`crates/mct-index/src/context_budget.rs`); the token size of the
  `find_dead_code` tool schema before and after (D18); false suppression =
  suppressed findings whose fingerprint does not match after scripted edits
  and a reindex (must be 0).
- Gates, fixed before measuring: omitted → identical bytes and p50 within run
  noise (≤ 2%); `show`/`hide` with 1,000 rows ≤ +5 ms or +10% p50; 100,000
  rows ≤ +50 ms p50 and ≤ +64 MiB RSS; one web save with 100,000 history rows
  ≤ 250 ms.
- Baseline before Phase 1 and the final run, both in
  `benchmarks/review-decisions.md`, with environment, commands, raw numbers,
  percentage change and limitations.

## 13. Implementation phases

| Phase | Files | Exit criterion | Blocked by | Rollback |
|---|---|---|---|---|
| P0 baseline | `crates/mct-mcp-server/examples/review_overhead.rs`, `benchmarks/review-decisions.md` | baseline recorded | go-ahead | delete the example |
| P1 domain | `crates/mct-index/src/reviews/*`, `crates/mct-index/src/lib.rs`, `crates/mct-index/Cargo.toml`, fixtures | §11.4 format, resolution, timestamp, tombstone, degraded, identity, policy and store tests green | D7, D11, D12, D14, D15, D16 | revert commit |
| P1b harness | `scripts/unix/review-compat.sh`, `crates/mct-cli/tests/review_compat.rs` | harness runs CR and CW tests against M0/MA (and MB per D16) | D16 | revert commit |
| P2 web | `crates/mct-cli/src/web/reviews.rs`, `crates/mct-cli/src/web.rs`, `crates/mct-cli/src/web/api/findings.rs`, `crates/mct-cli/src/web/app/{page-findings,components,i18n}.js`, `crates/mct-cli/src/web/tests.rs` | web suite green; `C3`-only writes; CW-1…CW-8 green | D9, D13 | revert commit |
| P3 MCP | `crates/mct-mcp-server/src/{server.rs,format.rs,tools.ttc}`, `crates/mct-mcp-server/tests/{tools.rs,toon_output.rs}` | default snapshot identical; filter tests green | published issue, D18 | argument removed |
| P4 CLI | `crates/mct-cli/src/main.rs`, `crates/mct-cli/src/duplicates.rs` | default outputs identical; flag tests green | D17, D18 | revert commit |
| P5 docs and final benchmark | §14 | gates met, `check.sh` green, CR matrix green | — | — |

## 14. Documentation impact

New ADR (next free number at integration, D8), partly superseding ADR-015 and
recording why ADR-018's `B2` format is read but not written. Update
`docs/01-architecture/web-dashboard.md` (today it says v1 files are upgraded),
`COMMANDS.md`, `README.md`, `rules.md` tool table (`find_dead_code` row),
`crates/mct-mcp-server/src/tools.ttc`, `CHANGELOG.md`,
`benchmarks/review-decisions.md` and `benchmarks/context-budget.md` (schema
token delta). The recovery steps of §7.8 go into `web-dashboard.md`. Website
pages only locally, never pushed.

## 15. Decisions

| Id | Decision | Status |
|---|---|---|
| D1 | canonical file `review-decisions.toon` | approved |
| D2 | `resolved` still detected with the same fingerprint is visible as `resolved-but-detected` | approved |
| D3 | optional `reviewed` on `find_dead_code`, default output unchanged | approved at design level; issue not yet published |
| D4 | CLI `--reviewed` opt-in on `dead-code` and `duplicates`; CSV column only with the flag | approved |
| D5 | degraded store: web starts read-only with a banner | approved |
| D6 | history full: reject the save, never drop rows | approved |
| D7 | dependencies (§15.1) | pending |
| D8 | ADR number: next free at integration | approved |
| D9 | the web stops writing `duplicate-reviews.toon` (§15.2) | pending |
| D10 | commit this specification locally | approved |
| D11 | equal times: less suppressing outcome first, then source rank | pending |
| D12 | logical time `max(now, t_seen + 1)` for N writes | pending |
| D13 | revision check on web saves, HTTP 409 on a stale view | pending |
| D14 | refuse all writes while any source is unreadable | pending |
| D15 | keep `review-decisions.toon.prev`; report leftover temporary files and never delete them automatically | pending |
| D16 | build MB from a read-only copy of the `feature/web-dashboard` working tree in a temporary directory, recording a source digest | pending |
| D17 | CLI `--reviewed` with a degraded store: full output, warning on stderr, exit code 0 | pending |
| D18 | "default output" means call results and command output; the tool schema and `--help` gain the optional parameter, and the token delta is recorded | pending |

### 15.1 D7: verified dependency requirements

| Need | Used by | Option | Effect on the build graph |
|---|---|---|---|
| SHA-256 for identities, fingerprints and legacy keys | MCP, CLI, web | `hmac-sha256 = "1.1.15"` in `mct-index` | none: already built for `mct-cli`, `mct-mcp-server` and `mct-obsidian` at the same version |
| decode `A2` notes; encode and decode `C3` notes | web (MCP never shows notes) | `form_urlencoded = "1.2.2"` in `mct-index` | adds `form_urlencoded` and `percent-encoding` to the `mct-mcp-server` build; both are already in the lockfile |
| temporary file for the atomic write | web | `std::fs::OpenOptions::create_new` with a unique name and a removal guard | none; `tempfile` is not needed in normal dependencies |
| temporary directories in tests | tests | `tempfile = "3.27.0"` as a dev-dependency of `mct-index` | none for release builds |

Recommendation: the four rows above. The alternative to `form_urlencoded` is to
keep notes encoded inside `mct-index` and decode them in `mct-cli`. That avoids
two small crates in the MCP build but splits the format between two crates.

### 15.2 D9: validation

- No released binary reads or writes review files (§2). Only binaries built
  from `main` between 2026-10-21 and this change are affected.
- With D9, a decision made in N is invisible to MA after a rollback. Nothing
  breaks and nothing is lost (CR-5, CR-6).
- The alternative, also writing `A2`, was rejected:
  - it converts `A1` to `A2`, which breaks M0 and MB, so it adds exactly the
    breakage this design avoids;
  - it writes two files that cannot be replaced atomically together, so a
    crash between them leaves them disagreeing;
  - `A2` cannot hold fingerprints, tombstones for findings decided in `B2`, or
    `needs-reevaluation`, so the mirror would be lossy;
  - it keeps a legacy format alive indefinitely.
- Recommendation: approve D9 as proposed.

## 16. Acceptance criteria and definition of done

1. A decision made in the web is applied identically by MCP `reviewed=hide` and
   CLI `--reviewed hide` (cross-consumer test).
2. Decisions survive `reindex --force` and index deletion (CR-15).
3. `A1`, `A2` and `B2` fixtures resolve completely, and their files are
   byte-identical after N saves (CR-1…CR-4).
4. No suppression without an exact identity and fingerprint match (tests in §9
   and §11).
5. Any unreadable source hides nothing and blocks writes (CR-13, CR-14).
6. Resolution is deterministic: the property test and CW-3 pass.
7. Simultaneous writes by old and new binaries lose no acknowledged save
   (CW-1…CW-8).
8. History is never truncated or rewritten (invariant tests, CW-6).
9. Pagination and counts are correct on filtered lists (test).
10. The consistency model in §7.9 holds (test with a write between two MCP calls).
11. Default MCP call results and CLI command output are byte-identical to the
    base (CR-11, CR-12).
12. The CR matrix passes on macOS and Linux; Windows behavior is covered by CI
    unit and integration tests.
13. §12 gates are met and recorded.
14. `scripts/unix/check.sh` is green on the integrated tree.
15. The documents in §14 are updated; the primary checkout's uncommitted work is
    untouched.
