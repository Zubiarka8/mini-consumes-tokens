# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `get_file_skeleton` MCP tool (9th tool): returns a single file's top-level
  declarations (functions, classes, structs, interfaces, types) with bodies
  collapsed to `// ...` — up to ~90% fewer tokens than reading the whole
  file when only its shape is needed. Brace-delimited languages (Rust, Go,
  Java, C++, C#, PHP, JS/TS, Kotlin) get precise body elision via brace
  matching; other languages fall back to declaration-line-only rendering.
  Nested members are not expanded individually.
- Multi-hop graph traversal (`depth`, default 1, clamped to
  `ccm_core::MAX_QUERY_DEPTH` = 32) and pagination (`offset`) on
  `find_references`, `find_calls`, `find_callers`, and `impact_analysis`.
  Implemented as a BFS layered over the existing single-hop queries
  (`ccm-index/src/traversal.rs`), cycle-guarded by a visited-symbol-name
  set so a cyclic call/reference graph can't loop forever. Each hit beyond
  depth 1 is tagged ` [depth N]` in tool output; at the default `depth: 1`
  and `offset: 0`, output is byte-identical to before this existed.

### Changed

- **Breaking**: the index directory is now `.ccm-index/` (was
  `.claude-index/`) — this tool is meant to work with any MCP-capable
  agent, not just Claude Code, so the on-disk name should not imply
  otherwise. No migration path: delete the old `.claude-index/` and run
  `ccm-cli --root . init` (or restart/reconnect your MCP client, which
  triggers the same reindex) to regenerate under the new name. Updated
  everywhere the old name appeared: `.gitignore`, the default exclusion
  patterns (`ccm-index/src/exclude.rs`), both `db_path` computations
  (`ccm-cli`, `ccm-mcp-server`), `CLAUDE.md`, `internal/checklist.md`,
  and `manual/index.html`.

### Fixed

- Stack-overflow DoS on adversarially deep/nested source: `ccm-lang-php`'s
  CI fuzz-smoke job crashed (AddressSanitizer `stack-overflow`) on a
  crafted input, root-caused to unbounded mutual recursion in every
  language crate's `Walker::visit`/`visit_children` AST traversal (all 17
  crates share the same pattern). Fixed by adding a new
  `ccm_core::MAX_TRAVERSAL_DEPTH` constant (256) and threading a `depth`
  counter through every crate's walker; once the ceiling is hit, the
  subtree is pruned (no further symbols recorded below that point)
  instead of recursing further, so a malicious/pathological file can no
  longer crash the indexer. A regression test
  (`ccm-lang-php/tests/parse.rs::deeply_nested_expression_does_not_stack_overflow`)
  reproduces the original crash shape (5,000 nested parenthesized
  expressions) and asserts `parse()` still returns `Ok`.

## [0.1.0] - 2026-09-17

First release. Development happened in the order below — later languages
genuinely arrived after earlier ones, this is not a flattened summary.

### Added

- Core engine: a language-agnostic `LanguageParser` trait (`ccm-core`) as the
  single integration surface for adding a language, and a `LanguageRegistry`
  that wires parsers in without touching the engine.
- Index (`ccm-index`): one SQLite schema (no per-language tables), versioned
  migrations, WAL, FTS5 over symbols; incremental reindexing via
  `git2::Oid::hash_object` blob hashing (no real git repo required); default
  opt-out exclusion of secret-shaped paths (`.env`, `*.pem`, `*.key`,
  `secrets/`, `credentials.json`, `.aws/`, `node_modules/`, and more);
  path-traversal validation (canonicalization + root containment, symlinks
  outside the root rejected).
- MCP server (`ccm-mcp-server`, via the official `rmcp` SDK over stdio)
  exposing 7 tools, each classified as atomic/composite/index-administration
  with anti-ambiguity guidance against its closest sibling tool:
  `find_symbol`, `find_references`, `find_calls`, `find_callers`,
  `impact_analysis` (composite: combines `find_callers` + `find_references`
  + a test-name heuristic), `reindex`, `get_indexing_status`.
- `ccm-cli` with `init`/`reindex`/`status` subcommands.
- Rust and Python language support — the initial two languages the engine
  and MCP server were built and proven against.
- Lua language plugin (`ccm-lang-lua`), added deliberately as an
  architecture-validation exercise: a language *outside* the original
  7-language scope, used to confirm the `LanguageParser` trait generalizes
  with zero changes to `ccm-core`/`ccm-index`/`ccm-mcp-server`. Not wired
  into the production registries (`ccm-cli`, `ccm-mcp-server`) — it stays a
  test-only proof.
- Java and C# language support, including method-overload handling (each
  overload is its own indexed symbol) and, for C#, get/set property pairs
  modeled as one `Field` symbol rather than two unrelated methods. Fuzzing
  (`cargo-fuzz`) added for Rust and Python at this point too, alongside
  fresh setups for Java/C#.
