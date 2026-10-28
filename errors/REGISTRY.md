# Error registry

Latest follow-up observation (2026-10-25): `get_indexing_status({})` on
`feature/dead-code-editor`, HEAD `b61cc7ce971fdc46a61259af95af7fac823ae352`
with existing local changes, reports 1,056 files / 18,475 symbols, index timestamp
`1792966895`. The 18 diagnostics are the same categories recorded on 2026-10-24:
16 deliberately malformed corpus fixtures (ERR-003, EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error` (previously observed;
cause not established here), and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020, OPEN).
The status call succeeded; source coverage excludes those rejected files. No new
probe, exclusion, parser fix, or resolution is claimed. Continue ERR-020 with
the existing minimized-reproduction next step; the scoped dashboard queries
succeeded. These observations do not establish index freshness against every
uncommitted byte.

Last observation: 2026-10-10. Checkout HEAD: `dab3059995beaaaf7cd56e0d52db124e207aae55` (`claude/library-onboarding-audit`, clean tree).
Evidence: `mct-cli --root . reindex --force` then `mct-cli --root . status` on that checkout, with no `.mctignore`; index timestamp `1791636301` (550 files, 11,501 symbols). Same 18 diagnostics as the initial snapshot at `1791629091`.

The snapshot reports **18 parse failures: 2 valid scripts rejected by upstream grammars (ERR-001, ERR-002) and 16 intentionally malformed corpus fixtures (ERR-003)**.

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-001 | KNOWN_LIMITATION | `scripts/unix/pr-body.sh:34`; tree-sitter-bash 0.25.1 | `line 34: syntax error`; script missing from the source symbol index. Cause: the grammar rejects `;` inside a `${var:+…}` alternative | Parser rejection of valid input, upstream. `scripts/unix/parse-probe.sh` on one-line files: `x="${a:+$a; }b"` and `x="${a:+x; }b"` fail, `x="${a:-; }"` and `x="${a:+$a, }b"` parse. `bash -n` accepts the script and `bash -c 'a=x; echo "${a:+$a; }b"'` prints `x; b`. 0.25.1 is the latest release. Recorded in [corpus progress](../internal/corpus-progress.md) (Bash row). Next: file upstream; the script is not rewritten to hide it. |
| ERR-002 | KNOWN_LIMITATION | `scripts/windows/pr-status.ps1:14`; tree-sitter-powershell 0.26.4 | `line 14: syntax error`; script missing from the source symbol index. Cause: the grammar rejects a comma-separated bareword argument (`--json number,title,…`), not the multiline jq string first suspected | Parser rejection of valid input, upstream. One-line repros: `Get-Process \| Select-Object Name,Id`, `gh x a,b` and `gh $a[0] --json a,b` fail; `Write-Output 1,2`, `Write-Output @(1,2)`, `gh $a[0] --json "a,b"` and every line of the jq string on its own parse. 0.26.4 is the latest release. Recorded in [corpus progress](../internal/corpus-progress.md) (PowerShell row). PowerShell 7's own parser accepts the script (Windows CI step "PowerShell scripts", PR #170, run 38055596733), confirming a grammar-only rejection. Next: file upstream. |
| ERR-003 | EXPECTED | `crates/mct-lang-*/tests/corpus/malformed/` | 16 invalid fixtures are rejected, as required by the corpus contract | Index diagnostic and [corpus policy](../CONTRIBUTING.md#language-corpus-testscorpus). Preserve rejection; investigate any additional failures outside this fixture set. The fixtures and `assert_malformed_rejected` stay; leaving them out of a day-to-day working index is optional and documented in [Malformed fixtures and the working index](../CONTRIBUTING.md#malformed-fixtures-and-the-working-index). Record observations without that exclusion. |
| ERR-004 | KNOWN_LIMITATION | CSS nesting | Nested selectors retain literal `&` instead of resolving against their parent | [Corpus progress](../internal/corpus-progress.md), issue [#115](https://github.com/Zubiarka8/mini-consumes-tokens/issues/115) (issue state not rechecked). Reproduced 2026-10-12 on `a3e10b0`: new nesting tests in `crates/mct-lang-css/tests/parse.rs` failed (e.g. `missing .a:hover: [".a", ".b", "&:hover", ".title", "&:focus"]`), and the Tailwind v4 corpus test asserted the literal `&:hover`. Fix candidate on branch `claude/css-nesting-115` (`1fa4b3f`, `3cbb88c`), not merged: `cargo test --workspace` passes there (1249 passed, 0 failed, 24 ignored). Decided behaviour: only resolved names are indexed (no literal `&:hover`); repeated `&` takes every parent combination; at most 256 resolved names per rule, and a rule over it is indexed as written with a `/* mct: not expanded, … */` suffix, as are its nested rules. Next: review and merge, then mark RESOLVED after a forced reindex shows resolved names. |
| ERR-005 | KNOWN_LIMITATION | CSS / Tailwind grammar | Some valid at-rule preludes and nesting forms are documented as rejected by tree-sitter-css 0.25; affected files cannot be indexed | [Corpus progress](../internal/corpus-progress.md), issue [#116](https://github.com/Zubiarka8/mini-consumes-tokens/issues/116). Documentation evidence only; reproduce individual forms before diagnosing a current failure. |
| ERR-006 | KNOWN_LIMITATION | JavaScript/TypeScript / React JSX | Component logic is indexed, but JSX rendering relationships are not modeled | [Framework coverage](../CONTRIBUTING.md#frameworklibrary-coverage-beyond-the-language-table). `tsx_component_logic_is_indexed_without_structuring_jsx`, now in `crates/mct-lang-js-ts/src/libraries/react/mod.rs`, passed on 2026-10-10 (`cargo test -p mct-lang-js-ts`). |
| ERR-007 | KNOWN_LIMITATION | Python / Django | Module-level assignments such as `urlpatterns` and model fields lack symbols | [Corpus progress](../internal/corpus-progress.md). Documentation evidence only; no dedicated reproduction run in this session. |
| ERR-008 | RESOLVED | MCP `get_tool_schema` invocation | Caller supplied `tool: "build_context_pack"`; tool returned ``failed to deserialize parameters: missing field `name` `` | First/last observed 2026-10-10. Retried with `name: "build_context_pack"` and received the schema successfully. Caller argument error; no server change needed. |
| ERR-009 | OPEN | macOS native watcher delivery / integration-test environment | FSEvents emits no events in the observed host; kqueue delivers the same file creation | Reproduced with an independent notify 8.2.0 probe: `Fsevent` times out and `Kqueue` reports `Create(File)`. The candidate uses a macOS dev-dependency to select kqueue for test builds; normal builds retain `macos_fsevent` (verified with `cargo tree --target aarch64-apple-darwin -e normal`). Broader validation also exposed parent-directory notifications being announced as source updates even when the scan parsed and removed zero files. The watcher now records those hash-skipped scans at debug level while retaining directory reconciliation. All 11 background-watcher, four delay end-to-end, and five timing tests pass without weakening assertions. Native FSEvents delivery in this host remains unresolved; green kqueue tests do not establish production FSEvents coverage. |
| ERR-010 | RESOLVED | Rust parser / MCP `find_references`, `impact_analysis` | A path in value position (`Arc::new(mct_lang_rust::RustParser)`) left no relation, so `find_references RustParser` returned no hit for `mct-mcp-server`/`mct-cli`/`mct-eval` while those registries used it; only the compiler found every use when the registry moved | First observed 2026-10-10 (`find_references` with `path: crates/mct-mcp-server`, then `crates/mct-cli`, `crates/mct-eval`: "No reference(s) found"). Cause: the `scoped_identifier` arm of the Rust walker recorded nothing, a guard against resolving `other::helper` to this file's `helper`. Fixed on `claude/library-onboarding-audit` (PR #170): the path is a `References` with the same `module`/`qualifier`/`external` evidence as a call through it. Verified 2026-10-10: `a_path_in_value_position_is_a_reference_qualified_by_its_module` (no longer ignored) and `a_path_to_another_module_never_names_this_files_item` pass; Rust corpus snapshot re-blessed (+73 references, reviewed); `check.sh` 1201 passed, 0 failed, 24 ignored, eval no regressions. Independent review of `57d5365` reproduced the live MCP result after `reindex` with `force: true`: `find_references` for `RustParser` reports `build_registry` at `crates/mct-languages/src/lib.rs:12:47`, uniquely resolved to `crates/mct-lang-rust/src/lib.rs:15`. Unchanged files require the documented forced reindex after updating the parser. |
| ERR-011 | RESOLVED | MCP `find_references` invocation | Caller supplied `name: "RustParser"` instead of the required `symbol`; diagnostic: ``failed to deserialize parameters: missing field `symbol` ``. The lookup did not run | First/last recorded 2026-10-10. Historical mistake reported for reconciliation; exact original response was not retained. Reproduced during this review, inspected `get_tool_schema(name: "find_references")`, then retried with `symbol: "RustParser", format: "toon", limit: 1`: succeeded, reporting eight references with one displayed. Caller argument error; no server change. Before invoking a tool, check its required schema fields rather than copying another tool's argument names. Recurred 2026-10-24 for `find_callers` (`name` instead of `function`); retried with `function` and succeeded. Connected primary index evidence only, not a fresh main-index verification. |

First and last observed dates for ERR-001 through ERR-003 and ERR-010: 2026-10-10. ERR-004 through ERR-007 were first recorded here on 2026-10-10 from existing documentation; their original discovery dates are not established.

## Expected fixture diagnostics (ERR-003)

Paths are relative to the project root. Message for every entry: `syntax error`.

| Path | Line |
| --- | --- |
| `crates/mct-lang-xml/tests/corpus/malformed/broken_context.xml` | 1 |
| `crates/mct-lang-lua/tests/corpus/malformed/broken_order.lua` | 50 |
| `crates/mct-lang-php/tests/corpus/malformed/BrokenServices.php` | 1 |
| `crates/mct-lang-rust/tests/corpus/malformed/broken_service.rs` | 1 |
| `crates/mct-lang-html/tests/corpus/malformed/broken_checkout.html` | 52 |
| `crates/mct-lang-java/tests/corpus/malformed/BrokenOrder.java` | 150 |
| `crates/mct-lang-bash/tests/corpus/malformed/broken_config.sh` | 1 |
| `crates/mct-lang-csharp/tests/corpus/malformed/BrokenServices.cs` | 41 |
| `crates/mct-lang-css/tests/corpus/malformed/broken_theme.css` | 1 |
| `crates/mct-lang-go/tests/corpus/malformed/broken_service.go` | 1 |
| `crates/mct-lang-kotlin/tests/corpus/malformed/BrokenOrder.kt` | 50 |
| `crates/mct-lang-js-ts/tests/corpus/malformed/broken_order.ts` | 61 |
| `crates/mct-lang-xaml/tests/corpus/malformed/BrokenOrdersView.xaml` | 1 |
| `crates/mct-lang-cpp/tests/corpus/malformed/broken_service.cpp` | 42 |
| `crates/mct-lang-python/tests/corpus/malformed/broken_services.py` | 63 |
| `crates/mct-lang-powershell/tests/corpus/malformed/Broken.Api.psm1` | 1 |


## Privacy review environment (2026-10-12)

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-012 | RESOLVED | Local MCP startup in the privacy review worktree / filesystem sandbox | `Error: database error: unable to open database file`; cause: `Error code 14: unable to open database file`. The local server closed stdout before the initialize reply, preventing source lookup | First/last observed 2026-10-12 on `feat/index-privacy-hygiene`, base `32f8a79ccfcd09c3d790e43edc284d7602e9737b`, dirty tree. The same binary and root started successfully with authorized filesystem access. `get_indexing_status` then succeeded (594 files, 12,370 symbols, timestamp 1791792386), reporting only the 16 intentionally malformed ERR-003 fixtures. Targeted `build_context_pack` calls succeeded. Environment restriction; no database-tool bypass or code fix required. |

## Issue #207 planning (2026-10-15)

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-013 | RESOLVED | MCP `get_file_skeleton` invocation (inside `batch`) | Caller supplied `path: "crates/mct-core/src/parser.rs"`, a file that does not exist; diagnostic: `` `crates/mct-core/src/parser.rs` was not found under the indexed project root ``. Only that sub-query failed | First/last observed 2026-10-15 on `feat/207-token-twins` at `2705d64`. `LanguageParser` is defined in `crates/mct-core/src/lib.rs`; `build_context_pack(symbol: "LanguageParser")` then succeeded. Caller path guess; no server change. |
| ERR-014 | RESOLVED | MCP `build_context_pack` invocation (inside `batch`) | Caller supplied `name: "dead_code_report"` instead of the required `symbol`; diagnostic: ``invalid args for `build_context_pack`: missing field `symbol` `` | First/last observed 2026-10-15 on `feat/207-token-twins` at `2705d64`. Retried with `symbol: "dead_code_report"`: succeeded. Caller argument error, same class as ERR-011; no server change. |


## Issue #207 improvement validation (2026-10-15)

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-015 | RESOLVED | MCP server startup when the binary is older than the index schema (first: local release binary against a temporary tracked-crates index; recurrence: installed `mct-mcp-server` against the primary checkout's index) | `Error: migration error: rusqlite_migration error in migrations definition: Attempt to migrate a database with a migration number that is too high`. Connection closed before the initialize response; auxiliary source review unavailable | First/last observed 2026-10-15 on the dirty `feat/207-token-twins` tree based on `ac525ca`. Operation: `target/release/mct-mcp-server --root <temporary tracked-crates snapshot>`, JSON-RPC initialize. Rebuilt this branch's release server without touching the primary index. Retried initialize and `tools/list` successfully; `get_indexing_status` succeeded with 517 files, 10,510 symbols, timestamp 1792070934, and only the 16 expected ERR-003 fixtures. Stale binary/schema mismatch; no schema change or database bypass. Recurred 2026-10-16: the Claude Code session reported `mini-consumes-tokens` `CONNECTION_CLOSED`; `scripts/unix/mcp-smoke.sh` in the primary checkout (`claude/semantic-model-eval`, HEAD `ac525ca`, dirty) printed the same migration error from the installed server. Not fixed: the documented fix (`scripts/unix/reinstall.sh --reindex`) reinstalls the binaries and rebuilds the primary checkout's index, which that session was told not to touch. Exploration used `mct-cli --root <feat/207 worktree> call` against the worktree's own index instead. Recurred again 2026-10-19 (last observed): the Claude Code session reported `mini-consumes-tokens` `CONNECTION_CLOSED`; `scripts/unix/mcp-smoke.sh` in the same primary checkout (still `ac525ca`, dirty, while main is `19e3733`) printed the same migration error. Not fixed for the same reason; the session inspected Git state only and needed no index query. Next: run the reinstall from the checkout that should own the installed binaries, reconnect, and confirm initialize succeeds before closing. Resolved 2026-10-19: `scripts/unix/reinstall.sh --reindex` from the primary checkout on `main` (`83dff6d`) installed `mct-mcp-server` and `mct-cli` v0.2.0-326-g83dff6d and rebuilt the index (1289 parsed, 21031 symbols); `mcp-smoke.sh` listed 22 tools; after `/mcp` the client reported `Reconnected to mini-consumes-tokens` and `get_indexing_status` answered with 1289 files / 21031 symbols. |
| ERR-016 | RESOLVED | Wrong MCP corpus selected for project lookups | Connected `cargo_deps` reports 6 files / 293 symbols; `get_file_tree(depth=2)` confirms the `smallvec-1.16.1` dependency corpus. Missing project symbols in that server do not demonstrate failed project indexing | First observed 2026-10-15; scope confirmed 2026-10-18 on dirty `claude/semantic-model-eval`, base `ac525ca`. Fresh isolated CLI and server query the explicit project root successfully (1,206 files / 19,662 symbols). Continue with explicit-root isolated tools; the client's intended project-server registration/reconnection remains unverified. See the [investigation](../project-management/checkpoints/index-parse-investigation-2026-10-18.md). Resolved 2026-10-19: after the reinstall and `/mcp` reconnect, the connected `mini-consumes-tokens` server reports this project (1289 files / 21031 symbols, 18 expected ERR-003 diagnostics), not the 6-file dependency corpus. |

## Semantic proposal review (2026-10-16)

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-017 | RESOLVED | Installed CLI/server / current checkout schema | Installed CLI `status` and `scripts/unix/mcp-smoke.sh` both exit 1 with `Attempt to migrate a database with a migration number that is too high`; the installed server fails before MCP initialize | First observed 2026-10-16; reproduced 2026-10-18 on dirty `claude/semantic-model-eval`, HEAD `ac525cac1453f2b296588abe621d0e002ac96d66`. Both existing local release binaries and freshly built isolated binaries open the same project index successfully; fresh server initialize/status and fresh CLI forced reindex exit 0. Global binaries and client registration were not changed. Use per-agent absolute executable paths and explicit roots; synchronize a shared installation only under one owner. No database deletion or manual schema change. Resolved 2026-10-19: the installed CLI and server were reinstalled from `main` (`83dff6d`) with the index rebuilt; `mcp-smoke.sh` printed `server ok` and the reconnected server's `get_indexing_status` succeeded. Same cause and fix as ERR-015. |

ERR-016 observation, 2026-10-16: connected `cargo_deps.get_indexing_status` still reports 6 files / 293 symbols, timestamp `1792140027`, insufficient coverage for this checkout. Explicit-root local release MCP instead reports 1,192 files / 19,548 symbols, timestamp `1792140247`; targeted source context packs succeed. Client reconnection/root reconciliation remains pending; this is scope mismatch evidence, not a failed query or a confirmed parser defect.

ERR-003 observation, 2026-10-16: the explicit-root status reports the same 16 intentionally malformed fixtures, plus copies of `broken_order.lua:50` and `BrokenOrdersView.xaml:1` under `prompt/github/discarded/resolve-pr151-snapshot/`. All 18 diagnostics are `syntax error` in malformed fixtures; no unexpected parse rejection was observed in this snapshot. The duplicate source snapshot also affects corpus counts and must be identified in semantic benchmarks.

## Concurrent-agent isolation investigation (2026-10-18)

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-018 | OPEN | Normal Cargo target artifacts on dirty `claude/semantic-model-eval` | `cargo build --locked -p mct-cli` exits 101 with five errors for core APIs and fields that are present in the source; `cargo check` exits 0 | Same sources build CLI and server successfully in a fresh `CARGO_TARGET_DIR` (both exit 0). The new CLI forcibly indexes the project and the new server completes initialize/status. Temporary isolated launchers are validated; the original target remains unchanged. Artifact-dependent failure is established, but the precise invalidation/overwrite mechanism and writer are unproven. Use separate target directories, binaries and worktrees/indexes per agent. See the [investigation](../project-management/checkpoints/index-parse-investigation-2026-10-18.md). |

ERR-003 observation, 2026-10-18: 18 current malformed-file diagnostics are reproduced by direct probes: 16 corpus fixtures and two identical Lua/XAML copies in the untracked discarded snapshot. Fresh CLI reindex succeeds with those diagnostics. A disposable corpus reproduces 18 → 16 → 0 diagnostics when excluding the snapshot, then the intentional malformed fixtures, while retaining a valid control. Real project exclusions and fixtures remain unchanged. The claimed 28 diagnostics have not been reproduced in this snapshot. The shared malformed-rejection contract test passes.

## MCP startup timeout (2026-10-20)

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-019 | RESOLVED | Codex CLI, primary mini-consumes-tokens registration | Client reports `connected (0 tools)`; server starts initial reindex but initialization times out. Tools unavailable to the session. | First/last observed 2026-10-20 on dirty feature/obsidian-index, base a243703. Client log at 00:03:26 local: `MCP server startup failed ... error=timed out handshaking with MCP server after 29.999999917s`; initial reindex began at 00:02:56. Installed binary answers initialize/tools-list on an empty temporary project: 22 tools including get_indexing_status. Installed CLI opens the primary index successfully, so this observation is not the previous schema mismatch. Project-local startup timeout raised to 180 seconds and TOML verified. Later primary-root get_indexing_status succeeded (1303 files / 21257 symbols, timestamp 1792448999), together with search and context queries; client recovery is verified. No index deletion, tool filtering or application-code workaround. |

ERR-019 verification, 2026-10-20: the primary MCP now answers search_symbols, build_context_pack and get_indexing_status: 1303 files / 21257 symbols, timestamp 1792448999. Correct-root tool discovery/query recovery verified. Nineteen parse diagnostics remain pre-existing: eighteen intentionally malformed fixtures/copies and scripts/windows/review-check.ps1:29.

ERR-014 recurrence, 2026-10-20 (RESOLVED caller error): three primary build_context_pack calls supplied name instead of symbol (run, DuplicateReport, DuplicateMember); exact diagnostic: ``failed to deserialize parameters: missing field `symbol` ``. Retried all three with symbol and the same paths; all succeeded. No server defect or code change.

ERR-014 recurrence, 2026-10-24 (RESOLVED caller error): `build_context_pack` for `SymbolListEntry` used `name` instead of `symbol`; exact diagnostic: ``failed to deserialize parameters: missing field `symbol` ``. Retrying with `symbol: SymbolListEntry`, `source_lines: 25`, `format: toon` succeeded and returned its definition. Source lookup was briefly delayed; no server defect.

## Dashboard audit observation — 2026-10-24

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-020 | OPEN | `crates/mct-cli/src/web/spa/spa.css:1` | Project parser rejects the generated dashboard CSS: `syntax error`; stylesheet is absent from source symbol coverage. This is a parser rejection, not evidence that browser styling fails. | Latest MCP `get_indexing_status({})` on 2026-10-25: 1,056 files / 18,475 symbols, timestamp `1792966895`; same CSS line-1 and PowerShell line-29 rejections among 18 diagnostics. Earlier CLI probe reproduced both. Minimize the CSS construct before classifying grammar support or creating a parser fix. No exclusions added. |
| ERR-021 | RESOLVED | MCP `build_context_pack` invocation | Caller guessed symbols `with_index` and `lookup`, which are not indexed; diagnostics: `No symbol named `with_index` found in the index — try search_symbols for a partial name.` and `No symbol named `lookup` found in the index — try search_symbols for a partial name.` Those lookups did not run. | First/last observed 2026-10-25 during Phase 4 audit on dirty `feature/dead-code-editor`, HEAD `b61cc7ce971fdc46a61259af95af7fac823ae352`. Retried with `list_symbols` on `cache.rs`, then `build_context_pack` for `lookup_exact`, `validate`, and `check`: succeeded. Caller name-guess error; no source defect inferred. |
| ERR-022 | OPEN | Claude Code session startup, primary mini-consumes-tokens registration (installed `mct-mcp-server`) | Client reported `MCP server mini-consumes-tokens connection timed out after 30000ms` (CONNECT_TIMEOUT); MCP tools unavailable for the whole session. Source lookup fell back to this project's `mct-cli call`, as rules.md permits. | Observed 2026-10-26 on dirty feature/dead-code-editor, HEAD b61cc7c. At that time at least eight long-running `mct-mcp-server --root <primary>` processes from other sessions were alive, and the installed binary dates from 2026-10-19, older than the candidate. `mct-cli call --tool get_indexing_status` on the same root succeeded in about 3.1 s (1069 files / 18614 symbols, published generation 1, timestamp 1793007675; the same 18 parse diagnostics), so the index itself opens. Cause not established: possibly startup reindex/lock contention or the older installed binary (compare ERR-019). The installed connector was deliberately not restarted or reinstalled during Phase 9.1. 2026-10-26 independent audit reproduced the mechanism: the server answers `initialize` only after its startup indexing. On the primary root, with seven installed-binary servers alive and host load average 22-39, `initialize` took 100.7 s and 96.7 s (installed binary) and 136.3 s (candidate 839b5c85); `scripts/unix/mcp-smoke.sh` succeeded (22 tools) after 78.5 s. On an isolated 2000-file fixture with no other server, the candidate took 57.6 s cold, 14.1 s unchanged and 22.6 s with half the files changed offline. Both exceed or approach the 30 s client limit, so this is a startup-ordering defect aggravated by contention, not an index fault. Next: answer `initialize` before startup indexing (report readiness via `get_indexing_status`), retire duplicate installed-binary servers, then reverify with `scripts/unix/mcp-smoke.sh`. No index deletion or tool filtering. Recurred 2026-10-27 (last observed) on the same branch and HEAD b61cc7c: a Claude Code session again reported `connection timed out after 30000ms` (CONNECT_TIMEOUT), with only two `mct-mcp-server` processes alive and load average 3-5, so heavy contention is not required to exceed the 30 s limit. The installed binary still dates from 2026-10-19. That session needed no index query (native dashboard acceptance on a disposable fixture) and did not restart or reinstall the connector. |

2026-10-25 Phase 8 audit follow-up: caller-side MCP name guesses (`duplicate`,
`duplicate_report`, `run_duplicates`, `reviewed_duplicate_groups`,
`find_dead_code_filters_valid_false_positives`, and a path-scoped
`build_context_pack` symbol) returned “No symbol named … found in the index”.
The calls were corrected by querying the catalog and searching indexed names;
`DuplicateGroup`, `find_duplicate_groups`, `duplicate_visibility`, and the
actual `run` entry points were found. Caller discovery misses, no server defect.

ERR-003/known parse observation, 2026-10-24: the other 17 diagnostics are the 16 deliberate malformed fixtures and the previously observed `scripts/windows/review-check.ps1:29: syntax error`. The latter reproduces with the same direct probe; no new fix or resolution is claimed. Caller initially used an unsupported `parse-probe` CLI subcommand (exit 2), checked CLI help and reran the supported `probe` command. This was not a failed MCP request.

2026-10-25 copy-label implementation startup: get_indexing_status({}) succeeded (1025 files, 17886 symbols, timestamp 1792915721). Same 18 parse diagnostics: 16 expected malformed fixtures (ERR-003), `scripts/windows/review-check.ps1: line 29: syntax error`, and `crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN). No new parser probe or resolution.


