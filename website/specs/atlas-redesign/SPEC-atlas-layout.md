# Spec: atlas-layout

Module of [CAPABILITY-MAP.md](CAPABILITY-MAP.md). Depends on: nothing.

## Objective

In full screen the scene and the scroll tour are the whole experience: the file explorer
(`.symbol-controls`), the context pack (`.context-pack`) and the caption (`.atlas-caption`)
are not shown while the 3D scene is available. The full-screen button leaves the top bar and
joins the scene's control row ("Rotate the atlas", "Reset view") at its right end, in the
normal view and in full screen. It keeps working when WebGL is unavailable, so it cannot be
rendered by `SpatialAtlas`.

User stories:
- As a visitor in full screen, I see only the scene and the tour, without a duplicate explorer
  and context pack below.
- As a visitor, I find the full-screen control next to the other scene controls.
- As a visitor without WebGL, I can still enter full screen and read the explorer, the context
  pack and the whole tour.

## Tech Stack

Next.js 16.3.8 (static export), React 19.3, `@react-three/fiber` 9.8, `three` 0.186,
`motion` 14.1, `lucide-react`, `react-i18next`, Playwright 1.64 (WebKit). No new dependencies.

## Commands

Run from `website/`:

```sh
npm run build                                      # static export into out/ (the tests read out/_next chunks)
npm run types:check                                # next typegen && tsc --noEmit
npm run format:check                               # prettier over landing components, CSS and tests
npx playwright test tests/landing.spec.ts -g "<test name>"   # one test (starts the preview server)
npm run test:browser                               # full Playwright suite
```

## Project Structure

```
website/components/landing/atlas.tsx          Atlas: state, full-screen dialog, tour, explorer, context pack
website/components/landing/spatial-atlas.tsx  SpatialAtlas: R3F canvas, projected HTML labels (lazy chunk)
website/components/landing/atlas-data.ts      examples, connections, tourStages, layerKeys
website/app/(home)/landing.css                all landing styles
website/messages/*.ts                         translations (only if a label changes)
website/tests/landing.spec.ts                 Playwright end-to-end tests
```

## Design

- `rotation` and `reset` state move from `SpatialAtlas` to `Atlas` and are passed down as
  props. `SpatialAtlas` no longer renders `.spatial-controls`.
- `Atlas` renders one control row (the existing `.spatial-controls` class name is kept) at the
  bottom of the scene area: rotate and reset at the left, only while the scene is ready; the
  full-screen button at the right, always. The button keeps `ref={expandButton}`, its
  `aria-label` and its icon; below 768px it shows the icon only, with a target of at least
  44×44px (DESIGN.md contract).
- `.atlas-top-actions` keeps "MCT / 001" only.
- With WebGL unavailable, `.atlas-space:has(+ .atlas-fallback)` is hidden today. The row must
  stay visible in that state: it is rendered where it survives (e.g. after `.atlas-fallback`
  inside `.atlas-view`), still bottom-right of the scene area.
- The root gets `data-scene="loading" | "ready" | "failed"`. Full-screen hiding is CSS only:
  `.atlas-expanded:not([data-scene="failed"])` hides the three blocks with `display: none`,
  which also removes their controls from the focus trap (it already filters elements without
  client rects).

## Code Style

Match the surrounding code: function components, hooks at the top, terse comments only where
the why is not obvious, class names in kebab-case, no inline styles for layout.

```tsx
<div className="spatial-controls">
  {ready && (
    <>
      <button type="button" onClick={() => setRotation((value) => value + Math.PI / 4)}>
        {t("rotate")} ↻
      </button>
      ...
    </>
  )}
  <button ref={expandButton} type="button" onClick={() => setExpanded((value) => !value)} ...>
```

## Testing Strategy

Playwright end-to-end tests in `tests/landing.spec.ts` (the only test level in `website/`).
Each criterion below names what it measures. Tests are written red first.

## Success Criteria

1. **Hidden in full screen with WebGL.** At 1440×900 and 390×844, after entering full screen
   with the scene ready: `.symbol-controls`, `.context-pack` and `.atlas-caption` are
   `toBeHidden()`; `.atlas-space` and the first `.atlas-stage` are visible.
2. **Visible in full screen without WebGL.** With `getContext("webgl2")` returning `null`:
   in full screen the three blocks are `toBeVisible()` and `.atlas-fallback` is visible.
3. **Button position, normal view and full screen.** The full-screen button's box is inside
   the scene area's box, its right edge within 24px of the area's right edge and its bottom
   edge within 24px of the area's bottom edge; its `y` equals the "Rotate the atlas" button's
   `y` within 2px. Measured at 1440 and 390 widths.
4. **Button without WebGL.** With WebGL unavailable, the button is visible, enters full screen,
   and its box is bottom-right of the scene area (same 24px tolerances, area = `.atlas-view`
   region holding `.atlas-fallback`).
5. **Focus.** Entering full screen focuses "Exit full screen"; Tab/Shift+Tab cycle only
   through visible controls (none inside the three hidden blocks); Escape exits and focuses
   "Full screen"; `document.body.style.overflow` is restored.
6. **Selection preserved.** Selecting a symbol through a spatial label in full screen, then
   pressing Escape, shows that symbol in `.context-pack strong`.
7. **No regressions.** The "no overflow" tests at 320/375/768/1024/1440, the hydration tests,
   the WebGL-absence tests and the tour tests stay green unchanged.
8. `npm run types:check`, `npm run format:check` and `npm run build` succeed.

## Tests that contradict this module (change only with explicit authorization, one by one)

- **"full screen retains selection, traps focus and exits on Escape"** (`landing.spec.ts:575`):
  line 600 expects Shift+Tab to reach `.atlas-source` (hidden now); lines 605-608 click
  `.symbol-controls` in full screen (hidden now). Proposed rewrite: criteria 5 and 6.
- **"full screen shows the scene and tour before the explorer at {1440, 390}px"**
  (`landing.spec.ts:663`): `boundingBox()` of the hidden `.symbol-controls` returns `null`.
  Proposed replacement: criteria 1 and 2.

## Boundaries

- Always: keep the button outside `SpatialAtlas`; keep existing accessible names and
  translations; verify at 320, 768, 1024 and 1440px; run the commands above before reporting.
- Ask first: changing any existing test; editing `DESIGN.md`; changing a translation string.
- Never: new dependencies; edit `VALIDATION.md`, `project-management/` or `prompt/`; commit
  without the user's request.

## Open Questions

- Non-blocking: rotate/reset stay at the left of the row and the full-screen button at its
  right end (default), or the whole row moves to the right?
