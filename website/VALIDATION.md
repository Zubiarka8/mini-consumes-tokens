# Local validation

Validated on 2026-10-10 with Node 24.12.0, Next.js 16.3.8 and Playwright WebKit 27.2. No Chrome/Chromium was launched. Work stays uncommitted on feat/official-landing-page, based on 1ad997242c5629df14d77acb1f5c36baec202140.

## Six-file atlas and full-screen exploration — 2026-10-10

User requests: add more file icons and context, then add a full-screen option. The illustrative Rust service now has six selectable files: parser.rs, server.rs, validation.rs, main.rs, normalize.rs and response.rs. Five directed edges follow routing, parsing, validation, normalization and response construction. Each file has a projected code-file icon, a short role in the external selector, source with original line numbers, caller/dependency information and a translated agent question/explanation. The same data drives all three 3D layers. It remains a synthetic example, not a live repository index.

The full-screen button opens a viewport-sized in-page dialog, preserves the current scene and selection, locks background scrolling and traps keyboard focus. Escape or the close button restores normal layout, scrolling and focus. Source/context remain accessible by scrolling inside the expanded view. Narrow screens use two columns of file controls and only selected projected labels.

Six-file validation passed: production build/types, formatting, 125-key EN/ES resource parity, static export (29 HTML, 1127 local targets), and all 20 WebKit checks across the initial run and two focused reruns. One initial assertion still expected three labels; it was updated to six before rerunning scene behavior and new-file selection. Full-screen verification is pending. No Chrome/Chromium, dependencies, commit, publication or deployment.

## 3D-only atlas and project context — 2026-10-10

User request: remove 2D and provide more context without replacing the approved layered design. The SVG renderer and mode switch are removed. The WebGL scene loads automatically when the atlas enters view on desktop and mobile. Reduced motion preserves manual 3D exploration and makes rotation immediate. Unsupported WebGL, failed chunks and context loss leave an explicit status plus usable source selection and context; no 2D substitute is rendered.

The example repository now has a name, language, purpose and request-flow breadcrumb. Selecting a symbol updates the agent question, explanation, definition, caller, dependency and original-line-number source. This remains a synthetic Rust request-service example, clearly identified as illustrative; it is not a live repository viewer.

Mobile uses a fixed-height viewport with an aspect-aware camera. Only selected file/symbol labels appear on narrow screens to avoid overlap; layer controls and external symbol buttons remain available. The first browser run found that combining a minimum height with the desktop aspect ratio forced horizontal overflow; explicit mobile height/width corrected it. All 19 WebKit tests passed after that correction. After the final mobile label styling, all seven focused WebKit viewport/hydration checks passed with no skips. Visual review confirmed readable selected labels in `test-results/atlas-spatial-320-es.png` and project context in `test-results/atlas-spatial-1440-es.png`. Types, build, 113-key translation parity and export checks passed. No Chrome/Chromium, new dependencies, commit, publication or deployment.

## Layered atlas (proposal B) — 2026-10-10

User-selected direction: an exploded architecture with source files above the symbol graph and focused context below. Three physical boards replace the spherical graph and orbital decoration. Both SVG and WebGL render the same deterministic illustrative fixture; the static example is not a connection to the repository index.

Selection highlights the corresponding file card, graph cube, context sheets and vertical route. Projected file and symbol buttons select the same line-numbered code inspector. Layer buttons control visibility, retaining at least one visible layer; the spacing slider works in both renderers. Orbit, zoom, reset, lazy loading, SVG retention until the first frame, reduced-motion eligibility, offscreen pause and error recovery remain intact. The layout widens to 880px on desktop and stacks its toolbar on mobile. No dependencies added.

