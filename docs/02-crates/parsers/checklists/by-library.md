# Parser checklists by library

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

*Done when:* The manifest is read and the package is stored in `dependencies` with its version as written.

Done 4 · Partial 1 · Not done 5 · Not verified 0 · Not applicable 7

- [x] JS/TS: `package.json`: all four dependency sections (test: `package_json_reads_all_four_dependency_sections`)
- [~] Python: `requirements.txt` only; `pyproject.toml`, Poetry and Pipenv are not read
- [x] Rust: `Cargo.toml`, one name kept once across sections (test covers it)
- [x] Go: `go.mod`, block and single-line `require` (test: `go_mod_reads_block_and_single_line_require`)
- [x] Lua: LuaRocks rockspec, `luarocks.lock` and tree manifest, analysed without running the rockspec
- [ ] Java: no manifest reader (`pom.xml`, Gradle files are not read)
- [ ] Kotlin: no manifest reader (Gradle files are not read)
- [ ] C#: no manifest reader (`.csproj`, NuGet files are not read)
- [ ] C++: no manifest reader (CMake, Conan and vcpkg files are not read)
- [ ] PHP: no manifest reader (`composer.json` is not read)
- — Bash: Bash scripts have no package manifest.
- — PowerShell: PowerShell scripts have no package manifest.
- — HTML: HTML is markup; it declares no packages.
- — CSS: CSS is markup; it declares no packages (CSS frameworks are listed below).
- — XML: XML is markup; it declares no packages.
- — XAML: XAML is markup; it declares no packages.
- — Markdown: Markdown is markup; it declares no packages.

## Frameworks and libraries

### React

Host language: JS/TS

Done 1 · Partial 3 · Not done 0 · Not verified 0 · Not applicable 1

- [x] LB01 Declared dependency: declared in `package.json`
- [~] LB02 Framework structure: function components are ordinary symbols; JSX elements are not symbols (roadmap 35)
- [~] LB03 Framework calls and hooks: component-to-component JSX use is indexed; `useState` remained unresolved in the MCT context probe
- [~] LB04 Template and markup links: JSX opening and self-closing tags are generic `References`; no render relation or attribute model
- — LB05 Stylesheet selectors: not applicable to React

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: React is a JavaScript UI library; these languages have no React support.

### Vue (`.vue` SFCs)

Host language: JS/TS

Done 1 · Partial 0 · Not done 3 · Not verified 0 · Not applicable 1

- [x] LB01 Declared dependency: declared in `package.json`; plain `.ts`/`.js` Vue code is covered by JS/TS
- [ ] LB02 Framework structure: no `mct-lang-vue` crate; `.vue` is not a registered extension
- [ ] LB03 Framework calls and hooks: no `.vue` parser, so no calls or hooks from SFCs
- [ ] LB04 Template and markup links: `<template>` is not parsed
- — LB05 Stylesheet selectors: not applicable to Vue

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Vue is a JavaScript framework; SFCs are not parsed by any language here; these languages have no Vue support.

### Angular

Host language: JS/TS

Done 1 · Partial 2 · Not done 1 · Not verified 0 · Not applicable 1

- [x] LB01 Declared dependency: declared in `package.json`
- [~] LB02 Framework structure: `AngularComponent` is indexed as a class, but `@Component` has no reference relation (MCT probe)
- [ ] LB03 Framework calls and hooks: no Angular-specific call or hook model observed in the TSX probe
- [~] LB04 Template and markup links: template string is not linked to a template file (MCT probe)
- — LB05 Stylesheet selectors: not applicable to Angular

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Angular is a TypeScript framework; these languages have no Angular support.

### Svelte

Host language: JS/TS

Done 1 · Partial 0 · Not done 3 · Not verified 0 · Not applicable 1

- [x] LB01 Declared dependency: declared in `package.json`
- [ ] LB02 Framework structure: `.svelte` fixture is not indexed; no component symbol returned
- [ ] LB03 Framework calls and hooks: `.svelte` files are not indexed, so their calls/hooks are unavailable
- [ ] LB04 Template and markup links: `.svelte` markup is not indexed
- — LB05 Stylesheet selectors: not applicable to Svelte

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Svelte is a JavaScript framework; these languages have no Svelte support.

### Next.js and Nuxt routing

Host language: JS/TS

Done 1 · Partial 1 · Not done 0 · Not verified 0 · Not applicable 3

- — LB01 Declared dependency: not applicable to Next.js and Nuxt routing
- [x] LB02 Framework structure: Next `src/app/page.tsx` route file is indexed as `Page`
- [~] LB03 Framework calls and hooks: generic TSX indexing only; no route-specific model was observed
- — LB04 Template and markup links: not applicable to Next.js and Nuxt routing
- — LB05 Stylesheet selectors: not applicable to Next.js and Nuxt routing

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Next.js and Nuxt are JavaScript frameworks; these languages have no Next.js and Nuxt routing support.

### Express and Fastify route registration

Host language: JS/TS

Done 0 · Partial 1 · Not done 1 · Not verified 0 · Not applicable 3

- — LB01 Declared dependency: not applicable to Express and Fastify route registration
- [~] LB02 Framework structure: handler function is indexed, but no framework route relation was found
- [ ] LB03 Framework calls and hooks: no Express/Fastify-specific route or hook relations observed
- — LB04 Template and markup links: not applicable to Express and Fastify route registration
- — LB05 Stylesheet selectors: not applicable to Express and Fastify route registration

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Express and Fastify are JavaScript frameworks; these languages have no Express and Fastify route registration support.

