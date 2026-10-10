# Website validation

## Reconciliation of PR #169 and the local Code Atlas landing

The candidate retains the Fumadocs reference manual and GitHub Pages workflow from PR #169 (base `4d7d2ea`). The 27 reference content files match that base byte for byte. Landing source, translation resources, static preview/export scripts and WebKit tests are imported from the existing local landing. Dependencies, generated exports, local indexes, research artifacts and screenshots are excluded.

Integration fixes preserve the documentation component contract when React Three Fiber augments JSX, disable stale incremental type-cache reuse after reproducing stale diagnostics in separate build/type checks, validate the repository prefix on pull requests, and check exported routes/assets before Pages uploads. Browser tests start their own static server; `PORT` selects a different port when another preview is running. Keyboard traversal uses the platform's WebKit convention.

## Repeatable checks

```sh
npm ci
npm run types:check
npm run format:check
npm run test:resources
npm run build
npm run test:export
NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run build
NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run test:export
PORT=4189 npm run test:browser
```

The browser suite runs WebKit exclusively and covers language switching/storage, English documentation navigation, direct routes, no JavaScript, clipboard feedback, keyboard navigation, narrow layouts, hydration, reduced motion, optional 3D rendering and recovery. Actual WebGL tests explicitly skip when the runtime lacks WebGL2; fallback checks still run.

## Evidence and limits

Validated on 2026-10-10 with Node 24.12.0: offline clean install (284 packages audited, zero vulnerabilities), strict types, scoped formatting, 102 matching translation keys, both static build configurations, and both export checks passed. Each export check covered 29 HTML files and 1,127 local link/asset targets. All 18 WebKit tests passed, zero skipped, including actual 3D rendering and recovery. Mobile Spanish and rendered 3D screenshots were inspected. Prior local landing validation is not treated as validation of the reconciled revision. Screenshots are generated under ignored `test-results/` and are not shipped.

Builds can report upstream Fumadocs CSS highlight/cache warnings. React Three Fiber may report the upstream Three.js Clock deprecation. Node's TypeScript stripping API can emit an experimental warning. These warnings do not establish a rendering failure or a performance claim.

No Chrome/Chromium, remote publication or deployment is performed during local verification. The optional 3D view is illustrative; source line counts are not token benchmarks. No full screen-reader or all-device audit is claimed.
