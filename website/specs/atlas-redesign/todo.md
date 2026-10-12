> **Archive status (verified 2026-10-12 against 65d4990).** Historical task list; the checkboxes
> were never maintained and are intentionally left unticked, so they do not reflect progress.
> Shipped: L1-L4 (PR #178, 5620e3c), M2-M4 and the loop policy of M1 (PR #180, 21c97b9).
> **Not implemented in 65d4990 and not approved:** the `labelsDirty` part of M1, M5 (`DESIGN.md`
> line 9 still says the scene "renders on demand" and that reduced-motion users "keep the SVG"),
> and all of C1-C7 (per-tool activation `data-active-tool`, progressive text `data-revealed` and
> `--step`, the per-tool cue table, new 3D elements). `prompt2` to `prompt4` are untracked local
> files, and the cited test line numbers and names are stale.

# Tasks: Atlas 3D redesign

Plan: [plan.md](plan.md). Paths are relative to `website/`.

Files are abbreviated:

| Short name | Path |
|---|---|
| `atlas` | `components/landing/atlas.tsx` |
| `spatial` | `components/landing/spatial-atlas.tsx` |
| `data` | `components/landing/atlas-data.ts` |
| `css` | `app/(home)/landing.css` |
| `spec` | `tests/landing.spec.ts` |

Every task also passes `npm run types:check` and `npm run format:check`. Run `npm run build`
before Playwright, because the tests read `out/`.

## Module 1: atlas-layout (prompt2)

### L1. Scene state attribute
- [ ] **Files:** `atlas`, `spec`.
- [ ] **Do:** set `data-scene="loading|ready|failed"` on the atlas root, derived from `ready`
  and `failed`.
- [ ] **Test first:** a new test, "atlas exposes the scene state", checks three cases:
  - `ready` once the canvas loads;
  - `failed` with `getContext("webgl2")` returning `null`;
  - `loading` in the server HTML.
- [ ] **Check:** `npx playwright test tests/landing.spec.ts -g "atlas exposes the scene state"`

### L2. Control row moves to `Atlas`
- [ ] **Files:** `atlas`, `spatial`, `css`.
- [ ] **Do:**
  - Lift `rotation` and `reset` into `Atlas` and pass them as props.
  - `SpatialAtlas` stops rendering `.spatial-controls`; `Atlas` renders the row inside the
    scene area.
  - Rotate and reset are shown only while the scene is ready.
- [ ] **Test first:** none new. This is a refactor; the existing rotate and reset assertions
  (`spec` lines 301-318) are the guard.
- [ ] **Check:** `-g "3D scene loads on approach"` and `-g "no overflow"` stay green.

### L3. Full-screen button in the row
- [ ] **Files:** `atlas`, `css`, `spec`.
- [ ] **Do:**
  - Move the button (keeping `ref={expandButton}`, its label and its icon) to the right end
    of the row.
  - `.atlas-top-actions` keeps "MCT / 001" only.
  - Render the row so it survives `.atlas-fallback`.
  - Below 768px: icon only, with a target of at least 44×44px.
- [ ] **Test first:**
  - spec criteria 3 (position at 1440 and 390, in the normal view and in full screen) and 4
    (without WebGL);
  - at 320 and 390px, the button's box does not overlap "Rotate the atlas" or "Reset view";
  - `.atlas-top-actions` contains only "MCT / 001".
- [ ] **Check:**
  - `-g "full-screen button sits bottom-right"`
  - `-g "full-screen button works without WebGL"`
  - `-g "no overflow"`
- [ ] **Before writing CSS:** measure the existing `.spatial-controls` buttons at 390px. If
  they already reach 44×44px, reuse their size. If not, report the conflict between the spec
  (44px target) and prompt2 ("no new sizes") and ask.

### L4. Hide the three blocks in full screen
- [ ] **Gate:** ask for authorization, one test at a time, to rewrite:
  - "full screen retains selection, traps focus and exits on Escape" (`spec:575`);
  - "full screen shows the scene and tour before the explorer" (`spec:663`).
- [ ] **Files:** `css`, `spec`.
- [ ] **Do:** `.atlas-expanded:not([data-scene="failed"])` hides `.symbol-controls`,
  `.context-pack` and `.atlas-caption` with `display: none`.
- [ ] **Test first:** the two rewritten tests, covering spec criteria 1, 2, 5 and 6.
- [ ] **Check:** `-g "full screen"`

### Checkpoint A
- [ ] `npm run build && npm run test:browser`: all green; list any skipped test as skipped.
- [ ] `npm run format:check && npm run test:export && npm run test:resources`
- [ ] Visual check of the normal view and full screen at 320, 768, 1024 and 1440px, with and
  without WebGL.
- [ ] Screenshots for the report:
  - normal view at 1440px;
  - full screen at 1440px;
  - full screen at 390px;
  - full screen without WebGL.
- [ ] `git diff --exit-code package.json package-lock.json`
- [ ] Report to the user and wait for review.

## Module 2: atlas-motion (prompt3)

### M1. Loop policy and label projection
- [ ] **Gate:** ask for authorization to change "3D scene loads on approach, rotates and
  handles context loss" (`spec:206`). Its `settled() > 100` assertions at lines 275 and 322
  move under `reducedMotion: "reduce"`; line 328 stays.
- [ ] **Files:** `spatial`, `spec`.
- [ ] **Do:**
  - `useFrame` calls `invalidate()` every frame while active and not reduced; with reduced
    motion, only until a transition ends.
  - Add a `labelsDirty` ref; project labels only when it is set.
  - Hoist `place` and the per-label callbacks out of the frame callback.
- [ ] **Test first:** new tests for spec criteria:
  - 1, idle draws: at least 10 over 500ms;
  - 2, zero draws offscreen;
  - 3, zero draws with the tab hidden;
  - 4, reduced motion settles.
- [ ] **Check:** `-g "3D scene"` and `-g "draws"`.

### M2. Idle motion
- [ ] **Files:** `spatial`, `spec`.
- [ ] **Do:**
  - Connection pulses, using `getPoint(t, scratch)` into a preallocated vector.
  - Emissive breathing on the selected cube and the focused board.
  - Context sheets float.
  - Every mesh is created once; nothing that carries a label moves.
- [ ] **Test first:** spec criteria 5 (labels still within 0.5px over 1000ms) and 8 (two
  screenshots 600ms apart differ; identical under reduced motion).
- [ ] **Check:** `-g "labels stay still"` and `-g "idle motion"`.

### M3. Eased transitions
- [ ] **Files:** `spatial`, `spec`.
- [ ] **Do:** ease each of these toward its target, ending when the remaining change is
  below 0.001, and snapping under reduced motion:
  - selection colour and emissive;
  - focused board;
  - layer toggle (y scale);
  - spacing (y position).
- [ ] **Test first:** spec criterion 7 (labels stable again within 1000ms after a layer
  toggle), and criterion 4 extended: a select or toggle under reduced motion draws at least
  once.
- [ ] **Check:** `-g "transitions end"` and `-g "3D scene"` (criterion 6: orbit, zoom and
  reset still move the labels).

### M4. Cue mechanism
- [ ] **Files:** `data`, `spatial`, `atlas`, `spec`.
- [ ] **Do:**
  - In `data`: export `CueKey`, `Cue` and `CUE_STEP = 300`.
  - Add a `cue?: Cue` prop.
  - `Graph` records the start time when `cue.id` changes and steps on `k * CUE_STEP`.
  - Ship the `"idle"` cue; any other key is a no-op.
  - `.spatial-layer[data-cue]` reflects the current key.
  - `Atlas` passes `{ key: "idle", id: 0 }` for now.
- [ ] **Test first:** `.spatial-layer` has `data-cue="idle"` once ready.
- [ ] **Check:** `-g "cue"`, plus `-g "3D scene"` still green.

### M5. DESIGN.md
- [ ] **Files:** `DESIGN.md`.
- [ ] **Do:** line 9 describes the new loop policy:
  - continuous idle motion while visible;
  - zero frames offscreen or hidden;
  - on demand under reduced motion.
- [ ] **Check:** `git diff DESIGN.md`. Only that sentence changes.

### Checkpoint B
- [ ] `npm run build && npm run test:browser`: all green; list any skipped test as skipped.
- [ ] Review the `useFrame` body by hand: no `setState`, no `new`, no array, object or closure
  literals. Report the result.
- [ ] Colours: only the nine listed in the spec appear in `spatial`.
- [ ] `git diff --exit-code package.json package-lock.json`
- [ ] Watch it run: idle motion visible, labels still, and frames stop when scrolled away
  (draw counter in the console).
- [ ] Report to the user and wait for review.

## Module 3: tour-choreography (prompt4)

### C1. Activation by tool
- [ ] **Files:** `atlas`, `spec`.
- [ ] **Do:**
  - The existing observer also watches `.atlas-tools li[data-tool]`.
  - Add `data-active-tool` on `.atlas-tour`.
  - Every activation sets `cue = { key, id: id + 1 }`.
  - Stages without tools use their stage key.
- [ ] **Test first:** spec criteria 1 (activation for every tool, and cleared on `tourWhat`,
  `tourHow` and `start`), 6 (the same without WebGL) and 7 (no scroll listeners; the
  translated-stages test unchanged).
- [ ] **Check:** `-g "tour"`.

### C2. Progressive text
- [ ] **Files:** `atlas`, `css`, `spec`.
- [ ] **Do:**
  - `[data-revealed]` on the `li`, and `--step` on each item.
  - A CSS animation with delay `calc(var(--step) * 300ms)`.
  - The full text stays in the DOM.
  - Under reduced motion, everything is visible.
- [ ] **Test first:** spec criteria 2 (the third `find_calls` line goes from opacity 0 to 1
  within `4·S + 200ms`, with its text present from the start) and 3 (reduced motion).
- [ ] **Check:** `-g "progressive"` and `-g "reduced motion"`.

### C3. Cue table and shared 3D elements
- [ ] **Files:** `data`, `spatial`.
- [ ] **Do:**
  - Move the choreography table into `data` as a step count per key.
  - Create, once and hidden, the scan line, ring, sweep marker, progress bar and
    `find_symbol` link, using palette colours.
- [ ] **Test first:** none new; there is no visible change yet.
- [ ] **Check:** `npm run build`, and `-g "3D scene"` and `-g "draws"` stay green.

### C4. Cues: `tourWhat`, `tourHow` and the Orient tools
- [ ] **Files:** `spatial`, `spec`.
- [ ] **Do:** the table rows `tourWhat`, `tourHow`, `get_file_tree`, `get_project_overview`,
  `list_symbols` and `get_file_skeleton`.
- [ ] **Test first:** spec criterion 4, under reduced motion. For each trigger, the canvas
  screenshot before activation differs from the one after.
- [ ] **Check:** `-g "choreography orient"`.

### C5. Cues: Lookup tools
- [ ] **Files:** `spatial`, `spec`.
- [ ] **Do:** `find_symbol`, `find_callers`, `find_calls`, `find_references`,
  `search_symbols` and `hybrid_search`.
- [ ] **Test first:** spec criterion 4 for these six triggers.
- [ ] **Check:** `-g "choreography lookup"`.

### C6. Cues: Composite, Advanced and `start`
- [ ] **Files:** `spatial`, `spec`.
- [ ] **Do:**
  - `impact_analysis`, `build_context_pack`, `find_dead_code`, `batch`, `reindex` and
    `get_indexing_status`.
  - `discover_tool_categories` and `get_tool_schema` stay no-ops.
  - `start` sends the `"idle"` cue.
- [ ] **Test first:** spec criterion 4:
  - the changed screenshot for each trigger with 3D steps;
  - an identical screenshot for the two no-ops;
  - `start` after `find_symbol` equals the `tourWhat` final state.
- [ ] **Check:** `-g "choreography composite"`.

### C7. Sync and frame budget
- [ ] **Files:** `spec`.
- [ ] **Do:** tests only.
- [ ] **Test first:**
  - spec criterion 5: `.spatial-layer[data-cue]` matches the active tool within one
    animation frame;
  - spec criterion 8: activating a tool while the scene is offscreen causes zero draws.
- [ ] **Check:** `-g "cue sync"` and `-g "draws"`.
- [ ] **If either test fails:** the fix goes in `atlas` or `spatial`, still within the
  five-file limit.

### Checkpoint C
- [ ] `npm run build && npm run test:browser`: all green; list any skipped test as skipped.
- [ ] Repeat the Checkpoint B review: `useFrame`, colours and `package.json`.
- [ ] Watch the full-screen tour at 1440 and 390px, with motion and with reduced motion, and
  confirm the text and the scene keep pace.
- [ ] Report to the user.
