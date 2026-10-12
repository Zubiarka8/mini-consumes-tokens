> **Archive status (verified 2026-10-12 against 65d4990).** Partly implemented in PR #180
> (4c3fd65, 21c97b9): continuous loop while visible with zero frames offscreen or hidden, idle
> pulses, breathing and floating sheets, eased transitions, and the cue interface (`Cue`,
> `CueKey`, `CUE_STEP = 300`, `data-cue`). **Not implemented in 65d4990 and not approved as
> requirements:** the `labelsDirty` ref (labels are projected on every frame by `place` inside
> `useFrame`, which is still declared in the frame callback), and the `DESIGN.md` line 9 update.
> The cue interface ships with stage-level connection steps, not the per-tool cues. The test
> names and line numbers cited below are historical.

# Spec: atlas-motion

Module of [CAPABILITY-MAP.md](CAPABILITY-MAP.md). Depends on: nothing (built after
atlas-layout because both edit `spatial-atlas.tsx`, `atlas.tsx` and `landing.css`).

## Objective

The 3D scene is never still while someone is looking at it, and costs nothing when nobody is.
It gains continuous idle motion, eased transitions for every state change, and a cue interface
that lets tour-choreography play a short, finite animation per tool. HTML labels, which are
clickable, never move while idle.

## Tech Stack, Commands, Project Structure

As in [SPEC-atlas-layout.md](SPEC-atlas-layout.md).

## Design

