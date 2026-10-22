# Web dashboard (`mct-cli web`)

Specification and design notes for the local dashboard. It supersedes the
server-rendered pages described in [ADR-014](adrs/014-local-project-web.md);
the security model of that ADR (loopback only, private URL key, Host/Origin
checks, strict CSP) is unchanged.

## Goals and non-goals

- A professional, local-first dashboard over the existing index: Dashboard,
  indexed code, duplicates (with side-by-side compare), dead code, search,
  dependencies, reviews, statistics, Obsidian notes, unified workspace, settings.
- Spanish (Spain) UI with all strings in one table (`app/i18n.js`) so other
  locales can be added without touching components.
- Never invent data. Anything the index does not record is marked unavailable
  through `meta.capabilities` (history, semantic search, call graph, reindex,
  tokens, similarity, confidence, severity, reviewers are all `false`).
- Non-goals: editing source, reindexing from the browser, remote access,
  third-party scripts, fonts or images.

## Architecture

- `web.rs` builds an immutable in-memory `Site` snapshot at startup (navigation,
  duplicate report, dead-code candidates, reference counts, Obsidian scan) and
  serves it over a single-threaded HTTP/1.1 loop on `127.0.0.1`.
- `web/api/*` exposes read-only JSON endpoints: `meta`, `summary`, `stats`,
  `files`, `symbols`, `symbol`, `source`, `findings`, `finding`, `search`,
  `dependencies`, `notes`, `note`, `notes-graph`. Lists page with a bounded
  `limit`; source ranges are bounded and validated against the indexed file.
- `POST api/review` is the only mutation (JSON, CSRF nonce, same-origin).
  `web/reviews.rs` stores state, note and append-only history in
  `.mct-index/duplicate-reviews.toon` (v2; v1 files are upgraded). Finding keys
  are HMAC-SHA256 over path, name, kind, lines and file content hash, so a
  decision follows the code, not a list position.
- `web/app/*` is an ES-module front end embedded with `include_str!`: hash
  routing (`#/page?query`, filters live in the URL), a DOM builder that never
  uses `innerHTML`, inline SVG charts, a bounded force-directed graph, an LCS
  line diff and a safe Markdown renderer.

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

## Known limits

Single snapshot per process (restart after `reindex`); review history is local
to the project; keyword search only; the graph shows verified import edges only.
