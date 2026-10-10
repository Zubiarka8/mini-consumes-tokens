# Website

Use Node 22.13 or newer (validated with Node 24.12.0).

Official Code Atlas homepage and the existing Fumadocs reference manual. See [DESIGN.md](DESIGN.md) for architecture, content sources and compatibility decisions.

```sh
npm ci
npm run dev -- --webpack
# http://localhost:3000/
npm run types:check
npm run build
npm run preview
# http://127.0.0.1:4173/
```

For an exact GitHub Pages preview:

```sh
NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run build
NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run preview
# http://127.0.0.1:4173/mini-consumes-tokens/
# http://127.0.0.1:4173/mini-consumes-tokens/docs/
```

The preview server serves only exported files; it does not hide missing routes with a SPA fallback. Use the same prefix for build and preview. All content remains local until the maintainer explicitly publishes it.

```sh
npm run test:resources
NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens npm run test:export
npm run test:browser
```

Browser checks use Playwright WebKit exclusively. Install that engine with `npx playwright install webkit` when needed. No Chrome/Chromium runner is configured.

See [VALIDATION.md](VALIDATION.md) for check results, screenshot evidence and known upstream warnings.
