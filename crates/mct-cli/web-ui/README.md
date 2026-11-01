# Review workspace (web UI source)

Source of the React "island" used by the local dashboard (`mct-cli web`) for the
duplicate-code and dead-code review pages. The rest of the dashboard is plain ES
modules in `crates/mct-cli/src/web/app/`.

## Build output is committed

`npm run build` writes `review.js` and `review.css` to
`crates/mct-cli/src/web/review-dist/`. `src/web/assets.rs` embeds them with
`include_str!`, so building the Rust binary does not need Node. Rebuild and
commit `review-dist/` whenever this source changes.

## Commands

Run from this directory after `npm ci`:

| Command | What it does |
| --- | --- |
| `npm run build` | Type-check (`tsc --noEmit`), then bundle with Vite. |
| `npm test` | Run the Vitest suite (`*.test.tsx`, jsdom). |

## Constraints

- The dashboard serves the bundle same-origin with a strict CSP
  (`script-src 'self'; style-src 'self'`): no remote fonts, scripts or styles,
  and no injected `<style>` elements. `vite.config.ts` strips the stylesheet
  injection that `sonner` adds for this reason.
- Review decisions are stored by the Rust side; see
  [ADR-019](../../../docs/01-architecture/adrs/019-shared-review-decisions.md)
  and [web-dashboard.md](../../../docs/01-architecture/web-dashboard.md).
