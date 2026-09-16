# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An MCP server (`ccm-mcp-server`) and CLI (`ccm-cli`) that index a code repository — any supported language, identically on any OS — via tree-sitter AST parsing into a symbol graph stored in SQLite. It exposes the index as MCP tools (`list_symbols`, `find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `reindex`, `get_indexing_status`) so an agent can get precise project context instead of reading whole files with `Read`/`Grep`/`Glob`.

## Dogfooding: how Claude must explore this repo's own source

This project's entire point is that an agent queries a symbol graph instead of grepping/reading whole files. Claude Code must dogfood that here — using `Read`/`Grep`/`cat`-style whole-file exploration to answer symbol questions about this repo's own source undercuts the thing being built.

**Scope of this rule**: it governs *exploration/lookup* questions — "what's defined in X", "where is Y", "who calls Z", "what does Z call", "what would break if I change Z", "what symbols/functions does file/crate X have". It does **not** apply to reading a file as a precondition for editing it (`Edit` requires a prior `Read`) or to general code-modification work — `Read`/`Grep`/`Edit` remain normal there.

For exploration/lookup questions, the **only** permitted tools are this project's own software: the `ccm-mcp-server` MCP tools (`list_symbols`, `find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `reindex`, `get_indexing_status`) and `ccm-cli` (`crates/ccm-cli`) subcommands. Nothing else — no `Grep`, `Read`, `Bash cat|ls|grep`, `python`, or any other method outside `crates/`:
- **What symbols does a file/crate have, without knowing a name yet?** → `list_symbols` (discovery step — takes a file path or a directory/crate prefix, optional `kind`/`language` filters)
- **Where is X defined?** → `find_symbol`
- **Who calls X directly?** → `find_callers`
- **What does X call?** → `find_calls`
- **Every reference to X** (calls, imports, extends/implements) → `find_references`
- **Full blast radius before changing/removing X** → `impact_analysis`
- **Is the index stale / healthy?** → `get_indexing_status`, and `reindex` only if it looks stale

**`.claude-index/index.sqlite3` may only be touched through this project's own software** — the MCP tools listed above, or `ccm-cli` subcommands. Never open it with an external tool: no `sqlite3` CLI, no `python3`/`sqlite3` module, no DB browser, nothing outside `crates/`. If it isn't a tool this codebase ships, it doesn't get to touch the index — full stop.

**Former gap, now closed**: prior to `list_symbols` (added 2026-09-16), none of the MCP tools could answer "what symbols exist in file/crate Y" without an exact name. `list_symbols` covers that now — a file path is matched exactly, a directory/crate path (no file extension) is matched as a prefix, and `kind`/`language` narrow the result. If a *future* question still falls outside every tool's coverage, say so explicitly and ask the user whether to allow `Read`/`Grep` for that specific instance, rather than falling back automatically or treating one approval as a standing exception.

**When Read/Grep are still correct** (unrelated to the gap above): the index only captures symbols and relations, not comment/doc-string prose — checking whether a doc-comment's *wording* still matches something requires `Grep`/`Read`. `Glob` for listing/finding files by pattern is fine (it's not a symbol lookup).

**If the MCP tools are absent from the deferred-tools list** (not even shown as "failed to connect" — just missing): that means the session started before `.mcp.json` was written/updated, or before `ccm-mcp-server` was built. Tell the user to restart Claude Code / reconnect MCP rather than treating direct sqlite queries as the steady-state solution — sqlite-direct is a same-session fallback only, and even then only through this project's own tooling per the rule above.

## Commands

```sh
cargo build --workspace --all-targets                              # build everything
cargo test --workspace                                              # run all 222+ tests
cargo test -p ccm-lang-go                                            # run one crate's tests
cargo test -p ccm-lang-go idiomatic_syntax                           # run one test by name
cargo clippy --workspace --all-targets --all-features -- -D warnings # lint (CI-gating, zero warnings)

cargo run -p ccm-cli -- --root . init                                # first index of a project
cargo run -p ccm-cli -- --root . status                              # coverage / health report
cargo run -p ccm-cli -- --root . reindex --force
cargo run -p ccm-cli -- --root . mcp-register [--name N]             # write/merge .mcp.json for this project
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
ccm-cli      init/reindex/status/mcp-register subcommands for manual/scripted use
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
