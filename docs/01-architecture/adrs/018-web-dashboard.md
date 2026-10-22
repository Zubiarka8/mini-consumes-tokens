# ADR-018: Local web dashboard with unified finding reviews

## Status

Implemented on `feature/web-dashboard` (2026-10-21), not yet merged; browser rendering is unverified (see Implementation notes). Builds on ADR-014 (local web) and ADR-015 (duplicate review decisions). Milestones are listed below; each ends with tests green before the next starts.

## 1. Current verified state

Read from the code on `feature/web-dashboard` (branched from `feature/obsidian-index`):

| Area | State | Where |
|---|---|---|
| HTTP server | Implemented. `std::net` loopback only, random capability path, Host/Origin/Fetch-Metadata checks, one request per connection, 2 s deadline, 16 KiB headers, 4 KiB forms. CSP `default-src 'none'; style-src 'self'`: no script runs. | `crates/mct-cli/src/web/http.rs` |
| Views | Implemented, English, server-rendered: overview, atlas (dir/file/symbol/import browsing, 100 per page), duplicates (20 per page), group detail, `duplicates.toon`. Nav: Overview, Atlas, Duplicates. | `web/render.rs`, `web/style.css` |
| Snapshot | Immutable at startup (`Site::load`); refresh = restart. | `web.rs` |
| Duplicate detection | Implemented (ADR-007..012): groups, aligned evidence, verdicts, subgroups, priority. Reused as is. | `mct-index/src/duplicates*` |
| Duplicate reviews | Implemented, two states (`false-positive`, `dismissed`), file `.mct-index/duplicate-reviews.toon` v1, key = SHA-256 over normalization, language and members (path, name, kind, lines, **file content hash**). Atomic write, per-project lock, symlink/hard-link guards. A changed file yields a different key, so the old decision is **silently orphaned** (kept in the file, never shown). | `web/reviews.rs` |
| Dead code | Detector implemented (`find_dead_code_candidates`: allowlisted kinds, no `main`, no test-like names, **zero references by name anywhere**). Heuristic; no proof of unreachability. Exposed in MCP and CLI, **not in the web**. | `mct-index/src/dead_code.rs` |
| Obsidian | Vault written to `.mct-index/obsidian/` at startup (ADR-013); the web only links to the folder. No note viewer. | `mct-obsidian` |
| Statistics | Only the file count. `IndexStatus` has per-language file/symbol counts; `NavigationSnapshot` has files, symbols (kind, language, parent) and import edges. | `mct-index` |
| Index | SQLite, incremental by content hash, schema migrations to 11. Not touched by this ADR. | `mct-index` |

Technical debt: review key couples identity with content; English-only UI; no JS, so no client-side filtering; `render.rs` builds HTML with `format!` strings.

## 2. Problem

The web is hard to read, shows only duplicates, offers no dead-code or per-language view, cannot open notes, and a review decision is lost from view when the code changes, with no way to tell the user.

## 3. Decisions

### D1. Stay server-rendered; add one tiny first-party script
No framework, no Node, no new dependency. Pages remain HTML produced in Rust. The CSP gains `script-src 'self'` and `/app.js` (embedded with `include_str!`, like `style.css`) for progressive enhancement only: live filtering of an already-paginated table, keyboard shortcuts, the note graph canvas, and POSTing review decisions without a reload. Every page and action keeps working with scripts disabled. No inline scripts, no `eval`, no external origin.

### D2. Navigation
Persistent header, Spanish labels, order fixed: **Código duplicado** (default `/`), **Código muerto**, **Obsidian**, **Revisiones**, plus **Estado** (index freshness, counts, warnings) and **Ajustes** (read-only view of exclusions and paths). The old routes `atlas`, `duplicates`, `group`, `file` stay as redirects or keep working so saved links do not break.

### D3. One review model for both finding kinds
Statuses: `pending` (derived: no record), `confirmed`, `false_positive`, `ignored`, `needs_reevaluation` (derived: a record exists but its fingerprint no longer matches). False positive (a detector error) and ignored (a consciously accepted issue) are different values everywhere: store, filters, counts, wording.

Each finding has two hashes:
- **Identity** (stable across edits): duplicates = SHA-256 over kind, language and the sorted members' (path, symbol name, symbol kind); dead code = kind, language, path, symbol kind, parent, name. No line numbers, no content.
- **Fingerprint** (detects change): the existing ADR-015 key for duplicates (members with lines and file content hashes); for dead code, the file content hash plus the symbol's line range.

