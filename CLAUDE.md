# CLAUDE.md

Guidance for Claude Code (claude.ai/code) when working with this repository.

## What this is

An MCP server (`mct-mcp-server`) and CLI (`mct-cli`) that index a code repository — any supported language, identically on any OS — via tree-sitter AST parsing into a symbol graph stored in SQLite. The index is published as MCP tools so an agent gets precise project context instead of reading whole files with `Read`/`Grep`/`Glob`.

## Dogfooding: how Claude must explore this repo's own source

**The rule:** for *exploration/lookup* questions about this repo's own source, the only permitted tools are this project's own software — the `mct-mcp-server` MCP tools and `mct-cli` (`crates/mct-cli`) subcommands. Nothing else: no `Grep`, `Read`, `Bash cat|ls|grep`, `python`, or any other method outside `crates/`. This project's entire point is that an agent queries a symbol graph instead of grepping whole files; doing it the old way here undercuts the thing being built.

**Scope:** exploration/lookup means "what's defined in X", "where is Y", "who calls Z", "what does Z call", "what would break if I change Z", "what symbols does file/crate X have". It does **not** cover reading a file as a precondition for editing it (`Edit` requires a prior `Read`), or general code-modification work — `Read`/`Grep`/`Edit` remain normal there.

| Question | Tool |
|---|---|
| What symbols does a file/crate have, without knowing a name yet? | `list_symbols` — the discovery step; a file path matches exactly, a directory/crate path (no extension) matches as a prefix, `kind`/`language` narrow it |
| What is one file's overall shape (top-level declarations, bodies collapsed)? | `get_file_skeleton` — single file; use `list_symbols` first if you don't know which file |
| What does an unfamiliar file/directory/crate/project look like as a whole? | `get_project_overview` — capped hierarchical digest (modules, key symbols, top callers) in one call; coarser, so switch to the two above once you know the file |
| What's the directory/file layout, before you know which file or crate to look at? | `get_file_tree` — plain directory tree (no symbol data), depth-limited, pruned of the same noise dirs (`target`, `node_modules`, `.git`) reindexing skips |
| Where is X defined? | `find_symbol` |
| Who calls X directly? | `find_callers` |
| What does X call? | `find_calls` |
| Every reference to X (calls, imports, extends/implements) | `find_references` |
| Full blast radius before changing/removing X | `impact_analysis` |
| Candidate unused functions/classes/structs/enums/traits/interfaces/type-aliases (heuristic — zero indexed references, not true visibility) | `find_dead_code` |
| Is the index stale / healthy? | `get_indexing_status`, and `reindex` only if it looks stale |

`find_references`/`find_calls`/`find_callers`/`impact_analysis` also take `depth` (multi-hop BFS beyond the direct hit, default 1, clamped to 32) and `offset` (pagination past `limit`) — reach for `depth` before manually chaining calls to walk the relation graph further out.

**`.mct-index/index.sqlite3` may only be touched through this project's own software** — the MCP tools above or `mct-cli` subcommands. Never an external tool: no `sqlite3` CLI, no `python3`/`sqlite3` module, no DB browser. If this codebase doesn't ship it, it doesn't get to touch the index.

**When `Read`/`Grep` are still correct:** the index captures symbols and relations, not comment/doc-string prose — checking whether a doc-comment's *wording* still matches something requires `Grep`/`Read`. `Glob` for listing/finding files by pattern is fine (it's not a symbol lookup).

**If a question falls outside every tool's coverage:** say so explicitly and ask the user whether to allow `Read`/`Grep` for that specific instance. Never fall back automatically, and never treat one approval as a standing exception.

**If the MCP tools are absent from the deferred-tools list** (not shown as "failed to connect" — simply missing): the session started before `.mcp.json` was written/updated, or before `mct-mcp-server` was built. Tell the user to restart Claude Code / reconnect MCP rather than treating direct sqlite queries as the steady state — sqlite-direct is a same-session fallback only, and even then only through this project's own tooling.

**If the MCP server shows `CONNECTION_CLOSED`** (a genuine connect failure, not "absent from the list" above): this usually means `.mct-index/index.sqlite3` was migrated by a branch with more `M::up` entries in `crates/mct-index/src/schema.rs` than the branch currently checked out — the checked-out binary sees a migration number "from the future" and aborts on startup with `migration error: Attempt to migrate a database with a migration number that is too high`. Confirm by running the server binary directly (`./target/debug/mct-mcp-server.exe --root <path>` and feeding it a JSON-RPC `initialize` request over stdin) — the error prints there even though Claude Code only reports `CONNECTION_CLOSED`. Fix: delete `.mct-index/index.sqlite3` and rebuild it for the current branch's schema with `cargo run -p mct-cli -- --root . init` (the index is fully derived from source, safe to delete), then `/mcp` in Claude Code to reconnect. This will recur any time you switch between branches with a different migration count without reindexing first.

