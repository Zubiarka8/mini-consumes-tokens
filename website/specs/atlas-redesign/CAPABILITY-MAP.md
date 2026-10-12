> **Archive status (verified 2026-10-12 against 65d4990).** Historical planning record, not a
> current requirement. The "approved" wording below is the author's own status line from design
> sessions; no merged PR description, ADR or accepted-spec marker confirms it independently.
> Implementation evidence: atlas-layout shipped in PR #178 (5620e3c: 5d79e25, a2201c0, 9cf1d5d,
> f6ed65d); atlas-motion (loop policy, idle motion, eased transitions, cue mechanism) shipped in
> PR #180 (4c3fd65: 21c97b9). tour-choreography is **not implemented** in 65d4990 and is not
> approved for implementation: no `data-active-tool`, `data-revealed` or `--step` exists, and the
> example it was written for (`request-service`) was replaced by the project's own index in 21c97b9.
> Branch `feat/landing-scroll-tour` and base `20910db` are historical; see
> [SPEC-landing-story.md](SPEC-landing-story.md) and [SPEC-atlas-cinematic.md](SPEC-atlas-cinematic.md)
> for the later, merged story work (PR #184, 32f8a79).

# Capability Map: Atlas 3D redesign

Status: approved (Phase 0). Branch `feat/landing-scroll-tour`, base commit `20910db`.

| Module id | Responsibility | Depends on |
|---|---|---|
| atlas-layout | Full screen hides `.symbol-controls`, `.context-pack` and `.atlas-caption` while the 3D scene is available (with WebGL unavailable they stay visible). The full-screen button moves to the bottom-right of the scene, in the row of "Rotate the atlas" and "Reset view", in both views. That row moves from `SpatialAtlas` to `Atlas`, so the button works without WebGL. | — |
| atlas-motion | Render loop on need: continuous idle motion while visible, the tab is shown and motion is not reduced; zero frames offscreen or hidden. State transitions with a defined end. A cue interface that tour-choreography fills. Label projection only when the camera, rotation or a transition changes it. | — (built after atlas-layout: shared files) |
| tour-choreography | Activation by the tool crossing the middle of the dialog (stage for stages without tools). The per-tool choreography table mapped to atlas-motion cues. Question and sample output revealed in step with the 3D action. Final state at once with reduced motion. | atlas-motion, atlas-layout |

Build order: atlas-layout → atlas-motion → tour-choreography

## Approved assumptions

1. Idle motion: pulses along the connections, emissive "breathing" on cubes and boards, gentle float of the context sheets (no HTML label). No object with an HTML label moves while idle; no whole-scene auto-rotation.
2. Idle motion runs in the normal view and in full screen, whenever the scene is visible.
3. The full-screen button moves in both views.
4. With WebGL unavailable, the explorer and the context pack stay visible in full screen.
5. The layer toggles and the spacing slider stay visible in full screen.
6. Initial focus on entering full screen stays on the full-screen button; Escape still exits.
7. `search_symbols` and `hybrid_search` share a choreography; the "start" stage returns the scene to idle.
8. `reduced` stays `true` until hydration (`atlas.tsx:59`), so the first frame never animates.

## Documents

- [SPEC-atlas-layout.md](SPEC-atlas-layout.md)
- [SPEC-atlas-motion.md](SPEC-atlas-motion.md)
- [SPEC-tour-choreography.md](SPEC-tour-choreography.md)
- [plan.md](plan.md), [todo.md](todo.md): Phases 2 and 3 (specs approved with all non-blocking defaults).
