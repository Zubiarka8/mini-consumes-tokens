# ADR-038: Declared join points as possible entry points

## Status

Proposed (2026-11-19, amended 2026-11-20). Issues: [#561](https://github.com/Zubiarka8/mini-consumes-tokens/issues/561) and [#566](https://github.com/Zubiarka8/mini-consumes-tokens/issues/566).

## Context

A framework or an external client can invoke a handler without leaving an indexed call. A declared route is evidence of a possible entry point even when no client in the repository uses it. The convention hints in ADR-027 have different semantics: they annotate candidates and do not exclude them.

## Proposed decision

- A join point records a kind, a normalized literal value, a declaration or use role, and the exact symbol in the same parsed file. The initial kind is an HTTP endpoint whose value contains a method and path.
- Persist join points separately from prose literals and convention hints. File replacement removes the old points with their symbols. A migration must request a reparse of existing files.
- A declaration is evidence of a possible entry point, not proof that the symbol executes. A dead-code scan excludes the associated symbol and reports its reason separately. A missing matching use does not change this decision.
- Preserve ADR-027 hints and their existing candidate behavior. Resolve a declaration to its exact symbol, never to every symbol sharing its name.
- The first HTTP extractor handles only explicitly identifiable route declarations and literal paths. Computed paths and ambiguous handler identities produce no declaration. Other frameworks and join kinds need their own evidence-backed extractors.
- Matching a use to a declaration is decided in the amendment below.

## Verification

Exercise persistence and reindexing, handler identity, unmatched declarations, ordinary unreferenced functions, convention hints, and the dead-code report. Record candidate counts and removed candidates before and after on this repository.

The initial extractor recognizes qualified Actix Web method attributes such
as `#[actix_web::get("/api/notes")]`. Migration 20 stores their declarations
and invalidates existing content hashes.

Forced reindex of the same `69296cc1` repository snapshot before and after
the change produced 122 dead-code candidates in both runs. The CSV reports
were identical. The snapshot had no matching route declarations. The MCP
fixture exercises the removal of two handlers from the candidate list.

## Amendment: matching uses to declarations (issue #561, first join: HTTP)

### Decision

- The value is `"<METHOD> <path>"`, built by `mct_core::http_endpoint`. The path is an absolute literal: no query, fragment, backslash, wildcard, space or empty inner segment. A trailing `/` is dropped. A whole-segment parameter (`:id`, `{id}`, `[id]`) is written `{}`, so every framework's spelling gives one value. Anything else gives no join point. A client's literal first loses its query string and fragment (`http_use_endpoint`).
- A **use** is a client call with a literal path. A URL to this machine (`localhost`, `127.0.0.1`, `[::1]`) counts as its path; a URL to any other host gives no use. JS/TS: `fetch` (`GET` unless its options object has a literal `method`; a computed path, a hole that is not the last whole segment, a `method` that is not a literal, or a spread gives none) and `axios.<method>`. Python: `requests`, `httpx`, `client` or `session` `.get("/x")`. Go: `httptest.NewRequest` and `http.NewRequest`. Java: MockMvc `get("/x")`. C#: `HttpClient.GetAsync("/x")`. PHP: `$this->getJson('/x')` and Guzzle `$client->get('/x')`. Lua: `ngx.location.capture`. C++: cpp-httplib `cli.Get("/x")`. Bash: `curl`. PowerShell: `Invoke-RestMethod` and `Invoke-WebRequest`.
- A **declaration** is a route in a file that mentions its framework (the evidence it is not another API): Actix Web method attributes, axum `.route("/p", get(h).post(g))`, Express `app.get("/p", ..., h)`, Flask/Quart `@app.route(.., methods=[..])` and FastAPI/Starlette `@app.get`, Go `HandleFunc("GET /p", h)` and gin/echo/fiber/chi `r.GET("/p", h)`, Spring `@GetMapping` and `@RequestMapping(method = ..)` with the class's `@RequestMapping` prefix (Java and Kotlin), ASP.NET `[HttpGet("p")]` with the class's `[Route]` prefix and minimal-API `app.MapGet("/p", h)`, Symfony `#[Route('/p', methods: [..])]` with the class prefix, Lapis `app:get("/p", h)`, cpp-httplib `svr.Get("/p", h)`. A route handler passed by name must be exactly one function of the same file; an imported, inline or duplicated handler gives no declaration. A route without a method (Go `HandleFunc("/p", h)`, Symfony `#[Route('/p')]`) answers every method and gives none. An ASP.NET `[controller]` token, a constant path and several paths give none. Laravel's `Route::get` lives apart from its controller, so it declares nothing; Ktor's `get("/p") { .. }` has no named handler.
- Uses keep their line, column and length (`join_points`, migration 21).
- After each indexing run, `join_resolution::resolve` writes one `calls` relation from the symbol that uses a value to each symbol that declares it, marked `relations.join_kind = 'http'`. The relation names the declarer's file, language, kind and parent, so it resolves to exactly that symbol. The pass compares the relations it wants with the ones it wrote earlier and changes only the difference: a run that moved nothing writes nothing, and a deleted file or handler removes its links.
- One declarer is `resolved` with `declared` evidence. Several declarers give one relation each, and the relation queries report every one of them as `ambiguous` with `candidates` evidence and no target. A value with no declarer gives no relation: the endpoint may belong to a third party, so it is never `external`, and there is nothing to mark `unresolved`.
- Name resolution and every relation that existed before are unchanged. MCP signatures are unchanged: a link appears as an ordinary `calls` hit in `find_callers`, `find_calls`, `find_references` and `impact_analysis`. A declaration still excludes its handler from the dead-code candidates (see above).
- A same-named function in another scope of the declaring file cannot be told apart by the relation evidence (it has no "no parent" form), so the link is reported `ambiguous`, never resolved to the wrong one.

### Not decided here

Other clients and frameworks (`$.ajax`, `XMLHttpRequest`, reqwest, Retrofit, JAX-RS, Laravel `Route::`, Ktor, Crow, Next.js), HTML `<form action>`, a hole in the middle of a template path, case-insensitive matching (ASP.NET routes are, matching here is exact), the dashboard's hand-written router, and the other join kinds (SQL, i18n, env, FFI, library references) each need their own extractor and fixtures. The `FrameworkOverlay` of ADR-025 and `role=route` display are not implemented: the extractors live in the parsers, as in #566.

### Verification

`crates/mct-mcp-server/tests/http_join.rs`: `fetch` and `axios` to axum and Express, one positive test per language above, parameter spellings, method mismatch, computed and foreign-host paths, a route declared twice (both handlers ambiguous), an imported or inline handler, a same-named function in another scope, and reindexing (no write when nothing changed, a removed route, a deleted frontend file).

Same `origin/main` snapshot (`650e97ff`), `mct-cli init --force` before and after: 66,208 symbols and 158,697 relations in both, none made by the join pass; 121 dead-code candidates in both, and the CSV reports identical once the root path is ignored. The snapshot has one use (`POST /api/invoices` in a test fixture) and no matching declaration. Index size 32,935,936 to 33,107,968 bytes (+0.5%); `init --force` 7.85 s to 7.63 s (one run each, release build).

## Amendment: declared tests (issue #630)

### Decision

- A Rust function with a test attribute (`#[test]`, `#[tokio::test]`, `#[rstest]`, `#[test_case(..)]`, any path ending in one of those) declares a `test` join point. Its value is the attribute path as written (`#[tokio::test]`), and its role is `declares`. `#[bench]` and any other attribute declare nothing. Migration 34 reparses Rust files to record them.
- A test join point is no framework entry point. It keeps its function out of the dead-code candidates but adds no `possible entry point` line, so a test suite does not bury the route declarations.
- An affected test (`impact_analysis`, `build_context_pack`) says why it is one, in this order: the declaring syntax (`#[test]`), then the path convention (`path`: `tests/`, `*_test.*`, `*.spec.*` and the like), then the name convention (`name`: exactly `test` or a `test_` prefix). A name that merely contains `test` (`attest_render`, `latest`) is no test.
- A test is `likely` affected when its relation to the symbol resolved, and only `possible` when it is ambiguous or unresolved. `impact_analysis` adds the TOON columns `test` (the reason) and `affected` (`likely` or `possible`), and the text line ends in `(#[test])` or `(#[test], possible)` plus the ambiguity reason. The pack names the reason in the role: `test:#[test]`, `test:path`, `test:name`.

### Not decided here

JUnit `@Test`, Jest `it`/`test` calls, pytest fixtures and Go `func TestX(t *testing.T)` signatures need their own extractors; until then they fall back to the path and name conventions.

### Verification

`crates/mct-mcp-server/tests/affected_tests.rs`: an inline `#[test]` and `#[tokio::test]` under `src/`, a function under `tests/` with no attribute (`path`), a caller named `attest_render` that is not a test, and a test reached through an ambiguous call (`possible`). `crates/mct-lang-rust/tests/parse.rs` checks which attributes declare a test.