- Multiplatform CI (GitHub Actions: Linux/macOS/Windows) — build, test,
  clippy (default lint group) on all three OSes; `cargo-audit` on Linux;
  short `cargo-fuzz` smoke campaigns on Linux/macOS (deliberately excluded
  on Windows — `cargo fuzz run` fails there with `STATUS_DLL_NOT_FOUND`
  regardless of sanitizer flags, documented in `.github/workflows/ci.yml`).
  At this point the fuzz-smoke matrix covered the 5 real language crates
  that existed then (Rust, Python, Lua, Java, C#).
- JavaScript/TypeScript language support (`ccm-lang-js-ts`): one crate,
  three tree-sitter grammars selected by file extension, reported under a
  single combined `language_id` because real projects mix `.js`/`.ts`
  freely. Handles ES module and CommonJS symbols/imports/exports in the
  same file, JSX/TSX left unstructured deliberately (only the logic inside
  components/handlers is indexed). Implemented in this working period but
  not committed until the following C++/Go change — see below.
- C++ and Go language support:
  - C++ (`ccm-lang-cpp`): the central case is correlating a class member
    declared in a `.h` with its out-of-line definition in a `.cpp` — both
    resolve to the same logical symbol via shared name+parent, without any
    new relation kind or core/index change.
  - Go (`ccm-lang-go`): structs, interfaces (with their declared methods
    indexed), receiver methods (pointer and value receivers normalize to
    the same parent type), packages, imports, calls. `find_implementations`
    is deliberately **not** implemented for Go — interface satisfaction is
    structural (the compiler decides it from the method set), not a
    keyword, and an AST heuristic would not be reliable; this is deferred
    to real type resolution (see "Known limitations" below).
  - This same change is also where the already-implemented but
    not-yet-committed `ccm-lang-js-ts` crate was finally committed, so the
    next CI run covered all 8 real language crates in one push.
- `clippy::unwrap_used` / `clippy::expect_used` / `clippy::panic` resolved
  across the workspace and enforced in CI (`-D` alongside the default lint
  group): one genuine risky `.expect()` fixed for real
  (`ccm-index/src/indexer.rs`), the remaining `src/`-level occurrences
  narrowly `#[allow]`-ed with a `// SAFETY:` comment at the exact site
  (grammar setup in each language crate, two static-pattern glob builds),
  and every `tests/`/`examples/` file given an explicit file-level
  `#![allow(...)]` explaining why panicking on a broken test precondition
  is correct there.
- Versioned integration fixtures for all 8 real languages (each already
  its own small, self-authored fixture under `tests/fixtures/`, never an
  externally cloned repository) plus a new polyglot fixture
  (`ccm-mcp-server/tests/fixtures/polyglot-app/`: a Go backend, a
  TypeScript frontend, a Python deploy script) with a dedicated
  integration test confirming the index holds and queries symbols from
  several languages in the same project without them bleeding into each
  other.
- Token-usage benchmark (`ccm-cli/examples/token_benchmark.rs`, results in
  `benchmarks/token-benchmark.md`) comparing MCP tool queries against a
  Read/Grep/Glob baseline: 88–98% character reduction across three
  canonical queries, run per-language as each one landed (Java, C#,
  JavaScript/TypeScript, C++, Go).
- `ccm-cli mcp-register [--name N]`: writes (or merges into) `.mcp.json` at
  the project root with the correct `ccm-mcp-server` entry, as an alternative
  to `claude mcp add` or hand-editing the JSON. Preserves any other server
  already configured in the file, and strips Windows' `\\?\` verbatim-path
  prefix from the written `--root` so the value stays a normal path.
- `list_symbols(path, kind?, language?, limit?)` MCP tool: lists symbol
  definitions under a file (exact match) or directory/crate (prefix match),
  closing the gap where every other tool required an exact symbol name up
  front. `end_line` tracking added to every `SymbolRecord` across all 16
  language crates, so `list_symbols` (and any future consumer) can report
  real `L<start>-L<end>` ranges, not just a start line.
- Kotlin language support (`ccm-lang-kotlin`, via `tree-sitter-kotlin-ng`):
  classes/interfaces (the grammar shares one node kind for both, told apart
  by an anonymous `interface` token), `object` declarations indexed as
  singleton classes, extension functions attached to their receiver type,
  primary-constructor `val`/`var` property promotion, and `Extends`/
  `Implements` distinguished by constructor-call vs. bare type in the
  supertype list — no positional heuristic needed, unlike C#.
- Prebuilt-binary installers: `install.sh` (Linux/macOS,
  `curl -sSL .../install.sh | bash`) and `install.ps1` (Windows,
  `irm .../install.ps1 | iex`), both downloading the latest GitHub Release
  archive for the detected OS/arch and installing `ccm-cli`/`ccm-mcp-server`
  into a user-writable directory.

### Known limitations (not blocking this release; tracked for 0.2.0+)

- Type resolution is AST-only (tree-sitter) for all 8 languages; no LSP
  integration yet.
- Secret-exclusion patterns cover Rust/Python/Java/C#/JS-TS/C++/Go
  conventions; Ruby/PHP/Swift have no dedicated patterns because there is
  no language crate for them yet.
- `find_implementations` is not implemented for Go (see above).
- HTML/CSS/JSX are not indexed structurally — only the script/logic
  embedded in them is, via the same generic recursion as any unrecognized
  node.
