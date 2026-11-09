# Repository rules

Shared rules for Codex, Claude Code and any other agent working in this repository. This file is the single source of truth for project instructions. `AGENTS.md` and `CLAUDE.md` only point here.

Read it at the start of a session. Open the longer references in [`docs/07-agents/`](docs/07-agents/) only when a task needs them.

## Table of contents

- [Project coordination](#project-coordination)
- [Contributor workflow](#contributor-workflow)
- [What this is](#what-this-is)
- [Dogfooding: exploring this repository's source](#dogfooding-exploring-this-repositorys-source)
- [Commands](#commands)
- [Architecture](#architecture)
- [Reference documents](#reference-documents)

## Project coordination

This file decides architecture, technical restrictions and commands. Use project or global skills when they help.

Write all repository content in English: file and branch names, docs, code, comments, task records, commit messages, PRs and issues. Leave external identifiers and literal fixture data alone when translating would change behavior.

When a project MCP tool reports an error (connection failure, invalid arguments, indexing or parse failure, query failure), tell the user and log it in [errors/REGISTRY.md](errors/REGISTRY.md), following [errors/README.md](errors/README.md).

- Log the date, tool or operation, arguments (no secrets), the exact diagnostic, the impact, and the check or next action.
- A known problem gets its entry updated and the change appended to [errors/HISTORY.md](errors/HISTORY.md). That includes errors fixed in the same session.
- Separate intentional malformed-fixture rejections from unexpected failures, and caller mistakes from server defects.
- Keep resolved entries. Verify before you close one.
- Keep the same records when you investigate a documented parser limitation.

## Contributor workflow

Use any editor or assistant. No AI provider, model, paid plan or multi-agent setup is required.

- Keep changes small and reviewable. Leave local work you cannot explain alone. Record acceptance criteria, verify the behavior you changed when allowed, and report any check you could not run.
- With several writers, give each its own branch and worktree, and coordinate overlapping changes.
- Do not put personal model choices, subscription limits or private agent state in shared docs.
- When a branch is done, remove its worktree to free disk space. First check that nothing still uses it and that every commit, untracked file and report you need is saved elsewhere. Use `git worktree remove` and keep the branch. Never remove the primary checkout or shared resources. Recreate the directory from the branch if you need it later.
- If `LOCAL_INSTRUCTIONS.md` exists, read it as extra instructions for that checkout. It is usually absent and never blocks a contribution. It cannot override the user's instructions, client permissions or the engineering rules here.

## What this is

An MCP server (`mct-mcp-server`) and a CLI (`mct-cli`). They parse a repository with tree-sitter, in any supported language on any OS, into a symbol graph stored in SQLite. The index is served as MCP tools, so an agent gets precise context without reading whole files with `Read`, `Grep` and `Glob`.

## Dogfooding: exploring this repository's source

### The rule

For exploration and lookup questions about this repository's own source, use only this project's software: the `mct-mcp-server` MCP tools and the `mct-cli` subcommands. Do not look inside `crates/` with `Grep`, `Read`, `Bash cat|ls|grep`, `python` or anything else. This project exists so that an agent queries a symbol graph instead of grepping whole files.

### Scope

Exploration means "what is defined in X", "where is Y", "who calls Z", "what does Z call", "what breaks if I change Z", "what symbols does file or crate X have". It does not cover reading a file before you edit it (`Edit` needs a prior `Read`) or general code changes. There, `Read`, `Grep` and `Edit` stay normal.

### Which tool

| Question | Tool |
|---|---|
| Symbols in a file or crate, name unknown | `list_symbols` (file path matches exactly, directory or crate path matches as a prefix) |
| Shape of one file, bodies collapsed | `get_file_skeleton` |
| Overview of an unfamiliar file, directory, crate or project | `get_project_overview` |
| Directory and file layout before choosing a file | `get_file_tree` |
| Where is X defined | `find_symbol` |
| Name unknown, words or partial identifier | `search_symbols_like` |
| Only know what the code does | `hybrid_search_like` (a `"double-quoted"` query is an exact phrase) |
| Search by meaning only, with similarity score | `semantic_search` |
| Who calls X / what does X call | `find_callers` / `find_calls` |
| Every reference to X | `find_references` |
| Blast radius before changing or removing X | `impact_analysis` |
| Which files a file or directory imports, who imports it, import cycles | `get_dependency_graph` |
| Definition, source, callers, callees and tests of X in one call | `build_context_pack` |
| Candidate unused code (heuristic, zero indexed references) | `find_dead_code` |
| Candidate duplicate code, with review decisions | `find_duplicate_code` |
| Several known lookups at once (up to 25, read-only) | `batch` |
| Server started with `MCT_TOOL_ATTENTION=1` (only `select_tools` listed) | `select_tools` with the user's request first |
| Is the index stale or healthy | `get_indexing_status`; `reindex` only if it looks stale |

`find_references`, `find_calls`, `find_callers` and `impact_analysis` take `depth` (default 1, clamped to 32) and `offset`. Use `depth` before you chain calls by hand. Full parameters, budgets, the Markdown literal search and progressive tool discovery are in the [MCP tools reference](docs/07-agents/mcp-tools-reference.md).

### The index file

Touch `.mct-index/index.sqlite3` only through this project's own software, the MCP tools or `mct-cli`. No `sqlite3` CLI, no `python3` or `sqlite3` module, no database browser.

### When `Read` and `Grep` are still right

The index holds symbols and relations, not the wording of comments and doc strings. To check whether a doc comment still matches the code, use `Grep` or `Read`. `Glob` to list files by pattern is fine.

### When no tool covers the question

Say so, and ask the user whether `Read` or `Grep` may be used for that one case. Never fall back on your own, and never treat one approval as a standing exception.

### When the MCP tools are missing or fail

If the tools are absent from the list, the session started before `.mcp.json` or the `mct-mcp-server` build existed. Ask the user to restart the client or reconnect MCP. Direct SQLite access is a same-session fallback only, and only through this project's own tooling. For `CONNECTION_CLOSED`, run `scripts/unix/mcp-smoke.sh` to see the real startup error, then `scripts/unix/reinstall.sh --reindex`. Causes and the full fix are in [MCP troubleshooting](docs/07-agents/mcp-troubleshooting.md).

### Enforcement hooks

`.codex/hooks.json` registers only the direct `sqlite3` CLI guard from `internal/claude-hooks/` (see its `README.md`). Formatting and Clippy are explicit project checks and do not run on every edit or Stop. The retired Dogfooding and Clippy hook entry points are no-ops for clients with cached registrations.

## Commands

When you give the user Git or GitHub commands to paste, make them work from any directory. Use the real, quoted, absolute checkout path with `git -C`. For `gh`, pass `--repo OWNER/REPO` and use absolute paths for local files, or change to the quoted absolute checkout path when the command needs repository context. No placeholders, relative paths or assumed working directory.

```sh
cargo build --workspace --all-targets                                # build everything
cargo test --workspace                                               # run workspace tests
cargo test -p mct-lang-go                                            # one crate's tests
cargo test -p mct-lang-go idiomatic_syntax                           # one test by name
cargo clippy --workspace --all-targets --all-features -- -D warnings # lint (CI-gating, zero warnings)
cargo fmt --all                                                      # format (CI runs `cargo fmt --all --check`)
# CI also denies unwrap/expect/panic project-wide:
#   cargo clippy --workspace --all-targets --all-features -- -D warnings \
#     -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic
# A src, tests or examples file that legitimately uses them (test or bench code, never a path
# that processes repo content) needs its own #![allow(clippy::unwrap_used, clippy::expect_used,
# clippy::panic)]. See crates/mct-mcp-server/tests/*.rs or the inline `mod *_tests` blocks in
# crates/mct-mcp-server/src/format.rs for the convention.

cargo run -p mct-cli -- --root . init                                # first index of a project
cargo run -p mct-cli -- --root . status                              # coverage and health report
cargo run -p mct-cli -- --root . reindex --force
cargo run -p mct-cli -- --root . mcp-register [--name N]             # write or merge .mcp.json for this project
cargo run -p mct-mcp-server -- --root <project>                      # run the MCP server over stdio
cargo run -p mct-eval [-- --verbose]                                 # quality suite vs. baseline
cargo run -p mct-eval -- --write-baseline                            # refresh crates/mct-eval/baseline.json after an intended change
```

Prefer the scripts in `scripts/unix/` (macOS and Linux) or `scripts/windows/` (PowerShell). They do the same work and print a short summary instead of hundreds of lines of cargo output. Full logs go to `target/script-logs/` (see `scripts/README.md`). Every script exists in both folders with the same name and flags, for example `scripts\windows\check.ps1 -p mct-core`.

```sh
scripts/unix/check.sh [-p <crate>]                 # before committing: fmt + tests + CI clippy + mct-eval; summary kept for pr-body.sh
scripts/unix/branch-worktree.sh <branch> [base]    # new branch in its own worktree from origin/main; one per concurrent change
scripts/unix/pr-body.sh [file]                     # PR description draft from the last check.sh and token-report.sh; warns if stale
scripts/unix/reinstall.sh [--reindex]              # after switching branches or merging a server change: install binaries, fix index schema, smoke-test; then reconnect MCP from the client (/mcp)
scripts/unix/mcp-smoke.sh [--dev] [--expect T]     # CONNECTION_CLOSED? prints the server's startup error; --expect checks tool T is listed
scripts/unix/new-tool-check.sh <tool>              # adding an MCP tool: places that still miss it, then the catalog tests
scripts/unix/new-language-check.sh <suffix>        # adding crates/mct-lang-<suffix>: the CONTRIBUTING.md checklist, what is missing, then its tests
scripts/unix/token-report.sh [--markdown F]        # every token measurement in one screen
scripts/unix/corpus-report.sh <lang> [--bless] [--update-progress]  # issue #74 corpus: tests, totals, heuristic bug checks
scripts/unix/ci-failures.sh [<run-id>]             # why a CI run failed (default: latest failed run on this branch)
scripts/unix/pr-status.sh <number>                 # a PR in a few lines: state, review, checks, latest comment
scripts/unix/corpus-progress-check.sh              # issue #74 table vs GitHub (needs gh)
scripts/unix/parse-probe.sh <files…>               # parse files without indexing: counts or first syntax error and line
scripts/unix/install-hooks.sh                      # once per clone: post-checkout hook that rebuilds the index after a branch switch breaks it
```

CI (`.github/workflows/ci.yml`) runs `build-test` (build, test and clippy, which includes the `mct-eval` gate) on Linux, macOS and Windows, `cargo-audit` on Linux, and `fuzz-smoke` (30 s `cargo-fuzz` per language crate) on Linux and macOS only. Fuzzing is excluded on Windows on purpose (ASan DLL and MSVC-sancov issues, see the comment above that job). `.github/workflows/quality-report.yml` reruns the `mct-eval` suite monthly and publishes the report (see `benchmarks/quality-eval.md`).

On Windows the standard Rust MSVC toolchain is enough (`link.exe` and `cl.exe` from VS Build Tools' "Desktop development with C++", which `rustup` prompts for). `git2` builds with `default-features = false` (no ssh or https transport, no OpenSSL), because this project only reads local repo state for blob-hash change detection and never clones or fetches.

## Architecture

### Plugin boundary

The design rests on one boundary. `mct-core` defines `LanguageParser` (`language_id()`, `file_extensions()`, `parse()`) and `LanguageRegistry` (extension to parser lookup). It knows nothing about tree-sitter, SQLite or MCP. Each `crates/mct-lang-*` crate implements the trait for one language through its own tree-sitter grammar. Source-language parser selection goes through the registry. Dependency manifests and other non-source inputs have their own filename routing in `mct-index`.

```
mct-core     LanguageParser trait, SymbolRecord/SymbolRelation model, LanguageRegistry
mct-index    SQLite schema and migrations (one schema for every language: a `language`
             column on `files`, not per-language tables), reindex orchestration, queries.
             Knows no language's grammar.
mct-tree-sitter  Grammar-neutral tree-sitter helpers shared by the mct-lang-* crates (first error,
             node locations, child lookup); knows no language and no SQLite or MCP
mct-lang-*   One crate per language, each a LanguageParser impl over its tree-sitter grammar
mct-languages  The shipped language set: the one build_registry() every binary indexes with
mct-mcp-server  MCP tools over stdio (rmcp). The authoritative tool list is
                docs/01-architecture/mcp-protocol-spec.md (deprecated aliases search_symbols and
                hybrid_search stay for now; select_tools only with MCT_TOOL_ATTENTION=1)
mct-obsidian Writes the Obsidian code atlas from the index; used by mct-cli and mct-mcp-server
mct-cli      Subcommands for manual and scripted use (`mct-cli --help` is authoritative) and the
             local web dashboard
mct-corpus   Test-only long-fixture corpus harness shared by the mct-lang-* crates; not published
mct-eval     Quality evaluation of the MCP tools against a fixture suite and a checked-in
             baseline (issue #21); dev-only, not published
```

### Adding a language

A new language touches exactly these places. The full checklist, with fuzz harnesses, is in [CONTRIBUTING.md](CONTRIBUTING.md#adding-a-new-language), and the base layout is in [lang-crate-base.md](docs/06-templates/lang-crate-base.md).

1. A new crate `crates/mct-lang-<name>`, depending on `mct-core` and `tree-sitter-<name>`.
2. `LanguageParser::parse()` as a pure AST walk that never executes or evaluates input. A syntax error returns `ParseError::Syntax`, never a panic, because this runs over arbitrary third-party source.
3. One registration, in `crates/mct-languages` (a `Cargo.toml` dependency and one line in `src/lib.rs::build_registry`). The CLI, the MCP server and `mct-eval` all use that registry. Nothing else in `mct-core`, `mct-index`, `mct-cli` or `mct-mcp-server` changes.
4. Tests in `crates/mct-lang-<name>/tests/parse.rs`: function and call extraction, one idiomatic-syntax case, one syntax-error case.
5. The language table in `README.md` and `internal/checklist.md`.

### Libraries and frameworks

Follow the [implementation map](docs/01-architecture/library-support.md).

- Keep dependency-format parsing in the private ecosystem modules under `mct-index/src/manifests/`. `manifests.rs` owns file-name routing and shared deduplication.
- Keep source and framework syntax in its language crate, and persistence in `mct-index`.
- In JS/TS, grammar selection, AST traversal, imports and exports have separate private modules.
- Put library-specific extraction and focused parser regressions under `crates/mct-lang-<language>/src/libraries/<name>/`, with English names. Keep common grammar and traversal shared.
- A test-only library module uses `#[cfg(test)]` and documents its real scope. A folder's existence does not mean production support.
- Keep corpus fixtures, snapshots and index integration under `tests/`, and keep their paths during organizational refactors.
- Update the framework coverage and corpus records when behavior changes. Language support alone is not complete framework support.

### Cross-cutting changes

Changing the `LanguageParser` trait, the SQLite schema or a published MCP tool signature affects every language crate, every index and the live tool contracts of connected clients. Open an issue first, not a drive-by PR.

### Code style

No `unwrap()` or `panic!` on any path that processes repo content. Use `.unwrap_or_default()` or an early return, because parsers run over arbitrary, possibly hostile source. Each crate has one job: `mct-core` does not know SQLite, and a `mct-lang-*` crate knows neither SQLite nor MCP.

Commit format: `type(scope): short description` (`feat`, `fix`, `refactor`, `docs`, `test`, `chore`, `perf`), with breaking changes marked explicitly.

## Reference documents

Open these when the task needs them.

| Document | Covers |
|---|---|
| [mcp-tools-reference.md](docs/07-agents/mcp-tools-reference.md) | Full tool table with parameters, Markdown literal search, tool discovery |
| [mcp-troubleshooting.md](docs/07-agents/mcp-troubleshooting.md) | Missing tools and `CONNECTION_CLOSED` |