Verified final implementation: production-prefix build, types, static export (29 HTML exports, 1127 local targets), translation resources (106 matching keys), and touched-file formatting passed. All 19 WebKit tests passed with no skips, including layer visibility/spacing, file selection/source synchronization, last-layer preservation, hydration, delayed 3D chunks, orbit/zoom/reset, offscreen pause/resume, context loss, failed chunks and EN/ES layouts at 320, 375, 768, 1024 and 1440px. The first run exposed a test gesture outside the viewport after the taller canvas scrolled; bringing the canvas into view fixed the gesture without changing orbit behavior. Browser tests required escalation because the sandbox denied the local preview server socket. No Chrome/Chromium was used.

Visual review: `test-results/atlas-layers.png`, `test-results/landing-3d.png` and `test-results/atlas-flat-320-es.png`. Layer labels and symbols remain legible in the desktop scene; mobile uses the static overview with accessible symbol controls and a horizontally scrollable source preview. The header brand remains one line. Website MCT indexing was refreshed after the changes. Work remains uncommitted on the existing landing-page branch; no deployment or publication.

## Hero source-selection update — 2026-10-10

### Spatial atlas interaction and visual update — 2026-10-10

Acceptance: smooth rendering at the device pixel ratio (capped at 2), readable projected symbol labels, direct symbol selection linked to the context pack, pointer orbit/scroll zoom, keyboard-accessible rotation and reset, and highlighted connections for the selected symbol. Keep lazy loading, deterministic hydration, reduced-motion eligibility, demand rendering and offscreen pause. Keep the SVG visible until the first spatial frame and restore it on scene/chunk failure or WebGL context loss.

Implementation: native Three.js OrbitControls with bounded zoom and no panning; lit spherical symbols, file cards and subtle orbital guides; DOM labels projected from the same node positions. No new dependencies. The scene error boundary now restores 2D instead of leaving a hidden SVG. English/Spanish interaction hints and reset labels added.

Final validation: types, scoped formatting, 102 matching translation keys, production-prefix build and export checks passed. All 12 focused WebKit tests passed (zero skips), including deferred chunk loading with the SVG retained, projected label selection/context synchronization, pointer orbit, scroll zoom, reset, rotation, idle/offscreen pause/resume, context loss and lazy-chunk failure recovery. Hydration, reduced-motion eligibility, unavailable WebGL, existing symbol/keyboard behavior and EN/ES layouts at five widths also passed. WebKit exposed an eagerly mounted native canvas fallback callback and stale camera matrices during DOM-label projection; both were corrected before the final run. Reviewed the final spatial screenshot at test-results/landing-3d.png and the 320px Spanish layout. No Chrome/Chromium was used.

Additional user requirement: the header brand is one uninterrupted `mini-consumes-tokens` text node with no wrapping, including mobile. The existing EN/ES viewport checks now verify that its text occupies one line. A narrow viewport can wrap the header controls onto another row.

### Atlas hydration fix — 2026-10-10

Acceptance: the 3D control is absent in SSR and the first client render, then mounts with the browser's viewport and reduced-motion preferences without hydration warnings. The 2D atlas remains server-rendered. Atlas reads both media queries in its effect and subscribes to preference changes instead of using Motion's render-time reduced-motion value.

Follow-up after the user continued reporting a missing disabled attribute: the local dev server delivered that attribute, and two WebKit checks on localhost:3000 passed. The control now mounts only after hydration, removing its browser-dependent disabled attribute from the initial HTML. Two further checks against the active dev server passed, including cold navigation, reload and both motion preferences. The user's exact browser mutation remains unconfirmed.

Verified: type check, scoped Prettier check, production-prefix build and export check passed. Two WebKit regression tests passed for initial reduce/no-preference settings, console/page errors, live preference changes and mobile eligibility. Existing reduced-motion, WebGL fallback and 3D rendering/context-loss tests also passed. The initial regression run used an accessibility selector that excluded the CSS-hidden mobile button; switching to its DOM selector fixed the test. No Chrome/Chromium was used. The reported missing server-side disabled attribute was not present in the existing local export, which already included it; an older dev render/cache remains a possible source of that exact report.

User reference: full-file / relevant-context invoice screenshots. The hero now shows a synthetic 51-line invoice module and a six-line selection, with original line numbers, highlighted dependencies, a minimap and visible line counts above the view controls. The code viewport has a stable 260px height and scrolls. The existing interactive 2D/3D atlas moved to the mechanism section.

