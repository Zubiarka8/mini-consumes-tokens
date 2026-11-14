# Language coverage gaps

## Language and library overview

All 17 language parsers are registered in `crates/mct-languages/src/lib.rs`.
The following is a coverage summary, not a claim that every construct is
supported or independently verified. The detailed inventories and limitations
remain in sections 1–6. Duplicate and dead-code analysis are outside this table.
On 2026-10-30, the combined `cargo test` run for all 17 language crates and
`mct-languages` passed; some tests were marked ignored by the existing suite.

| Language | Indexing in use | Libraries and dependency manifests | Principal known limit / evidence |
|---|---|---|---|
| Rust | Registered; symbols and relations | `Cargo.toml` declarations | Some import forms are not covered; target-specific Cargo dependency tables are skipped. |
| Python | Registered; symbols and relations | `requirements.txt`; Django and Flask source fixtures | Other Python manifests are not detected; framework fixtures test the generic parser. |
| JavaScript / TypeScript | Registered; symbols, imports, exports and JSX references | `package.json` four dependency sections; React source fixtures | Generic JSX references are not framework-specific render resolution; some imports are unresolved. |
| Java | Registered; symbols and relations | No Maven or Gradle manifest reader | Some constructor calls and method references are not emitted. |
| Kotlin | Registered; symbols and relations | No Gradle manifest reader | `typealias`, companion objects and some call forms are not emitted. |
| C# | Registered; symbols and relations | No NuGet or `.csproj` manifest reader | Base-type classification is heuristic. |
| C++ | Registered; symbols and relations | No CMake, vcpkg or Conan manifest reader | Include-path resolution is limited; some valid syntax can be rejected. |
| Go | Registered; symbols and imports | `go.mod` require entries | Implicit interface implementations and package fields are not indexed. |
| PHP | Registered; symbols and relations | No Composer manifest reader | Some imports and computed includes are not resolved. |
| Lua | Registered; symbols, relations and duplicate analysis | `*.rockspec` declarations (scope, constraints, platform tables, module maps) with installed and pinned state read from project-local trees and `luarocks.lock`, through `get_indexing_status` and `get_dependency_graph` | Module files whose name differs from their module (custom `package.path` templates) are not linked; `dofile`/`loadfile` stay unresolved; the version LuaRocks would select at run time is not predicted. See section 5. |
| Bash | Registered; functions, top-level variables and relations | No package manifest reader | Dynamic commands and some source paths are not resolved. |
| PowerShell | Registered; functions, classes, enums, top-level variables and relations | No package manifest reader | `[Type]` uses other than `[T]::new()` are no relations; some source paths are unresolved. |
| HTML | Registered; selected elements and relations | No language-specific package manifest | Elements without IDs and inline script/style contents are not indexed as separate symbols. |
| CSS | Registered; rules, selectors and imports | Bootstrap and Tailwind source fixtures; packages may be declared in `package.json` | Framework fixtures test the generic parser; some Tailwind syntax is unsupported. |
| XML | Registered; selected elements | No dialect-specific dependency manifest reader | No relations are emitted. |
| XAML | Registered; selected elements and relations | No NuGet or `.csproj` manifest reader | Events use a fixed list; some resource URIs are skipped. |
| Markdown | Registered; notes, headings, references, aliases and prose literals | No package manifest applicable | Table and escape fixes are local changes; unchanged files require forced reindexing to pick them up. |

Manifest dispatch is implemented in `crates/mct-index/src/manifests.rs` and its
private format modules, including Lua on this unreleased branch. `cargo test -p mct-index --test manifests --quiet`
passed all five integration tests on 2026-10-30. The LuaRocks analysis also reads
project-local installed and pinned metadata, subject to the limits in section 5;
it does not install packages or index installed third-party source. Framework directories under `src/libraries/` contain tests, not
production adapters; see `docs/01-architecture/library-support.md` and the
authoritative framework coverage table in `CONTRIBUTING.md`.

### Framework and library examples

These rows describe the tested application-source patterns, not full framework
support. The framework-specific `libraries/` modules are test-only; production
uses the general language parsers. The combined language-crate test run passed
on 2026-10-30, including these library unit tests.

| Library / framework | Covered by existing parser tests | Known scope and gaps | Evidence |
|---|---|---|---|
| React | JSX/TSX component tags as generic references, component ownership, member names, calls and hooks | No dedicated render relation or React-specific resolution; intrinsic HTML tags are excluded from component references. | `crates/mct-lang-js-ts/src/libraries/react/mod.rs` |
| Django | Model and view classes, inheritance, decorators, calls, imports, signal receiver and URL-registration calls | These are generic Python symbols and relations; fields appear as calls, not modeled Django fields or resolved framework routes. | `crates/mct-lang-python/src/libraries/django/mod.rs` |
| Flask | Route and hook decorators, nested functions, `MethodView` inheritance, registration calls and imports | Decorators and calls are generic relations; no dedicated Flask route model or runtime framework resolution is claimed. | `crates/mct-lang-python/src/libraries/flask/mod.rs` |
| Bootstrap | CSS imports and application-defined selectors, including compound and nested examples | Tests do not index the external Bootstrap distribution or compute the CSS cascade. | `crates/mct-lang-css/src/libraries/bootstrap/mod.rs` |
| Tailwind CSS | CSS imports, application selectors, escaped classes and nested rules in v3/v4 fixtures | v4 `@utility` names and several `@theme`/`@custom-variant` constructs are not indexed; package resolution is separate. | `crates/mct-lang-css/src/libraries/tailwind/mod.rs` |
| Vue / Svelte | No framework-specific source support established | Vue single-file components and Svelte are not supported by the shipped language registry. | `docs/01-architecture/library-support.md`, section 6 below |

