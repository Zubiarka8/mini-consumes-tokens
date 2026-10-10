# Atlas design research

Date: 2026-10-10. Scope: the illustrative atlas on the mini-consumes-tokens landing page. Research and concept selection, not a production redesign.

## Decision context

The owner confirmed that the existing 3D interaction works well but the presentation does not attract attention. The latest explicit preference is **a clear, interactive professional tool**. This supersedes the earlier selection of a spectacular/futuristic direction. The recommendation therefore prioritizes useful exploration, visible structure and readable information. High-impact lighting is not the direction.

**Recommendation: a repository workbench with an interactive dependency trace and a context lens.** Keep 3D as a way to reveal relationships, give the scene more room, and make the selection result the main event. Also compare a layered architecture view and a restrained code city before choosing the final visual metaphor.

Companion: [six original concept sketches](atlas-directions.html). The board contains schematic SVG compositions, links to real references and a preference selector. These are not screenshots of those references or finished Three.js renders.

## Method and evidence boundaries

Reviewed primary product documentation, author-written engineering articles, official graphics documentation and maintained example repositories. Reference capabilities are sourced below; all proposed designs, ratings, sequencing and preference predictions are our own judgments. The external interactive experiences were researched through web text/source retrieval, not operated in Chrome. The current atlas was previously checked in WebKit; no new production implementation or performance benchmark is claimed by this research.

The investigation separates three questions: what the picture should communicate, what happens when a person interacts, and how the result fits this website. A technically impressive scene can be weak on the second and third questions. A professional presentation can be visually distinctive without permanent animation.

## What the current implementation provides

MCT context packs for `Atlas`, `SpatialAtlas` and `Graph`, plus a scoped project overview, confirm the existing React/Three.js implementation. The scene has lit symbols, projected HTML labels, orbit/zoom controls, rotation/reset, selected-edge highlighting and a synchronized context panel. It loads lazily, retains the SVG while loading, restores 2D on failure and pauses rendering outside the viewport. Browser preferences are read after mount.

The scene still uses a small illustrative graph with fixed positions. This makes it reliable, but offers little visible structure to investigate. The outer orbital guides have no code meaning. Three symbol buttons repeat the labels already in the scene. Most of the interaction changes the camera rather than revealing more information. The context panel lists three facts but does not show how the selected relationships produce an agent's useful source context.

These are design observations, not functional defects. The first improvement should change the information hierarchy and interaction reward rather than replace working camera controls.

The connected MCP server uses the parent repository index; scoped lookups use `website/...` paths. Some JSX relationships are not resolved by the extractor, and one `invalidate` relation resolves to a fixture elsewhere in the parent graph. Do not treat these relations as a complete frontend component graph. The separate website-root index described below avoids unrelated repository fixtures for future website-specific indexing.

## Research findings

### 1. A graph becomes useful when its neighborhood is inspectable

