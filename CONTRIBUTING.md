# Contributing

## Adding a new language

This is the extension point most contributions will touch, so it gets its own checklist:

1. **New crate.** `crates/mct-lang-<name>`, depending on `mct-core` (path dependency) and `tree-sitter-<name>` (pin an exact version).
2. **Implement `mct_core::LanguageParser`:**
   - `language_id()` — a stable lowercase identifier (`"go"`, `"kotlin"`), stored in the index's `language` column.
   - `file_extensions()` — extensions this parser owns, without the leading dot. Two parsers can never claim the same extension (`LanguageRegistry::register` panics on conflict — a startup-time configuration error, not something repo content can trigger).
   - `parse()` — must never execute, `eval`, or otherwise run any part of the input; parsing is purely AST-based. A syntax error in the input is a `ParseError::Syntax` return value, never a panic — this method runs over arbitrary third-party source.
3. **No `unwrap()`/`panic!`** on any path that processes file content from the indexed repo. `.unwrap_or_default()` / early-return on `Option`/`Result` instead.
4. **Register it** — three places, nowhere else changes:
   - `mct-mcp-server/src/registry.rs::build_registry`
   - `mct-cli/src/main.rs::build_registry`
   - if you're also adding a `fuzz/` harness for it (see step 5a below), the `matrix.crate` list in `.github/workflows/ci.yml`'s `fuzz-smoke` job — otherwise CI silently never fuzzes it.
5. **Tests** in `crates/mct-lang-<name>/tests/parse.rs`:
   - A function-and-call extraction test.
   - At least one test covering the language's distinctive idiomatic syntax (generics for C#/Java, templates for C++, decorators for Python — whatever the equivalent is for your language).
   - A syntax-error case asserting `ParseError::Syntax`, not a panic.