A record whose identity matches a current finding but whose fingerprint differs is shown as `needs_reevaluation`, never hidden and never auto-resolved.

### D4. Persistence without damaging existing data
- `.mct-index/duplicate-reviews.toon` (v1) is **never rewritten and never deleted**. It is read as legacy: a current duplicate group whose ADR-015 key is listed is `false_positive` / `ignored` with the recorded reason, mapped 1:1 (`false-positive` → `false_positive`, `dismissed` → `ignored`). It matches by fingerprint only (that is all v1 stored), so v1 decisions never produce `needs_reevaluation`; the first time the user changes them they are written to the new file.
- New `.mct-index/reviews.toon`, version 2, same TOON table style, same lock + temp file + atomic rename + symlink/hard-link guards + 8 MiB / 100,000 row bounds as ADR-015. Two tables: `reviews[n]{kind,identity,fingerprint,status,at,reason}` (current decision) and `history[n]{kind,identity,fingerprint,status,at,reason}` (append-only, bounded; the oldest rows are dropped only when the bound is hit, and that is reported). Timestamps are Unix seconds, written by the server, never taken from the request. `reason` is free text, max 500 bytes, TOON-escaped (no newline, no comma ambiguity).
- Restore = a new history row with status `pending` and removal of the current row; nothing is erased from history.
- Persistence survives refresh and server restart (file), and reindex (identity does not depend on the index).
- `.mct-index` is not in Git by default; the files are local preferences, as in ADR-015.

### D5. Reuse of the engines
Duplicates: `find_duplicate_groups` with the default policy, as today. Dead code: `find_dead_code_candidates`, called once at startup on the same read-only `Index`. Statistics: `NavigationSnapshot` + `IndexStatus` data, computed once in a `Stats` struct (no extra SQL per request). Note content: read from the generated vault only through a whitelist of relative `.md` names enumerated at startup; a request names an entry of that list, never a path.

### D6. Honest wording and no invented numbers
- Dead code is labelled **"Posiblemente sin uso"**. The detector looks for references by name in the index; the UI states this and lists known blind spots: reflection, dynamic imports, generated code, framework entry points (routes, handlers, decorators), exports used by other repositories, cross-language references. No "proven unreachable" category is shown because no detector produces one; the filter exists only if one ever does.
- Duplicates show only data the detector computes (token and line counts, verdict level and reasons, similarity statistics from the evidence). No confidence percentage is shown unless the report carries one.
- Language counts: **file** = indexed non-asset file, counted under its `language`. **Function** = symbol kind `function` with no parent; **method** = kind `method`, or `function` whose parent is a class-like symbol. **Class** = `class`; **other** = `struct`, `enum`, `trait`, `interface`, `type_alias`. Remaining kinds are counted in an `Otros` column. Mixed-language files (for example HTML with CSS) count under the language the indexer assigned. Nested directories count every descendant. The definition is shown in the UI next to the table.
- Excluded/unsupported: only what the index reports (`unsupported_languages`, `.mctignore` exclusions that the navigation snapshot filters). Total files on disk is not measured, so it is labelled "No medido" unless the walk is cheap and exact.

### D7. Security
Unchanged guarantees (loopback, capability path, Origin/CSRF/Host, bounds, no path from request to disk). New: POST body bound stays 4 KiB (reason ≤ 500 bytes fits); POST only to `review`; `app.js` and `style.css` are static strings; all dynamic text is HTML-escaped and JSON for the graph is embedded in a `data-` attribute and parsed with `JSON.parse`, never concatenated into script; Markdown is rendered by a small Rust renderer that escapes everything and only emits links whose target is a listed note or an `http(s)` URL with `rel="noreferrer noopener"`; raw HTML in notes is escaped.