Obsidian's documented graph supports highlighting connections, grouping, filtering, link direction and a local graph with selectable depth. These are useful patterns for an atlas: the whole repository gives orientation, while the selected neighborhood supplies an understandable task. We can adopt the interaction model without copying Obsidian's appearance. [Obsidian Graph view](https://obsidian.md/help/plugins/graph)

**Proposed application:** start with three labeled module groups. Selecting a symbol isolates its callers and dependencies; unrelated edges become quiet. A depth control offers one or two hops. An inspector explains the currently selected relationship.

### 2. Spatial organization should encode something concrete

CodeCharta maps files to buildings and folders to districts. Area, height and color encode chosen metrics, while its documentation explains camera navigation and labeling. The metaphor is grounded in folder structure, not decorative geometry. [CodeCharta map](https://codecharta.com/docs/visualization/user-controls/map/)

**Proposed application:** group code by file/module, or put source, graph and context on distinct layers. If choosing a city, use an explicit metric such as symbol count for height. Do not imply that a tall building means complexity unless a real or clearly labeled illustrative complexity value exists.

### 3. Product meaning can supply the visual spectacle

Stripe's author-written globe article explains how the globe communicated interconnectedness and service scale, then describes layers, arcs, animation and performance tradeoffs. GitHub also documents the engineering behind its homepage globe. Both are relevant precedents for giving a visual a specific product message; neither is a reason to introduce a geographic globe into this code atlas. [Stripe globe](https://stripe.com/blog/globe), [GitHub globe](https://github.blog/engineering/how-we-built-the-github-globe/)

**Proposed application:** show a query selecting a neighborhood and producing a context pack. The meaningful transformation attracts attention. Keep the animation short, replayable and optional.

### 4. Graph libraries demonstrate useful interaction patterns

The 3d-force-graph catalog includes focus-on-node, expandable neighborhoods, directional arrows/particles, highlighting, curved links and bloom. Its directional-particle example encodes direction along an edge. These are concrete implementation references; installing the entire library is not required for our small curated scene. [3d-force-graph](https://github.com/vasturiano/3d-force-graph), [directional-particle source](https://github.com/vasturiano/3d-force-graph/blob/master/example/directional-links-particles/index.html)

**Proposed application:** use arrows for stable call direction and a short pulse only when tracing a selected call. Focus the camera on selection. Expand connected symbols deliberately rather than running a continuously moving force simulation.

### 5. Progressive disclosure is more valuable than a dense overview

React Flow documents expandable/collapsible hierarchical nodes. Its referenced example is a Pro example with a Pro license; its implementation is not a free template to copy. The broad interaction idea—show relevant descendants on demand—can be implemented independently. [React Flow expand/collapse](https://reactflow.dev/examples/layout/expand-collapse)

**Proposed application:** a collapsed module shows its name and symbol count. Opening it reveals a few symbols with a breadcrumb back to the repository. A landing-page demo should stay small enough to understand without search expertise.

### 6. Better lines matter more than adding lights

Three.js `LineMaterial` supports adjustable line widths. This is a useful route for a visually stronger selected dependency path, rather than relying on the thin WebGL lines of the current scene. The current Three.js dependency already exposes the relevant addons. [Three.js LineMaterial](https://threejs.org/docs/pages/LineMaterial.html), [fat-line example](https://threejs.org/examples/webgl_lines_fat.html)

**Proposed application:** differentiated edge weights, arrowheads and legible labels. An active path should be identifiable through shape and text as well as color.

### 7. Expensive materials and bloom are optional design choices

The official bloom documentation explains selective glow via material values. Three.js describes physically based features such as transmission and clearcoat in MeshPhysicalMaterial. These techniques can make a scene polished, but they do not add repository meaning. [Bloom](https://react-postprocessing.docs.pmnd.rs/effects/bloom), [MeshPhysicalMaterial](https://threejs.org/docs/pages/MeshPhysicalMaterial.html)

For the owner's latest preference, reserve these for subtle material finish, if used at all. Do not add glass, blur, bright halos or postprocessing dependencies just to make the result feel more advanced. Any future dependency should be version-checked against the installed Fiber/Three.js versions.

### 8. Continuous motion changes the existing rendering contract

React Three Fiber documents demand rendering and manually invalidating frames when camera controls mutate state. This matches the current implementation. [R3F performance guidance](https://github.com/pmndrs/react-three-fiber/blob/master/docs/advanced/scaling-performance.mdx)

**Proposed application:** animate a finite focus/trace transition, then return to idle. Stop when the tab is hidden or the scene is offscreen. Keep readable static states and the existing reduced-motion behavior. Relative cost judgments below are not measured frame-rate claims.

### 9. Immersive worlds are a different product decision

Bruno Simon's own site documents a navigable Three.js world, input controls, quality settings and its source. It is a useful boundary reference: an immersive experience can become the whole interface. [Bruno Simon](https://bruno-simon.com/?lang=en)

A game-like world would substantially broaden this landing's scope. The owner's professional-tool preference makes that a poor first direction. The Software Galaxies project is another useful scale reference, but a vast universe of nodes would overwhelm this small teaching example. [Software Galaxies](https://github.com/anvaka/pm)

## Six professional directions

### A. Repository workbench — strongest overall fit

**Appearance:** a wide charcoal workspace; restrained mineral-green accents; crisp module regions; readable symbol chips; a slim inspector on the right. Remove the decorative orbital rings. The graph fills the available space without floating in a large empty void.

**Interaction:** choose a module, inspect a symbol, filter callers/callees, adjust neighborhood depth and focus the camera. Selection updates both the graph and inspector. The panel offers a small source snippet and an explanation of the active edge.

**Why it could appeal:** it looks like a usable development tool, with an immediately understandable task. It offers more control than the current three-symbol toggle while keeping the site's visual identity.

**Tradeoff:** excess toolbars can make a marketing page feel like a dashboard. Start with a few visible controls and expose deeper options on demand. Relative implementation effort: medium.

### B. Layered architecture — clearest 3D explanation

**Appearance:** three horizontally separated planes: source files, symbol relationships and the resulting context pack. An oblique camera makes the layers visible; opaque labels keep text readable. Thin vertical connectors explain the transformation.

**Interaction:** selecting a file reveals its symbols; selecting a symbol illuminates the related graph; selecting the context layer reveals exactly what was assembled. Layers can separate or collapse with a simple control.

**Why it could appeal:** depth has a clear purpose. It makes the existing source → symbols → context stages tangible and gives the illustration a distinctive silhouette.

**Tradeoff:** the layers can overlap from arbitrary camera angles. Use bounded navigation and a reset view, and offer a flat 2D version on narrow screens. Relative effort: medium to high.

### C. Module districts — repository orientation

**Appearance:** a restrained isometric map with three or four flat module regions. Symbols occupy small raised markers within each region. Connections cross the boundaries clearly. File and folder labels provide landmarks.

**Interaction:** select a district, expand its symbols, inspect cross-module edges and use a breadcrumb to return to the overall map. The starting state could show parser, server and validation modules.

**Why it could appeal:** it answers “where does this code live?” before showing individual details. More architectural than a cloud of identical spheres.

**Tradeoff:** clusters must reflect real or clearly illustrative grouping. A force simulation may scramble the landmarks; a deterministic curated layout is preferable here. Relative effort: medium to high.

### D. Dependency trace — strongest task interaction

**Appearance:** a deliberately legible chain of calls with secondary branches receding into the background. Nodes identify functions; arrowheads identify call direction. A small step list sits beside the 3D graph.

**Interaction:** choose “Who calls this?” or “What does this call?”, then advance along the path. Each step reveals the selected definition and its relevant neighbors. A finite pulse can show direction, but stable arrows remain after it stops.

**Why it could appeal:** the scene visibly does something useful. It directly illustrates MCT call/caller queries and makes relationships easier to discuss.

**Tradeoff:** it is less useful as a whole-repository overview. Combine it with A or C. Relative effort: medium.

### E. Context lens — strongest explanation of the product promise

**Appearance:** a broader, subdued graph with a clearly marked neighborhood around the active symbol. Relevant nodes and edges stay readable; unrelated code becomes secondary. An adjacent panel contains the resulting source snippets and original line locations.

**Interaction:** choose a question, change the neighborhood depth and compare full graph versus relevant context. The selected neighborhood and panel update together. The demo explains why each item was included.

**Why it could appeal:** it turns the name mini-consumes-tokens into an understandable experience. Visual interest comes from selection and reduction rather than decorative motion.

**Tradeoff:** do not invent token-saving percentages. Illustrative counts must describe the prepared data, and a semantic relevance claim needs a defensible selection rule. Relative effort: medium to high.

### F. Restrained code city — most distinctive alternative

**Appearance:** an isometric miniature with file buildings and folder districts, soft lighting and minimal streets. Selected dependencies appear as explicit connectors. Preserve greens, charcoal and ivory rather than a neon skyline.

**Interaction:** select a file building, inspect its symbols, highlight dependencies and zoom into its district. A legend explains the height metric.

**Why it could appeal:** tangible structure makes the codebase memorable, with a recognizable visual identity.

**Tradeoff:** it naturally communicates file structure/metrics more strongly than symbol context. A visually rich city can imply an analysis product MCT does not provide. Keep it modest and explicitly illustrative. Relative effort: high.

## Comparison and shortlist

These are qualitative design judgments for this landing, not experimental scores or promises about the owner's taste.

| Direction | Immediate attraction | Clarity | Fit to MCT's context story | Relative effort | Recommendation |
| --- | --- | --- | --- | --- | --- |
| A. Workbench | High with strong composition | Very high | Very high | Medium | Primary frame |
| B. Layers | High | High with bounded camera | Very high | Medium–high | Prototype competitor |
| C. Districts | High | High | High | Medium–high | Useful grouping pattern |
| D. Trace | Medium–high | Very high | Very high | Medium | Add to A |
| E. Context lens | High during selection | Very high | Very high | Medium–high | Add to A |
| F. Code city | Very high | Medium–high | Medium | High | Optional visual alternative |

The first prototype should combine **A + D + E**. The second should test **B**, because it changes the visual metaphor without abandoning the product story. Test **F** only if the owner wants a more tangible miniature after seeing the first two. Do not implement all six modes in the production widget.

## Proposed first experience

1. Show a clear repository view with three labeled groups and a visible selected symbol. The initial state must communicate meaning without animation.
2. Offer three prepared questions: locate a definition, follow a call and assemble relevant context. These describe user outcomes rather than implementation features.
3. On selection, frame the active neighborhood, emphasize directional relationships and explain the selected result in the inspector.
4. Offer a depth control and a return-to-overview action. Keep orbit, zoom and reset as secondary controls.
5. Offer an explicit replay of the query-to-context transition. Use a short finite sequence rather than compulsory scrolling or constant movement.

Use a wider composition in the mechanism section: the scene and inspector sit beside each other on desktop, stacking on smaller screens. Keep the existing hero source-selection example; the atlas can complement it by showing the relationships that explain the selected context. Keep the single-line project brand.

## Implementation boundaries for the chosen direction

Retain React Three Fiber and Three.js initially. The current graph is small enough for a curated deterministic layout; a new force-graph dependency is not a prerequisite. Group positions, node types and directed edges should live in one shared illustrative dataset. The SVG, 3D scene and inspector should all describe that same dataset.

Represent files, symbols and context items with distinct shapes and labels. Avoid decorative objects that appear to be data. Prevent label overlap in the default camera view; test click targets at the actual 600px layout and any proposed expanded width. Add keyboard alternatives for focus, trace, zoom and selection; do not make color or pointer gestures the sole means of understanding relationships.

MCT indexing is complete locally, but it does **not** connect the public landing to a live index. Publishing a real repository graph would require a separate export/data decision and careful selection of what source is public. For the next design experiment, keep the dataset illustrative or use a deliberate public snapshot. Do not imply live queries or live token savings.

Validation for a future production implementation: no hydration warnings; source fallback during loading and failures; preserved lazy-loading and reduced-motion behavior; no offscreen/background animation; synchronized selection and snippets; responsive/no-overflow layouts in EN/ES; keyboard-operable controls; visual review of default, focused and expanded states. Measure frame timing and transferred bytes on actual target hardware before making performance claims.

## MCT indexing record

Commands executed from the website root:

```sh
mct-cli --root . init
mct-cli --root . ignore-init --import-gitignore
mct-cli --root . gitignore-init
mct-cli --root . reindex
mct-cli --root . status
```

The first pass included generated `.next` and `out` assets and reported three generated-CSS parse failures. Importing this website's `.gitignore` into `.mctignore` removed 406 generated files from the index. The clean source snapshot contained **35 files and 602 symbols, with no reported parse failures**, plus 24 declared package dependencies. Newly added research artifacts are indexed in the final refresh; their counts are recorded in the final check below.

The generated index is now excluded from Git. Node modules, build/export output and test artifacts are excluded. Coverage is for MCT-supported source types: a successful index does not mean every MDX block or every dynamic React relationship is represented. The CLI uses a website-root index; the already connected MCP server continues using its parent-root index, with scoped website lookups.

## Reference register

All accessed on 2026-10-10. Some client-rendered demos did not expose readable content through web retrieval; the author's source/catalog was used instead. No claim of operating these external demos is made.

| Reference | Verified contribution | Use in this investigation |
| --- | --- | --- |
| [Obsidian graph](https://obsidian.md/help/plugins/graph) | Groups, filters, local depth, selection and navigation | Professional graph interactions |
| [CodeCharta map](https://codecharta.com/docs/visualization/user-controls/map/) | File buildings, folder districts, metric encodings | City and district alternatives |
| [Stripe globe](https://stripe.com/blog/globe) | Author-written visual/engineering design account | Product meaning and finite transitions |
| [GitHub globe](https://github.blog/engineering/how-we-built-the-github-globe/) | Author-written homepage graphics account | Purposeful landing visuals |
| [3d-force-graph](https://github.com/vasturiano/3d-force-graph) | Catalog of focus, expand, highlight and direction examples | Trace and neighborhood patterns |
| [Directional particles](https://github.com/vasturiano/3d-force-graph/blob/master/example/directional-links-particles/index.html) | Particle direction/speed example source | Optional finite trace pulse |
| [Graph bloom example](https://github.com/vasturiano/3d-force-graph/blob/master/example/bloom-effect/index.html) | Postprocessing integration source | Investigated, not recommended as the main direction |
| [React Flow expand/collapse](https://reactflow.dev/examples/layout/expand-collapse) | Hierarchy disclosure; Pro example/license | Interaction inspiration, not copied code |
| [Three.js LineMaterial](https://threejs.org/docs/pages/LineMaterial.html) | Adjustable line width | Stronger selected edges |
| [Three.js fat lines](https://threejs.org/examples/webgl_lines_fat.html) | Official line-rendering example | Implementation reference |
| [MeshPhysicalMaterial](https://threejs.org/docs/pages/MeshPhysicalMaterial.html) | Physical material features | Optional restrained material polish |
| [Bloom documentation](https://react-postprocessing.docs.pmnd.rs/effects/bloom) | Selective glow behavior | Investigated alternative, deprioritized |
| [R3F demand rendering](https://github.com/pmndrs/react-three-fiber/blob/master/docs/advanced/scaling-performance.mdx) | Demand loop, invalidate, reuse and instancing | Preserve existing rendering contract |
| [R3F examples](https://github.com/pmndrs/react-three-fiber/blob/master/docs/getting-started/examples.mdx) | HTML markers, curves/nodes and other examples | Reusable technical patterns |
| [Bruno Simon](https://bruno-simon.com/?lang=en) | Navigable world, controls, quality and stack | Scope boundary; not the chosen direction |
| [Software Galaxies](https://github.com/anvaka/pm) | Package graph visualizations and precomputed layouts | Scale boundary; not the chosen direction |

## Delivery status

Research and six concept sketches are delivered locally. The current production atlas is unchanged by this task. Next design decision: compare A + D + E against B using the board, then develop one focused prototype after the owner chooses the composition. No deployment, publication, new runtime dependency or production redesign is claimed.

Final MCT refresh after adding this report and the concept board: **37 files, 645 symbols, no reported parse failures**. The source-only baseline remains 35 files / 602 symbols; the extra files are the two research artifacts. Index exclusions and Git protection are configured with MCT's own CLI.

Concept-board verification: WebKit checked all six cards, selecting A + D + E, zero page errors and no horizontal overflow at 375, 768 and 1440px. The full comparison screenshot is [atlas-directions.png](atlas-directions.png). No production build was needed because this task adds standalone research artifacts and indexing configuration only.