**Render loop policy** (single source of truth, computed in `Graph`'s `useFrame`):

| Condition | Frames |
|---|---|
| `active === false` (offscreen or tab hidden; `frameloop="never"`) | 0 |
| `active`, not reduced | continuous: `useFrame` calls `invalidate()` every frame |
| `active`, reduced | on demand only: one frame per state change, no transition frames |

`active` stays `inView && visible` (computed in `Atlas`), and `frameloop` stays
`active ? "demand" : "never"`. The continuous loop is driven by `invalidate()`, so stopping it
needs no React state change.

**Idle motion** (not reduced only; reads `state.clock.elapsedTime`, writes refs):
- one pulse per connection travelling along its curve (reuses the curve; position written into
  a preallocated `Vector3` with `getPoint(t, target)`), loop period about 2.4s;
- emissive "breathing" on the selected cube and the focused board (`emissiveIntensity` within
  ±0.1 of its resting value, period about 3s);
- vertical float of the context sheets, at most 0.02 units, period about 4s.
Nothing that carries an HTML label (file cards, symbol cubes, board origins) moves while idle.

**Transitions** (eased in `useFrame` toward a target, with an explicit end at |remaining| <
0.001; reduced motion snaps to the target):
- rotation (exists today), selection (cube colour and emissive), focused board (emissive),
  layer toggle (board scale 0↔1 on the y axis), layer spacing (board y position).

**Label projection**: a `labelsDirty` ref set by orbit `change`, size change, rotation,
spacing or layer transitions and cue steps that move labelled objects. `useFrame` projects
labels only when it is set, then clears it. The `place` helper and the per-label callbacks are
hoisted out of the frame callback (today a closure is created per frame).

**Cue interface** (provided to tour-choreography):

```ts
// atlas-data.ts
export type CueKey = string; // a tool name, or a tool-less stage key
export type Cue = { key: CueKey; id: number }; // id increments on every activation
// SpatialAtlas / Graph prop
cue?: Cue;
```

When `cue.id` changes, `Graph` records the start time in a ref and the frame callback plays
the cue from that time: step `k` starts at `k * CUE_STEP` (`CUE_STEP = 300` ms, exported from
`atlas-data.ts` so the DOM uses the same clock). At the end the cue's final state holds until
the next cue. With reduced motion the final state is applied in one frame. An unknown key is a
no-op. atlas-motion ships the mechanism and one cue (`"idle"`, which clears any held state);
the per-tool cues are tour-choreography's.

## Code Style

```tsx
useFrame(({ clock }, delta) => {
  if (!active || !group.current) return;
  const remaining = rotation - group.current.rotation.y;
  group.current.rotation.y += remaining * (reduced ? 1 : Math.min(1, delta * 9));
  if (!reduced) pulse.current!.position.copy(curve.getPoint(phase(clock), scratch));
  if (labelsDirty.current) projectLabels();
  if (!reduced || Math.abs(remaining) > 0.001) invalidate();
});
```

Refs and preallocated objects only inside `useFrame`: no `setState`, no `new`, no array or
closure literals per frame. Palette limited to the colours already in `spatial-atlas.tsx` and
`landing.css` (`#b0d897`, `#899c8c`, `#60814b`, `#334f3c`, `#405246`, `#48624f`, `#3c5842`,
`#507047`, `#dce6d4`).

## Testing Strategy

Playwright with the existing `drawElements` counter (`window.atlasDraws`, `landing.spec.ts:209`)
and label bounding boxes. Headless WebKit 27.2 here has WebGL2 (checked on 2026-10-10), so
these tests run instead of skipping.

## Success Criteria

1. **Idle draws while visible.** No reduced motion, scene in view, no interaction: the draw
   count increases by at least 10 over 500ms.
2. **Zero frames offscreen.** After scrolling the canvas out of view: the draw count is
   unchanged over 500ms, and after a programmatic "Rotate" click plus two
   `requestAnimationFrame` ticks (the existing check at lines 330-339).
3. **Zero frames when hidden.** With `document.hidden` emulated (overriding `document.hidden`
   and dispatching `visibilitychange`): the draw count is unchanged over 500ms.
4. **Reduced motion settles.** With `reducedMotion: "reduce"`: after load and after "Rotate",
   `settled() > 100` (the current assertion, now scoped to reduced motion), and a change
   (rotate, select, toggle a layer) causes at least one draw.
5. **Labels still while idle.** No reduced motion, no interaction: every visible
   `.spatial-label` and `.spatial-board-labels span` box keeps `x` and `y` within 0.5px across
   1000ms while criterion 1 holds.
6. **Labels follow transitions.** Orbit drag, wheel zoom and "Reset view" still move and
   restore the `parse_request` label (existing assertions at lines 301-318).
7. **Transitions end.** With no reduced motion, after toggling a layer the labels' boxes are
   stable again (same 0.5px rule) within 1000ms.
8. **Visible idle motion.** Two canvas screenshots 600ms apart differ (non-zero pixel
   difference) with no reduced motion; under reduced motion they are identical.
9. **Code constraints** (review, not automatable here): no `setState` and no allocations inside
   `useFrame`; `package.json` unchanged; only the listed colours.
10. `npm run types:check`, `npm run format:check`, `npm run build` succeed; the rest of
    `landing.spec.ts` stays green.

## Tests that contradict this module (change only with explicit authorization, one by one)

- **"3D scene loads on approach, rotates and handles context loss"** (`landing.spec.ts:206`):
  `settled() > 100` with the scene visible at lines 275 and 322 can never hold with idle
  motion. Proposed change: run those two assertions under `reducedMotion: "reduce"` and add
  criterion 1. Line 328 (offscreen) stays as is.

## Boundaries

- Always: zero frames when inactive; labels static while idle; eased transitions end; check
  with the commands above.
- Ask first: changing a test; updating `DESIGN.md`, whose line 9 still says the scene "renders
  on demand" and that reduced-motion users "keep the SVG", which is out of date.
- Never: new dependencies (no drei, gsap, framer-motion-3d); whole-scene auto-rotation;
  `setState` or allocations in `useFrame`; new colours; edit `VALIDATION.md`,
  `project-management/` or `prompt/`.

## Open Questions

- Non-blocking: periods and amplitudes above are defaults to tune visually.
- Non-blocking: update `DESIGN.md` line 9 in this module? (default: yes, in its last task, if
  you approve.)