5a. **Add a `cargo-fuzz` harness** (`crates/mct-lang-<name>/fuzz/`) — copy the structure of an existing one (e.g. `mct-lang-lua/fuzz/`): a standalone-workspace `Cargo.toml` (`[workspace]` empty table, so it stays out of the root workspace's members/lockfile) and one `fuzz_targets/parse_<name>.rs` calling `<Name>Parser::parse` on arbitrary bytes. Remember the CI matrix entry from step 4.
6. **Update docs:** the language table in `README.md` and the "Cobertura de lenguajes" section in `internal/checklist.md`.
7. **Run the workspace test suite** (`cargo test --workspace`) and fix any regressions before opening a PR — or `scripts/unix/check.sh`, which runs the tests, the CI clippy invocation and the `mct-eval` gate and prints only a summary (see `scripts/README.md`).

## Framework/library coverage, beyond the language table

The language table in `README.md` says which *languages* have a `LanguageParser`. It does not say how well the index captures the *frameworks/libraries* built on top of those languages — a `.tsx` file parses fine, but that doesn't mean every structural relationship a framework introduces is modeled. Tracked in [#51](https://github.com/Zubiarka8/mini-consumes-tokens/issues/51):

| Library/framework | Status | Gap |
|---|---|---|
| React (`.jsx`/`.tsx`) | Partial | `mct-lang-js-ts` indexes function/class components, hooks calls, and event-handler methods as ordinary `Function`/`Class`/`Method`/`Calls`. JSX elements (`<Foo prop={x} />`) have no dedicated `SymbolKind`/`RelationKind`, so "what does `<App/>` render" or "who renders `<Button/>`" can't be answered from the index — deferred pending an `mct-core` symbol-model extension (cross-cutting, see below). |
| Vue (`.vue` SFCs) | Not supported | No `mct-lang-vue` crate and no `.vue` extension registered in any `build_registry`. A `.vue` file is invisible to the index entirely. Vue projects using plain `.ts`/`.js` (Composition API outside SFCs) still get normal JS/TS coverage. |
| CSS frameworks (Tailwind, Bootstrap, Bulma, etc.) | Partial by design | `mct-lang-css` indexes full selectors and their atomic class/id/tag/pseudo components (compound selectors, descendant combinators, escaped Tailwind utility names). No specificity/cascade modeling (would need a DOM, not an AST) and no indexing of declarations/values inside a rule's block — selectors only. This is a deliberate scope boundary, not a bug. |
| Angular, Svelte, Next.js/Nuxt routing, Express/Fastify-style route registration, server-side templating (Handlebars/EJS/Pug) | Not yet audited | Suspected same shape of gap ("logic is indexed, structural/template/routing relationships are not"), not individually verified against source. Triage before implementing. |

Extending `mct-core`'s symbol/relation model to represent generic "component renders/uses component" relationships (needed for React JSX, Vue, Svelte, Angular alike) is a schema/trait change — it falls under "Changing the `LanguageParser` trait, the SQLite schema, or an already-published MCP tool signature" below, not a per-language drive-by. Adding a new `mct-lang-vue`/`mct-lang-svelte` crate for SFC parsing follows the normal "Adding a new language" checklist above, but note SFCs mix `<template>`/`<script>`/`<style>` in one file, so the parser needs to walk more than one grammar/section.

## Adding a new MCP tool, and the TTC description format

`crates/mct-mcp-server/src/server.rs` defines each tool's name, parameters and `#[tool(...)]` attribute, but that attribute's `description` is only a short fallback literal — the description an MCP client actually receives comes from `crates/mct-mcp-server/src/tools.ttc`, parsed by `crates/mct-mcp-server/src/ttc.rs` and applied to the tool router in `MctServer::new`. This is TTC (Tool Terse Catalog): a compact `WHEN`/`ERR`/`TAGS` format that replaced long prose descriptions to shrink the catalog's token footprint (~68% smaller across the 11 tools as of this writing) without dropping the "when to use this" / "when NOT to" information an agent actually needs.

When adding a tool, add a block to `tools.ttc`:

```
TOOL your_tool_name
WHEN one line: when an agent should reach for this tool.
ERR  one line: what NOT to use it for, or its caveats/failure modes.
TAGS comma-separated short keywords, for discovery.
```

`WHEN`/`ERR`/`TAGS` may appear in any order after the `TOOL` line, but each must appear exactly once, on a single line — no wrapping. A line starting with `#` is a comment anywhere in the file. `ttc::parse` never panics on malformed input (a missing/duplicate field, an unrecognized line, a bad `TOOL` header all return a `TtcParseError`); if it fails at server startup, `MctServer::new` logs a warning and falls back to every tool's compiled-in `#[tool(description = "...")]` literal instead of refusing to start.

Two tests keep `tools.ttc` and `server.rs` from drifting apart:
- `ttc::tests::catalog_source_parses_and_covers_every_known_tool` (in `ttc.rs`) checks `tools.ttc` parses and has exactly one block per name in `ttc::KNOWN_TOOL_NAMES` — update that list when adding/removing a tool.
- `catalog.rs` (integration test) checks the *live* tool router's installed descriptions actually equal `tools.ttc`'s expansion, not the compiled-in fallback.

## Windows build prerequisites

Building this workspace on Windows needs nothing beyond the standard Rust-on-Windows setup — no manual system dependency to install for `git2` or any tree-sitter grammar specifically:

- **`git2`** is a dependency with `default-features = false` (see `mct-index/Cargo.toml`) — it skips the `ssh`/`https` transports and their system OpenSSL/libssh2 requirement, since this project only reads local repo state (blob hashing), never clones/fetches/pushes. No OpenSSL install needed on any OS for this reason.
- **tree-sitter grammars** (and `rusqlite`'s bundled SQLite) are plain C, compiled via the `cc` crate at build time. On Windows that means the MSVC linker/toolset (`link.exe`, `cl.exe`) must be available — in practice, the **Desktop development with C++** workload from Visual Studio Build Tools (or full Visual Studio), which `rustup`'s own Windows installer already prompts for as a prerequisite of the `x86_64-pc-windows-msvc` toolchain. If that's installed (required to use Rust on Windows at all), building this workspace needs nothing further.
- GitHub's `windows-latest` Actions runner ships Visual Studio 2022 with the C++ toolset preinstalled, so CI needs no extra setup step for this either — confirmed by `.github/workflows/ci.yml`'s `build-test` job actually building and testing green on `windows-latest`.

If `cargo build` fails on Windows with a `link.exe`/`cl.exe`-not-found error, the fix is installing the C++ Build Tools workload, not a project-specific dependency.

## Continuous integration

`.github/workflows/ci.yml` has three jobs:

- **`build-test`** — matrix over Linux/macOS/Windows: `cargo build --workspace --all-targets`, `cargo test --workspace`, `cargo clippy --workspace --all-targets --all-features -- -D warnings`. Runs on every push/PR, on all three OS, no exceptions.
- **`cargo-audit`** — Linux only (one run is enough for a dependency audit). Two passes: a full report of every severity (informational, never fails the job — low/medium findings stay visible in the log), then a second pass with `severity_threshold = "high"` in a generated `.cargo/audit.toml`, whose exit code is what actually gates the job. Advisories with no CVSS score bypass the severity filter and always fail this step (fail-safe for unscored issues).
- **`fuzz-smoke`** — matrix over `{ubuntu-latest, macos-latest} × {every crate listed in matrix.crate}` (8 as of C++/Go — check the workflow file for the current, authoritative list rather than this count, which will go stale again), short (30s) `cargo-fuzz` campaigns confirming each harness builds and runs without crashing. **Deliberately excludes Windows** — see the long comment above that job in the workflow file for exactly why (an ASan runtime DLL PATH issue and a separate MSVC linker limitation with sancov instrumentation, both confirmed locally before this decision was made). The regular `build-test` job still covers Windows fully; only fuzzing is Linux/macOS-only.

## Quality evaluation

`crates/mct-eval` scores the MCP tools on accuracy, MRR, success rate, latency and token cost over a fixed suite (`crates/mct-eval/suite.json`, run against the `omni-app` fixture) and compares the run with `crates/mct-eval/baseline.json` — see `benchmarks/quality-eval.md` for the metrics and thresholds.

- **Per-PR gate:** `crates/mct-eval/tests/regression.rs` runs inside `cargo test --workspace`, so `build-test` fails on any regression.
- **Monthly report:** `.github/workflows/quality-report.yml` (1st of each month, or on demand via *Run workflow*) re-runs the suite in a release build and publishes the report as the job summary plus a JSON/Markdown artifact.

A change that intentionally moves a number (a new case, a better ranking, a deliberate output change) refreshes the baseline in the same PR: `cargo run -p mct-eval -- --write-baseline`, then commit `baseline.json`.

## Changing the `LanguageParser` trait, the SQLite schema, or an already-published MCP tool signature

These are cross-cutting: the trait is implemented by every language crate, the schema is shared by every language's data, and a published tool signature is part of the contract Claude Code agents rely on. Open an issue describing the change before sending a PR — these need explicit sign-off, not a drive-by PR.

## Code style

- No `unwrap()`/`panic!` on repo-input processing paths, in any crate.
- Single responsibility per crate: `mct-core` doesn't know about any specific language or about SQLite; a `mct-lang-*` crate doesn't know about SQLite or MCP.
- Comment the *why*, not the *what* — non-obvious constraints, not a restatement of the code.

## Commit messages

`type(scope): short description` — types: `feat` / `fix` / `refactor` / `docs` / `test` / `chore` / `perf`. Mark breaking changes explicitly.
