# codebase-context-mcp

An MCP server and installable Claude Code plugin that indexes a code repository — in any supported language, identically on any OS — using AST parsing (tree-sitter) into a symbol graph stored in SQLite. It exposes that index as MCP tools (`find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `reindex`, `get_indexing_status`) so Claude Code can get precise project context from the index instead of reading whole files with `Read`/`Grep`/`Glob` — cutting token spend without losing context quality.

Languages are **plugins**, not a hardcoded list: a `LanguageParser` trait in `ccm-core` is the entire integration surface. Adding a language means implementing that trait in a new crate and registering it — no changes to the indexing engine or the MCP server.

## Status

7 languages implemented end-to-end (Rust, Python, JS/TS, Java, C#, C++, Go), plus a Lua acceptance-test crate validating the plugin architecture without touching `ccm-core` or `ccm-mcp-server`. See [`checklist.md`](checklist.md) for the current state of every deliverable.

## Stack

Rust (multi-crate workspace) · [tree-sitter](https://tree-sitter.github.io/tree-sitter/) + per-language grammars · [`rusqlite`](https://docs.rs/rusqlite) (SQLite, WAL, FTS5) · [`git2`](https://docs.rs/git2) (blob-hash based change detection) · [`rmcp`](https://github.com/modelcontextprotocol/rust-sdk) (official Rust MCP SDK) · `clap` · `thiserror`

No network calls by default — zero telemetry. Source code is parsed statically only; nothing indexed is ever executed or evaluated.

## Installation

Requires a Rust toolchain ([rustup.rs](https://rustup.rs)). Works identically on Linux, macOS, and Windows.

```sh
# From source, anywhere:
cargo install --path crates/ccm-cli
cargo install --path crates/ccm-mcp-server
```

As a Claude Code plugin: point the plugin's MCP server entry at the built `ccm-mcp-server` binary; it indexes `--root <project>` (defaults to the current directory) automatically at startup.

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
| C++ | Implemented | `crates/ccm-lang-cpp` |
| Go | Implemented | `crates/ccm-lang-go` |
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
  ccm-lang-js-ts     LanguageParser impl for JavaScript/TypeScript/TSX (tree-sitter-javascript, tree-sitter-typescript)
  ccm-lang-cpp       LanguageParser impl for C++ (tree-sitter-cpp)
  ccm-lang-go        LanguageParser impl for Go (tree-sitter-go)
  ccm-lang-lua       LanguageParser impl for Lua — plugin-architecture acceptance test, not registered in production
  ccm-mcp-server     MCP tools over stdio (rmcp) — find_symbol/find_references/find_calls/find_callers/impact_analysis/reindex/get_indexing_status
  ccm-cli            init/reindex/status subcommands for manual or scripted use
```

## Running tests

```sh
cargo test --workspace
```

95 tests across the workspace: `ccm-index` (reindex/query pipeline, incremental skip, deletion, syntax-error/unsupported-language reporting, secret-pattern exclusion), each of the 8 language crates (idiomatic-syntax extraction at the parser level — generics, traits/impls, decorators, imports, overloads, interfaces — with an added end-to-end `ccm-index` integration fixture for every crate except `ccm-lang-rust`/`ccm-lang-python`, covering language-specific cases like Go's implicit interfaces or C++'s header/source declaration correlation), and `ccm-mcp-server` (all 7 tools against a versioned Rust+Python fixture, plus a dedicated 3-language Go+TypeScript+Python fixture confirming the index doesn't bleed symbols across languages).

## Benchmark of tokens saved

Planned, not yet implemented — see `checklist.md`. Will compare token cost of answering "find the definition of X" / "what calls this" / "who uses this symbol" via the MCP tools versus via `Read`/`Grep`/`Glob`, across at least 3 supported languages.

## How to add a new language

1. Create a new crate, e.g. `crates/ccm-lang-go`, depending on `ccm-core` and the relevant `tree-sitter-<lang>` grammar crate.
2. Implement `ccm_core::LanguageParser`: `language_id()`, `file_extensions()`, and `parse()` — walk the tree-sitter AST and emit `SymbolRecord`s (functions, methods, classes/structs, types) and `SymbolRelation`s (calls, imports, extends/implements, references). See `ccm-lang-rust/src/lib.rs` or `ccm-lang-python/src/lib.rs` for the pattern (module-root pseudo-symbol for file-level relations, an id-map from local to database symbol ids handled downstream by `ccm-index`).
3. Register it: add one `registry.register(Arc::new(YourLangParser))` line in `ccm-mcp-server/src/registry.rs` and `ccm-cli/src/main.rs::build_registry`. No other file in `ccm-core`, `ccm-index`, or `ccm-mcp-server` changes — that's the architecture's whole point.
4. Add fixtures covering idiomatic syntax for the language (generics, decorators, whatever the language's equivalent is) in `crates/ccm-lang-<lang>/tests/parse.rs`, following the existing tests.
5. Update the language coverage table in this README and in `checklist.md`.
6. Run the token benchmark for the new language once it exists (see above).

See `CONTRIBUTING.md` for the same checklist in contributor-facing form.

## License

Licensed under the [Apache License 2.0](LICENSE).
