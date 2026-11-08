# Dashboard bundles (web UI source)

Source of the three bundles the local dashboard (`mct-cli web`) loads on demand.
The rest of the dashboard is plain ES modules in `crates/mct-cli/src/web/app/`.

| Bundle | Source | Output | Loaded when |
| --- | --- | --- | --- |
| Review workspace (React "island") for the duplicate-code and dead-code review pages | `src/` (not `src/notes/`) | `review.js`, `review.css` in `src/web/review-dist/` | a finding is opened |
| Notes renderer: Obsidian-flavoured Markdown to DOM (`marked`, `lowlight`, Floating UI) | `src/notes/` | `notes.js` in `src/web/notes-dist/` | the first note is opened |
| Large-vault graph in WebGL (`sigma`, `graphology`, ForceAtlas2, Louvain communities) with display and force sliders | `src/graph/` | `graph.js` in `src/web/graph-dist/` (served as `graph-gl.js`) | the notes graph has more than 250 notes |

## Build output is committed

`npm run build` writes all outputs. `src/web/assets.rs` embeds them with
`include_str!`, so building the Rust binary does not need Node. Rebuild and
commit `review-dist/`, `notes-dist/` and `graph-dist/` whenever their source or the pinned
dependencies change; CI rebuilds them and fails if the committed files differ.

## Commands

Run from this directory after `npm ci`:

| Command | What it does |
| --- | --- |
| `npm run build` | Type-check (`tsc --noEmit`), then build every bundle. |
| `npm run build:review` | Type-check, then the review bundle only (`vite.config.ts`). |
| `npm run build:notes` | Type-check, then the notes bundle only (`vite.notes.config.ts`). |
| `npm run build:graph` | Type-check, then the graph bundle only (`vite.graph.config.ts`). |
| `npm test` | Run the Vitest suite (`*.test.ts`/`*.test.tsx`, jsdom). |

The page's side of the contract with each bundle is tested in
`../tests/dashboard` (`npm test` there): `review.js` runs against a stub, and
`notes.js` runs for real, so those tests exercise the committed file.

## Constraints

- The dashboard serves the bundles same-origin with a strict CSP
  (`default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self'
  data:; font-src 'self'`): no remote scripts, fonts or styles, no injected
  `<style>` elements and no `style` attributes. `vite.config.ts` strips the
  stylesheet injection that `sonner` adds for this reason. Libraries that
  write inline styles (KaTeX, Mermaid) cannot be used.
- The notes renderer never builds an HTML string: it turns `marked` tokens and
  lowlight's syntax tree into DOM nodes, so raw HTML in a note is shown as
  text. Keep it that way; see
  [ADR-033](../../../docs/01-architecture/adrs/033-notes-markdown-renderer-bundle.md).
- Tailwind (review bundle) ignores `src/notes/`, `src/graph/` and this README; their strings are not utilities.
- The graph draws on a canvas and sets styles through classes or CSSOM only. It never runs ForceAtlas2 in a worker (a `blob:` worker would need a CSP exception): the layout runs in short slices on the main thread. The SVG graph in `app/graph.js` stays for small graphs and as the fallback when this bundle fails to load.
- Review decisions are stored by the Rust side; see
  [ADR-019](../../../docs/01-architecture/adrs/019-shared-review-decisions.md)
  and [web-dashboard.md](../../../docs/01-architecture/web-dashboard.md).
