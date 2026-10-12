# Plan: Atlas 3D redesign

Status: Phase 2. Specs approved (Phase 1) with every non-blocking default:

- rotate and reset on the left of the row, full screen on the right;
- revealed text stays revealed;
- `CUE_STEP = 300ms`;
- `get_file_skeleton` draws only on the selected card;
- the idle periods and amplitudes in the spec;
- `website/DESIGN.md` line 9 is updated in the last atlas-motion task.

Branch `feat/landing-scroll-tour`, base `20910db`. Tasks are in [todo.md](todo.md).

## Build order and why

| Order | Module | Prompt | Why this position |
|---|---|---|---|
| 1 | atlas-layout | prompt2 | No dependencies. Moves the control row and `rotation`/`reset` state into `Atlas`, which atlas-motion builds on. |
| 2 | atlas-motion | prompt3 | Shares `spatial-atlas.tsx`, `atlas.tsx` and `landing.css` with atlas-layout. Provides the cue interface. |
| 3 | tour-choreography | prompt4 | Needs the cue interface and `CUE_STEP` (atlas-motion), and the full-screen layout (atlas-layout). |

## Slicing

Each module is cut vertically. Each task leaves the page working and the suite green, except
for the tests the task itself turns red and then green.

- **atlas-layout:**
  1. `data-scene` attribute.
  2. Lift the row into `Atlas`.
  3. Move the button.
  4. Hide the blocks in full screen.
- **atlas-motion:**
  1. Loop policy and label projection (no visible change yet).
  2. Idle motion.
  3. Eased transitions.
  4. Cue mechanism.
  5. Docs.
- **tour-choreography:**
  1. Activation.
  2. Text reveal.
  3. Cue table and shared 3D elements.
  4. Cues, in three groups by tour stage.
  5. Cross-cutting checks.

## Test-first

Every task names the test it adds or changes. The test is written first and must fail for the
expected reason before any code is written. The task's check is that test plus
`npm run types:check`.

## Gates the plan cannot skip

- **Changing an existing test.** Each one needs explicit authorization, one test at a time,
  asked at the start of the task that changes it. If authorization is denied, the task stops
  and the conflict is reported. The tests:

  | Test | Line | Task |
  |---|---|---|
  | "3D scene loads on approach, rotates and handles context loss" | `landing.spec.ts:206` | M1 |
  | "full screen retains selection, traps focus and exits on Escape" | `landing.spec.ts:575` | L4 |
  | "full screen shows the scene and tour before the explorer at {1440, 390}px" | `landing.spec.ts:663` | L4 |

- **Creating a file.** Ask first. The plan creates no source files; everything lives in the
  existing five.
- **Commits.** Only when the user asks, one per task or per module as the user prefers.
- **Never touch:** `VALIDATION.md`, `project-management/`, `prompt/`, `package.json` or the
  lockfile.

## Checks used by every task

Run from `website/`:

```sh
npm run types:check
npm run format:check
npx playwright test tests/landing.spec.ts -g "<test name>"   # needs npm run build first: tests read out/
```

Checkpoints add `npm run build`, `npm run test:browser` (full suite), visual checks at 320, 768,
1024 and 1440px, and `git diff --exit-code package.json package-lock.json`.

## Risks

| Risk | Impact | Mitigation |
|---|---|---|
| CI may lack WebGL2. Not verified; checked only locally on WebKit 27.2, 2026-10-10. | 3D tests skip in CI, so motion and cues go unchecked there. | Run the checkpoints locally. Report any skipped test as skipped, not passed. |
| Timing tests (idle draws, reveal within `4·S + 200ms`, labels stable within 0.5px) are flaky under load. | False reds. | Use `expect.poll` with the spec's bounds. Never loosen a bound without asking. |
| Canvas screenshot comparisons. | Antialiasing noise. | Compare only under reduced motion (deterministic). Criterion "differ" means a non-zero pixel diff. |
| `spatial-atlas.tsx` grows with roughly 20 cues. | Harder review. | Cues are data in `atlas-data.ts`. If the file exceeds about 600 lines, propose a `spatial-cues.ts` split and ask, because it is a new file. |
| The "no setState / no allocation in `useFrame`" rule. | Cannot be automated here. | Each checkpoint reviews the `useFrame` body by hand and reports what it found. |
| A hidden or reduced-motion first frame animates. | Flash on load. | `reduced` stays `true` until hydration (assumption 8). M1 checks it. |

## Checkpoints

- **A, after L4:** atlas-layout complete. Report to the user, who reviews before atlas-motion
  starts.
- **B, after M5:** atlas-motion complete, plus a performance review. Report to the user, who
  reviews.
- **C, after C7:** tour-choreography complete, plus a final full-suite run. Report to the user.
