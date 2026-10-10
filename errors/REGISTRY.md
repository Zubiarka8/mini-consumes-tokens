# Error registry

Last observation: 2026-10-10. Checkout HEAD: `dab3059995beaaaf7cd56e0d52db124e207aae55` (`claude/library-onboarding-audit`, clean tree).
Evidence: `mct-cli --root . reindex --force` then `mct-cli --root . status` on that checkout, with no `.mctignore`; index timestamp `1791636301` (550 files, 11,501 symbols). Same 18 diagnostics as the initial snapshot at `1791629091`.

The snapshot reports **18 parse failures: 2 valid scripts rejected by upstream grammars (ERR-001, ERR-002) and 16 intentionally malformed corpus fixtures (ERR-003)**.

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-001 | KNOWN_LIMITATION | `scripts/unix/pr-body.sh:34`; tree-sitter-bash 0.25.1 | `line 34: syntax error`; script missing from the source symbol index. Cause: the grammar rejects `;` inside a `${var:+…}` alternative | Parser rejection of valid input, upstream. `scripts/unix/parse-probe.sh` on one-line files: `x="${a:+$a; }b"` and `x="${a:+x; }b"` fail, `x="${a:-; }"` and `x="${a:+$a, }b"` parse. `bash -n` accepts the script and `bash -c 'a=x; echo "${a:+$a; }b"'` prints `x; b`. 0.25.1 is the latest release. Recorded in [corpus progress](../internal/corpus-progress.md) (Bash row). Next: file upstream; the script is not rewritten to hide it. |
| ERR-002 | KNOWN_LIMITATION | `scripts/windows/pr-status.ps1:14`; tree-sitter-powershell 0.26.4 | `line 14: syntax error`; script missing from the source symbol index. Cause: the grammar rejects a comma-separated bareword argument (`--json number,title,…`), not the multiline jq string first suspected | Parser rejection of valid input, upstream. One-line repros: `Get-Process \| Select-Object Name,Id`, `gh x a,b` and `gh $a[0] --json a,b` fail; `Write-Output 1,2`, `Write-Output @(1,2)`, `gh $a[0] --json "a,b"` and every line of the jq string on its own parse. 0.26.4 is the latest release. Recorded in [corpus progress](../internal/corpus-progress.md) (PowerShell row). PowerShell 7's own parser accepts the script (Windows CI step "PowerShell scripts", PR #170, run 38055596733), confirming a grammar-only rejection. Next: file upstream. |
| ERR-003 | EXPECTED | `crates/mct-lang-*/tests/corpus/malformed/` | 16 invalid fixtures are rejected, as required by the corpus contract | Index diagnostic and [corpus policy](../CONTRIBUTING.md#language-corpus-testscorpus). Preserve rejection; investigate any additional failures outside this fixture set. The fixtures and `assert_malformed_rejected` stay; leaving them out of a day-to-day working index is optional and documented in [Malformed fixtures and the working index](../CONTRIBUTING.md#malformed-fixtures-and-the-working-index). Record observations without that exclusion. |
| ERR-004 | KNOWN_LIMITATION | CSS nesting | Nested selectors retain literal `&` instead of resolving against their parent | [Corpus progress](../internal/corpus-progress.md), issue [#115](https://github.com/Zubiarka8/mini-consumes-tokens/issues/115). Documentation evidence only; current issue state and reproduction not checked. |
| ERR-005 | KNOWN_LIMITATION | CSS / Tailwind grammar | Some valid at-rule preludes and nesting forms are documented as rejected by tree-sitter-css 0.25; affected files cannot be indexed | [Corpus progress](../internal/corpus-progress.md), issue [#116](https://github.com/Zubiarka8/mini-consumes-tokens/issues/116). Documentation evidence only; reproduce individual forms before diagnosing a current failure. |
| ERR-006 | KNOWN_LIMITATION | JavaScript/TypeScript / React JSX | Component logic is indexed, but JSX rendering relationships are not modeled | [Framework coverage](../CONTRIBUTING.md#frameworklibrary-coverage-beyond-the-language-table). `tsx_component_logic_is_indexed_without_structuring_jsx`, now in `crates/mct-lang-js-ts/src/libraries/react/mod.rs`, passed on 2026-10-10 (`cargo test -p mct-lang-js-ts`). |
| ERR-007 | KNOWN_LIMITATION | Python / Django | Module-level assignments such as `urlpatterns` and model fields lack symbols | [Corpus progress](../internal/corpus-progress.md). Documentation evidence only; no dedicated reproduction run in this session. |
| ERR-008 | RESOLVED | MCP `get_tool_schema` invocation | Caller supplied `tool: "build_context_pack"`; tool returned ``failed to deserialize parameters: missing field `name` `` | First/last observed 2026-10-10. Retried with `name: "build_context_pack"` and received the schema successfully. Caller argument error; no server change needed. |
| ERR-009 | RESOLVED | MCP server `background_watcher` integration tests / execution environment | In the sandbox, three tests failed to observe file changes: reindex after edit, ignore-rule reload, and deletion/rename | First/last observed 2026-10-10 on the library-maintainability worktree based on `7558113`. The sandbox run stopped at 1,013 passed, 3 failed, 23 ignored. The same watcher test binary passed all 11 tests outside the sandbox with `cargo test --offline --locked -p mct-mcp-server --test background_watcher` (exit 0, 20.30 s), without code changes. Environment-dependent validation failure; use the authorized native rerun for filesystem-event tests. |
| ERR-010 | RESOLVED | Rust parser / MCP `find_references`, `impact_analysis` | A path in value position (`Arc::new(mct_lang_rust::RustParser)`) left no relation, so `find_references RustParser` returned no hit for `mct-mcp-server`/`mct-cli`/`mct-eval` while those registries used it; only the compiler found every use when the registry moved | First observed 2026-10-10 (`find_references` with `path: crates/mct-mcp-server`, then `crates/mct-cli`, `crates/mct-eval`: "No reference(s) found"). Cause: the `scoped_identifier` arm of the Rust walker recorded nothing, a guard against resolving `other::helper` to this file's `helper`. Fixed on `claude/library-onboarding-audit` (PR #170): the path is a `References` with the same `module`/`qualifier`/`external` evidence as a call through it. Verified 2026-10-10: `a_path_in_value_position_is_a_reference_qualified_by_its_module` (no longer ignored) and `a_path_to_another_module_never_names_this_files_item` pass; Rust corpus snapshot re-blessed (+73 references, reviewed); `check.sh` 1201 passed, 0 failed, 24 ignored, eval no regressions. Independent review of `57d5365` reproduced the live MCP result after `reindex` with `force: true`: `find_references` for `RustParser` reports `build_registry` at `crates/mct-languages/src/lib.rs:12:47`, uniquely resolved to `crates/mct-lang-rust/src/lib.rs:15`. Unchanged files require the documented forced reindex after updating the parser. |

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