2026-10-25 database lifecycle planning: get_indexing_status({}) succeeded on dirty
feature/dead-code-editor, HEAD b61cc7ce971fdc46a61259af95af7fac823ae352: 1031 files,
17987 symbols, index timestamp1792917470. Same18 diagnostics: 16 expected malformed
fixtures (ERR-003), scripts/windows/review-check.ps1: line29: syntax error, and
crates/mct-cli/src/web/spa/spa.css: line1: syntax error (ERR-020 OPEN).
No new reproduction or resolution; source context batches succeeded.


2026-10-25 editor bulk controls implementation: get_indexing_status({}) succeeded
with 1032 files, 18001 symbols, timestamp1792917879. The same18 diagnostics
persist:16 intentional malformed fixtures (ERR-003 EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error`, and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Scoped context queries succeeded. No parser fix or resolution claimed.


2026-10-25 dashboard reliability audit: get_indexing_status({}) succeeded at
feature/dead-code-editor b61cc7ce971fdc46a61259af95af7fac823ae352 with local work:
1034 files, 18019 symbols, index timestamp 1792921220. The same 18 diagnostics
remain: 16 expected malformed fixtures (ERR-003),
`scripts/windows/review-check.ps1: line 29: syntax error` and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Context/skeleton queries succeeded. No primary reindex, parser fix or resolution
is claimed; those diagnostics do not establish browser styling failure.


2026-10-25 dashboard delivery follow-up: get_indexing_status({format: "toon"})
succeeded on dirty feature/dead-code-editor at b61cc7ce971fdc46a61259af95af7fac823ae352:
1035 files, 18043 symbols, index timestamp 1792925238. Same 18 diagnostics:
16 deliberate malformed fixtures (ERR-003 EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error` and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Scoped build_context_pack(run, web.rs) succeeded. No new parser probe, primary
reindex or resolution claimed. Official asset generation now succeeds; this does
not resolve the generated CSS parser rejection or prove browser rendering.


2026-10-25 native acceptance recheck: get_indexing_status({}) succeeded on dirty
feature/dead-code-editor at b61cc7ce971fdc46a61259af95af7fac823ae352: 1036 files,
18059 symbols, index timestamp 1792925556. Same 18 diagnostics: 16 deliberately
malformed fixtures (ERR-003 EXPECTED), scripts/windows/review-check.ps1: line 29:
syntax error, and crates/mct-cli/src/web/spa/spa.css: line 1: syntax error
(ERR-020 OPEN). Scoped context/search queries succeeded. No parser probe,
manual primary reindex, parser fix or resolution claimed; runtime socket EPERM
is an independent environmental observation, not attributed to these diagnostics.

2026-10-25 runtime/hook investigation: get_indexing_status({}) succeeded on dirty
feature/dead-code-editor at b61cc7ce971fdc46a61259af95af7fac823ae352: 1036 files,
18072 symbols, index timestamp 1792926155. Same 18 diagnostics: 16 deliberate
malformed fixtures (ERR-003 EXPECTED), scripts/windows/review-check.ps1: line 29:
syntax error, and crates/mct-cli/src/web/spa/spa.css: line 1: syntax error
(ERR-020 OPEN). get_file_skeleton(sqlite_guard.py) and scoped
build_context_pack(main, sqlite_guard.py) succeeded. No parser fix/reindex claimed.
Independent socket EPERM remains an environment issue, not a failed MCP request.
Effective sandbox_check network denial, hook checks and native Terminal access
denial are documented in lifecycle section 15. Exact inherited policy unresolved;
no security controls or hooks changed.


2026-10-25 storage reliability audit: `get_indexing_status({})` succeeded on
b61cc7ce971fdc46a61259af95af7fac823ae352 plus existing local changes: 1,036 files,
18,081 symbols, index timestamp 1792929587. Same eighteen diagnostics: sixteen
intentional malformed corpus fixtures (ERR-003 EXPECTED),
`scripts/windows/review-check.ps1: line 29: syntax error`, and
`crates/mct-cli/src/web/spa/spa.css: line 1: syntax error` (ERR-020 OPEN).
Scoped context packs and batch lookups succeeded. No parser probe, primary
reindex, exclusion change or resolution claimed. Existing classifications remain;
next parser action is the existing minimized CSS/PowerShell investigation.


2026-10-25 writer-coordination audit: project MCP get_indexing_status reported
1,045 files /18,183 symbols, timestamp1792931650. Same18 parse diagnostics:
16 intentional malformed fixtures (ERR-003), scripts/windows/review-check.ps1:
line29 syntax error and generated web/spa/spa.css:line1 syntax error (ERR-020).
No parser correction claimed. One caller lookup requested nonexistent
crates/mct-index/tests/read_only.rs: `was not found under the indexed project root`;
no source defect inferred; use the actual indexed test paths instead.

2026-10-25 Phase 3 audit: get_indexing_status({}) succeeded, 1049 files,
18240 symbols, timestamp1792935229. Same18 diagnostics as writer phase:16
intentional malformed fixtures(ERR-003), scripts/windows/review-check.ps1 line29
syntax error and web/spa/spa.css line1 syntax error(ERR-020). No parser fix or
primary reindex claimed. Scoped skeleton queries succeeded.


2026-10-25 Phase 5 audit: get_indexing_status({format: "toon"}) succeeded: 1053 files, 18374 symbols, timestamp 1792940148. Same 18 diagnostics: 16 malformed corpus fixtures (ERR-003 EXPECTED), scripts/windows/review-check.ps1 line 29 and generated web/spa/spa.css line 1 (ERR-020 OPEN). Three batch build_context_pack calls incorrectly used name instead of symbol: `invalid args for `build_context_pack`: missing field `symbol`` (ERR-021 caller error). Retrying with symbol succeeded. No parser correction or production reindex claimed.

Phase6 re-observation (2026-10-25): ERR-003/020 unchanged in status with the
same18 indexed diagnostics; ERR-021 three absent-name context lookups resolved
by symbol discovery and actual-name calls. See HISTORY; no server/parser fix.


2026-10-25 Phase7: get_indexing_status({}) succeeded with1055files/18429symbols,
timestamp1792964470. Same18 diagnostics:16 intentional malformed corpus fixtures
(ERR-003 EXPECTED), scripts/windows/review-check.ps1:line29 syntax error and
web/spa/spa.css:line1 syntax error(ERR-020 OPEN). Scoped discovery/context
lookups succeeded. No parser fix, forced primary reindex or production action.

Phase7 dashboard follow-up: native Safari exposed rendered false-positive
reevaluation after unchanged reindex, distinct from MCP/parser diagnostics.
Same-server HTTP regression reproduced it; source-only read applicability
with unchanged strict write freshness corrected it. Final native candidate
2c0468eb... verified unchanged FP1/reevaluate0, then modified body FP0/
reevaluate1. Detailed evidence in storage-reliability-verification.md Phase7.

2026-10-26 Phase 8.1: ERR-021 caller error re-observed: build_context_pack used
query instead of required symbol (`failed to deserialize parameters: missing
field `symbol``). Corrected symbol/path requests succeeded for find_dead_code,
find_duplicate_groups and duplicate_visibility. No server defect inferred.
get_indexing_status succeeded:1060files/18543symbols, timestamp1792997392,
same18diagnostics (16 intentional malformed fixtures ERR-003; PowerShell
review-check.ps1:29 and generated SPA CSS:1 ERR-020 OPEN). No forced primary
reindex or parser fix. An absent stale_snapshot symbol lookup was nonfatal;
call_one context identified the actual fixture issue: that CLI wrapper reindexes
before dispatch. Read-only MCP regression now calls the read-only dispatcher.

2026-10-26 Phase9: get_indexing_status({}) succeeded:1063files/18565symbols, timestamp1792999842. Same18 parse diagnostics:16 intentionally malformed fixtures ERR-003 EXPECTED, scripts/windows/review-check.ps1 line29 syntax error and crates/mct-cli/src/web/spa/spa.css line1 syntax error ERR-020 OPEN. No parser correction or forced primary reindex. Connected catalog lacks local find_duplicate_code; use newly built isolated process for acceptance, do not restart installed services.

2026-10-26 Phase9 browser hardening: Safari's inspector reproduced Alpine CSP errors for `activeFile.warning` before asynchronous finding load. Initial activeFile omitted warning; loaded rows supplied it. A new Node regression failed (`undefined !== ''`) before the one-property initialization fix. Security policy unchanged; no eval/inline workaround. Fresh embedded binary/native recheck pending. Also a real stale POST api/comment/update returned409 (27.9ms), retained draft A and saved newer B; native AX evidence redacted.

## Phase 9 acceptance findings (2026-10-26)

| ID | Status | Scope | Evidence / next action |
| --- | --- | --- | --- |
| QA-001 | OPEN (local correction tested) | Alpine CSP bootstrap | Native Safari logs `Alpine is unable to interpret ... activeFile.warning`; default activeFile omitted warning. New Node test failed undefined-versus-empty before adding warning initialization, then43frontend tests passed. Rebuilt native recheck pending; no CSP relaxation. |
| QA-002 | OPEN | Native watcher delivery/timing acceptance | First official check:1958passed/1failed/27ignored; reindex_delay_e2e::a_saved_file_becomes_searchable_only_after_the_delay exceeded30s. Isolated same target4/4passed. Root cause not established; do not increase timeout or hide the failure. Final full rerun pending. |
| QA-003 | BLOCKED | Current dependency audit / asset regeneration | npm tailwindcss@3.4.17 registry GET403 after official build and escalated retry. Fresh RustSec fetch failed after escalation; cached4b587f00 dated2026-10-21 reports no vulnerabilities and paste1.0.15 unmaintained via optional fastembed/tokenizers. Fresh audit remains release gate. |

ERR-021 Phase9 discovery recurrence: guessed `open_connection` in store.rs was absent; no source defect inferred. Main/background symbol discovery and exact context calls succeeded. Profiling harness initially assumed a directory page contained all functions plus modules, causing a fixture assertion; corrected to query renamed file and indexed-file count. This was not an MCP/server failure.

Phase9 final: QA-002 final full retry1959/0/27 PASS; initial failure remains open, linked to ERR-009 native watcher acceptance without assuming identical cause. Existing kqueue test backend is not proof of productionFSEvents. QA-00143Node tests pass/new embedded CLI built; corrected native run still pending. QA-003 fresh audit/assets remain blocked. No parser/legacy/production acceptance closure.

2026-10-26 Phase9.1: status retains known parse limitations; absent
spawn_background_indexer lookup was a caller discovery miss, resolved with
list_symbols and spawn_watcher_with_reconciliation (ERR-021). No parser fix.
QA-00210bounded repetitions4/4eachPASS, historical failure remains OPEN.
New QA-004: expired receive timeout still consumes queued Changed::Nothing,
confirmed deterministic regression red (Ok(Nothing) versus Err(Timeout)).
Deadline priority correction and two unit tests PASS; unrelated to QA-002 until
evidence links them. Capture bounded subprocess stderr on future failures.
Production-only build logs backend=Fsevent; native harness failed to observe
first create within8s with reconciliation600s. This is FAIL in this environment,
not proof of backend-wide corruption or a kqueue result. Preserve ERR-009.
Official npm registry403 persists after escalation; offline official build
returns ENOTCACHED for @alpinejs/csp metadata despite tarballs in cache.
Fresh audit in writable target also fails network after escalation. No integrity
checks, permissions or registry configuration were bypassed.

Phase9.1 native operator evidence (current session, supplied by owner): normal
MCP SHA25685df433b32305cd375b7f473f8e79a4dc14d5c06d69ed1cdc504839b04efa843,
macOS27.0ARM64, backend=Fsevent at2026-10-26T08:55:03Z. All10create/modify/
rename/delete rounds and64file burst PASS, failures0, reconciliation600s.
104observations,median2.144ms,p95151.056ms,variance4052.247ms²; observations
include already-absent/unchanged lookups, not104independent event latencies.
No fallback within the8s observation bounds. This establishes native delivery
for this fixture; agent-side missing events remain environment-specific evidence
and QA-002 is not closed by this success alone. Separately the agent's exact
Fsevent build51a1f6df... with supported60s interval recovers a new file at
60024.849ms (n=1). Initial interval1s harness run was rejected before MCP
startup; corrected validation honors the CLI minimum60, no product limit change.

Phase9.1 new full-check failure retained:1960passed/1failed/27ignored;
index_generations::source_edit_during_parse_aborts_instead_of_publishing_stale_bytes
found0old_a symbols instead of1. Isolated rerun passed. Test fixture allocator
used only SystemTime and create_dir_all: forced repeated nonce reproduces shared
DB paths deterministically. Fixed fixture allocation with process/atomic serial
and atomic create_dir; added repeated-clock regression and seed assertions.
All7generation tests and20repetitions pass. This removes a demonstrated fixture
isolation flaw, but the original run did not log the root, so its exact cause
is not proven. Keep incident QA-005 OPEN pending final evidence/risk review.
Clippy's two unwrap diagnostics in new test setup corrected with assert(is_ok);
no production panic allowance added. Final rerun recorded separately.

Phase9.1 QA-002 reproduction now diagnosed for the second run: child stderr
backend=Fsevent at09:01:10Z. Normal builds replaced sharedtarget/debug MCP
while compiled integration tests were running.1961/1/27,Clippy/evalPASS.
Coordinator corrects validation isolation with a separate target directory.
Original Phase9 incident lacks backend logs and remains unproven. Preserve
this failure; nativeTerminalFSEventsPASS and agentfallbackPASS are distinct.


## Rust-first Batch 1 — 2026-10-27

Local working-tree verification on `feature/dead-code-editor`, base `b61cc7c`.
These statuses concern disposable fixtures and current source, not an installed
server or a refreshed production index. See
[Rust functional coverage](../docs/01-architecture/rust-functional-coverage.md).

| ID | Status | Scope | Evidence and next action |
| --- | --- | --- | --- |
| ERR-023 | RESOLVED | D1 MCP source attribution, local source | A context pack applied old `alpha` ranges to `pub fn unrelated()` and used `pub fn different()` as the related `beta` signature. The regression failed before the exact-byte indexed-hash guard and passes after it. Stale search/context source is omitted; skeleton requests reindexing. No installed-binary verification claimed. |
| ERR-024 | RESOLVED | D7 binary conversion diagnostic, local source | Enabled `a_broken_file_turned_binary_drops_its_syntax_error` first failed with an obsolete `src/util.fake` error. Full/path regression now clears it while retaining the other file's syntax error and matching a fresh index. No production reconciliation performed. |
| ERR-025 | OPEN | D3 Rust macro methods | Explicit ignored `a_method_call_inside_a_macro_token_tree_leaves_a_relation` fails: actual `[]`, expected lines `[4, 5]`. Token-tree accessor guard omits the methods. Separate Rust extractor batch required; preserve receiver ambiguity. |
| ERR-026 | KNOWN_LIMITATION | Rust same-name dead-code false negative | Isolated project has `used::helper` and uncalled `unused::helper`. `find_calls(function=main)` targets only `src/used.rs:1`; `find_dead_code` reports no candidates. Name/language aggregation hides the other function. Separate labeled detector regression and conservative identity-resolution design required; not review-state loss. |
| ERR-027 | RESOLVED (local MCP configuration) | Codex `cargo-deps` MCP root | Configured root `vendor/mct-pilot/smallvec-1.16.1` no longer exists; using the Cargo registry cache directly fails because MCP cannot create `.mct-index` there (`Operation not permitted`). | Repointed ignored local Codex config to `/private/tmp/mct-cargo-deps/smallvec-1.16.1`, a writable copy of cached `smallvec 1.16.1`. Verified MCP `initialize`, `tools/list` (22 tools), and `get_indexing_status` (5 files / 291 symbols). The temporary copy must be recreated if `/private/tmp` is cleared. |

ERR-021 caller-error recurrence: the new context fixture initially used `name`
instead of `symbol` (invalid args for build_context_pack: missing field symbol). An isolated CLI probe initially passed `symbol` rather than
`function` to `find_calls` (invalid args for find_calls: missing field function). Corrected calls succeeded. Two initial CLI probes also omitted
`--tool` and were rejected by Clap before MCP invocation. These are fixture/
caller mistakes, not production defects. The new D8 fixture initially assumed
incorrect end lines; corrected from actual source/output to L1-L9/L1-L11.
