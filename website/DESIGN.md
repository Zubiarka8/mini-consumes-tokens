# Code Atlas landing page

## Design contract

The page introduces a local Rust CLI/MCP server and leads developers to the existing English reference manual. The provided Code Atlas brief is the visual reference: a source-file constellation, a selected symbol and an extracted neighborhood. Use asymmetry in the hero, strong typographic hierarchy, crisp borders, graphite, warm ivory and mineral green. Reject fake metrics, testimonials, unrelated imagery and a repeated card grid.

The hero uses a synthetic 51-line invoice file with a six-line selection: calculateTotal and its taxRate dependency. Full-file and relevant-context buttons preserve original line numbers; a source minimap and derived line counts make the selection visible. Counts describe code lines, not measured token savings. The code viewport has a stable height and supports keyboard scrolling. Both views and all explanatory text are translated.

The atlas sits in the mechanism section and is an explicitly illustrative repository, explained in plain language: a short brief and a three-step flow (Your files, A map of the pieces and how they connect, Only what the assistant needs) joined by proportional ArrowIcon arrows, never text glyphs. The three layers are named Your files, Map of the pieces and What the assistant needs (EN and ES). The 3D scene loads when it approaches the viewport, renders on demand with DPR 1, and stops rendering outside the viewport or while the document is hidden. Its labels are plain words (Answers, Settings, Map keeper, Full scan, Quick update, Health check), never code names. The layer spacing is fixed (the former spacing slider was removed), and the symbol panel, source listing, context pack, "MCT / 001" header and caption were removed; selecting a piece or file only highlights it. WebGL errors, context loss and a failed chunk show a plain-language fallback that still explains the idea, and no essential action depends on WebGL. Full screen keeps the dialog, focus trap and Escape, and adds the fourteen-stage tour.

The hero card explains the context benefits (finds what matters without reading everything, saves time and cost, keeps the assistant less confused, shows where each answer comes from) and compares working Without MCP and With MCP in one plain sentence. Figures stay marked as illustrative.

English is prerendered, including navigation and documentation links. react-i18next receives both resource bundles synchronously, without middleware, browser detection or runtime translation downloads. Each mounted homepage has its own instance. An explicit stored choice is restored after hydration; unavailable storage falls back safely. Homepage cleanup restores the previous document language, so the English documentation keeps its language. Spanish resources are typed against the English resource keys.

The responsive layout stacks at 767px; controls retain at least 44px targets. System fonts avoid runtime font requests. All SVG and 3D graphics are original. The social SVG is an English brand asset; the displayed atlas alternative is translated. Native details elements provide keyboard-accessible FAQ interaction; the FAQ summaries show a Lucide chevron that points down when closed and up when open. Arrows come from the shared ArrowIcon (Lucide ArrowRight, ISC licence), sized in proportion to the adjacent text. There is no skip link: it appeared as an unexpected hidden menu on the first Tab, so the first Tab lands on the brand link; the page keeps its header, nav, main and footer landmarks. Styles and messages are split per owner: app/(home)/landing-shell.css, landing-context.css and landing-atlas.css, and messages/landing/{shell,context,atlas}.{en,es}.ts.

## Sources and compatibility

- Product facts: repository README.md, COMMANDS.md, and imported website/content/docs. No numerical performance claims are made.
- Documentation source: docs/fumadocs-site at 4d7d2ea5993bf8e3d5fad45fa10dad2f285e03d2. Only website source was imported; no branch merge or workflow was added.
- [react-i18next SSR](https://react.i18next.com/latest/ssr): bundled initial resources and language support prerendering without asynchronous translation loading.
- [i18next configuration](https://www.i18next.com/overview/configuration-options): synchronous initialization, fallback language and embedded resources.
- [Next.js static exports](https://nextjs.org/docs/app/guides/static-exports): output export and trailingSlash emit directory index files for direct documentation URLs.
- [Motion accessibility](https://motion.dev/docs/react-accessibility): MotionConfig and useReducedMotion respect the operating system preference.
- [R3F installation](https://r3f.docs.pmnd.rs/getting-started/installation): Fiber 9 pairs with React 19.
- [R3F performance](https://r3f.docs.pmnd.rs/advanced/scaling-performance): demand rendering avoids a continuous frame loop.
- [Three.js documentation](https://threejs.org/docs/): the scene uses simple meshes, line segments and a low-power WebGL renderer.

## Review boundaries

Keep documentation content and navigation unchanged. The root font switches from a build-time Google font download to system fonts. The export adds trailing slashes to support cold direct URLs. No backend, analytics, sign-in, deployment or external service is introduced.
