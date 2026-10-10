# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `mct-cli update` (also `-u` / `--update`) installs the latest GitHub
  release of `mct-cli` and `mct-mcp-server` into the folder `mct-cli` runs
  from, through the official installer. `mct-cli --version` (`-v`) shows
  the installed version.
- `scripts/unix/release.sh` (`bump`, `dry-run`, `publish`) automates the
  scriptable steps of `RELEASING.md`.
- Web/3D foundation (ADR-003 P0b): asset files (`glb gltf png jpg jpeg webp
  ktx2 hdr exr`) are indexed as `files` rows with language `asset` and one
  `asset` symbol each; images are only stat'ed, glTF JSON is read under an
  8 MiB cap (a `.glb`'s BIN chunk never), and bad glTF records a
  `syntax_error` while keeping the asset. New symbol kinds `asset`,
  `model_node`, `material`, `animation`, `finding`;
  `mct_core::resolve_reference_path`; `mct_tree_sitter::mask`;
  `get_indexing_status` takes an optional `dependency` to list the
  manifests declaring a package; `build_context_pack` prints no source for
  an asset. No schema migration and no `reindex --force` needed: the next
  normal reindex adds the assets.

### Changed

- New crate `mct-languages` holds the one `build_registry()` that
  `mct-cli`, `mct-mcp-server` and `mct-eval` index with. Adding a language
  now means one dependency and one `register` line there, instead of two
  copies kept in sync by a parity test. `mct_mcp_server::registry::build_registry`
  still works (re-export). No change to the shipped language set.

### Fixed

- Rust: a path in value position (`Arc::new(m::Parser)`, `Severity::Warning`,
  `.any(char::is_control)`) is now a `references` relation with the same
  module/type evidence as a call through that path, so `find_references`,
  `impact_analysis` and `find_dead_code` see it. Run `reindex --force` once
  so unchanged Rust files pick it up.

## [0.2.0] - 2026-10-09

Second release, and the first intended to ship prebuilt binaries: the
`v0.1.0` tag never produced a GitHub Release. It contains breaking renames
(crates, binaries, index directory, Windows installer settings); read
[Upgrading from 0.1.0](#upgrading-from-010) first.

### Upgrading from 0.1.0

You can go straight to the final 0.2.0 state; the intermediate `ccm-*` /
`.ccm-index` names used during development were never released and need no
separate step.

1. **Install the new binaries.** They are now `mct-cli` and `mct-mcp-server`
   (were `ccm-cli` and `ccm-mcp-server`); the crates are `mct-*` (were
   `ccm-*`). From source: `cargo install --locked --path crates/mct-cli` and
   `cargo install --locked --path crates/mct-mcp-server` in an updated clone.
   Prebuilt archives come from the 0.2.0 GitHub Release through
   `install.sh` / `install.ps1` once it is published. The crates are **not**
   on crates.io (see `RELEASING.md`), so `cargo install mct-cli` from the
   registry does not work. Remove the old binaries with
   `cargo uninstall ccm-cli ccm-mcp-server` if you installed them that way.
2. **Point your MCP configuration at the new server.** In `.mcp.json` (or your
   client's equivalent) change `"command"` from `ccm-mcp-server` to
   `mct-mcp-server`; the server key and `--root` arguments can stay as they
   are. `mct-cli --root . mcp-register` writes the correct entry and keeps
   any other servers in the file. This file is configuration, not index
   data: edit it, don't delete it.
3. **Rebuild the index.** The index now lives in `.mct-index/index.sqlite3`.
   0.1.0 used `.claude-index/`; that directory is not read, migrated or
   indexed by 0.2.0 (`.claude-index/` and `.ccm-index/` are both on the
   built-in exclusion list). Run `mct-cli --root . init` — or just start the
   new server, which indexes at startup. The index is derived entirely from
   your source files, so nothing is lost; once the new index works you can
   delete `.claude-index/` (or `.ccm-index/`) to reclaim the space, and add
   `.mct-index/` to `.gitignore` (`mct-cli --root . gitignore-init` does
   that).
4. **Restart or reconnect your MCP client** (Claude Code / Codex: `/mcp`) so
   it launches `mct-mcp-server` and loads the new tool list.
5. **Windows installer:** the install directory override is now
   `MCT_INSTALL_DIR` (was `CCM_INSTALL_DIR`), and the default directory is
   `%LOCALAPPDATA%\mct\bin` (was `%LOCALAPPDATA%\ccm\bin`); add the new
   directory to your `PATH`. The Linux/macOS installer still uses
   `INSTALL_DIR` (default `$HOME/.local/bin`). No 0.1.0 archives were ever
   published, so this only affects scripts or notes that set the old
   variable.

Schema and binary compatibility: index schema migrations run forward
automatically when a newer binary opens an older `.mct-index`. An older
binary refuses to open an index migrated by a newer one (`migration number
that is too high`); delete `.mct-index/` and run `init` again with the binary
you intend to use. If you already ran a development build from `main` against a
project, run `mct-cli --root . reindex --force` once after upgrading so
unchanged Markdown files gain the new literal and note data.

### Added

New MCP tools (18 in total, up from 8 in 0.1.0):

- `get_file_skeleton`: one file's top-level declarations with bodies collapsed
  to `// ...`. Brace-delimited languages get precise body elision; others
  fall back to declaration lines. Nested members are not expanded.
- `get_project_overview`: a capped, ranked digest of a file, directory, crate
  or the whole project (modules, key symbols, top callers).
- `get_file_tree`: a depth-limited directory/file tree without symbol data,
  pruned of the same noise directories reindexing skips.
- `search_symbols`: BM25-ranked full-text search over symbol names split at
  camelCase/snake_case/kebab-case/acronym boundaries, exact name first, with
  `limit`/`offset`/`snippet_lines`.
- `hybrid_search`: `search_symbols` fused with local-embedding similarity by
  weighted Reciprocal Rank Fusion, with `alpha` picked from the query shape
  when omitted. The lexical side expands common developer-verb synonyms and
  resolves `Type::member` / `module.fn` qualifiers. The semantic half needs a
  `--features semantic` build of `mct-mcp-server`; without it (including the
  prebuilt release binaries) the tool returns the lexical ranking and says so
  on its first line.
- Exact-phrase search: a `"double-quoted"` `hybrid_search` query matches the
  words consecutively and in order (lexical only, whatever `alpha`), ranks a
  literal name match first, and lists prose string literals holding the
  phrase as `path:line`. Literals are extracted from Rust string literals and
  from Markdown paragraphs and pipe-table rows only; other languages are not
  covered. Works without the semantic feature.
- `build_context_pack`: one symbol's definition, doc comment and capped
  source plus each caller, callee, dependency and related test listed once,
  with ambiguous callees reported separately instead of guessed.
- `batch`: up to 25 read-only queries in one call, each under its own header;
  a failing sub-query is reported inline without failing the rest.
- `find_dead_code`: top-level symbols with zero indexed references. A
  heuristic: dynamic dispatch, reflection, exports used outside the indexed
  tree and framework conventions are invisible to it, so check a hit with
  `find_references` / `impact_analysis` before deleting anything.
  `mct-cli dead-code` writes the same candidates as a CSV report.
- Progressive tool discovery: `discover_tool_categories` (names and one-line
  purposes, grouped, no schemas) and `get_tool_schema` (one tool's full
  schema on demand). The standard `tools/list` handshake is unchanged.

Improvements to existing tools:

- `find_references`, `find_calls`, `find_callers` and `impact_analysis` take
  `depth` (multi-hop traversal, default 1, clamped to 32, cycle-guarded) and
  `offset` (pagination). Hits beyond depth 1 are tagged `[depth N]`.
- `find_symbol` takes `match: exact|prefix|fuzzy`.
- Optional `format: toon` compact table output on the tabular tools.
- A query cache for `search_symbols` and `hybrid_search`: exact repeats (and,
  with the semantic build, near-identical queries) reuse a validated earlier
  ranking, and an unchanged repeated reply collapses to one line.
  `MCT_CACHE=off` disables it; see `docs/03-performance/query-cache.md`.
- Shorter tool descriptions in the catalog, served over `tools/list`.

Indexing:

- Background watcher in `mct-mcp-server`: debounced file-system events
  reindex only the changed paths (falling back to a full rescan when a batch
  cannot be mapped to paths), so the index stays current without manual
  `reindex` calls.
- `.mctignore`: gitignore-style project exclusions on top of the built-in
  ones, scaffolded by `mct-cli ignore-init`. `@import-gitignore`
  (`ignore-init --import-gitignore`) also excludes everything the project's
  `.gitignore` excludes. `!pattern` lines re-include what an earlier line
  excluded (last match wins; never inside an excluded directory, never a
  built-in exclusion). `mct-cli gitignore-init` adds `.mct-index/` to
  `.gitignore`.
- Markdown notes are first-class graph nodes, including notes without any
  heading: nested ATX/Setext sections, heading levels, frontmatter metadata,
  `[[wikilinks]]` (with `#Heading` anchors), Markdown links and `![[embeds]]`
  resolved by path, and note-aware context and backlinks.
- Relations resolve to qualified symbol identities from parser evidence;
  ambiguous or unresolved targets are reported as such rather than
  attributed to an arbitrary homonym.
- Read diagnostics: files that cannot be read, and files above the size
  limit (16 MiB by default, `MCT_MAX_FILE_BYTES` to change it), are recorded
  as read failures; their last indexed data is kept and marked as possibly
  stale. Ignore-file read or pattern errors are reported on stderr while the
  built-in exclusions stay active. (`get_indexing_status` does not list read
  failures yet — issue #133.)
- Lua is now registered in `mct-cli` and `mct-mcp-server` (it was a test-only
  parser in 0.1.0): 17 languages are indexed.

Parsers and test coverage:

- A long multi-file fixture corpus for each of the 17 language parsers, with
  malformed-input cases and snapshot checks, plus framework fixtures for CSS
  and Python; numerous parser fixes found through it (C++ scope awareness,
  Python decorator relations, CSS compound/combinator selectors and hex
  escapes, and more). Framework coverage remains partial (issue #51).
- `mct-eval`: a quality suite for the MCP tools against a fixture set and a
  checked-in baseline, run in CI.

CLI:

- `mct-cli ignore-init`, `gitignore-init`, `dead-code` and `probe` (parse
  files without indexing them and report counts or the first syntax error)
  subcommands; `-v` as an alias for `--version`; fuller `--help` text with
  examples.

### Changed

- **Breaking:** every crate, binary and Rust module path is renamed from the
  `ccm` prefix to `mct` (`ccm-core` → `mct-core`, `ccm-cli` → `mct-cli`,
  `ccm-mcp-server` → `mct-mcp-server`, every `ccm-lang-*` → `mct-lang-*`,
  `ccm_core::` → `mct_core::`, `CcmServer` → `MctServer`). MCP tool names
  are unchanged.
- **Breaking:** the index directory is `.mct-index/` (was `.claude-index/` in
  0.1.0); no automatic migration — see the upgrade steps above.
- **Breaking:** the Windows installer reads `MCT_INSTALL_DIR` (was
  `CCM_INSTALL_DIR`) and defaults to `%LOCALAPPDATA%\mct\bin`.
- License: the project's own code and documentation are Apache-2.0 (0.1.0
  declared `MIT OR Apache-2.0`). Third-party components keep their own
  licenses; see `THIRD_PARTY_NOTICES.md`.
- Relation queries page in the database instead of loading every relation,
  for both direct and multi-hop queries; multi-hop results report a lower
  bound when the total was not computed.
- Embedding refresh (semantic build only) runs in batches without holding
  the index lock, reducing peak memory.

### Fixed

- Relation pagination returned wrong or duplicated windows in some cases;
  `impact_analysis` now filters and deduplicates affected tests by symbol
  identity before paging them.
- A renamed directory left stale rows behind.
- `get_file_skeleton` and `get_project_overview` treated file-level
  containers in some languages as nested symbols.
- An MCP client that probed `discover_tool_categories` first could have its
  later standard requests rejected.
- `tools/list` served the long descriptions instead of the compact catalog.
- A duplicate dependency in a manifest aborted the whole reindex.
- Installer documentation: the custom install directory is applied to the
  process that runs the installer.

### Security

These close specific crash classes found by fuzzing and review; they are not
a claim that the parsers are free of vulnerabilities (see issue #22).

- Stack exhaustion on deeply nested input: every parser's AST walk and
  syntax-error traversal is depth-bounded (`mct_core::MAX_TRAVERSAL_DEPTH`),
  pruning below the limit instead of recursing.
- Markdown: `tree-sitter-md` 0.5.3 is replaced by a patched copy in
  `vendor/tree-sitter-md` fixing two memory-safety bugs in its block scanner
  reachable from ordinary Markdown (a `serialize` buffer overflow on deep
  nesting, and `isdigit` called with full Unicode code points). Upstream has
  not released either fix; this is why the crates are not published to
  crates.io.
- Kotlin: inputs that sent tree-sitter's error recovery into a pathological
  time/memory blowup are bounded by a parse budget, and an end-of-file
  scanning hang is fixed.

### Known limitations

- Type resolution is AST-only (tree-sitter); relations are matched from
  syntax, not resolved types.
- Release binaries do not include the `semantic` feature.
- Parser limitations tracked upstream or in open issues: Bash `coproc`
  (#79), some valid C++ rejected by tree-sitter-cpp (#82), tree-sitter-css
  0.25 syntax gaps including Tailwind v4 preludes and nesting (#87, #115,
  #116); Markdown frontmatter aliases do not resolve wikilinks (#135).
- Framework-specific structure (React JSX usage, Vue SFCs, …) is only
  partially indexed (#51).
- Linux release binaries are built on the `ubuntu-latest` runner and need a
  glibc at least as new as that image's.

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
