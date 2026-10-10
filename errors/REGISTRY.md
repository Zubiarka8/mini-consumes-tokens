# Error registry

Last observation: 2026-10-10. Checkout HEAD: `7558113d1926a99beffdd265913bcd75f15f8e6e` (`docs/fumadocs-site`, with unrelated local changes).
Evidence: project MCP `get_indexing_status`; latest index timestamp `1791630208` (568 files, 11,236 symbols). The watcher refreshed the index after local changes; no clean forced reindex is claimed. The failure list below is unchanged from the initial snapshot at `1791629091`.

The snapshot reports **18 parse failures: 2 unexpected script failures and 16 intentionally malformed corpus fixtures**. Root causes of the two script failures are not yet established.

| ID | Status | Location / scope | Observed behavior and impact | Evidence / next action |
| --- | --- | --- | --- | --- |
| ERR-001 | OPEN | `scripts/unix/pr-body.sh:34` | `line 34: syntax error`; script missing from the source symbol index | Reproduced on 2026-10-10 with `scripts/unix/parse-probe.sh scripts/unix/pr-body.sh scripts/windows/pr-status.ps1`. `bash -n scripts/unix/pr-body.sh` passes. Investigate the parser/grammar rejection of the `case` line; runtime behavior has not been tested. |
| ERR-002 | OPEN | `scripts/windows/pr-status.ps1:14` | `line 14: syntax error`; script missing from the source symbol index | Reproduced on 2026-10-10 with the same parse-probe command. Failure points at the `gh pr view` line opening a multiline quoted jq expression. Native PowerShell syntax validation has not been run; determine whether input or grammar is responsible. |
| ERR-003 | EXPECTED | `crates/mct-lang-*/tests/corpus/malformed/` | 16 invalid fixtures are rejected, as required by the corpus contract | Index diagnostic and [corpus policy](../CONTRIBUTING.md#language-corpus-testscorpus). Preserve rejection; investigate any additional failures outside this fixture set. |
| ERR-004 | KNOWN_LIMITATION | CSS nesting | Nested selectors retain literal `&` instead of resolving against their parent | [Corpus progress](../internal/corpus-progress.md), issue [#115](https://github.com/Zubiarka8/mini-consumes-tokens/issues/115). Documentation evidence only; current issue state and reproduction not checked. |
| ERR-005 | KNOWN_LIMITATION | CSS / Tailwind grammar | Some valid at-rule preludes and nesting forms are documented as rejected by tree-sitter-css 0.25; affected files cannot be indexed | [Corpus progress](../internal/corpus-progress.md), issue [#116](https://github.com/Zubiarka8/mini-consumes-tokens/issues/116). Documentation evidence only; reproduce individual forms before diagnosing a current failure. |
| ERR-006 | KNOWN_LIMITATION | JavaScript/TypeScript / React JSX | Component logic is indexed, but JSX rendering relationships are not modeled | [Framework coverage](../CONTRIBUTING.md#frameworklibrary-coverage-beyond-the-language-table). Documentation evidence and indexed `tsx_component_logic_is_indexed_without_structuring_jsx` test symbol; test not run in this session. |
| ERR-007 | KNOWN_LIMITATION | Python / Django | Module-level assignments such as `urlpatterns` and model fields lack symbols | [Corpus progress](../internal/corpus-progress.md). Documentation evidence only; no dedicated reproduction run in this session. |
| ERR-008 | RESOLVED | MCP `get_tool_schema` invocation | Caller supplied `tool: "build_context_pack"`; tool returned ``failed to deserialize parameters: missing field `name` `` | First/last observed 2026-10-10. Retried with `name: "build_context_pack"` and received the schema successfully. Caller argument error; no server change needed. |
| ERR-009 | RESOLVED | MCP server `background_watcher` integration tests / execution environment | In the sandbox, three tests failed to observe file changes: reindex after edit, ignore-rule reload, and deletion/rename | First/last observed 2026-10-10 on the library-maintainability worktree based on `7558113`. The sandbox run stopped at 1,013 passed, 3 failed, 23 ignored. The same watcher test binary passed all 11 tests outside the sandbox with `cargo test --offline --locked -p mct-mcp-server --test background_watcher` (exit 0, 20.30 s), without code changes. Environment-dependent validation failure; use the authorized native rerun for filesystem-event tests. |

First and last observed dates for ERR-001 through ERR-003: 2026-10-10. ERR-004 through ERR-007 were first recorded here on 2026-10-10 from existing documentation; their original discovery dates are not established.

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
