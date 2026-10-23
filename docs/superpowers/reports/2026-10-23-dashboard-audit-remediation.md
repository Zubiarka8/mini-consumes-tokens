# Dashboard audit remediation repor

Date: 2026-10-23
Base: `e707e20f82598e40303f1b2ba4c67b8a8ed36c50` (`feature/code-page-quick-filters`)
Candidate: `codex/dashboard-audit-remediation` in `/private/tmp/mct-dashboard-remediation`
Status: changes are uncommitted; visual/browser acceptance remains outstanding.

## Repository and audit evidence

The dashboard is served by the Rust `mct-cli` crate. Its first-party JavaScrip
ES modules and CSS live in `crates/mct-cli/src/web/app`; Rust embeds those assets
and exposes synchronous JSON endpoints tested through an in-memory HTTP harness.
There is no dashboard TypeScript build or frontend bundler. The separate
`website/` tree is a Next.js/TypeScript site and was not changed. The baseline
web tests covered API shape, filtering and review storage, but not browser focus,
request cancellation, responsive geometry, or rendered contrast.

The findings derive from `/private/tmp/mct-local-dashboard-audit-2026-10-23.md`
and were matched to the candidate source before editing. The connected MCP index
described another checkout, so it was not used as evidence. Existing UI strings
and pages remain bilingual; new strings were added in both dictionaries.

## Finding status

| Finding | Status | Evidence and remaining limit |
| --- | --- | --- |
| P0.1 comparison cancellation | Fixed and verified | Separate left/right request keys, stale-generation guard, abort suppression, retained pair selectors, Back and Retry; same-file requests and cancellation/retry tests pass. |
| P0.2 duplicate file filter | Fixed and verified | The API matches every original group member, including beyond the six-member preview, while preserving group identity; Rust regression covers eight members across files, exact path/directory boundaries, query and totals. |
| P0.3 mobile symbol details | Implemented, not fully verified | Inspector moves into a named native modal at compact widths; focus enters, Escape/Close clears selection and restores the row; DOM interaction test passes. A real browser is still needed to verify native focus containment and background inertness. |
| UI-01 320px overflow | Implemented, not fully verified | Responsive grid, header, metric and wrapping changes; no real layout engine was available to verify 320/375/768/1440px geometry. |
| UI-02 mobile search width | Implemented, not fully verified | Search fills the wrapped mobile header and its input may shrink; visual measurement remains outstanding. |
| UI-03 recent activity key | Fixed and verified | Server labels include the finding key; recent links include key and `state=all`. Rust API and rendered-link contract tests pass. |
| UI-04 Clear All stale search | Fixed and verified | Search control synchronizes from URL changes and cancels delayed input on clear/navigation/unmount. Interaction test confirms input, URL and result request agree. |
| UI-05 backlink truncation | Implemented, contract verified | Link lists reveal batches of 30 with a status count and persistent Show More control; a 111-item test reaches all entries without losing button focus. Browser/screen-reader announcement remains unverified. |
| UI-06 Graph navigation highlight | Fixed and verified | Navigation is redrawn from the current view; interaction test switches Backlinks to Graph after facets have loaded. |
| UI-07 arbitrary duplicate pair | Fixed and verified | Both comparison sides have independent selectors for any distinct members; same-file and third-member selection plus retained retry pair are tested. |
| UI-08 contrast | Implemented, token-verified | Dark/light token tests check normal text, focus/control edges and status badges against WCAG thresholds. Rendered theme combinations and every component state still need browser review. Reference: [WCAG 2.2 Contrast Minimum](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html). |
| UI-09 review-file path | Fixed and verified at API/config source | `/api/meta` exposes the canonical review path and Settings reads it instead of hard-coding an obsolete filename; Rust API test asserts the path. No browser screenshot was taken. |
| UI-10 locale number formatting | Fixed and verified | Statistics now use the selected shared formatter; English and Spanish decimal/grouping formats are tested. |
| P2 graph density | Partly implemented | Large graphs display at most 25 labels (all labels remain accessible on each node); responsive side-filter drawer already exists. Panel resizing/collapse and review of permanent technical explanations were not implemented. |
| P3 graph responsiveness | Measured and improved, incomplete | Pairwise work now yields in bounded animation-frame batches, including reduced-motion mode. Measurements and limitations are below. 5,000-node behavior, browser paint/INP and reliable heap profiling remain open. |
| P3 filtering, pagination and API | Pending | Correctness regressions were added, but CPU, memory, overfetch, request latency and cache-invalidation profiling were not performed. No speculative optimization was made. |

## Changes

- `crates/mct-cli/src/web/app/core.js`: explicit request cancellation, ownership-safe
  cleanup through JSON decoding, and locale-aware number formatting.
- `page-findings.js`: independently cancellable compare requests, stale-operation
  guards, any-member pair selection, preserved retry state and usable Back action.
