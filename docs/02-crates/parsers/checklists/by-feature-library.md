# Parser checklists by feature and library

Libraries and packages, with the same marks as the feature checklists. Definitions are in
[features.md](../features.md#libraries-and-packages). Every language is named: a language a library
does not apply to is listed under *Not applicable* with the reason.

Sources: the repository records named in each note (the CONTRIBUTING.md framework table, library-support.md,
the library module docs and the manifest tests), plus the MCT probes noted below. Other rows have not been re-run in this audit. `[?]` means not audited.

Marks:

- `[x]` done
- `[~]` partial: works only in some forms; the note says which
- `[ ]` not done
- `[?]` not verified

## Package manifests (LB01)

| Done | Partial | Not done | Not verified | Not applicable |
| ---: | ---: | ---: | ---: | ---: |
| 5 | 1 | 4 | 0 | 7 |


*Done when:* The manifest is read and the package is stored in `dependencies` with its version as written.


- [x] JS/TS: `package.json`: all four dependency sections (test: `package_json_reads_all_four_dependency_sections`)
- [~] Python: `requirements.txt` only; `pyproject.toml`, Poetry and Pipenv are not read
- [x] Rust: `Cargo.toml`, one name kept once across sections (test covers it)
- [x] Go: `go.mod`, block and single-line `require` (test: `go_mod_reads_block_and_single_line_require`)
- [x] Lua: LuaRocks rockspec, `luarocks.lock` and tree manifest, analysed without running the rockspec
- [ ] Java: no manifest reader (`pom.xml`, Gradle files are not read)
- [ ] Kotlin: no manifest reader (Gradle files are not read)
- [ ] C#: no manifest reader (`.csproj`, NuGet files are not read)
- [ ] C++: no manifest reader (CMake, Conan and vcpkg files are not read)
- [x] PHP: `composer.json` `require` and `require-dev` (test: `composer_json_reads_packages_and_skips_platform_requirements`)
- — Bash: Bash scripts have no package manifest.
- — PowerShell: PowerShell scripts have no package manifest.
- — HTML: HTML is markup; it declares no packages.
- — CSS: CSS is markup; it declares no packages (CSS frameworks are listed below).
- — XML: XML is markup; it declares no packages.
- — XAML: XAML is markup; it declares no packages.
- — Markdown: Markdown is markup; it declares no packages.

## Frameworks and libraries

### JavaScript / TypeScript

#### React

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | [~] |
| LB03 Framework calls and hooks | [~] |
| LB04 Template and markup links | [~] |
| LB05 Stylesheet selectors | — |
| RC01 Props and component bindings | [~] |
| RC02 Context and hook relationships | [~] |
| RC03 Route-to-page links | [ ] |
| RC04 Lazy components and Suspense | [ ] |

Host language: JS/TS

- [x] LB01 Declared dependency: declared in `package.json`
- [~] LB02 Framework structure: function components are ordinary symbols; JSX elements are not symbols (roadmap 35)
- [~] LB03 Framework calls and hooks: component-to-component JSX use is indexed; `useState` remained unresolved in the MCT context probe
- [~] LB04 Template and markup links: JSX opening and self-closing tags are generic `References`; no render relation or attribute model
- — LB05 Stylesheet selectors: not applicable to React
- [~] RC01 Props and component bindings: JSX component references are indexed; props and callback attributes are not linked to declarations.
- [~] RC02 Context and hook relationships: Calls are indexed generically; providers, consumers and custom hooks have no framework-specific links.
- [ ] RC03 Route-to-page links: Not implemented: React Router paths and parameters are not linked to page components.
- [ ] RC04 Lazy components and Suspense: Not implemented: lazy imports are not associated with the Suspense boundary that renders them.

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: React is a JavaScript UI library; these languages have no React support.

#### Vue (`.vue` SFCs)

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | [ ] |
| LB03 Framework calls and hooks | [ ] |
| LB04 Template and markup links | [ ] |
| LB05 Stylesheet selectors | — |
| VU01 SFC component structure | [ ] |
| VU02 Template bindings | [ ] |
| VU03 Router and composable links | [ ] |
| VU04 Store state and actions | [ ] |

Host language: JS/TS

- [x] LB01 Declared dependency: declared in `package.json`; plain `.ts`/`.js` Vue code is covered by JS/TS
- [ ] LB02 Framework structure: no `mct-lang-vue` crate; `.vue` is not a registered extension
- [ ] LB03 Framework calls and hooks: no `.vue` parser, so no calls or hooks from SFCs
- [ ] LB04 Template and markup links: `<template>` is not parsed
- — LB05 Stylesheet selectors: not applicable to Vue
- [ ] VU01 SFC component structure: Not implemented: .vue files and their script/template/style blocks are not indexed.
- [ ] VU02 Template bindings: Not implemented: component uses, props, emitted events and bindings are not linked.
- [ ] VU03 Router and composable links: Not implemented: Vue Router records and composable calls are not linked to their targets.
- [ ] VU04 Store state and actions: Not implemented: Pinia/Vuex stores, state reads and actions are not linked to their consumers.

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Vue is a JavaScript framework; SFCs are not parsed by any language here; these languages have no Vue support.

#### Angular

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | [~] |
| LB03 Framework calls and hooks | [ ] |
| LB04 Template and markup links | [~] |
| LB05 Stylesheet selectors | — |
| AN01 Component metadata links | [~] |
| AN02 Router-to-component links | [ ] |
| AN03 Dependency-injection graph | [ ] |
| AN04 Standalone component imports | [~] |

Host language: JS/TS

- [x] LB01 Declared dependency: declared in `package.json`
- [~] LB02 Framework structure: `AngularComponent` is indexed as a class, but `@Component` has no reference relation (MCT probe)
- [ ] LB03 Framework calls and hooks: no Angular-specific call or hook model observed in the TSX probe
- [~] LB04 Template and markup links: template string is not linked to a template file (MCT probe)
- — LB05 Stylesheet selectors: not applicable to Angular
- [~] AN01 Component metadata links: The component class is indexed; selectors, templates and styles are not linked through @Component.
- [ ] AN02 Router-to-component links: Not implemented: route configuration and lazy routes are not linked to components.
- [ ] AN03 Dependency-injection graph: Not implemented: providers and injected services are not linked.
- [~] AN04 Standalone component imports: Component classes are indexed; standalone imports and template selector references are not linked.

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Angular is a TypeScript framework; these languages have no Angular support.

#### Svelte

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | [ ] |
| LB03 Framework calls and hooks | [ ] |
| LB04 Template and markup links | [ ] |
| LB05 Stylesheet selectors | — |
| SV01 SFC component structure | [ ] |
| SV02 Component bindings | [ ] |
| SV03 Route, load and action links | [ ] |
| SV04 Store and action flow | [ ] |

Host language: JS/TS

- [x] LB01 Declared dependency: declared in `package.json`
- [ ] LB02 Framework structure: `.svelte` fixture is not indexed; no component symbol returned
- [ ] LB03 Framework calls and hooks: `.svelte` files are not indexed, so their calls/hooks are unavailable
- [ ] LB04 Template and markup links: `.svelte` markup is not indexed
- — LB05 Stylesheet selectors: not applicable to Svelte
- [ ] SV01 SFC component structure: Not implemented: .svelte script, markup, styles and component symbols are not indexed.
- [ ] SV02 Component bindings: Not implemented: props, events and bindings are not linked to declarations or handlers.
- [ ] SV03 Route, load and action links: Not implemented: SvelteKit file routes, load functions, actions and page components are not linked.
- [ ] SV04 Store and action flow: Not implemented: store declarations/subscriptions and form actions are not linked to their consumers.

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Svelte is a JavaScript framework; these languages have no Svelte support.

#### Next.js and Nuxt routing

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | — |
| LB02 Framework structure | [x] |
| LB03 Framework calls and hooks | [~] |
| LB04 Template and markup links | — |
| LB05 Stylesheet selectors | — |
| NX01 File-route graph | [~] |
| NX02 Layouts and route boundaries | [ ] |
| NX03 Middleware and redirects | [ ] |

Host language: JS/TS


- — LB01 Declared dependency: not applicable to Next.js and Nuxt routing
- [x] LB02 Framework structure: Next `src/app/page.tsx` route file is indexed as `Page`
- [~] LB03 Framework calls and hooks: generic TSX indexing only; no route-specific model was observed
- — LB04 Template and markup links: not applicable to Next.js and Nuxt routing
- — LB05 Stylesheet selectors: not applicable to Next.js and Nuxt routing
- [~] NX01 File-route graph: A Next.js page file is indexed; dynamic segments and route relationships are not modeled.
- [ ] NX02 Layouts and route boundaries: Not implemented: layouts, loading/error boundaries and API handlers are not linked to route subtrees.
- [ ] NX03 Middleware and redirects: Not implemented: middleware, rewrites and redirects are not linked to affected routes.

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Next.js and Nuxt are JavaScript frameworks; these languages have no Next.js and Nuxt routing support.

#### Express and Fastify route registration

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | — |
| LB02 Framework structure | [~] |
| LB03 Framework calls and hooks | [ ] |
| LB04 Template and markup links | — |
| LB05 Stylesheet selectors | — |
| EF01 Route-to-handler links | [~] |
| EF02 Middleware and plugin chains | [ ] |
| EF03 Error handlers and API schemas | [ ] |

Host language: JS/TS


- — LB01 Declared dependency: not applicable to Express and Fastify route registration
- [~] LB02 Framework structure: handler function is indexed, but no framework route relation was found
- [ ] LB03 Framework calls and hooks: no Express/Fastify-specific route or hook relations observed
- — LB04 Template and markup links: not applicable to Express and Fastify route registration
- — LB05 Stylesheet selectors: not applicable to Express and Fastify route registration
- [~] EF01 Route-to-handler links: Handler functions are indexed; HTTP method/path declarations are not linked to handlers.
- [ ] EF02 Middleware and plugin chains: Not implemented: middleware, plugins and validation schemas are not linked to routes.
- [ ] EF03 Error handlers and API schemas: Not implemented: error handlers and response schemas are not linked to the routes they govern.

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Express and Fastify are JavaScript frameworks; these languages have no Express and Fastify route registration support.

#### Handlebars, EJS and Pug templates

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | — |
| LB02 Framework structure | — |
| LB03 Framework calls and hooks | — |
| LB04 Template and markup links | [ ] |
| LB05 Stylesheet selectors | — |
| TE01 Template-file parsing | [ ] |
| TE02 Render and layout links | [ ] |
| TE03 Template data flow | [ ] |

Host language: JS/TS


- — LB01 Declared dependency: not applicable to Handlebars, EJS and Pug templates
- — LB02 Framework structure: not applicable to Handlebars, EJS and Pug templates
- — LB03 Framework calls and hooks: not applicable to Handlebars, EJS and Pug templates
- [ ] LB04 Template and markup links: `.hbs`, `.ejs` and `.pug` are not registered extensions; templates are not indexed
- — LB05 Stylesheet selectors: not applicable to Handlebars, EJS and Pug templates
- [ ] TE01 Template-file parsing: Not implemented: template files and their named blocks/includes are not indexed.
- [ ] TE02 Render and layout links: Not implemented: render calls, layouts and partials are not linked.
- [ ] TE03 Template data flow: Not implemented: literal locals passed by render calls are not linked to identifiers read by templates.

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Handlebars, EJS and Pug are JavaScript template engines; these languages have no Handlebars, EJS and Pug templates support.

### Python

#### Django

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [~] |
| LB02 Framework structure | [x] |
| LB03 Framework calls and hooks | [x] |
| LB04 Template and markup links | [ ] |
| LB05 Stylesheet selectors | — |
| DJ01 URL configuration graph | [~] |
| DJ02 ORM relationship graph | [ ] |
| DJ03 Template inheritance graph | [ ] |
| DJ04 Signal receivers | [ ] |

Host language: Python

- [~] LB01 Declared dependency: `requirements.txt` only (see the Python manifest row)
- [x] LB02 Framework structure: `django_view` and its `@login_required` reference are indexed (MCT probe; no framework-specific route model)
- [x] LB03 Framework calls and hooks: `render` call and Django imports are indexed (MCT probe)
- [ ] LB04 Template and markup links: `render(request, "page.html")` creates no edge to `templates/page.html` (MCT probe)
- — LB05 Stylesheet selectors: not applicable to Django
- [~] DJ01 URL configuration graph: URL and view symbols/calls are indexed generically; paths, names and reverse lookups are not linked.
- [ ] DJ02 ORM relationship graph: Not implemented: model relationship fields are not linked to related models/tables.
- [ ] DJ03 Template inheritance graph: Not implemented: render calls and template inheritance/includes are not linked.
- [ ] DJ04 Signal receivers: Not implemented: signal senders and registered receivers are not linked by signal and sender type.

**Not applicable:**

- — Rust, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Django is a Python web framework; these languages have no Django support.

#### Flask

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [~] |
| LB02 Framework structure | [x] |
| LB03 Framework calls and hooks | [x] |
| LB04 Template and markup links | [ ] |
| LB05 Stylesheet selectors | — |
| FL01 Endpoint and blueprint graph | [~] |
| FL02 URL and template links | [ ] |
| FL03 Request lifecycle hooks | [~] |

Host language: Python

- [~] LB01 Declared dependency: `requirements.txt` only (see the Python manifest row)
- [x] LB02 Framework structure: `flask_view` and its `@app.route` reference are indexed (MCT probe; no framework-specific route model)
- [x] LB03 Framework calls and hooks: `flask_render` call and Flask imports are indexed (MCT probe)
- [ ] LB04 Template and markup links: `flask_render("page.html")` creates no edge to `templates/page.html` (MCT probe)
- — LB05 Stylesheet selectors: not applicable to Flask
- [~] FL01 Endpoint and blueprint graph: Route-decorated views are indexed; endpoints and registered blueprints are not linked.
- [ ] FL02 URL and template links: Not implemented: url_for calls and render_template calls are not linked to definitions/files.
- [~] FL03 Request lifecycle hooks: Hook functions/decorators are indexed generically; lifecycle order and blueprint scope are not linked.

**Not applicable:**

- — Rust, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Flask is a Python web framework; these languages have no Flask support.

### PHP

#### Laravel

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | [~] |
| LB03 Framework calls and hooks | [~] |
| LB04 Template and markup links | [x] |
| LB05 Stylesheet selectors | — |
| LA01 Route-group semantics | [~] |
| LA02 Eloquent and migration schema links | [ ] |
| LA03 Blade layout graph | [ ] |
| LA04 Event and queued-job wiring | [~] |
| LA05 Service-container bindings | [ ] |

Host language: PHP

- [x] LB01 Declared dependency: `composer.json` `require` and `require-dev` are reported by `get_indexing_status`
- [~] LB02 Framework structure: controller classes and anonymous migration `up`/`down` methods are indexed; routes are relations, not route symbols
- [~] LB03 Framework calls and hooks: route actions resolve to controller methods; convention-discovered listeners carry a possible entry-point note, not a claim that Laravel loads them
- [x] LB04 Template and markup links: view/Blade imports and Blade component references connect templates to files or component classes
- — LB05 Stylesheet selectors: not applicable to Laravel; stylesheet selector extraction belongs to CSS frameworks
- [~] LA01 Route-group semantics: Controller grouping is partially recognized; prefixes, route names and middleware are not linked yet.
- [ ] LA02 Eloquent and migration schema links: Not implemented: model relationships and migration tables/columns are not linked to models.
- [ ] LA03 Blade layout graph: Not implemented: extends, includes, sections, yields and slots are not linked as a layout graph.
- [~] LA04 Event and queued-job wiring: Listener convention hints exist; event dispatches, registrations and queued jobs are not linked yet.
- [ ] LA05 Service-container bindings: Not implemented: providers, interface bindings and contextual bindings are not linked to resolved classes.

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Laravel is a PHP framework; these languages have no Laravel support.

### CSS

#### Bootstrap

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | — |
| LB03 Framework calls and hooks | — |
| LB04 Template and markup links | [x] |
| LB05 Stylesheet selectors | [x] |
| BS01 Interactive component links | [~] |
| BS02 JavaScript component API | [ ] |

Host language: CSS

- [x] LB01 Declared dependency: declared in `package.json`
- — LB02 Framework structure: not applicable to Bootstrap
- — LB03 Framework calls and hooks: not applicable to Bootstrap
- [x] LB04 Template and markup links: HTML class selector references resolve to matching elements (HTML integration tests)
- [x] LB05 Stylesheet selectors: selectors and imports in application CSS (library module tests)
- [~] BS01 Interactive component links: Generic class references resolve; data-bs-target controls are not linked to modal/tab/collapse elements.
- [ ] BS02 JavaScript component API: Not implemented: Bootstrap JS instances and lifecycle events are not linked to target elements.

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, XML, XAML, Markdown: Bootstrap is a CSS framework; only CSS handles it; these languages have no Bootstrap support.

#### Tailwind

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | — |
| LB03 Framework calls and hooks | — |
| LB04 Template and markup links | [x] |
| LB05 Stylesheet selectors | [~] |
| TW01 Utility and theme links | [~] |
| TW02 Source directives | [ ] |
| TW03 Variant and responsive expansion | [~] |

Host language: CSS

- [x] LB01 Declared dependency: declared in `package.json`
- — LB02 Framework structure: not applicable to Tailwind
- — LB03 Framework calls and hooks: not applicable to Tailwind
- [x] LB04 Template and markup links: HTML class selector references resolve to matching elements (HTML integration tests)
- [~] LB05 Stylesheet selectors: selectors indexed, including escaped utility names; `@source`, `@theme`, nesting and some `@import` forms are not (#87, #115, #116)
- [~] TW01 Utility and theme links: CSS utility selectors are indexed; utility use, @apply and @theme tokens are not connected.
- [ ] TW02 Source directives: Not implemented: @source does not contribute files to template/class analysis.
- [~] TW03 Variant and responsive expansion: CSS selectors are indexed; variant and responsive prefixes are not expanded against static configuration.

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, XML, XAML, Markdown: Tailwind is a CSS framework; only CSS handles it; these languages have no Tailwind support.

#### Bulma

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [x] |
| LB02 Framework structure | — |
| LB03 Framework calls and hooks | — |
| LB04 Template and markup links | [x] |
| LB05 Stylesheet selectors | [~] |
| BU01 Component and modifier links | [~] |
| BU02 Sass theme tokens | [ ] |

Host language: CSS

- [x] LB01 Declared dependency: `package.json` dependencies are read by the JS/TS manifest parser
- — LB02 Framework structure: not applicable to a CSS framework
- — LB03 Framework calls and hooks: not applicable to a CSS framework
- [x] LB04 Template and markup links: HTML class selector references resolve to matching elements (HTML integration tests)
- [~] LB05 Stylesheet selectors: the shared CSS parser indexes selectors; it does not index declarations or values, and there is no Bulma-specific regression fixture
- [~] BU01 Component and modifier links: Generic class/selector links exist; Bulma component modifiers and Sass overrides are not modeled.
- [ ] BU02 Sass theme tokens: Not implemented: Sass variables and derived theme values are not linked to Bulma component selectors.

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, XML, XAML, Markdown: Bulma is a CSS framework; these languages have no Bulma stylesheet-selector support.

### Lua

#### OpenResty (`ngx_lua`)

| Feature | Status |
| --- | --- |
| LB01 Declared dependency | [~] |
| LB02 Framework structure | [~] |
| LB03 Framework calls and hooks | [~] |
| LB04 Template and markup links | [ ] |
| LB05 Stylesheet selectors | — |
| OR01 Nginx-to-Lua entry points | [ ] |
| OR02 Shared-dictionary links | [~] |
| OR03 Async callback flow | [~] |

Host language: Lua

- [~] LB01 Declared dependency: `resty.*` imports resolve to nothing local; LuaRocks rows cover declared modules
- [~] LB02 Framework structure: module tables indexed; `ngx.*` calls are recorded by name, handlers are not framework-aware
- [~] LB03 Framework calls and hooks: `ngx.*` calls recorded by name only
- [ ] LB04 Template and markup links: nginx.conf phase bindings (`content_by_lua_file`) are not indexed, so the entry point is not found
- — LB05 Stylesheet selectors: not applicable to OpenResty
- [ ] OR01 Nginx-to-Lua entry points: Not implemented: nginx phase directives are not linked to Lua files or inline handlers.
- [~] OR02 Shared-dictionary links: ngx.shared calls are recognized by name; declarations in nginx configuration are not linked to uses.
- [~] OR03 Async callback flow: ngx calls are recognized generically; timers, threads and cosocket callbacks are not linked to resumed handlers.

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: OpenResty is a Lua web platform; these languages have no OpenResty support.
