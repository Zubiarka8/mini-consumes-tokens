# Project State

Last update: 2026-10-10
Updated by: Claude Code session (continuing the Codex coordinator)
Main branch: `main`
Observed main commit: `a94d10129d95c43d8cd56d80ba1656517a354beb` (PR #170)
Scope: reconciliation of these records with merged PRs and open issues; not a full backlog audit.

## Current objective

Integrate the reconciled website (PR #169), then recover the partial local work listed below without repeating what is already merged.

## Merged since the last update

PR #96 (`adbc43a`) shared coordination rules; #157 (`7558113`) language-crate base and fuzzing; #170 (`a94d101`) one language registry, grammar version policy, Windows script CI and the ERR-010 fix. Also merged: the issue #74 corpora (#130), shared AST helpers (#131), index read-failure and pagination fixes (#134, #138, #141, #142), semantic memory work (#137, #139), conflict automation (#156), Markdown exact-phrase literals (#162), intent extraction (#160), the 0.2.0 preparation (#163), `mct-cli update` (#164) and the web/3D indexing foundation (#166). PRs #167 and #168 were closed as duplicates carried by #170.

## Active work

- [TASK-WEB-169](TASKS.md#task-web-169) is in REVIEW: PR #169 updated with the reconciled landing and a merge of `main`.

## Completed but not integrated

- JSX component references for #51 on `codex/framework-followup-51` (`2d3def5`): focused checks passed, not reviewed or published. #51 stays open; Vue SFC support is not started.

## Partial, uncommitted work (not validated)

- #135 Markdown wikilinks by frontmatter alias: schema migration 11 and alias table drafted; ambiguous-identity edge case not fixed.
- #11 tool selection and #17 tool contracts: drafted in the MCP server, not compiled against the locked `rmcp` version, no tests.
- #115 CSS nesting: test file only.

## Blocked

None. #24 benchmarks and #22 sandbox audit are investigation only; no paid benchmark model runs are authorized.

## Known risks

- Several stale local worktrees exist; their branches are not all merged or abandoned. Check Git before reuse or cleanup.
- #133 remains open although #134 merged; compare its acceptance criteria before closing.
- Updates to the seven shared records require one coordinator writer; this is an operational convention, not an enforced lock.

## Next recommended action

Merge PR #169 once CI is green and the maintainer accepts it. Then review and publish the JSX change, and resume #135 and #11/#17 from their uncommitted diffs.
