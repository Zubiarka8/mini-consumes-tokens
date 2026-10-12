# Spec: landing-story

Status: approved 2026-10-11 (decisions: move the step cards, new worktree branch
`feat/landing-scroll-story` from `origin/main` 4c3fd65, Playwright WebKit screenshots).
Implemented, uncommitted. Layout as built: side by side from 1280px (scene column fixed at
880px, the stacked size, so the camera framing and label spacing are unchanged), stacked with a
sticky scene below that (`min(420px, 45svh)`, `min(300px, 42svh)` under 768px, showing only
selected labels and no hint). Builds on [SPEC-tour-choreography.md](SPEC-tour-choreography.md)
(stage cues) and [SPEC-atlas-motion.md](SPEC-atlas-motion.md) (loop policy). Paths relative to
`website/`.

## Objective

The atlas in the normal landing view tells a four-stage story as the visitor scrolls the page.
No full-screen step is needed. Each stage's text and the 3D scene change together.

## Current state (2026-10-11, HEAD 2641f50)

- Normal view: scene, layer toggles, spacing slider, symbol explorer and context pack. No stages;
  `cue` stays `idle` and `focusLayer` is undefined.
- Full screen only: 14 `tourStages`, one `IntersectionObserver` rooted on the dialog, `stage`
  drives `selected`, `focusLayer` and a cue that lights connections step by step.
- Implemented from `todo.md` (boxes are unticked, but the commits exist): L1-L4, M1-M4 (render
  loop, idle motion, eased transitions, cue mechanism), stage-level cues. Not implemented: the
  per-tool choreography (C1-C7), M5 (`DESIGN.md`).

## Stages (existing EN/ES copy only, no new strings)

| # | Idea | Title / text keys | Scene final state (data in `atlas-data.ts`) |
|---|---|---|---|
| 0 | What the atlas represents | `tourWhatTitle` / `tourWhatText` | all boards; symbol board lit; connections light in 3 steps (reuses the `tourWhat` cue) |
| 1 | Files connect to symbols | `step1Title` / `step1Text` | file board lit; `Index` selected, so lib.rs and the column down to its cube light |
| 2 | A query walks the relations | `step2Title` / `step2Text` | symbol board lit; `Index` selected; Index→reindex, →reindex_paths, →status light in turn (the `tourIndex` steps) |
| 3 | A context pack forms | `step3Title` / `step3Text` | context board lit; `reindex` selected, so its sheets light; Index→reindex, then reindex_paths→reindex |

The three `.steps` cards above the atlas hold the same copy. They move into the story as stages
1-3 (no duplicated text on the page). **This needs approval** (see Decisions).

## Design

- **Data.** `storyStages` in `atlas-data.ts`: `{ key, title, text, layer, symbol?, steps? }`.
  `Graph` looks up cue steps in `tourStages` and then `storyStages`. No second choreography
  code path.
- **Activation.** When not expanded, one `IntersectionObserver` (root: viewport, `rootMargin`
  `"-45% 0px -50% 0px"`) watches `.atlas-story-stage`. The last intersecting entry sets
  `storyStage`. `stage = expanded ? tourStages[activeStage] : storyStages[storyStage]`. No scroll
  listeners. Before the first stage is reached, stage 0 applies.
- **Selection.** A stage selects its symbol when it activates. A click on a label, cube or
  explorer button still selects and stays selected until the next stage activates.
- **Layout.** At 1024px and wider, `.atlas-view` becomes a grid: scene (sticky, `top: 0`, height
  `min(510px, 100svh)`) and story column side by side. `.mechanism-atlas` widens to the section
  shell. Below 1024px, the scene sticks above the stages at `min(260px, 40svh)`, as the full-screen
  mobile layout does. Below 560px of viewport height, nothing sticks. Stages have a
  `min-height` of 60svh; the active one gets `aria-current="step"` and full contrast, the
  others are dimmed, not hidden.
- **Text.** Always in the DOM and readable. No motion on the text beyond the existing
  `whileInView` fade, which is skipped under reduced motion.
- **Reduced motion.** The cue applies its final state in one frame (existing behaviour); stages
  render at opacity 1 with no transform.
- **No WebGL.** The fallback replaces the scene; stages read as a plain list, activation still
  sets `aria-current`.
- **Full screen.** Unchanged. The story is hidden in full screen, which renders its own tour.

## Success criteria

1. Without opening full screen, `.atlas-story` shows 4 stages with the titles above, in EN and ES.
2. Scrolling each stage to the viewport centre sets `.atlas[data-story-stage]` to its index, sets
   `aria-current="step"` on that stage only, and sets `.spatial-layer[data-cue]` to the stage key.
3. Under reduced motion, a canvas screenshot after each activation differs from the previous one,
   and stays identical across 600ms with no scrolling.
4. The selected symbol follows the stage (`.context-pack strong` shows `Index`, `Index`,
   `reindex`); a label click then changes it until the next stage.
5. No WebGL: 4 stages readable, activation still updates `aria-current`.
6. No horizontal overflow at 320, 390, 768, 1024 and 1440px, in EN and ES. Each heading is not
   covered by the sticky scene when centred.
7. Zero draws while the atlas is offscreen. No new scroll listeners. Scene still loads only on
   approach. No hydration warnings.
8. Full-screen tests stay green, except the one assertion listed below.
9. `npm run types:check`, `format:check`, `build`, `test:export`, `test:resources`,
   `test:browser` pass. `package.json` unchanged.

## Tests that conflict (change only with explicit authorization)

- "full screen opens a scroll tour that reveals stages inside the dialog" (`landing.spec.ts:707`)
  asserts `.atlas-tour` count 0 before full screen. The story uses a separate `.atlas-story`
  class, so this assertion still holds. **No test change is needed** if that name is kept.
- Tests counting `.atlas-stage` inside `.atlas` (14) still hold because story stages use
  `.atlas-story-stage`.

## Decisions (pending)

1. Move the three `.steps` cards into the story (recommended), or keep them and repeat their copy
   in the story.
2. Branch: `feat/landing-scroll-tour` is merged (#178, #180) and its remote is gone; local HEAD is
   behind `main`. Recommended: a new branch `feat/landing-scroll-story` in its own worktree from
   `origin/main`, leaving the dirty primary checkout untouched.
3. Visual check: name an allowed tool. Chrome or browser automation is not used without
   authorization. Playwright WebKit (the project's test runner) produces screenshots.

## Boundaries

- Always: one observer per view; refs only in `useFrame`; existing palette.
- Ask first: test changes; copy or translation changes; `DESIGN.md`.
- Never: new dependencies; scroll listeners; edits to `VALIDATION.md`, `project-management/`,
  `prompt/`.

## Plan

1. Data: `storyStages`, step lookup over both arrays. Check: types.
2. `atlas.tsx`: story markup, observer, `stage` selection, `data-story-stage`. Tests first for
   criteria 1, 2, 4, 5.
3. CSS: grid/sticky layout and media queries. Tests for criterion 6.
4. Move the step cards (if decision 1 is approved).
5. Reduced-motion and draw tests (criteria 3, 7), then the full check suite and screenshots.