- `crates/mct-cli/src/web/api/findings.rs`: duplicate path and text matching agains
  all group members rather than the truncated response preview.
- `page-symbols.js`: compact-width native dialog lifecycle, focus entry/restoration,
  Escape/Close handling and stale symbol-request suppression.
- `components.js`, `page-notes.js`: debounced search synchronization/disposal,
  incremental outbound/backlink lists, and stale note/facet response handling.
- `overview.rs`, `page-dashboard.js`: finding identity in recent activity and the
  canonical review-file path in server metadata.
- `page-settings.js`, `page-stats.js`, both locale dictionaries and `app.css`:
  server-driven settings, locale numbers, accessible labels, mobile sizing and
  measured text/control color adjustments.
- `graph.js`: bounded force-calculation work per frame, including reduced motion,
  and a cap on visible labels for large graphs.
- `crates/mct-cli/src/web/tests.rs` and
  `crates/mct-cli/tests/dashboard/`: Rust API regressions, DOM interaction tests,
  contrast-token tests and a deterministic graph benchmark harness.

No production dependency was added. The dashboard test harness uses the existing
offline npm cache to install `linkedom@0.18.9` as a development-only dependency.

## Performance measurements

Environment: Node `v24.12.0`, macOS `darwin arm64`; deterministic chain graphs;
one sample per size and motion setting. The benchmark constructs SVG-shaped DOM
elements with LinkeDOM and drains queued animation callbacks synchronously. Thus
the first-frame value includes graph DOM construction and initial force work, and
the total is CPU time without real browser frame pacing or paint.

To repeat the candidate measurement, run
`MCT_GRAPH_SAMPLES=1 node --experimental-vm-modules crates/mct-cli/tests/dashboard/benchmark.mjs`.
For a baseline comparison, extract `graph.js` from the recorded base commit to a
temporary file and set `MCT_GRAPH_SOURCE` to that file for the same command.

| Nodes | Baseline first synchronous block, normal | After first block, normal | Baseline reduced-motion block | After reduced-motion first block | Total after (normal / reduced) |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 100 | 16 ms | 14 ms | 125 ms | 12 ms | 137 / 127 ms |
| 600 | 104 ms | 38 ms | 4,041 ms | 32 ms | 4,349 / 4,251 ms |
| 1,347 | 426 ms | 70 ms | 4,120 ms | 78 ms | 4,436 / 4,891 ms |

After-change runs completed all scheduled ticks. Total computation remains close
to baseline and can be slightly slower in this harness; the measured benefit is
that large calculations no longer occupy one multi-second JavaScript task. The
benchmark captures raw heap deltas, but garbage collection makes some negative
and inconsistent, so they are not treated as memory measurements. An attempted
full 5,000-node baseline did not complete within 80 seconds and was stopped; no
5,000-node before/after claim is made.

## Validation

- `node --experimental-vm-modules --test crates/mct-cli/tests/dashboard/*.test.mjs`:
  14 passed, 0 failed (12 interaction tests and 2 theme-level contrast tests
  covering text, controls, focus and status colors).
- Focused Rust dashboard tests after API changes: 23 web unit tests and one web
  CLI integration test passed.
- `cargo fmt --all -- --check`: passed after formatting the new Rust tests.
- `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/private/tmp/mct-dashboard-target cargo clippy --workspace --all-targets -- -D warnings`: passed.
- `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/private/tmp/mct-dashboard-target cargo test --workspace`:
  passed across all workspace crates. Existing compatibility, scale, corpus-repor
  and local-model tests that require external fixtures remain ignored by their
  repository-defined conditions.
- `CARGO_NET_OFFLINE=true CARGO_TARGET_DIR=/private/tmp/mct-dashboard-target cargo build --workspace`:
  passed; this compiles the production Rust binary with the embedded dashboard.
- `git diff --check`: passed. All Rust checks use the isolated candidate checkout.
- No real browser, 320px layout, native dialog, screen reader, or screen-reader
  audit was executed. Firefox/WebKit could not be started or installed in this
  environment; Chrome automation is prohibited by repository instructions.

## Remaining work

1. Complete the workspace test and production build, then rerun final status,
   formatting, frontend tests and diff checks.
2. Validate the dashboard at 320, 375, 768 and 1440 CSS pixels in a supported
   non-Chrome browser; test native dialog focus containment, Escape, background
   inertness, keyboard-only navigation, contrast and responsive overflow.
3. Profile 5,000-node graphs and real browser long tasks/INP/heap, then measure
   filtering, pagination, API overfetch and end-to-end request latency on a
   reproducible indexed fixture before proposing further optimizations.
4. Decide whether side-panel resizing and less persistent technical explanations
   solve demonstrated problems without adding interface complexity.

The candidate is deliberately uncommitted. No changes were made to the original
working checkouts, Obsidian vault, or review data.
