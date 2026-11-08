# ADR-033: Notes Markdown renderer as a built bundle

## Status
Accepted (2026-11-08).

## Context
- The notes page rendered note bodies with `app/markdown.js`, a 169-line renderer written for the generated code atlas. Fed 22 samples of Obsidian and GFM syntax, it rendered 19 wrongly: nested lists were flattened, task lists, strikethrough, setext headings, `~~~` fences, hard breaks, footnotes, callouts and `==highlights==` stayed literal text, `![[embed]]` drew a stray `!`, `%%comments%%` (private in Obsidian) were shown, and `[x](javascript:alert(1))` rendered as `x)`.
- The dashboard's CSP is `default-src 'none'; script-src 'self'; style-src 'self'; connect-src 'self'; img-src 'self' data:; font-src 'self'`: no CDN, no inline script, and no `style` attribute or injected `<style>` element.
- The shell has no build step and the Rust build must not need Node. The review workspace already solves this with a bundle built by `crates/mct-cli/web-ui` and committed (`review-dist/`).
- The renderer builds DOM nodes and never assigns an HTML string, so raw HTML in a note is shown as text. That property is worth keeping.

## Decision
- `web-ui/src/notes/` builds `src/web/notes-dist/notes.js` (`npm run build:notes`, also part of `npm run build`) with its own `vite.notes.config.ts`. `assets.rs` embeds it as `js/notes.js`; `markdown.js` becomes a loader that `import()`s it the first time a note opens and falls back to the note's plain text if the load fails.
- Libraries, pinned exactly in `package-lock.json`: `marked` (Markdown lexer), `marked-footnote`, `lowlight` with `highlight.js` (a fixed set of 17 languages), all MIT or BSD-3.
- Only the lexers are used. Obsidian syntax (wikilinks with section and alias, embeds, tags, `==highlight==`, `$math$`, `%%comments%%`, callouts with types and folding) is a small `marked` extension in `obsidian.ts`; `dom.ts` turns tokens into DOM; `highlight.ts` turns lowlight's syntax tree into DOM. No HTML string exists at any point, so DOMPurify is not needed.
- Wikilinks resolve as Obsidian does: case-insensitively, with or without `.md`, by name or by `folder/name` (`wikiMatches` in `page-notes.js`).
- Footnote links never change the URL hash, which the dashboard routes on.
- Rendered markup uses classes only (`align-right`, `callout`, `hljs-*`); `app.css` maps them to the theme variables, so light and dark need nothing else.

## Consequences
- `notes.js` is 143,970 bytes (45,632 gzip), fetched once, when the first note opens. The dashboard shell does not grow.
- `notes-dist/notes.js` is generated and committed, like `review-dist/`: rebuild and commit it with any change under `web-ui/src/notes/` or to the pinned libraries. CI rebuilds every bundle and fails if the committed files differ.
- Images are not shown: `![alt](pic.png)` and `![[pic.png]]` render a placeholder, because no endpoint serves vault files. Only `data:image/png|jpeg|gif|webp` images are drawn.
- Math (`$x$`) is shown as code and Mermaid blocks as code: KaTeX and Mermaid write `style` attributes and inline styles, which `style-src 'self'` blocks.

## Alternatives rejected
- `marked` or `markdown-it` rendering HTML, plus DOMPurify: gives up the "no HTML string" property for a sanitizer that has to be trusted.
- Extending the hand-written parser: each missing construct is another regex, and CommonMark's nesting and list rules are exactly what such parsers get wrong.
- Copying minified library files into `src/web/app/vendor/`: no lockfile, no tree-shaking (the 17 highlight.js languages are chosen at build time), and version, hash and license bookkeeping by hand.
- `remark`/`unified` with a wikilink plugin: heavier, and its wikilink plugins do not cover callouts, embeds or comments either.

## Verification
- `web-ui/src/notes/render.test.ts`: every case above, plus raw HTML, unsafe URL schemes, inline styles and footnote state between notes.
- `crates/mct-cli/tests/dashboard/interactions.test.mjs`: an opened note renders Obsidian syntax through the committed bundle, hides comments and opens the note a wikilink names.

## Addendum: the graph bundle

A graph of a whole vault (1,500+ notes, 4,700+ links) is unreadable as SVG, and capping it hid notes. A third bundle, `web-ui/src/graph/` → `src/web/graph-dist/graph.js` (served as `js/graph-gl.js`, loaded by `app/graph-loader.js`), draws it with Sigma (WebGL), lays it out with ForceAtlas2 in short main-thread slices and colours it by Louvain community. It is used above 250 notes; the SVG graph remains for small graphs and as the fallback. Sigma 3.0.2 is pinned deliberately: Sigma 4 replaced the label and colour model. No worker is used, because a `blob:` worker would need a CSP exception.
