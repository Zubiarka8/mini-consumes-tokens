# Code Atlas landing page

## Design contract

The page introduces a local Rust CLI/MCP server and leads developers to the existing English reference manual. The provided Code Atlas brief is the visual reference: a source-file constellation, a selected symbol and an extracted neighborhood. Use asymmetry in the hero, strong typographic hierarchy, crisp borders, graphite, warm ivory and mineral green. Reject fake metrics, testimonials, unrelated imagery and a repeated card grid.

The hero uses a synthetic 51-line invoice file with a six-line selection: calculateTotal and its taxRate dependency. Full-file and relevant-context buttons preserve original line numbers; a source minimap and derived line counts make the selection visible. Counts describe code lines, not measured token savings. The code viewport has a stable height and supports keyboard scrolling. Both views and all explanatory text are translated.

The atlas now sits in the mechanism section and is an explicitly illustrative repository. Its three HTML symbol buttons update both the SVG graph and the context pack. The optional 3D view uses the same selected symbol. It loads only after an explicit request, renders on demand with DPR 1, and stops rendering outside the viewport or while the document is hidden. Small screens and reduced-motion users keep the SVG. WebGL errors/context loss have a readable fallback. No essential action depends on WebGL.

English is prerendered, including navigation and documentation links. react-i18next receives both resource bundles synchronously, without middleware, browser detection or runtime translation downloads. Each mounted homepage has its own instance. An explicit stored choice is restored after hydration; unavailable storage falls back safely. Homepage cleanup restores the previous document language, so the English documentation keeps its language. Spanish resources are typed against the English resource keys.

The responsive layout stacks at 767px; controls retain at least 44px targets. System fonts avoid runtime font requests. All SVG and 3D graphics are original. The social SVG is an English brand asset; the displayed atlas alternative is translated. Native details elements provide keyboard-accessible FAQ interaction.

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
