> **Archive status (verified 2026-10-12 against 65d4990).** Implemented and merged in 74b6a4a,
> shipped by PR #184 (32f8a79). The "Implemented, uncommitted" wording below is stale. The
> `referencias/` reference folder and the `test-results/before` and `test-results/after`
> captures are local and not in the repository (`test-results/` is gitignored), and the "47
> existing tests" count is a 2026-10-11 snapshot. The merged code and tests are authoritative.

# Spec: atlas-cinematic

Status: approved 2026-10-11 (all of it, including pointer parallax). Implemented, uncommitted;
all 47 existing tests pass unchanged. After captures: `test-results/after/`.
As built: close views also slide the other boards 0.7 apart, labels projected outside the
frame hide, and the orbit hint fades after the first stage. Builds on
[SPEC-landing-story.md](SPEC-landing-story.md), committed as 40e03c3 on `feat/landing-scroll-story`.
Paths relative to `website/`. Before captures: `test-results/before/` (ignored).

## Objective

The page-story atlas gets choreographed motion with a clear before and after: the camera
travels between stages, the boards unfold on entry, queries trace their path, the context pack
fans out, and objects answer the pointer. Copy is unchanged; story type is larger on wide screens.

## Reference decisions (`referencias/`)

- Easing: `cubic-bezier(0.84, 0, 0.16, 1)` (stabondar), close to ease-in-out quint, for camera
  and boards; `cubic-bezier(0.65, 0.05, 0, 1)` at 0.75s (landonorris) for story text.
- Hover: a back-out overshoot like serious.business `cubic-bezier(0.34, 1.56, 0.64, 1)`, 0.35s.
- Large type with contrast of scale (landonorris); outlined stage numerals.

## Design

- **Camera.** `storyStages[i].view` (distance, polar, azimuth) orbits the focused board's centre.
  Stage 0 and full screen keep the original pose. Each stage change tweens the camera over 1.1s
  in spherical coordinates. A drag or zoom cancels the tween; Reset view tweens back to the
  stage pose.
- **Entry.** On the first frame the stack rises, turns in and unfolds board by board (120ms apart).
- **Trace.** A cue's connections draw along their curves with a bright head, instead of only
  changing colour. The looping pulse is removed; subtle breathing and floating stay.
- **Column.** The selection's column drops from the upper board on each stage or selection change.
- **Context.** On the context stage the selected symbol's sheets lift and fan out.
- **Pointer.** Hovering a cube, file or sheet stack (or its label) grows it with an overshoot and
  highlights its label; with a fine pointer, the scene tilts slightly toward the cursor.
- **Text.** The active stage's accent bar draws and its content settles in; inactive stages are
  dimmed (contrast kept at AA), never hidden. From 1280px the story type scales with `clamp()`.
- **Reduced motion.** Every tween applies its final state in one frame; no parallax, no overshoot.
- **Budget.** No new dependencies, no per-frame allocations, demand rendering and offscreen pause
  unchanged, no scroll listeners.

## Success criteria

1. All existing tests pass unchanged (`landing.spec.ts`, WebKit).
2. Each story stage moves the camera: the canvas differs between stages under reduced motion
   (existing test) and the selected label moves between stages 0 and 1.
3. No horizontal overflow at 320-1920px; headings stay clear of the scene (existing tests).
4. `types:check`, `format:check`, `test:resources` pass; `package.json` unchanged.
5. After captures at the same sizes as `before/` show the change.
