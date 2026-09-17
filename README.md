# mini-consumes-tokens

An MCP server and installable Claude Code plugin that indexes a code repository — in any supported language, identically on any OS — using AST parsing (tree-sitter) into a symbol graph stored in SQLite. It exposes that index as MCP tools (`list_symbols`, `find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `reindex`, `get_indexing_status`) so Claude Code can get precise project context from the index instead of reading whole files with `Read`/`Grep`/`Glob` — cutting token spend without losing context quality.

Languages are **plugins**, not a hardcoded list: a `LanguageParser` trait in `ccm-core` is the entire integration surface. Adding a language means implementing that trait in a new crate and registering it — no changes to the indexing engine or the MCP server.

## Status

15 languages implemented end-to-end (Rust, Python, JS/TS, Java, C#, C++, Go, HTML, CSS, XML, XAML, Bash, PowerShell, PHP, Markdown), plus a Lua acceptance-test crate validating the plugin architecture without touching `ccm-core` or `ccm-mcp-server`. Two pairs cross-reference each other in the same index: HTML/CSS (an element's `id`/`class` attributes resolve to the matching CSS rule, `<link>`/`<script src>` resolve as `Imports`) and XAML/C# (an event-handler attribute like `Click="SaveBtn_Click"` resolves to the matching method in the paired code-behind file). Plain XML is deliberately structural-only, and Markdown (Phase 1) is headings-only with no relations yet — see the coverage table below. See [`checklist.md`](checklist.md) for the current state of every deliverable.

Beyond symbols, `get_indexing_status` also reports the project's declared dependencies: `Cargo.toml`, `package.json`, `requirements.txt`, and `go.mod` are detected by file name (not routed through a `LanguageParser` — they aren't source code) and their direct dependencies recorded per manifest.

## Stack

Rust (multi-crate workspace) · [tree-sitter](https://tree-sitter.github.io/tree-sitter/) + per-language grammars · [`rusqlite`](https://docs.rs/rusqlite) (SQLite, WAL, FTS5) · [`git2`](https://docs.rs/git2) (blob-hash based change detection) · [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk) (official Rust MCP SDK) · `clap` · `thiserror`

No network calls by default — zero telemetry. Source code is parsed statically only; nothing indexed is ever executed or evaluated.

## Installation

**Prebuilt binaries** (no Rust toolchain needed): download the archive for your OS/arch (Linux x86_64/arm64, macOS Intel/Apple Silicon, Windows x86_64) from the [GitHub Releases page](https://github.com/zubiarka8/mini-consumes-tokens/releases) and put `ccm-cli`/`ccm-mcp-server` on your `PATH`.

**From crates.io** (once published — see `RELEASING.md`; requires a Rust toolchain, [rustup.rs](https://rustup.rs)):

```sh
cargo install ccm-cli
cargo install ccm-mcp-server
```

**From source**, anywhere (works identically on Linux, macOS, and Windows):

```sh
cargo install --path crates/ccm-cli
cargo install --path crates/ccm-mcp-server
```

As a Claude Code plugin: point the plugin's MCP server entry at the released `ccm-mcp-server` binary (prebuilt or `cargo install`d — not an unversioned source checkout); it indexes `--root <project>` (defaults to the current directory) automatically at startup.

```sh
ccm-cli --root . init      # first index
ccm-cli --root . status    # coverage / health report
ccm-cli --root . reindex --force
```

## Supported languages

| Language | Status | Crate |
|---|---|---|
| Rust | Implemented | `crates/ccm-lang-rust` |
| Python | Implemented | `crates/ccm-lang-python` |
| JavaScript / TypeScript | Implemented | `crates/ccm-lang-js-ts` |
| Java | Implemented | `crates/ccm-lang-java` |
| C# | Implemented | `crates/ccm-lang-csharp` |
| Kotlin | Implemented | `crates/ccm-lang-kotlin` |
| C++ | Implemented | `crates/ccm-lang-cpp` |
| Go | Implemented | `crates/ccm-lang-go` |
| HTML | Implemented | `crates/ccm-lang-html` |
| CSS | Implemented | `crates/ccm-lang-css` |
| XML | Implemented (structural only — no dialect-generic cross-referencing) | `crates/ccm-lang-xml` |
| XAML | Implemented | `crates/ccm-lang-xaml` |
| Bash / POSIX shell | Implemented | `crates/ccm-lang-bash` |
| PowerShell | Implemented (functions/variables/calls/imports only — no classes) | `crates/ccm-lang-powershell` |
| PHP | Implemented | `crates/ccm-lang-php` |
| Markdown | Implemented (Phase 1 — ATX headings only, no relations yet: no internal links, anchors, setext headings, lists, tables, or code blocks) | `crates/ccm-lang-md` |
| Lua | Implemented (plugin-architecture acceptance test, not wired into production) | `crates/ccm-lang-lua` |

`get_indexing_status` reports, per repo, which of these it saw files for but has no parser registered yet — so a polyglot repo with an unsupported language degrades gracefully (that language's files are just skipped and reported) rather than failing the whole index.

## Workspace structure

```
crates/
  ccm-core          LanguageParser trait, symbol/relation model, LanguageRegistry — knows no language, no storage
  ccm-index         SQLite schema/migrations, reindex orchestration, queries — knows no language's grammar
  ccm-lang-rust      LanguageParser impl for Rust (tree-sitter-rust)
  ccm-lang-python    LanguageParser impl for Python (tree-sitter-python)
  ccm-lang-java      LanguageParser impl for Java (tree-sitter-java)
  ccm-lang-csharp    LanguageParser impl for C# (tree-sitter-c-sharp)
  ccm-lang-kotlin    LanguageParser impl for Kotlin (tree-sitter-kotlin-ng) — class/interface (grammar shares one node kind, distinguished by the anonymous `interface` token), object as singleton Class, extension functions attached to their receiver type, primary-constructor val/var property promotion, Extends/Implements distinguished by constructor-call vs. bare type in the supertype list
  ccm-lang-js-ts     LanguageParser impl for JavaScript/TypeScript/TSX (tree-sitter-javascript, tree-sitter-typescript)
  ccm-lang-cpp       LanguageParser impl for C++ (tree-sitter-cpp)
  ccm-lang-go        LanguageParser impl for Go (tree-sitter-go)
  ccm-lang-html      LanguageParser impl for HTML (tree-sitter-html) — id'd elements + id/class References, link/script Imports
  ccm-lang-css       LanguageParser impl for CSS (tree-sitter-css) — simple-selector Rules + @import
  ccm-lang-xml       LanguageParser impl for generic XML (tree-sitter-xml) — id/name/Name Elements, structural only
  ccm-lang-xaml      LanguageParser impl for XAML (tree-sitter-xml) — x:Name/Name Elements + event-attribute References into C# code-behind
  ccm-lang-bash      LanguageParser impl for Bash/POSIX shell (tree-sitter-bash) — functions, top-level variables, calls, source/. Imports
  ccm-lang-powershell LanguageParser impl for PowerShell (tree-sitter-powershell) — functions, top-level variables, calls, dot-source/Import-Module Imports
  ccm-lang-php       LanguageParser impl for PHP (tree-sitter-php) — classes/interfaces/traits/enums, methods/fields (incl. constructor property promotion), extends/implements, trait-use, calls, require/use Imports
  ccm-lang-md        LanguageParser impl for Markdown (tree-sitter-md) — ATX headings as nested Element symbols via the grammar's own section nesting, Phase 1, no relations yet
  ccm-lang-lua       LanguageParser impl for Lua — plugin-architecture acceptance test, not registered in production
  ccm-mcp-server     MCP tools over stdio (rmcp) — list_symbols/find_symbol/find_references/find_calls/find_callers/impact_analysis/reindex/get_indexing_status
  ccm-cli            init/reindex/status subcommands for manual or scripted use
```

## Running tests

```sh
cargo test --workspace
```

222 tests across the workspace: `ccm-index` (reindex/query pipeline, incremental skip, deletion, syntax-error/unsupported-language reporting, secret-pattern exclusion, manifest dependency detection), each of the 15 production language crates (idiomatic-syntax extraction at the parser level — generics, traits/impls, decorators, imports, overloads, interfaces — with an added end-to-end `ccm-index` integration fixture for every crate except `ccm-lang-rust`/`ccm-lang-python`, covering language-specific cases like Go's implicit interfaces, C++'s header/source declaration correlation, HTML/CSS cross-referencing each other by id/class, a XAML event-handler attribute resolving to a method in its paired C# code-behind file, PHP's `self::`/`parent::`/`static::` scoped calls, constructor property promotion, and trait composition, or Markdown's nested-heading `section` hierarchy — each through a real multi-file, multi-language fixture, not just both parsers running side by side), `ccm-mcp-server` (all 7 tools against a versioned Rust+Python fixture, plus a dedicated 3-language Go+TypeScript+Python fixture confirming the index doesn't bleed symbols across languages), and `ccm-cli` (`mcp-register`'s config-merge behavior: creating a new `.mcp.json`, preserving other already-configured servers, and stripping Windows' `\\?\` verbatim-path prefix from the written `--root`).

## Benchmark of tokens saved

Measured (see `benchmarks/token-benchmark.md` for full methodology and per-query tables): three canonical queries — "find the definition of X" (`find_symbol`), "what calls this function" (`find_callers`), "who uses this symbol" (`find_references`) — compared as MCP tool-call output (characters returned) versus a realistic `Grep`+`Read` baseline (grep the term across the fixture, then read every matched file in full).

15 of the 16 language crates (the 14 original production languages plus Lua, kept as the plugin-architecture validation case) now have real measured numbers on small fixture repos. Most results cluster in the high-80s to high-90s percent character reduction, topping out at 97.8% (Go). Two honest caveats, not smoothed over: `find_symbol` on a tiny XML fixture measured 72.0%, the lowest of any query benchmarked; and `find_callers` on CSS/HTML/XAML, plus both relation queries on XML, come out structurally close to 100% because those parsers never emit the relation being queried (declarative/markup languages have no `Calls` relation, and `ccm-lang-xml` emits no relations at all by design, see `MANUAL.md` §12) — real numbers, but not genuine navigation wins. Markdown (`ccm-lang-md`, added after this benchmark round) has no measured row yet — its Phase 1 scope emits no relations at all, so only `find_symbol` would produce a non-degenerate number. See `benchmarks/token-benchmark.md` for the full per-language, per-query table before citing a specific figure.

## How to add a new language

1. Create a new crate, e.g. `crates/ccm-lang-go`, depending on `ccm-core` and the relevant `tree-sitter-<lang>` grammar crate.
2. Implement `ccm_core::LanguageParser`: `language_id()`, `file_extensions()`, and `parse()` — walk the tree-sitter AST and emit `SymbolRecord`s (functions, methods, classes/structs, types) and `SymbolRelation`s (calls, imports, extends/implements, references). See `ccm-lang-rust/src/lib.rs` or `ccm-lang-python/src/lib.rs` for the pattern (module-root pseudo-symbol for file-level relations, an id-map from local to database symbol ids handled downstream by `ccm-index`).
3. Register it: add one `registry.register(Arc::new(YourLangParser))` line in `ccm-mcp-server/src/registry.rs` and `ccm-cli/src/main.rs::build_registry`. No other file in `ccm-core`, `ccm-index`, or `ccm-mcp-server` changes — that's the architecture's whole point.
4. Add fixtures covering idiomatic syntax for the language (generics, decorators, whatever the language's equivalent is) in `crates/ccm-lang-<lang>/tests/parse.rs`, following the existing tests.
5. Update the language coverage table in this README and in `checklist.md`.
6. Run the token benchmark for the new language once it exists (see above).

See `CONTRIBUTING.md` for the same checklist in contributor-facing form.

## License

Licensed under [MIT](LICENSE-MIT).
