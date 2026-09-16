# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An MCP server (`ccm-mcp-server`) and CLI (`ccm-cli`) that index a code repository — any supported language, identically on any OS — via tree-sitter AST parsing into a symbol graph stored in SQLite. It exposes the index as MCP tools (`find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `reindex`, `get_indexing_status`) so an agent can get precise project context instead of reading whole files with `Read`/`Grep`/`Glob`.

## Commands

```sh
cargo build --workspace --all-targets                              # build everything
cargo test --workspace                                              # run all 218+ tests
cargo test -p ccm-lang-go                                            # run one crate's tests
cargo test -p ccm-lang-go idiomatic_syntax                           # run one test by name
cargo clippy --workspace --all-targets --all-features -- -D warnings # lint (CI-gating, zero warnings)

cargo run -p ccm-cli -- --root . init                                # first index of a project
cargo run -p ccm-cli -- --root . status                              # coverage / health report
cargo run -p ccm-cli -- --root . reindex --force
cargo run -p ccm-mcp-server -- --root <project>                      # run the MCP server over stdio
```

CI (`.github/workflows/ci.yml`) runs `build-test` (build+test+clippy) on Linux/macOS/Windows, `cargo-audit` on Linux, and `fuzz-smoke` (30s `cargo-fuzz` runs per language crate) on Linux/macOS only — fuzzing is deliberately excluded on Windows (ASan DLL/MSVC-sancov issues, see the comment above that job).

**Windows**: no extra system deps beyond the standard Rust MSVC toolchain (`link.exe`/`cl.exe` via VS Build Tools' "Desktop development with C++", which `rustup` already prompts for). `git2` builds with `default-features = false` (no ssh/https transport, no OpenSSL) since this project only reads local repo state for blob-hash change detection, never clones/fetches.

## Architecture

**Plugin boundary is the whole design.** `ccm-core` defines `LanguageParser` (a trait: `language_id()`, `file_extensions()`, `parse()`) and `LanguageRegistry` (extension → parser lookup). It knows nothing about tree-sitter, SQLite, or MCP. Each `crates/ccm-lang-*` crate implements that trait for one language via its own tree-sitter grammar. `ccm-index` and `ccm-mcp-server` never match on language names or extensions directly — everything routes through the registry.

```
ccm-core     LanguageParser trait, SymbolRecord/SymbolRelation model, LanguageRegistry
ccm-index    SQLite schema/migrations (single schema for every language — a `language`
             column on `files`, not per-language tables), reindex orchestration, queries.
             Knows no language's grammar.
ccm-lang-*   One crate per language, each a LanguageParser impl over its tree-sitter grammar
ccm-mcp-server  MCP tools over stdio (rmcp): find_symbol/find_references/find_calls/
                find_callers/impact_analysis/reindex/get_indexing_status
ccm-cli      init/reindex/status subcommands for manual/scripted use
```

**Adding a language touches exactly these places** (see `CONTRIBUTING.md` for the full checklist including fuzz harnesses):
1. New crate `crates/ccm-lang-<name>`, depending on `ccm-core` + `tree-sitter-<name>`.
2. Implement `LanguageParser::parse()` — pure AST walk, never executes/evals input; a syntax error returns `ParseError::Syntax`, never a panic (this runs over arbitrary third-party source).
3. Register in exactly two places: `ccm-mcp-server/src/registry.rs::build_registry` and `ccm-cli/src/main.rs::build_registry`. Nothing else in `ccm-core`, `ccm-index`, or `ccm-mcp-server` changes.
4. Tests in `crates/ccm-lang-<name>/tests/parse.rs`: function/call extraction, one idiomatic-syntax case (generics, decorators, whatever the language's equivalent is), one syntax-error case.
5. Update the language table in `README.md` and `checklist.md`.

**Changing the `LanguageParser` trait, the SQLite schema, or a published MCP tool signature** is cross-cutting (every language crate, every index, and Claude Code's live tool contract) — open an issue first, don't drive-by PR it.

**Code-style invariants enforced across all crates**: no `unwrap()`/`panic!` on any path processing repo-input content (`.unwrap_or_default()` / early-return instead) — parsers run over arbitrary, potentially adversarial source; single responsibility per crate (`ccm-core` doesn't know SQLite, a `ccm-lang-*` crate doesn't know SQLite or MCP).

Commit format: `type(scope): short description` (`feat`/`fix`/`refactor`/`docs`/`test`/`chore`/`perf`), breaking changes marked explicitly.
