# Project State

Last update: 2026-10-10
Updated by: Codex coordinator (bounded records reconciliation)
Main branch: `main`
Observed main commit: `eee98e9ed0f1467dac32fe172cb90eb846628f83` (GitHub and local refs match)
Scope: reconciliation of these records with merged PRs and open issues; not a full backlog audit.

## Current objective

Reconcile website integration and ERR-011, inventory local worktrees, and prepare documentation-only changes for review. Stop before push, PR creation or cleanup.

## Merged since the last update

PR #96 (`adbc43a`) shared coordination rules; #157 (`7558113`) language-crate base and fuzzing; #170 (`a94d101`) one language registry, grammar version policy, Windows script CI and the ERR-010 fix. Also merged: the issue #74 corpora (#130), shared AST helpers (#131), index read-failure and pagination fixes (#134, #138, #141, #142), semantic memory work (#137, #139), conflict automation (#156), Markdown exact-phrase literals (#162), intent extraction (#160), the 0.2.0 preparation (#163), `mct-cli update` (#164) and the web/3D indexing foundation (#166). PRs #167 and #168 were closed as duplicates carried by #170.

## Active work

- [TASK-WEB-169](TASKS.md#task-web-169) is DONE: #169/#171/#173/#174 are merged and Docs build/deploy passed. See the task for the accepted four-page manual update and verification limits.
- ERR-011 is RESOLVED after schema inspection, controlled reproduction and successful corrected retry; see [error history](../errors/HISTORY.md).
- Worktree audit: 85 entries, 68 clean, 15 dirty, two prunable/unusable, seven detached; 57 HEADs are ancestors of main. Integration alone does not establish safe deletion; ownership and ignored data remain unverified.

## Additional integration observation

- The JSX candidate is already reachable from current main via PR #176 (`c8fb4b1`); do not implement it again. This observation does not close the wider #51 framework acceptance or assess Vue support.

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

Review the prepared documentation diff and proposed PR summary. Publication requires separate authorization in this task. Preserve every worktree; resume functional work only under a new scoped assignment.
