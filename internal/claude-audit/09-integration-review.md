# Reviewer agent — first-batch integration

Follow the shared rules and the model/effort selection in `Claude-Plan.md`. Use your own branch: `codex/audit-integration-review`. Review and validate the local integration of the three deliveries, prepare necessary fixes, and make an evidence-backed decision about proceeding with dependent tasks.

## Context

Audited base: `1880048dd8274b0b3a7f82b07bedd3219bde5ca1`. The coordinator has combined these commits on your branch while preserving their history:

- Exclusions: `52aa751e200bc27f34853433d6b156882eeb0bc6`.
- Lua: `39dd255084ccd3df728f993e7e9760cc9913aef6`.
- Rust dead code: `331218ea563e4f154a04298a63e252514a75dd59`.

Their individual tests passed, but there has been no independent review or combined result. Do not present those results as integration validation. PR #90 was a dependency of the original base; this branch is local and does not authorize publishing.

## Work

1. Check the branch, clean status, and presence of all three commits. Review the diff against the base and use this worktree's MCP for code queries. The installed MCP binary may have an older implementation: use it to explore sources, but run behavioral reproductions against your compiled binaries.
2. Look for concrete defects, regressions, contract changes, tests that do not prove the behavior, and scope expansion. Every finding must include priority, file/line, trigger, and effect. Do not report cosmetic or speculative observations.
3. Review especially:
   - **Exclusions:** concurrent rule changes, consistency between watcher and an in-progress reindex, un-exclusion, `.gitignore` import, fixed-set semantics, failed reads, and recovery. Determine whether the race acknowledged by the author can leave the index permanently incorrect.
   - **Lua:** actual registries, development dependencies and their effect on `cargo publish`, executable coverage, and correct fixture/count updates. Verify a real regression against the previous behavior if the new test was not run there.
   - **Rust:** scope/shadowing, references in macros/values, test attributes, nested/turbofish calls, and effects of synthetic references on other tools. A same-name test must not turn a known global limitation into a safety claim. Do not expand the task to solve all of F02.
   - **Integration:** consistent corpus/evaluation changes; no unrelated changes, secrets, or personal configuration.
4. Fix clear bugs within these three tasks in local commits separate from the original commits, with meaningful regressions. Keep the three original branches intact. For schema, `LanguageParser`, or MCP signature changes, prepare the required issue and report the blocker before implementing that part.
5. Run appropriate combined verification: formatting and `scripts/unix/check.sh`. If the local linker workaround observed by the authors is needed, set `SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk` for that command only; do not modify global configuration. Do not rerun the full suite after a text-only change. Do not install global binaries, push, or open/merge a PR.

## Delivery

Write `internal/audit-integration-review.md` in your branch with the base/final SHAs, reviewed commits, resolved/pending findings, corrective changes, actual checks, and limitations. End with an explicit decision: `READY FOR NEXT BATCH` or `BLOCKED`, explaining the conditions. If an out-of-scope risk remains, say whether it blocks this integration or belongs to a later task. Explicitly stage and commit your report and fixes.

Also provide an English summary with the SHA that later agents can use as their base. Do not start other agents or tasks on your own. Automatic context compaction is fine if it preserves branch/base, scope, findings, completed commands, and open items; do not use `/clear` during this task.
