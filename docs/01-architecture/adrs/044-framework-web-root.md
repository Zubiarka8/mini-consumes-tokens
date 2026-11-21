# ADR-044: Framework web roots for root-relative URLs

## Status
Proposed (2026-11-21). Issue: [#567](https://github.com/Zubiarka8/mini-consumes-tokens/issues/567) (the "Root-relative `/assets/x.css`" row). Adds schema migration 27, on top of migration 26 ([ADR-043](043-project-config-module-evidence.md)). Amends the leading-`/` rule of [ADR-003](003-web-3d-indexing.md) D3.

## Context
- `resolve_reference_path` reads `/assets/x.css` as `<app root>/public/assets/x.css`, the Vite and Astro default `publicDir`. The app root is the prefix before the file's first `src` segment, or else the file's directory.
- SvelteKit serves `/…` from `static/`, and an ASP.NET web project from `wwwroot/`. There, `public/` is wrong: the link stays unresolved, or resolves to a stray `public/` copy.
- A parser sees one file, so it cannot see the app's config.

## Decision
- **Parser:** an HTML link or CSS `@import` written root-relative keeps its `public/` path. It also records the app directory it resolved under, in `RelationTarget::web_app`, stored in the new `relations.web_app` column (`NULL` for every other relation).
- **Index:** the new table `web_roots(manifest_path, directory, root)` is filled by the manifest scan for each `*.csproj` whose SDK is `Microsoft.NET.Sdk.Web`, `Microsoft.NET.Sdk.Razor` or `Microsoft.NET.Sdk.BlazorWebAssembly` (root `wwwroot`). It is swept like the other manifest tables.
- **View:** when `web_app` is set, `relation_candidates` matches `<app>/<root>/<rest>` instead of the stored path. The root comes from these sources, in priority order:
  1. a `web_roots` row for that directory;
  2. a `svelte.config.{js,ts,mjs,cjs}` in `files` at the app directory (root `static`);
  3. otherwise the stored `public/` path, unchanged.

  This is a `CASE` on `r.web_app IS NULL`, so other relations pay nothing.
- **Stale data:** both sources are read at query time, so adding or deleting a config changes the answer without reparsing the page. Migration 27 reparses HTML and CSS files to fill `web_app`.

## Not done
- **Keep the `public/` default.** The issue asks for "unresolved rather than guessing" when no config names a web root. That would undo ADR-003's documented default, which the asset tests and the `portfolio-3d` eval rely on, so it is left to the maintainer.
- A web root set in code: Vite `publicDir`, SvelteKit `kit.files.assets`, `UseStaticFiles` options. These need evaluating JavaScript or C#.
- `dist/`: it holds build output, not the source of a link.
- JS/TS string asset loaders (`useGLTF("/m.glb")`) do not record `web_app`. Only HTML links and CSS `@import`s do.

## Verification
- `mct-lang-html/tests/depgraph.rs` › `a_root_relative_link_uses_the_framework_web_root`. It covers:
  - SvelteKit resolves to `static/` even when a `public/` copy exists.
  - ASP.NET web SDK resolves to `wwwroot/`.
  - A plain `Microsoft.NET.Sdk` project and a page with no config both resolve to `public/`.
  - Deleting `svelte.config.js` brings back `public/`.

  It fails with `web_app` unset (`kit/public/site.css`).
- `reference_path::tests::a_root_relative_url_names_its_app_directory`.
- `dependency_graph::tests::ambiguous_imports_do_not_make_file_imports_quadratic` stays under its bound.
