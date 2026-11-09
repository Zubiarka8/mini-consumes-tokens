# Web dashboard (`mct-cli web`)

Specification and design notes for the local dashboard. It supersedes the
server-rendered pages described in [ADR-014](adrs/014-local-project-web.md);
the current server uses loopback binding, Host/Origin checks, CSRF protection
and a strict CSP. Its localhost URL does not contain a private key.

## Goals and non-goals

- A professional, local-first dashboard over the existing index: Dashboard,
  indexed code, duplicates (with side-by-side compare), dead code, search,
  dependencies, reviews, statistics, Obsidian notes, unified workspace, settings.
- UI in English and Spanish (Spain) with all strings in one table (`app/i18n.js`) so other
  locales can be added without touching components. The language and the theme
  (dark, light or system) are remembered. Defaults: theme `system`; English,
  unless the browser's language settings list Spanish first. Each project has
  its own port and `localStorage` is per origin, so both choices are also
  mirrored in one cookie (`app/prefs.js`, cookies ignore the port) that a new
  project's dashboard reads when it has nothing saved.
- Background use is set in Settings and shared between projects like the theme
  (`app/power.js`): the time without use after which the server closes itself
  (default 30 minutes; 5 to 180 minutes, a custom value up to 24 hours, or never;
  `POST api/idle`, which also applies it to the running server) and the time without
  input after which the page stops its live-update checks (default 5 minutes). Only
  a person's input (at most one request a minute) or a normal request counts as use:
  the page's own check, `api/meta?passive=1`, does not, and nothing is requested
  while the tab is hidden or paused.
- Never invent data. Anything the index does not record is marked unavailable
  through `meta.capabilities` (history, semantic search, call graph, reindex,
  tokens, similarity, confidence, severity, reviewers are all `false`).
- Non-goals: editing source, reindexing from the browser, remote access,
  third-party scripts, fonts or images.

## Architecture

- `web.rs` builds an in-memory `Site` snapshot at startup and reloads it on a subsequent request after a published reindex (navigation,
  duplicate report, dead-code candidates, reference counts, Obsidian scan) and
  serves it over a single-threaded HTTP/1.1 loop on `127.0.0.1`.
- `web/api/*` exposes read-only JSON endpoints: `meta`, `summary`, `stats`,
  `files`, `symbols`, `symbol`, `source`, `findings`, `finding`, `search`,
  `dependencies`, `notes`, `note`, `notes-graph`. Lists page with a bounded
  `limit`; source ranges are bounded and validated against the indexed file.
  `notes-graph` sends each edge as `[from, to]` positions in its `nodes`
  array ([ADR-032](adrs/032-notes-graph-edges-as-positions.md)).
- `POST api/review` saves review decisions (JSON, CSRF nonce, same-origin, and
  the `revision` the page showed: a decision changed meanwhile by another
  process returns 409 with the current review). `web/reviews.rs` is a thin
  adapter over `mct_index::reviews`, shared with `mct-cli dead-code`/
  `duplicates --reviewed`. It writes only `.mct-index/review-decisions.toon`
  and still applies decisions from `duplicate-reviews.toon` (v1 and v2) and the
  earlier `reviews.toon`, which it never rewrites. Finding keys are an identity
  that survives edits plus a fingerprint of the exact code: changed code shows
  as `needs-reevaluation`, never hidden. If a review file cannot be read the
  dashboard starts read-only, hides nothing and names the file; to recover, fix
  or move that file aside (or restore `review-decisions.toon.prev`), then
  restart. Design: `docs/design/2026-10-22-shared-review-decisions-design.md`.
  Compatibility, stated separately: default outputs of older binaries are
  unchanged (identical bytes); this dashboard reads the older review files but
  writes only `review-decisions.toon`, so binaries built before this change do
  not see its decisions, and it does not see decisions they save later
  (one-way reading, not bidirectional interoperability). A writer killed during
  a save does not block the next one: the lock is an operating-system lock, and
  the next save removes the temporary files the killed save left
  (`docs/01-architecture/adrs/019-shared-review-decisions.md`).