## Commands

```sh
cargo build --workspace --all-targets                              # build everything
cargo test --workspace                                              # run all 222+ tests
cargo test -p mct-lang-go                                            # run one crate's tests
cargo test -p mct-lang-go idiomatic_syntax                           # run one test by name
cargo clippy --workspace --all-targets --all-features -- -D warnings # lint (CI-gating, zero warnings)

cargo run -p mct-cli -- --root . init                                # first index of a project
cargo run -p mct-cli -- --root . status                              # coverage / health report
cargo run -p mct-cli -- --root . reindex --force
cargo run -p mct-cli -- --root . mcp-register [--name N]             # write/merge .mcp.json for this project
cargo run -p mct-mcp-server -- --root <project>                      # run the MCP server over stdio
```

CI (`.github/workflows/ci.yml`) runs `build-test` (build+test+clippy) on Linux/macOS/Windows, `cargo-audit` on Linux, and `fuzz-smoke` (30s `cargo-fuzz` runs per language crate) on Linux/macOS only — fuzzing is deliberately excluded on Windows (ASan DLL/MSVC-sancov issues, see the comment above that job).

**Windows:** no system deps beyond the standard Rust MSVC toolchain (`link.exe`/`cl.exe` via VS Build Tools' "Desktop development with C++", which `rustup` already prompts for). `git2` builds with `default-features = false` (no ssh/https transport, no OpenSSL) — this project only reads local repo state for blob-hash change detection, never clones/fetches.

## Architecture

**The plugin boundary is the whole design.** `mct-core` defines `LanguageParser` (`language_id()`, `file_extensions()`, `parse()`) and `LanguageRegistry` (extension → parser lookup), and knows nothing about tree-sitter, SQLite, or MCP. Each `crates/mct-lang-*` crate implements that trait for one language via its own tree-sitter grammar. `mct-index` and `mct-mcp-server` never match on language names or extensions directly — everything routes through the registry.

```
mct-core     LanguageParser trait, SymbolRecord/SymbolRelation model, LanguageRegistry
mct-index    SQLite schema/migrations (a single schema for every language — a `language`
             column on `files`, not per-language tables), reindex orchestration, queries.
             Knows no language's grammar.
mct-lang-*   One crate per language, each a LanguageParser impl over its tree-sitter grammar
mct-mcp-server  MCP tools over stdio (rmcp): list_symbols/find_symbol/find_references/
                find_calls/find_callers/impact_analysis/find_dead_code/reindex/
                get_indexing_status/get_file_skeleton/get_project_overview/get_file_tree
mct-cli      init/reindex/status/mcp-register subcommands for manual/scripted use
```

**Adding a language touches exactly these places** (full checklist, including fuzz harnesses, in `CONTRIBUTING.md`):

1. New crate `crates/mct-lang-<name>`, depending on `mct-core` + `tree-sitter-<name>`.
2. Implement `LanguageParser::parse()` — a pure AST walk that never executes/evals input; a syntax error returns `ParseError::Syntax`, never a panic (this runs over arbitrary third-party source).
3. Register in exactly two places: `mct-mcp-server/src/registry.rs::build_registry` and `mct-cli/src/main.rs::build_registry`. Nothing else in `mct-core`, `mct-index`, or `mct-mcp-server` changes.
4. Tests in `crates/mct-lang-<name>/tests/parse.rs`: function/call extraction, one idiomatic-syntax case (generics, decorators, whatever the language's equivalent is), one syntax-error case.
5. Update the language table in `README.md` and `internal/checklist.md`.

**Changing the `LanguageParser` trait, the SQLite schema, or a published MCP tool signature** is cross-cutting (every language crate, every index, and Claude Code's live tool contract) — open an issue first, don't drive-by PR it.

**Code-style invariants across all crates:** no `unwrap()`/`panic!` on any path processing repo-input content (`.unwrap_or_default()` / early return instead) — parsers run over arbitrary, potentially adversarial source; single responsibility per crate (`mct-core` doesn't know SQLite, a `mct-lang-*` crate doesn't know SQLite or MCP).

Commit format: `type(scope): short description` (`feat`/`fix`/`refactor`/`docs`/`test`/`chore`/`perf`), breaking changes marked explicitly.
