# Dashboard architectures: legacy `feature/web-dashboard` against `origin/main`

Status: comparison for a decision. Nothing from the legacy dashboard is merged
into `integration/shared-reviews-web`, and nothing here changes either
implementation.

## Scope and evidence

- **Legacy**: `feature/web-dashboard`, commits `e642f39`..`f1fec4c`, on base
  `a52c3e4`. Preserved unchanged from uncommitted work. Specification:
  ADR-018 (`docs/01-architecture/adrs/018-web-dashboard.md` on that branch).
- **Current**: the dashboard in `origin/main` (`e68aec9`, PRs #226 and #227),
  as carried by this branch together with shared review decisions (ADR-019).
  Specification: `docs/01-architecture/web-dashboard.md`.
- `a52c3e4` is an ancestor of `origin/main` (14 commits behind it), and
  `origin/main` is an ancestor of this branch. The legacy branch therefore
  predates the redesign and was never rebased onto it.
- Observations marked **V** were read in code or measured in this session.
  Marked **A** are assumptions or intent that was not verified.

## Architecture

| Area | Legacy | Current |
|---|---|---|
| Rust layout (V) | `web.rs` plus per-view modules `view_*.rs`, `render.rs`, `i18n.rs`, `i18n/catalog.rs`, `markdown.rs`, `notes.rs`, `source.rs`, `stats.rs`; about 7,700 lines with the assets | `web.rs`, `web/api/{code,discovery,findings,notes,overview}.rs`, `assets.rs`, `response.rs`; about 6,400 lines with the assets |
| Rendering (V) | Server-rendered HTML pages; one first-party `app.js` (145 lines) only enhances | Static shell plus ES modules in `web/app/*` that call a JSON API |
| Works without JavaScript (V, claim in ADR-018) | Yes, each page is HTML | No, the shell needs the modules |
| GET routes (V) | `duplicados`, `duplicado`, `duplicados.toon`, `muerto`, `simbolo`, `obsidian`, `explorar`, `archivo`, `notas`, `nota`, `grafo`, `revisiones`, `estado`, `ajustes`, `style.css`, `app.js`; redirects `atlas`, `file`, `duplicates`, `group` | Hash routes in the client: dashboard, symbols, search, dependencies, findings, notes, workspace, statistics, settings |
| API (V) | None; data is in the HTML | 14 read-only JSON endpoints (`meta`, `summary`, `stats`, `files`, `symbols`, `symbol`, `source`, `findings`, `finding`, `search`, `dependencies`, `notes`, `note`, `notes-graph`) plus `POST api/review` |
| Mutations (V) | `POST review`, `POST reload`, `POST lang`, form posts with a CSRF field | `POST api/review` only, JSON, CSRF nonce, same-origin, `revision` |
| Security headers (V) | CSP `default-src 'none'; style-src 'self'; script-src 'self'; connect-src 'self'; img-src 'none'; form-action 'self'`; Host and Origin checks; private URL key | CSP `default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; font-src 'self'; form-action 'none'`; same Host and Origin checks and URL key |
| Tests (V) | 46 tests under `cargo test -p mct-cli web`, all passing (exit 0) | 19 tests under the same filter, all passing |
| Internationalization (V) | Spanish and English; `mct_lang` cookie, then `Accept-Language`, then Spanish | Spanish only; one dictionary (`app/i18n.js`) built for more locales |
| Performance (A) | Not measured | Not measured. Both load a snapshot at startup. |

## Review decisions

| Area | Legacy | Current (with ADR-019) |
|---|---|---|
| Store (V) | `reviews.toon` version 2, read and written; `duplicate-reviews.toon` v1 read | `review-decisions.toon` v3 written; v1 and v2 `duplicate-reviews.toon` and `reviews.toon` read, never rewritten |
| Concurrency (V) | `reviews.lock` created and removed by the writer; a test confirms a leftover lock file refuses a save | OS exclusive lock on `review-decisions.lock`, released by the kernel, never removed |
| Stale view (V) | No revision, no HTTP 409 (0 matches in the source) | `revision`; a changed decision returns 409 with the current review |
| Time (V) | Not checked | Unix ms; deterministic resolution |
| Empty hash (V) | Not checked in the legacy code | Never hides a finding (`cccb522`) |
| Corrupt store (V) | Not checked | Read-only dashboard, nothing hidden |
| Changed code (V) | `NeedsReevaluation` state | `needs-reevaluation`, also shared with CLI |

The legacy store format `reviews.toon` v2 is the B2 format that the current
resolver reads as read-only legacy. Decisions saved by the legacy dashboard stay
visible; the reverse is not true.

## Features

| Feature | Legacy evidence | Current evidence | Recommendation | Effort | Risk |
|---|---|---|---|---|---|
| Duplicates list and detail | `view_duplicates.rs` (631 lines), `duplicados`, `duplicado` | `page-findings.js`, `api/findings`, compare view (9 matches for "compare") | Keep current | none | none |
| Dead-code list and detail | `view_dead.rs`, `muerto`, `simbolo` | `page-findings.js`, `api/finding` | Keep current | none | none |
| Review actions and history | `view_reviews.rs`, `revisiones` | `page-findings.js` actions, `reviews.rs` adapter, 409 | Keep current | none | none |
| Obsidian notes, viewer, graph | `view_obsidian.rs` (998 lines), `notas`, `nota`, `grafo` | `page-notes.js`, `api/notes`, `notes-graph`, `obsidian.rs` | Keep current; compare note rendering | low | low |
| Indexed files and source | `explorar`, `archivo`, `source.rs` | `api/files`, `api/source`, `page-symbols.js` | Keep current | none | none |
| Statistics | `stats.rs`, real per-language counts | `api/stats`, `page-stats.js` | Keep current | none | none |
| Symbols, search, dependencies, workspace, command palette, charts | Not present | `page-symbols.js`, `page-search.js`, `page-deps.js`, `page-workspace.js`, `palette.js`, `charts.js` | Current only | none | none |
| English interface | `i18n.rs` catalog, 573 lines of strings | Not present | Migrate: add an `en` dictionary to `app/i18n.js`; reuse the legacy strings as source | medium | low; strings only |
| "Recargar análisis" (reload the snapshot) | `POST reload`, `estado` page | Absent; settings state `reindex` as unsupported | Decide first; the current model is a startup snapshot | medium | medium; it replaces a shared `Site` |
| Works without JavaScript | Server-rendered pages | Not supported | Do not migrate; it would duplicate the rendering path | high | high |
| Old URL redirects (`atlas`, `file`, `duplicates`) | Present | Not verified | Check whether released versions served those URLs before deciding | low | low |
| `duplicados.toon` download | Route present | Not present | Decide; an API endpoint could expose it | low | low |
| Real-repository detail benchmark | `benchmarks/real-repos.md`, `real_repos` example | Not present | Keep it on the legacy branch; it does not depend on the dashboard | none | none |

## Incompatibilities

- Both edit `web.rs`, `http.rs`, `reviews.rs` and `tests.rs`, and the legacy
  branch rewrites `render.rs` and `style.css` where the current one has
  `api/*` and `app/*`; a textual merge conflicts in all of them (A: not
  attempted, so the exact conflict list is unverified).
- The two use different routes, different JSON or HTML contracts and different
  review semantics, so the two cannot be served at once.
- Legacy CSP forbids images and allows form posts; the current one allows
  `data:` images and forbids forms.
- ADR numbering: ADR-017 and ADR-018 exist only on the legacy branch; this
  branch has ADR-019. The legacy ADRs would need a decision before merging.

## Features lost by discarding either side

- Discarding legacy: the English interface, the reload action, the
  no-JavaScript pages and the `duplicados.toon` route, plus the 46 tests that
  cover them. The real-repository benchmark is independent and survives.
- Discarding current: the symbols, search, dependency, workspace and statistics
  pages, the command palette, the ES-module front end, shared review decisions
  with revision and OS lock, and the 19 tests that cover them.

## Recommendation

Keep the current modular architecture. No concrete reason was found to prefer
the legacy one. Migrate selectively, not by merging the legacy files:

1. English dictionary for `app/i18n.js` (the cheapest and clearest gain).
2. Decide whether a reload action belongs in the current model (A: the legacy
   behavior replaces the whole snapshot while keeping the CSRF token).
3. Decide on the `duplicados.toon` route.

Open questions (assumptions not verified): whether users rely on the
no-JavaScript pages; whether the legacy performance is better or worse; whether
the legacy tests cover behavior the current tests do not. A test-by-test
mapping of the 46 legacy tests was not done.