Baseline inventory of the 17 `crates/mct-lang-*` language crates, prepared on 2026-10-30 against `main` at `59231f0` (workspace version 0.3.0). The tables are an inventory of emitted kinds, not proof of complete behavior. The focused changes and verification below do not constitute an independent re-audit of every cell.

## Audit changes and verification

The rows in this section record successive changes, not a single current-state
snapshot. Later Lua changes in the next section supersede earlier behavior and
test counts where they differ, including `dofile`/`loadfile` resolution and
variable indexing. The reported test counts are historical session results.

| Functionality / indexing | Previous state | Current state | Implementation | Validation |
|---|---|---|---|---|
| Markdown table links, embeds and tags | Table cells were omitted from relations | Extracted per header/body cell, including escaped alias separators; inline code remains excluded | `crates/mct-lang-md/src/lib.rs` (pre-existing uncommitted change) | `cargo test -p mct-lang-md --quiet`: 131 passed, 1 ignored; includes parser, corpus and incremental create/edit/delete note-graph tests |
| Markdown escaped wikilinks | An escaped opening bracket produced a false relation; an escaped embed marker produced an import | Odd numbers of preceding backslashes suppress the link; even numbers preserve it. An escaped `!` leaves a reference rather than an import | `crates/mct-lang-md/src/lib.rs`, `crates/mct-lang-md/tests/parse.rs` | Both regressions failed before their fixes; `cargo test -p mct-lang-md --quiet`: 133 passed, 1 ignored; crate Clippy passed |
| Markdown escaped backticks | Escaped backticks were incorrectly treated as code delimiters, hiding links | An escaped opening backtick no longer starts a code span | `crates/mct-lang-md/src/lib.rs`, `crates/mct-lang-md/tests/parse.rs`, `crates/mct-lang-md/tests/note_graph.rs` | Regression failed before the fix; 134 parser/crate tests passed, 1 ignored; incremental edit and deletion regression passed separately |
| Lua member calls named `require` | Member calls were incorrectly emitted as module imports | Only unqualified `require` is treated as an import; member calls remain calls | `crates/mct-lang-lua/src/lib.rs`, `crates/mct-lang-lua/tests/parse.rs`, `crates/mct-lang-lua/tests/depgraph.rs` | Parser regression failed before the fix; 34 crate tests passed, 1 ignored; incremental edit/deletion test and crate Clippy passed |
| Lua literal variables, constants and fields | Not indexed | File-scope literal assignments (including unary numeric expressions), literal `<const>` declarations and named literal fields in module tables are indexed; superseded for computed values and function locals by the next row | `crates/mct-lang-lua/src/lib.rs`, `crates/mct-lang-lua/tests/parse.rs`, `crates/mct-lang-lua/tests/depgraph.rs`, `crates/mct-lang-lua/tests/corpus/expected.snap` | Regression tests failed before the change; 37 crate tests passed, 1 ignored; incremental indexing and crate Clippy passed. Corpus snapshot reviewed: 17 additional symbols, relation totals unchanged. |
| Lua `require` of a package | `require("a.b")` did not match `a/b/init.lua` (module `init`) | A package's `init.lua` is named after its directory, so `require("a.b")` resolves to `a/b/init.lua`; a root `init.lua` keeps `init` | `crates/mct-lang-lua/src/lib.rs`, `crates/mct-lang-lua/tests/parse.rs`, `crates/mct-lang-lua/tests/depgraph.rs`, `crates/mct-lang-lua/tests/fixtures/depgraph/` | Resolution test failed before the change; passes with `Resolved` to `app/shapes/init.lua`. Side effect: the `Module` symbol of such files is no longer named `init`. |
| Lua `dofile` and `loadfile` | Recorded only as calls | An earlier implementation linked literal paths against the repository root. **Superseded:** the branch now keeps them unresolved because Lua resolves them relative to the runtime working directory; see the next section. | `crates/mct-lang-lua/src/lib.rs`, same tests | Earlier resolution test; current unresolved-path tests are listed below. |
| LuaRocks manifests | Name and version only, deduplicated by name, scope lost | `*.rockspec` files are analysed without executing them (issue #267): a private Lua-data reader folds exact constants and stops at anything else, so a rockspec with code is `incomplete` in every section; every entry is kept in `rockspec_items` with scope, namespace, platform and constraint, and `dependencies` is a projection written from the same analysis | `crates/mct-index/src/manifests/lua/` (`data.rs`, `rockspec.rs`, `tree.rs`, `version.rs`), `crates/mct-index/src/schema.rs`, `crates/mct-index/src/indexer.rs` | Unit tests per module; `crates/mct-index/tests/luarocks.rs` (reindex to report, 21 tests plus 1 ignored) |
| Lua computed assignments and locals | Excluded | File-scope assignments are indexed whatever the value (`Variable`, `Constant` or `Field`); names bound to `require`/`dofile`/`loadfile` are not (they are imports); computed `__…` slots are skipped; `local` declarations inside functions are `Variable`s whose `parent` is the function; `local a, b = f()` indexes every name | `crates/mct-lang-lua/src/lib.rs`, `crates/mct-lang-lua/tests/parse.rs`, `crates/mct-lang-lua/tests/corpus/expected.snap` | Tests failed before the change. Corpus: 17 → 162 variable symbols; the corpus report flags the function locals as intended. Index size and search noise grow accordingly. Globals and loop variables inside functions are not indexed. |
| Lua tables used as classes | Always `Module` | A file-level table becomes a `Class` after `T.__index = T` or `setmetatable(T, …)`; `local D = setmetatable({}, { __index = B })` or `setmetatable({}, B)` declares class `D`; a named base gives an `Extends` relation (last segment of a dotted name) | `crates/mct-lang-lua/src/lib.rs`, same tests | Tests failed before the change. A `setmetatable(x, mt)` value with no nameable base stays a variable; any identifier second argument is read as a base. |
| Existing Markdown indexes | Unchanged files are skipped by content hash | Existing indexes require one forced reindex to pick up the new relations | `mct-cli reindex --force` | Operational requirement; not a schema migration |
| Remaining language gaps | Partial or pending as described below | Not independently revalidated by this focused change | See sections 1–6 | No new claim of completeness |
| Markdown duplicate analysis | No token source | Pending design and implementation. The user requested comparisons of sections and paragraphs. | `crates/mct-languages/src/lib.rs`, `crates/mct-index/src/duplicates/`, `crates/mct-index/src/queries.rs` | MCT context shows the candidate query and allowlist accept only functions and methods; paragraphs are not symbols. |

## Lua indexing improvements (branch `feat/lua-indexing-improvements`)

Each row names the test that demonstrates it. `Verified` means the behavior is covered by a passing automated test; `Partial` states what is not covered. Earlier Lua rows above (literal variables, computed locals, `require` of a package, metatable classes) are the baseline this work builds on.

On 2026-11-01, `cargo test --offline -p mct-lang-lua` passed 81 tests with
one corpus-report test ignored. The LuaRocks index integration and no-process
tests passed 23 tests with one external-CLI test ignored; the MCP LuaRocks
integration tests passed five. These runs do not revalidate every inventory
cell below or the separate Lua/LuaRocks runtime experiments.

| Area | Original problem | Solution | Status | Evidence |
|---|---|---|---|---|
| Lua inheritance | `Extends` named the local alias of a `require`d base | A base reached through `require`, an alias of it, or a field of the module links to the required module with package and file evidence; `class()` / `:extend()` / `:subclass()` helpers declare classes only when provable; a functional `__index` returning `T[key]` is a `References` relation | Partial | `tests/inheritance.rs` (resolves to `core/base.lua`, not the same-named `other/base.lua`), `tests/parse.rs` (alias, shadowing, field, helpers, delegation). The edge targets the module, not a `Class` inside it; other `__index` functions and name-only helpers are not resolved |
| Module paths | `src/` roots, `?/init.lua`, `package.path`, rockspec `modules` | Roots and `init.lua` packages resolve under any root without configuration; duplicates across roots are ambiguous | Partial | `tests/module_paths.rs`. Checked with Lua 5.5.1: the default `package.path` only searches `./?.lua` and `./?/init.lua` from the working directory, so `require "base"` reaching `src/base.lua` assumes the project adds `src/` to `package.path`. Files named differently from their module, rockspec `modules` maps and `package.path` templates are not interpreted (a parser sees one file) and stay unresolved |
| `dofile` / `loadfile` | Path assumed relative to the repository root | Literal recorded as written, never linked; `dofile` is `Imports`, `loadfile` is `References` | Verified | `tests/parse.rs::dofile_and_loadfile_paths_are_unresolved…`, `tests/depgraph.rs::dofile_and_loadfile_stay_unresolved…`. No way to supply a known working directory. Confirmed with Lua 5.5.1: `dofile` is cwd-relative and `loadfile` only compiles |
| Variables | Globals, function loop variables and non-identifier keys missing | Lexical scopes; globals once at first assignment; loop variables and bare locals of a function; `["k"]`, `[10]`, `[expr]` keys as fields | Partial | `tests/parse.rs` (nine tests), corpus snapshot re-blessed (+59 symbols, reviewed). Parameters are scope-only; property assignments and read references are not indexed |
| LuaRocks | Only name lists | Declared (every scope, repeated constraints, platform entries), installed (each project-local store's `manifest`), pinned (`luarocks.lock`) and compatible, plus the origin of each unresolved Lua `require` (`mct_index::luarocks`, shown by `get_indexing_status` and `get_dependency_graph`) | Verified, with limits | `crates/mct-index/tests/luarocks.rs`, `luarocks_no_process.rs`, `crates/mct-mcp-server/tests/luarocks_tools.rs`; tree fixtures written by LuaRocks 3.13.0; `luarocks.lock` fixtures are hand-written. Limits are listed in section 5 |
| Duplicates | No Lua token source | `LuaTokenSource` registered in `build_token_sources`, run by the shared engine | Verified | `tests/tokens.rs` (eight), `tests/duplicates.rs` (five: across files, within a file, near-miss, incremental edit), `mct-languages` registry test |
| Frameworks | No Lua framework coverage | Test-only OpenResty module over the generic parser | Partial | `src/libraries/openresty/mod.rs` (three tests). Documents recorded patterns only; no production adapter, no other framework |

### Verification against real Lua and LuaRocks

Run on macOS with Lua 5.5.1 and LuaRocks 3.13.0 (installed with Homebrew), on throwaway projects outside the repository.

| Question | Observed | Consequence |
|---|---|---|
| Is a `dofile` path relative to the working directory? | Yes. `dofile("tools/x.lua")` ran from the project root and failed with `cannot open tools/x.lua` from `src/app/` and from `other/` | Confirms never resolving `dofile` against the repository root |
| Does `loadfile` run the file? | No. It returned a function and printed nothing; only `dofile` ran the script | Confirms `loadfile` is a reference, not an execution |
| Does `require "pkg.sub"` find `pkg/sub/init.lua`? | Yes, as `./pkg/sub/init.lua` from the project root | Confirms `?/init.lua` |
| Does `require "base"` find `src/base.lua` by default? | No. The default path searches only `./base.lua` and `./base/init.lua`; it worked after adding `./src/?.lua` to `package.path` | The index resolves it without configuration by path components. This is a heuristic: right when the project puts `src/` on `package.path`, not what Lua does alone |
| Format of a tree's `manifest` | `lib/luarocks/rocks-5.5/manifest` is a Lua data table; `repository.<rock>.<version>` is a **list** of entries with `arch = "installed"` and a `modules` map (`lpeg` lists `lpeg.so` and `re.lua`; `re` is a module of `lpeg`) | The reader takes modules from the manifest, as LuaRocks does, not from per-rock rockspecs. Fixtures are copies of real files |
| Version semantics | `~> 3.1` rejects `3.9-1`; `3.1.3-0` does not meet `3.1.3-1` or `>= 3.1.3-1`; `scm-1 >= 1.0` holds; `1.0rc1-1 >= 1.0` does not | 16 cases recorded from `luarocks.core.vers` are the unit test in `version.rs`. An earlier hand-written resolver was wrong on `~>` and on revisions and was replaced |
| `luarocks.lock` | A Lua module returning `{ dependencies = {...}, build_dependencies = {...}, test_dependencies = {...} }` (`luarocks/deplocks.lua`). A hand-written file in that shape made `luarocks build` report "Using dependencies pinned in lockfile" | Format taken from LuaRocks source and a real read of a hand-written file. **Not verified:** generating one (`luarocks build --pin` failed here with `lua 5.5-1 (not installed)`) |
| `build.install.lua` | `luarocks make` of a `type = "none"` rockspec with `install = { lua = { ["demo.pkg"] = "src/pkg/init.lua" } }` installed module `demo.pkg` (manifest: `demo.pkg = "demo/pkg.lua"`) | Keys are module names, values source paths. Validated only for LuaRocks 3.13.0 with build type `none`; the output says so |
| Which version `require` loads | `luarocks/loader.lua` chooses by the requiring rock's dependency context, the configured tree order and the highest version | Not knowable statically, so "resolved" is never reported as "the version LuaRocks would select" |

Not checked: lockfile generation by `--pin`; other Lua versions (5.1 to 5.4, LuaJIT); build types other than `none` for `install.lua`; trees named by a project's `.luarocks/config-*.lua` (their presence makes absence unknown; the file is not read); user and system trees (never inspected); rocks with C builds beyond `lpeg`; running a rockspec (never done on purpose). Read cost: a synthetic 4,999-rock `manifest` (about 1 MB) took about 60 ms per report in a release build on the development machine; larger manifests up to the 4 MiB cap were not timed.

Differences between the implementation and the text of issue #267 (the issue was not edited):

- The column is `version_constraint`, not `constraint` (`CONSTRAINT` is an SQL keyword).
- A rockspec over the global read limit (16 MiB, `MCT_MAX_FILE_BYTES`) is not cleared: it takes the existing read-failure path, so the last good rows stay and the path is listed in `read_failures`. The LuaRocks detail now also says so on the rockspec itself (`possibly stale: … the data below is from an earlier read`), its declarations count as incomplete, and a provider is `Unproven` instead of `Undeclared`. Only invalid UTF-8 clears the rows. Tests: `a_rockspec_over_the_read_limit_keeps_its_old_data_but_marked_as_possibly_stale` (index) and `a_rockspec_over_the_read_limit_is_shown_as_possibly_stale_not_as_current` (MCP output).
- An older binary meeting the migrated database refuses to open it for writing (`IndexError::Coordination`, schema "too far ahead") and changes no file or `user_version`; `the_write_path_refuses_a_schema_newer_than_the_binary_and_changes_nothing` in `crates/mct-index/tests/read_only_open.rs`. No older build was run: the test reproduces the same state with a `user_version` one past this binary's latest, which takes the same code path.

Still not verified, and not resolved:

- Lockfile generation by `luarocks build --pin`; `build.install.lua` outside LuaRocks 3.13.0 with build type `none`; Lua versions other than 5.5; `.luarocks/config-*.lua` is reported but not read; user and system trees are never inspected.
- Read time was measured only for a 1 MB `manifest`, not for one near the 4 MiB cap.
- Noise that remains: LuaRocks leaves the rockspecs of installed rocks inside a project's `lua_modules` tree, and `reindex` still indexes them as project manifests (unchanged behaviour). They add rows to `dependencies` and `rockspec_items` and appear as extra entries in `get_indexing_status` and as extra candidates in `get_dependency_graph`, although they describe installed rocks, not the project. No filter was added because that would need a rule for which directories are installed trees, and none was verified.

The ignored test (`a_tree_built_by_the_real_luarocks_cli_is_read_correctly`) builds its own tree with the real `luarocks make`, so it needs `luarocks` on `PATH`: `cargo test -p mct-index --test luarocks -- --ignored`.

Markdown duplicate analysis needs a design for paragraph boundaries, source offsets,
normalization, overlap between sections and paragraphs, and reporting through the
existing duplicate interface. A token source alone cannot provide paragraph candidates.
No implementation or validation of that feature is claimed here.

Method: for each crate, the `SymbolKind` and `RelationKind` variants it emits (comment lines excluded), the extensions it registers, the `tests/` directory, the fuzz harness, the `src/libraries/` directory, the dependency-manifest dispatch in `crates/mct-index/src/manifests.rs`, and the fuzz matrix in `.github/workflows/ci.yml`. Where a gap is documented in `ROADMAP.md`, `CONTRIBUTING.md` or `docs/01-architecture/library-support.md`, the source is cited in section 5.

Legend:

- ✅ implemented or emitted for at least one supported case; this does not imply complete coverage
- ⚠️ partial; the limits are listed in section 5
- ❌ the concept applies to the language, but the crate does not produce it
- — the concept does not apply to the language

In sections 1 and 2, ✅ indicates that the parser emits the kind or relation, not that every language construct is covered. Section 5 records known limitations. In section 3, ⚠️ indicates a known limitation in the named feature.

## 1. Symbols emitted by the parser

| Language | Function | Method | Class | Struct | Interface | Enum | Trait | TypeAlias | Module | Field | Constant | Variable | Element / Rule |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| Rust | ✅ | ✅ | — | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Python | ✅ | ✅ | ✅ | — | ❌ | ❌ | — | ✅ | ✅ | ❌ | ❌ | ❌ | — |
| JavaScript / TypeScript | ✅ | ✅ | ✅ | — | ✅ | ✅ | — | ✅ | ✅ | ✅ | ❌ | ❌ | — |
| Java | — | ✅ | ✅ | — | ✅ | ✅ | — | — | ✅ | ✅ | ❌ | ❌ | — |
| Kotlin | ✅ | ✅ | ✅ | — | ✅ | ❌ | — | ❌ | ✅ | ✅ | ❌ | ❌ | — |
| C# | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ | ✅ | ❌ | — |
| C++ | ✅ | ✅ | ✅ | ✅ | — | ✅ | — | ✅ | ✅ | ✅ | ✅ | ✅ | — |
| Go | ✅ | ✅ | — | ✅ | ✅ | — | — | ✅ | ✅ | ❌ | ❌ | ❌ | — |
| PHP | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ | — | ✅ | ✅ | ✅ | ❌ | — |
| Lua | ✅ | ✅ | ⚠️ | — | — | — | — | — | ✅ | ⚠️ | ⚠️ | ⚠️ | — |
| Bash | ✅ | — | — | — | — | — | — | — | ✅ | — | — | ✅ | — |
| PowerShell | ✅ | ❌ | ❌ | — | — | ❌ | — | — | ✅ | ❌ | — | ✅ | — |
| HTML | — | — | — | — | — | — | — | — | ✅ | — | — | — | ✅ (elements with `id`) |
| CSS | — | — | — | — | — | — | — | — | ✅ | — | — | — | ✅ (selectors and rules) |
| XML | — | — | — | — | — | — | — | — | ✅ | — | — | — | ✅ (`id`/`name`/`Name`) |
| XAML | — | — | — | — | — | — | — | — | ✅ | — | — | — | ✅ (`x:Name`/`Name`) |
| Markdown | — | — | — | — | — | — | — | — | ✅ (note) | — | — | — | ✅ (headings) |

## 2. Relations emitted by the parser

| Language | Calls | Imports | Extends | Implements | References |
|---|---|---|---|---|---|
| Rust | ✅ | ✅ | ✅ (supertraits) | ✅ (trait impls) | ✅ |
| Python | ✅ | ✅ | ✅ | — | ✅ (decorators) |
| JavaScript / TypeScript | ✅ | ✅ | ✅ | ✅ | ✅ (exports, JSX components) |
| Java | ✅ | ✅ | ✅ | ✅ | ❌ |
| Kotlin | ✅ | ✅ | ✅ | ✅ | ❌ |
| C# | ✅ | ✅ | ✅ | ✅ (heuristic) | ✅ |
| C++ | ✅ | ✅ | ✅ | — | ❌ |
| Go | ✅ | ✅ | — | ❌ (implicit interfaces, deferred) | ❌ |
| PHP | ✅ | ✅ | ✅ | ✅ (also `use Trait`) | ✅ |
| Lua | ✅ | ✅ | ⚠️ | — | ⚠️ |
| Bash | ✅ | ✅ | — | — | ❌ |
| PowerShell | ✅ | ✅ | — | — | ❌ |
| HTML | — | ✅ | — | — | ✅ (`#id`, `.class`) |
| CSS | — | ✅ | — | — | ❌ |
| XML | ❌ | ❌ | ❌ | ❌ | ❌ (no relations by design) |
| XAML | — | ✅ | — | — | ✅ (event handlers) |
| Markdown | — | ✅ (`![[embed]]`) | — | — | ✅ (`[[wikilink]]`, `#tag`) |

## 3. Index features beyond symbols and relations

| Language | Import resolution evidence | Prose literals | Symbol aliases | Duplicate analysis (token source) | Dependency manifest | Framework module |
|---|---|---|---|---|---|---|
| Rust | ⚠️ | ✅ | ❌ | ✅ | ✅ `Cargo.toml` | ❌ |
| Python | ⚠️ | ❌ | ❌ | ✅ | ⚠️ `requirements.txt` only | ⚠️ Django, Flask (tests only) |
| JavaScript / TypeScript | ⚠️ | ❌ | ❌ | ❌ | ✅ `package.json` | ⚠️ React (generic JSX references only) |
| Java | ⚠️ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Kotlin | ⚠️ | ❌ | ❌ | ❌ | ❌ | ❌ |
| C# | ⚠️ | ❌ | ❌ | ❌ | ❌ | ❌ |
| C++ | ⚠️ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Go | ⚠️ | ❌ | ❌ | ❌ | ✅ `go.mod` | ❌ |
| PHP | ⚠️ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Lua | ⚠️ | ❌ | ❌ | ✅ | ⚠️ `*.rockspec` (declared, installed, pinned; import origins via MCP) | ⚠️ OpenResty (tests only) |
| Bash | ⚠️ | ❌ | ❌ | ❌ | — | ❌ |
| PowerShell | ⚠️ | ❌ | ❌ | ❌ | — | ❌ |
| HTML | ✅ | ❌ | ❌ | ❌ | — | — |
| CSS | ⚠️ | ❌ | ❌ | ❌ | — | ⚠️ Bootstrap, Tailwind (tests only) |
| XML | — | ❌ | ❌ | ❌ | — | — |
| XAML | ⚠️ | ❌ | ❌ | ❌ | — | — |
| Markdown | ✅ | ✅ | ✅ | ❌ | — | — |

Only Rust, Python, Lua, JS/TS, Go, Java, Kotlin, C#, C++, PHP, Bash and PowerShell have a token source, so `mct-cli duplicates` reports every other language as unsupported.

## 4. Tests and fuzzing

- **All 17 crates:** parser tests (`tests/parse.rs`), integration through the real `mct-index` (`tests/index_integration.rs`), a corpus snapshot (`tests/corpus/`), and a `cargo-fuzz` harness in `fuzz/`. All 17 are in the CI `fuzz-smoke` matrix, which runs on Linux and macOS only.
- **Extra regression coverage:** Kotlin has `tests/scanner_eof.rs` and a `fuzz/regressions` corpus; Markdown has `tests/literals.rs` and a `fuzz/regressions` corpus.
- **Dependency-graph test (`tests/depgraph.rs`):** present in every crate except XML (it emits no imports) and Markdown (covered by `tests/note_graph.rs` instead).
- **Token tests (`tests/tokens.rs`):** Rust, Python, Lua and JS/TS. Lua, JS/TS, Go, Java, Kotlin, C#, C++, PHP, Bash and PowerShell also have `tests/duplicates.rs`, which runs the real parser and token source through the shared `mct-index` engine.
- **Dead-code test (`tests/dead_code.rs`):** Rust only. The dead-code heuristic itself is shared across languages in `crates/mct-index/src/dead_code.rs`.

## 5. Pending work per language

### Rust
- A method call inside a macro's arguments (for example `assert_eq!(a.f(), b)`) is a `references` relation, not a `calls`, because tree-sitter returns the arguments as a flat token tree. A bare call to an imported function there (`println!("{}", helper(x))`) is a `calls`.
- There is no `Union`, `Macro` or `Closure` kind: a `union` is a `struct`, `macro_rules!` and `let name = |..| ..` are `function` symbols. Closures that are not bound by a plain `let` are not indexed.
- Import resolution does not match `mod name;` to `name/mod.rs`, ignores `#[path]`, and leaves aliased `use` items without the original path qualification.
- No framework module under `src/libraries/`.
- Token benchmarks are not done yet, as `ROADMAP.md` notes.

### Python
- Not emitted: `Field` (class attributes), `Constant`, `Variable` (module-level assignments), `Enum` (enum classes are `Class`), and `Interface` (Protocols are plain `Class`).
- `import pkg` (a package's `__init__.py`) and `from . import x` do not resolve. Standard-library and third-party imports are not marked `external`.
- Dependency manifests: only `requirements.txt`. `pyproject.toml`, Poetry and Pipenv are not implemented (`docs/01-architecture/library-support.md`).
- Django and Flask have test modules under `src/libraries/`, but no framework-specific extraction (`docs/01-architecture/library-support.md`).
- Token benchmarks are not done yet, as `ROADMAP.md` notes.

### JavaScript / TypeScript
- Not emitted: `Constant` and `Variable` (module-level `const`/`let` values). `Field` exists only for class fields.
- Imports: bare specifiers (`lodash`) and path aliases (`@/x`) record nothing. An extensionless `./dir` that resolves to `dir/index.ts` does not resolve. `.json` and asset imports are skipped.
- JSX has no dedicated render relation and no framework-specific resolution (issue #51). Vue `.vue` SFCs are not supported (issue #51).
- No token source for duplicate analysis.

### Java
- `this(...)`, `super(...)` and method references such as `Money::plus` produce no `Calls` relation. No issue filed yet.
- Not emitted: `Constant` and `Variable`. `static final` fields are plain `Field` symbols.
- Package wildcard imports (`import a.b.*`) record nothing.
- No `References` relation.
- No manifest detection (`pom.xml`, Gradle).
- No token source and no framework module.

### Kotlin
- Companion objects and `typealias` declarations produce no symbol. No issue filed yet.
- No `Enum` kind: `enum class` is indexed as `Class`, and its entries as `Field`.
- Not emitted: `Constant`, `Variable`, `References`.
- Infix and operator expressions do not produce `Calls` relations (see `tests/corpus.rs::documented_limits_of_the_parser`).
- Imports: top-level function imports and wildcard imports record nothing. Resolution assumes the file is named after the type (`Type.kt`).
- No manifest detection (`build.gradle.kts`), no token source, no framework module.

### C#
- The `Implements` heuristic treats the first base type as `Extends` and the rest as `Implements`. C# syntax does not distinguish them. This is an accepted limitation.
- `using static` and generic aliases are not recorded as imports.
- Not emitted: `Variable`.
- No manifest detection (`.csproj`, NuGet), no token source, no framework module.

### C++
- Some valid C++ is rejected by tree-sitter-cpp. See issue #82 (open).
- Angle-bracket `#include` never resolves to a repository file. Includes that rely on `-I` paths do not resolve either.
- A header declaration (`.h`) and its definition (`.cpp`) stay two symbols, matched by name and parent. This is by design.
- No `References` relation.
- No manifest detection (CMake, vcpkg, Conan), no token source, no framework module.

### Go
- No `Implements` relation. Go interfaces are implicit, and `find_implementations` is deferred (`internal/checklist.md`, open decision #3).
- Not emitted: `Field` (struct fields), `Constant` and `Variable` (package-level `const`/`var`).
- Imports: standard-library paths (`errors`, `encoding/json`) and dotless local module names do not resolve. A multi-file package reports `ambiguous`.
- No token source.

### PHP
- `use TraitName;` is recorded as `Implements`, because no mixin relation kind exists in `mct-core` (module docs in `crates/mct-lang-php/src/lib.rs`).
- `use function` and `use const` add no file-level module relation. General computed includes such as `include $x` do not produce file-target evidence; `__DIR__ . '/x.php'` with a literal path is supported.
- No `composer.json` manifest detection (`vendor/` is excluded).
- Secret patterns for `wp-config.php` and `config/database.php` are not implemented (`ROADMAP.md` §2, `internal/checklist.md` open decision #2).
- No token source and no framework module (Laravel, WordPress).

### Lua
- **Classes and inheritance** are recognized from syntax only. `Extends` is emitted for a table-valued `__index` (`setmetatable({}, { __index = Base })`, `setmetatable(T, Base)`), for `class(...)` helpers and for `Base:extend()` / `Base:subclass()`; the helper must be provable (`class` is not defined in the file, and the base or receiver is an imported module or a class of the file). A base reached through `require` links to the required module's `Module` symbol (resolved by package and file name), or to the named field of it (`require("m").Base`). The index cannot know which class a module exports, so an `Extends` points at the module, not at a `Class` inside it. A functional `__index` that is exactly `return T[key]` is a `References` relation, not inheritance; any other `__index` function proves nothing and records nothing.
- **Variables:** lexical scopes cover parameters, locals, `local function` names, numeric and generic loop variables and nested blocks. Locals and loop variables inside a function are `Variable`s whose parent is the function; a global is indexed once, at its first assignment, with no parent; reassignments and shadowed names declare nothing. Parameters are scope-only and not symbols. Non-identifier table keys are `Field`s (or `Method`s) of a named table, named by the string (`["api-key"]`) or the bracketed expression as written (`[10]`, `[prefix .. "x"]`); positional entries have no key and are not indexed. Not done: `self.x = …` property assignments, and `References` from a function to the variables it reads.
- **`dofile` and `loadfile`** take their path relative to the process working directory, which source cannot prove. A call with a literal path records the literal as written (`Imports` for `dofile`, which runs the file; `References` for `loadfile`, which only compiles it) and never links it to a file. Computed paths and member calls are plain calls. This replaces an earlier assumption that the working directory is the repository root.
- **Module paths:** `require("a.b")` matches `a/b.lua` or `a/b/init.lua` under any root, because the index compares the module path with the components of each indexed path (so `src/`, `lib/` and nested roots need no configuration). The same module under two roots is reported ambiguous, never guessed. Not done: a file whose name differs from its module (`build.modules = { ["a.b"] = "src/impl/x.lua" }`, a `package.path` template such as `src/?/main.lua`) is not linked, and neither the rockspec map nor `package.path` is interpreted, because a `LanguageParser` sees one file and cannot read project configuration. `package.path` assignments, including dynamic ones, have no effect. `vendor/` is excluded from indexing by default.
- **LuaRocks:** a rockspec is analysed, never executed. A section (`dependencies`, `build_dependencies`, `test_dependencies`, `build.modules`, `build.install.lua`, `platforms`) is `complete` only if the whole file was folded exactly; any other statement or expression (a call, `if`, `t[#t + 1] = …`, an unknown name), a reached limit or invalid UTF-8 makes every section `incomplete`, and what was recovered stays listed. Platform entries are shown as declared under a platform, never as active. Installed rocks come from each project-local store's `manifest` (`lua_modules/lib/luarocks/rocks-<lua>/`); user and system trees are never inspected, so "not found" always means "in the project-local trees", and an unreadable, oversized, corrupt or excluded store makes the answer incomplete or unknown instead of absent. A provider is called sole only when every relevant source was read, and `ProvidedButUndeclared` only when the rockspec's declarations were complete. `luarocks.lock` is read when present (pin, whether the pinned version is installed, whether it meets the constraint). Limits: see the verification table; the effective run-time version is not predicted, and a project `.luarocks/config-*.lua` that may name other trees is reported but not read.
- **Duplicates:** Lua has a token source. Callables are `function f`, `local function f`, `function M.f`, `function M:f` and `target = function … end`, named like their index symbols; comments, shebang lines and literal values are normalized like the other languages. Anonymous callables carry no enclosing-construct labels or body shape.
- **Frameworks:** OpenResty has a test-only module under `src/libraries/openresty/`, written after the repository's own gateway corpus. It documents what the generic parser records (module tables, `ngx.*` calls, `resty.*` imports); it does not model the framework, and nginx.conf phase bindings are not indexed. No other Lua framework was analyzed.

### Bash
- A named `coproc` truncates the enclosing function. See issue #79 (open).
- `Variable` symbols are top-level only.
- `Calls` only for literal command names. Dynamic command names record nothing.
- `source "$VAR/x.sh"` proves only the file stem, so it resolves weakly.
- No token source.

### PowerShell
- Classes (properties as `field`, methods and constructors as `method`) and enums (members as `field`) are indexed, and `[Location]::new(…)` or `New-Object Location` calls the class; a class's first base is `extends` unless named like `IFoo` (the rest `implements`); any other type literal (`[StorageZone]::Frozen`) gives no relation.
- Computed dot-source paths (`$PSScriptRoot`-based) and bare module names (`Import-Module Pester`) do not resolve.
- `Variable` symbols are top-level scope-prefixed assignments only.
- No token source.

### HTML
- Only elements with an `id` become symbols. Elements with only a `class` are omitted.
- Inline `<script>` and `<style>` bodies are not parsed by this crate.
- Imports come only from `<link>` and `<script src>`. `<a href>` and `<img src>` are not imports.

### CSS
- Simple, compound and nested selectors can be indexed; the parser does not resolve their matches against a DOM or compute the cascade. See the Bootstrap and Tailwind fixtures above.
- Declarations and values are not modeled.
- Tailwind v4 at-rule preludes are not parsed (issues #87, #116). Nested rules are resolved against their parent selector (module docs in `crates/mct-lang-css/src/lib.rs`). Issue #115 is still open on GitHub. A nested rule that cannot be expanded keeps an `mct: not expanded` marker.
- A bare package import (`@import "tailwindcss"`) does not resolve.
- Bootstrap and Tailwind have test modules under `src/libraries/`, but no framework-specific extraction.

### XML
- Only elements with an `id`, `name` or `Name` attribute are indexed.
- No relations at all, so the dependency graph has no XML edges. Dialect conventions (MSBuild `<Import>`, Maven `<module>`) are not modeled. This is by design.

### XAML
- Event handlers are recognized only from the fixed `EVENT_ATTRIBUTE_NAMES` list. `{Binding …}` values are ignored.
- `ResourceDictionary` `pack:` URIs and `;component` URIs are skipped.
- The `Foo.xaml` ↔ `Foo.xaml.cs` pairing is a `References` link, not an import.

### Markdown
- Table header and body cells are scanned for wikilinks, embeds and tags. Code spans and fenced code are ignored. Existing indexes need a forced reindex to extract relations from unchanged files.
- No `tests/depgraph.rs`; `tests/note_graph.rs` covers the note graph.

## 6. Cross-cutting gaps

- **Duplicate analysis** works only for Rust, Python, Lua, JS/TS, Go, Java, Kotlin, C#, C++, PHP, Bash and PowerShell (`crates/mct-languages/src/lib.rs`, `build_token_sources`).
- **Dependency manifests** cover 5 ecosystems: Cargo, npm (`package.json`), pip (`requirements.txt` only), Go modules and LuaRocks (`*.rockspec`). The persisted `dependencies` table holds name and version for every ecosystem and is what the CLI and web read; LuaRocks detail lives in `rockspec_items` and is exposed only through the MCP tools `get_indexing_status` and `get_dependency_graph` (the CLI and web were deliberately not changed). There is no detection for Maven/Gradle, NuGet, CMake/vcpkg/Conan, Composer, or `pyproject.toml`.
- **Framework modules** under `src/libraries/` (Django, Flask, React, Bootstrap, Tailwind) are test modules only. Production extraction goes through the shared parser. Vue and Svelte are not supported (issue #51).
- **`CallableSpan` (`callables` in `TokenizedFile`)** is defined in `mct-core` and filled by the shared `mct_tree_sitter::tokenize_source` (`crates/mct-tree-sitter/src/tokens.rs`), which the Rust and Python token sources use; the Lua token source calls `tokenize_tree` directly so it can add the callables that `tokenize_source` cannot name (`M.f = function() end`). The other 14 crates have no token source.
- **Symbol kinds `Asset`, `ModelNode`, `Material`, `Animation` and `Finding`** exist in `mct-core`. No code-language crate emits them. `Asset` is emitted by `crates/mct-index/src/indexer.rs`, and the names also appear in `mct-corpus` and `crates/mct-index/tests/assets.rs`.
- **Fuzzing** does not run on Windows. The reason is documented in the comment above the `fuzz-smoke` job in `.github/workflows/ci.yml`.

## 7. Documentation that is out of date

- `ROADMAP.md` ("Uneven integration fixtures") says Rust and Python have parser-level tests only. `tests/index_integration.rs` exists for both, and for all 17 crates.
- `internal/checklist.md` says fuzzing is "not configured" for Bash and PowerShell. Both are in the CI matrix now.
- `internal/checklist.md` describes the state at the time each language was added. Use this file, or the code, as the current state.
- `MANUAL.md` is cited by `internal/checklist.md`, but the file is not in the repository. The crate module docs are the source for those decisions.