- `POST api/ignore` updates project ignore rules with same-origin and CSRF checks.
- `web/app/*` is an ES-module front end embedded with `include_str!`: hash
  routing (`#/page?query`, filters live in the URL), a DOM builder that never
  uses `innerHTML`, inline SVG charts, a bounded force-directed graph, an LCS
  line diff. Three bundles are built by `crates/mct-cli/web-ui` and committed:
  the review workspace (`review-dist/`), the notes Markdown renderer
  (`notes-dist/notes.js`: Obsidian syntax, highlighted code, wikilink previews,
  built from DOM nodes only, [ADR-033](adrs/033-notes-markdown-renderer-bundle.md))
  and the WebGL notes graph for large vaults (`graph-dist/graph.js`: all notes
  at once, communities coloured, display and force sliders; small graphs keep
  the SVG one).

## Duplicate row contract

`GET /api/findings?type=duplicate` pages **groups**, using `page` and `size`
(the maximum size is 200). A row retains its stable review `key` and readable
`number`; `rank`/`group` are positions in the current snapshot, not persistent
identities. Filtering matches every member before group pagination.

- `memberCount` counts all function/method locations, including several in one
  file. `fileCount` counts distinct repository-relative paths across the entire
  group. Clients use these server counts, never the preview's unique paths.
- `members` is an ordered preview of at most six `{path, start, end, name, kind}`
  entries. Ranges are 1-based and inclusive. Additive `membersOmitted` is the
  number not included in this response. Older clients can still derive it from
  `memberCount - members.length`.
- `title` is a display summary of the first six member identifiers, followed by
  `(+N)` when further locations exist. It is never a review identity. This fixes
  the previous silent two-name cutoff; explicit title sorting can consequently
  produce a different order. Recent activity shares the same title formatter.
- The list renders identifier words in English (`find_calls_bfs_scoped` →
  `Find calls BFS scoped`), independent of the selected UI language. Repeated
  identifiers share a label, while counts and the locations panel retain every
  occurrence. Original identifiers remain in tooltips, API data, source links
  and AI templates. This is presentation, not symbol renaming.
- `scope` retains its existing values: `same-file` means one distinct path;
  `cross-file` means several paths below the same first path component;
  `cross-module` means different first components. These are directory scopes,
  not Rust/JS module analysis. Root-level files share the empty directory
  component (correcting their previous classification as different modules).
  The UI labels are Same file, Several files and Several folders (localized).
- `GET /api/finding?key=...` returns **all members** with `membersOmitted: 0`.
  Its title remains a bounded summary. The copy-for-AI action fetches this detail
  before formatting; copying a list explicitly identifies preview omissions.
  Detail bodies retain the existing 1,500-distinct-line budget and per-read
  400-line cap. Metadata and repeated bodies still scale with group size: the
  detail endpoint has no total byte cap or member pagination. This existing
  limit is not solved by the bounded list preview.

No published MCP signature, detector normalization, membership, ranking,
review fingerprint or report-cache version changes in this correction.

## Acceptance criteria

1. Every page works with keyboard only; focus is visible; dialogs trap focus.
2. Dark and light themes meet WCAG AA text contrast; `prefers-color-scheme`
   and `prefers-reduced-motion` are honoured.
3. Layout works from 390 px to wide desktop; tables scroll inside their card,
   the sidebar and filters become drawers below 1025 px.
4. The CSP stays `default-src 'none'; script-src 'self'; style-src 'self'`.
5. A failed review save leaves state unchanged and reports an error.
6. No fabricated metric: unsupported features render an explicit unavailable
   state.

## Testing

- `cargo test -p mct-cli web` (wire-level tests, no sockets): assets, API
  shape and bounds, source safety, HTTP security, review persistence/CSRF.
- Browser checks with headless Chrome for Testing: each page in dark/light at
  1440, 1024 and 390 px, command palette, compare view.
- Review checks in a real browser, on scratch projects only: a finding whose
  stored content hash is empty is never hidden by a decision saved from the
  page, and a decision written by the older B2 binary is shown as decided, or
  as `needs-reevaluation` once the code has changed. Results are in
  `benchmarks/review-decisions.md`.

## Known limits

The snapshot reloads on the next request after a `reindex` (decisions and the page's CSRF token are kept); review history is local
to the project; keyword search only; the graph shows verified import edges only.
