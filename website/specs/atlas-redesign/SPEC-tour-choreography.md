# Spec: tour-choreography

Module of [CAPABILITY-MAP.md](CAPABILITY-MAP.md). Depends on: atlas-motion (cue interface,
`CUE_STEP`), atlas-layout (full-screen layout).

## Objective

Each MCP tool in the full-screen tour gives up its context little by little: when the tool
crosses the middle of the dialog, its question and sample output appear step by step, and the
scene shows at the same pace what that tool does to request-service.

## Tech Stack, Commands, Project Structure

As in [SPEC-atlas-layout.md](SPEC-atlas-layout.md).

## Design

**Activation.** The existing single `IntersectionObserver` (root: the dialog, `rootMargin:
"-45% 0px -50% 0px"`) also observes every `.atlas-tools li` (`data-tool={name}`). The last
intersecting entry wins: a tool sets `activeTool`; a stage without tools (`tourWhat`,
`tourHow`, `start`) sets the cue to its stage key. `.atlas-tour` exposes `data-active-stage`
(kept) and `data-active-tool`. Every activation increments `cue.id`. No scroll listeners.

**Text reveal.** A tool's revealable items are, in order: its question (`ask_<name>`), then
each line of `output`. Item `k` appears at `k * CUE_STEP` through a CSS animation driven by
`[data-revealed]` on the `li` and a `--step` custom property per line. The DOM always contains
the full text (`textContent` and the accessibility tree are complete from the start; items
are only visually hidden), so screen readers and `toContainText` see everything. Once revealed,
a tool stays revealed for the rest of the full-screen session; its 3D cue replays on every
activation.

**Cue length.** `steps = max(3D steps, text items)`; the 3D final state holds until the next
cue.

**Reduced motion.** Text is fully visible and the 3D final state is applied in one frame.

**No WebGL.** Activation and text reveal work the same (the reveal timing is DOM-only).

## Choreography table

`S = CUE_STEP = 300ms`. "Selected" is the example the stage selects (`tourStages[].symbol`), or
the current selection for stages without one.

| Trigger | 3D steps (one per S) | Final state | Text items |
|---|---|---|---|
| `tourWhat` (stage) | files board, then symbols board, then context board rise into place (3) | all boards in place | stage text (existing `whileInView`) |
| `tourHow` (stage) | a scan line crosses the file board (1); each file's cube drops from its card to the symbol board, server.rs → parser.rs → validation.rs (3) | all cubes on the symbol board | stage text |
| `get_file_tree` | the file cards appear one by one (3) | 3 cards | question |
| `get_project_overview` | each board's dividers light, files → symbols → context (3) | dividers lit | question |
| `list_symbols` | a cube appears under each file (3) | 3 cubes | question |
| `get_file_skeleton` | the selected card's line marks are drawn one by one (3) | lines drawn | question |
| `find_symbol` | `parse_request` lights (1); a link draws from parser.rs to it (1) | lit + link | question, 2 output lines |
| `find_callers` | a pulse travels `handle_request` → `parse_request` (2) | target lit | question, 2 lines |
| `find_calls` | a pulse travels `parse_request` → `validate_input` (2) | target lit | question, 3 lines |
| `find_references` | `validate_input` blinks twice (2) | lit | question, 2 lines |
| `search_symbols` | a sweep crosses the cubes and stops on `parse_request` (3) | `parse_request` lit | question, 2 lines |
| `hybrid_search` | same sweep, stops on `validate_input` (its sample result) (3) | `validate_input` lit | question, 2 lines |
| `impact_analysis` | a ring wave leaves `validate_input` toward its caller `parse_request` (2) | caller lit | replaces line, question, 4 lines |
| `build_context_pack` | the context sheets of `validate_input` drop one by one: definition → callers → calls → source (4) | 4 sheets | replaces line, question |
| `find_dead_code` | a sweep crosses every cube with no stop (3) | nothing lit | question |
| `batch` | two pulses at once, on both connections (2) | both targets lit | question, 7 lines |
| `discover_tool_categories` | none | unchanged | question |
| `get_tool_schema` | none | unchanged | question |
| `reindex` | the file cards flash (1); the cubes shrink and regrow (2) | unchanged scene | question |
| `get_indexing_status` | a progress bar on the file board's edge fills in 4 steps (4) | bar full | question |
| `start` (stage) | cue `"idle"`: held states clear, idle motion resumes | idle | stage text |

New 3D elements (scan line, ring, sweep marker, progress bar, link) use existing palette
colours and are created once (hidden until their cue), never per frame.

## Code Style

As in [SPEC-atlas-motion.md](SPEC-atlas-motion.md). The table lives in `atlas-data.ts` as data
(`steps` per cue key); the 3D side reads it, the DOM side reads `CUE_STEP`.

## Testing Strategy

Playwright: `data-*` attributes for activation, computed styles for the reveal, canvas
screenshots for the 3D effect (deterministic under reduced motion, where there is no idle
motion).

## Success Criteria

1. **Tool activation.** Scrolling each `.atlas-tools li` to the dialog's centre sets
   `.atlas-tour[data-active-tool]` to its name, and `data-active-stage` to its stage index.
   Scrolling `tourWhat`, `tourHow` and `start` to the centre clears `data-active-tool`.
2. **Progressive text.** With no reduced motion, right after `find_calls` activates, its third
   output line has computed `opacity` 0, and it reaches 1 within `4 * S + 200ms`. Before that
   time its `textContent` already contains the full output.
3. **Reduced motion.** Every tool's items have `opacity` 1 and `transform: none` with no
   scrolling (extends the existing reduced-motion test).
4. **3D effect per tool** (reduced motion, canvas screenshots): for every trigger with 3D
   steps, the screenshot after activation differs from the one before; for
   `discover_tool_categories` and `get_tool_schema` it is identical; activating `start` after
   `find_symbol` returns to the screenshot taken at `tourWhat`'s final state.
5. **Sync.** With no reduced motion, `.spatial-layer[data-cue]` equals the active cue key
   within one animation frame of `data-active-tool` changing.
6. **No WebGL.** With WebGL unavailable, criteria 1-3 hold.
7. **No scroll listeners** (existing test "tour scrolling registers no scroll listeners" stays
   green), and "tour explains the server in seven translated stages" stays green unchanged.
8. Draw counts: when the scene is offscreen, activating a tool causes no draws (zero-frame
   rule from atlas-motion).
9. `npm run types:check`, `npm run format:check`, `npm run build` succeed.

## Tests at risk

None contradicts this module if the text stays in the DOM (as specified). "tour explains the
server in seven translated stages" (`landing.spec.ts:682`, line 722) would fail if the reveal
were a typewriter that slices text; this spec forbids that.

## Boundaries

- Always: full text in the DOM; one observer, no scroll listeners; same `CUE_STEP` on both
  sides.
- Ask first: changing a test; changing tool copy or translations.
- Never: new dependencies; `setState` in `useFrame`; new colours; edit `VALIDATION.md`,
  `project-management/` or `prompt/`.

## Open Questions

- Non-blocking: text stays revealed after its first play (default) or re-hides and replays?
- Non-blocking: `S = 300ms` (a 7-line batch takes 2.4s); shorter?
- Non-blocking: `get_file_skeleton` draws the lines on the selected card (default) or on every
  card?