### Handlebars, EJS and Pug templates

Host language: JS/TS

Done 0 · Partial 0 · Not done 1 · Not verified 0 · Not applicable 4

- — LB01 Declared dependency: not applicable to Handlebars, EJS and Pug templates
- — LB02 Framework structure: not applicable to Handlebars, EJS and Pug templates
- — LB03 Framework calls and hooks: not applicable to Handlebars, EJS and Pug templates
- [ ] LB04 Template and markup links: `.hbs`, `.ejs` and `.pug` are not registered extensions; templates are not indexed
- — LB05 Stylesheet selectors: not applicable to Handlebars, EJS and Pug templates

**Not applicable:**

- — Rust, Python, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Handlebars, EJS and Pug are JavaScript template engines; these languages have no Handlebars, EJS and Pug templates support.

### Django

Host language: Python

Done 2 · Partial 1 · Not done 1 · Not verified 0 · Not applicable 1

- [~] LB01 Declared dependency: `requirements.txt` only (see the Python manifest row)
- [x] LB02 Framework structure: `django_view` and its `@login_required` reference are indexed (MCT probe; no framework-specific route model)
- [x] LB03 Framework calls and hooks: `render` call and Django imports are indexed (MCT probe)
- [ ] LB04 Template and markup links: `render(request, "page.html")` creates no edge to `templates/page.html` (MCT probe)
- — LB05 Stylesheet selectors: not applicable to Django

**Not applicable:**

- — Rust, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Django is a Python web framework; these languages have no Django support.

### Flask

Host language: Python

Done 2 · Partial 1 · Not done 1 · Not verified 0 · Not applicable 1

- [~] LB01 Declared dependency: `requirements.txt` only (see the Python manifest row)
- [x] LB02 Framework structure: `flask_view` and its `@app.route` reference are indexed (MCT probe; no framework-specific route model)
- [x] LB03 Framework calls and hooks: `flask_render` call and Flask imports are indexed (MCT probe)
- [ ] LB04 Template and markup links: `flask_render("page.html")` creates no edge to `templates/page.html` (MCT probe)
- — LB05 Stylesheet selectors: not applicable to Flask

**Not applicable:**

- — Rust, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: Flask is a Python web framework; these languages have no Flask support.

### Bootstrap

Host language: CSS

Done 3 · Partial 0 · Not done 0 · Not verified 0 · Not applicable 2

- [x] LB01 Declared dependency: declared in `package.json`
- — LB02 Framework structure: not applicable to Bootstrap
- — LB03 Framework calls and hooks: not applicable to Bootstrap
- [x] LB04 Template and markup links: HTML class selector references resolve to matching elements (HTML integration tests)
- [x] LB05 Stylesheet selectors: selectors and imports in application CSS (library module tests)

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, XML, XAML, Markdown: Bootstrap is a CSS framework; only CSS handles it; these languages have no Bootstrap support.

### Tailwind

Host language: CSS

Done 2 · Partial 1 · Not done 0 · Not verified 0 · Not applicable 2

- [x] LB01 Declared dependency: declared in `package.json`
- — LB02 Framework structure: not applicable to Tailwind
- — LB03 Framework calls and hooks: not applicable to Tailwind
- [x] LB04 Template and markup links: HTML class selector references resolve to matching elements (HTML integration tests)
- [~] LB05 Stylesheet selectors: selectors indexed, including escaped utility names; `@source`, `@theme`, nesting and some `@import` forms are not (#87, #115, #116)

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, XML, XAML, Markdown: Tailwind is a CSS framework; only CSS handles it; these languages have no Tailwind support.

### Bulma

Host language: CSS

Done 2 · Partial 1 · Not done 0 · Not verified 0 · Not applicable 2

- [x] LB01 Declared dependency: `package.json` dependencies are read by the JS/TS manifest parser
- — LB02 Framework structure: not applicable to a CSS framework
- — LB03 Framework calls and hooks: not applicable to a CSS framework
- [x] LB04 Template and markup links: HTML class selector references resolve to matching elements (HTML integration tests)
- [~] LB05 Stylesheet selectors: the shared CSS parser indexes selectors; it does not index declarations or values, and there is no Bulma-specific regression fixture

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Lua, Bash, PowerShell, HTML, XML, XAML, Markdown: Bulma is a CSS framework; these languages have no Bulma stylesheet-selector support.

### OpenResty (`ngx_lua`)

Host language: Lua

Done 0 · Partial 3 · Not done 1 · Not verified 0 · Not applicable 1

- [~] LB01 Declared dependency: `resty.*` imports resolve to nothing local; LuaRocks rows cover declared modules
- [~] LB02 Framework structure: module tables indexed; `ngx.*` calls are recorded by name, handlers are not framework-aware
- [~] LB03 Framework calls and hooks: `ngx.*` calls recorded by name only
- [ ] LB04 Template and markup links: nginx.conf phase bindings (`content_by_lua_file`) are not indexed, so the entry point is not found
- — LB05 Stylesheet selectors: not applicable to OpenResty

**Not applicable:**

- — Rust, Python, JS/TS, Java, Kotlin, C#, C++, Go, PHP, Bash, PowerShell, HTML, CSS, XML, XAML, Markdown: OpenResty is a Lua web platform; these languages have no OpenResty support.