Changed source: components/landing/context-demo.tsx, components/landing/landing.tsx, app/(home)/landing.css, messages/en.ts, messages/es.ts and tests/landing.spec.ts. All work remains local and uncommitted on feat/official-landing-page (base 1ad997242c5629df14d77acb1f5c36baec202140).

Verified: types passed; format passed; 100 matching EN/ES keys passed; production-prefix build and export check passed (29 HTML files / 1,127 local targets). The initial WebKit suite passed 14 of 15 tests; the clipboard test's global status selector matched the new hero status. After scoping it to the command panel, that test passed. Following the final counter placement and viewport-height adjustment, seven relevant WebKit tests passed: hero switching/dependencies/line numbers, clipboard/FAQ/keyboard and both languages at five widths (320–1440px). Earlier checks also verified retained 3D behavior and locale persistence. No Chrome/Chromium was used.

Final screenshots: test-results/hero-full.png, test-results/hero-relevant.png and test-results/landing-{width}-{language}.png. Desktop and mobile screenshots reviewed. Line counts illustrate this prepared selection only; no token benchmark is claimed. Existing upstream build warnings remain. No documentation content, backend, deployment or external publishing changed.

## Original landing checks

- `npm run types:check`: passed.
- `npm run format:check`: passed for landing source, translation resources, preview/export scripts and tests.
- `npm run test:resources`: passed; 89 matching, nonempty EN/ES keys, real i18next language switching and fallback.
- `npm run build` and `npm run test:export`: default configuration passed.
- `NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run build` and `NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run test:export`: production prefix passed; 29 HTML exports and 1,127 local asset/link targets, including cold documentation routes. The final out directory uses this prefix.
- `npm run test:browser`: 14 passed, zero skipped, in WebKit. Tests include English in a Spanish browser, Spanish switching/persistence, invalid or denied storage, English documentation after homepage navigation, cold docs refresh, no JavaScript, symbol selection, copy success/failure, native FAQ, keyboard skip link, EN/ES overflow at 320/375/768/1024/1440px, reduced motion, WebGL absence/context loss and actual 3D draw calls/rotation/offscreen pause. Three.js chunks are not requested before activating 3D.
- `npm audit`: zero vulnerabilities after replacing the inherited serve preview dependency.
- The 27 documentation content files match the docs/fumadocs-site seed byte for byte.
- Semantic text contrast pairs checked numerically: primary text 15.96:1, body 7.30:1, captions 5.77:1, accent headings 4.95:1, dark muted text 8.82:1, mineral on graphite 11.25:1. The composed UI was reviewed in WebKit screenshots.

## Visual evidence and limits

Screenshots in ignored test-results include both languages at all five widths and a real rendered/rotated 3D scene. Desktop English, mobile Spanish (320/375px) and the 3D scene were visually inspected. Overflow assertions cover all ten viewport/language combinations. This is not a full screen-reader audit or verification on every physical device/browser. No Firefox or Chrome checks are claimed.

WebKit on macOS uses Option+Tab to traverse links; the keyboard test uses that native behavior. Next.js prefetch requests canceled by a cold reload can emit WebKit access-control errors during unload; the hydration/error assertion runs before that explicit unload. Cold URL rendering is checked independently.

Builds report upstream Fumadocs CSS highlight/cache warnings. Three.js reports an upstream Clock deprecation through React Three Fiber. These do not block compilation or rendering. The resource-check script uses Node's TypeScript stripping API, which emits an experimental warning on the validated Node version. No benchmark or performance claim is inferred from these checks.

Turbopack did not finish the first build in this environment. The build script explicitly uses Next.js Webpack, which completed both static configurations. Default development can also be opened with `npm run dev -- --webpack`.

Generated artifacts, dependencies and screenshots are ignored. No Rust files, workflows, remote branches, PRs or deployments were changed. The original prepared local skill directories are preserved.