### D9. Interface languages (Spanish and English)
All interface text goes through `web/i18n.rs`: the Spanish text is the key and `web/i18n/catalog.rs` holds the English text. The language is chosen per browser: the `mct_lang` cookie (set by the ES/EN buttons through `POST lang`, same CSRF/Origin rules, `SameSite=Strict; HttpOnly`, scoped to the secret path), then the first language of `Accept-Language` (`en*` gives English), then Spanish. No server-side preference. A test fails if a text passed to `t`/`tf` has no English entry, if an entry is unused or loses a placeholder or markup, and a crawl of every page in English fails on any remaining accented Spanish character. Out of scope: data the project supplies (paths, symbol names, notes' own text), the detector's raw TOON report, terminal messages and the CLI. Adding a language means a new `Lang` variant and a second column in the catalog.

### D8. Not in this ADR
Automatic live reload when the index changes (explicit "Recargar análisis" action instead, see M2), editing sources, new MCP tools, schema migrations, authentication, non-loopback binding.

## 4. Interaction flows
1. Open the URL: **Código duplicado**, pending groups ranked by existing priority, filters (language, directory, verdict, status), search, page of 20. Choose a group: two fragments side by side with the aligned differences the evidence already computes; buttons **Revisado / Falso positivo / Ignorar** with optional reason; **Restaurar** on decided groups.
2. **Código muerto**: table (symbol, kind, language, location, reference count by name = 0, reason), filters, source preview (bounded), decisions as above.
3. **Obsidian**: stats cards → per-language table → every number links to a filtered list; directory tree with search; file page (symbols, imports, importers, preview); note explorer, rendered note, backlinks, graph.
4. **Revisiones**: all decisions of both kinds, filter by status/kind, history per finding, restore, and the `needs_reevaluation` queue.

## 5. Acceptance criteria
See the test list in §6; the milestone is done when its tests pass with real indexed fixtures, `cargo fmt --check`, `cargo clippy --workspace --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic` and `cargo test --workspace` are green, and the manual checklist (§7) is recorded as verified or as unverified.

## 6. Tests
Unit/integration (existing in-memory transport in `web/tests.rs`): stats counts equal a hand-computed fixture; clicking-through URLs return the filtered lists; pagination and filter bounds; review lifecycle for both kinds across a simulated restart (new `Site` over the same files); reindex stability (identity unchanged, fingerprint changed → `needs_reevaluation`); legacy v1 file read and byte-identical afterwards; concurrent writers; corrupted v2 file fails closed without touching v1; Markdown rendering escapes script/HTML and rejects non-listed targets; every new route is rejected without the token, with a wrong Host, with a cross-site Origin; traversal strings (`../`, absolute, encoded) rejected; CSP header contains no `unsafe-*`.

## 7. Milestones
- **M1 Foundation + persistence first**: shell, Spanish navigation, CSS, `app.js` scaffold, `reviews v2` store with legacy read, `Estado`. (Persistence is moved here because M2/M3 depend on it.)
- **M2 Código duplicado**: filters, side-by-side view, five actions, reload-analysis action.
- **M3 Código muerto**: table, preview, reasons, decisions.
- **M4 Obsidian**: `Stats`, language table, tree, file page, notes, graph.
- **M5 Revisiones**: central page, history, re-evaluation queue, docs/CHANGELOG/README.

Manual browser verification (rendering, dark mode contrast, keyboard use) is done with a real browser at the end and reported separately from automated tests.

## Implementation notes (as built)
- All five milestones landed together in `crates/mct-cli/src/web/` (`reviews.rs`, `stats.rs`, `source.rs`, `notes.rs`, `markdown.rs`, `view_*.rs`, `style.css`, `app.js`). No change to `mct-index`, the schema, the CLI subcommands or the MCP tools.
- Restore writes a `pending` row (kept in history) instead of deleting the current row, so a restored ADR-015 decision stays hidden without touching the old file.
- Source previews and code panes read the *current* file (bounded to 400 lines, symlinks refused); symbols, counts and findings come from the startup snapshot. After editing code, a preview can therefore be newer than the lists; the Estado page warns when the index files changed and "Recargar análisis" rebuilds the snapshot.
- Dead-code identity includes the order of same-named symbols in a file; its fingerprint is the file's content hash plus the symbol's line range, so any edit to the file asks for re-evaluation (conservative, as in ADR-015).
- The side-by-side view marks the lines the detector lists as differing (at most 12 distinct differences per group); it does not compute its own diff.

## Not verified
- Rendering, dark/light contrast, keyboard use and the canvas graph were **not** checked in a browser (the available browser blocks 127.0.0.1). Pages were exercised over HTTP only (status, timing, content, links).
