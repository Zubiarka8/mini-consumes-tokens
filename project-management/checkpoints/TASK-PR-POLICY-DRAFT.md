# TASK-PR-POLICY-DRAFT checkpoint

- Branch: `docs/pr-integration-policy-draft`; base `a3e10b0f450a93107061152feb862a1d242820a1` (origin/main).
- Status: REVIEW (draft). Nothing pushed, no PR, no merge.
- Committed policy text: one paragraph in `PROJECT_MANAGER.md` "Integration and completion", limited to documented scripts and enforced branch protection. Everything else below is undecided and NOT committed.

## Verified facts (2026-10-12, `gh api`)

| Fact | Source | Result |
| --- | --- | --- |
| Required status checks | `branches/main/protection` | `rustfmt`, `build + test + clippy (ubuntu-latest\|macos-latest\|windows-latest)`; `strict=false` |
| Required reviews | same | `required_pull_request_reviews` is `null`: NO review is required. The earlier claim "one approving review required" is false |
| Enforce for admins | same | `false` |
| Force-push / deletion of main | same | both disallowed |
| Linear history | same | not required |
| Rulesets | `repos/.../rulesets` | none (0) |
| Merge methods | `repos/...` | merge commit, squash, rebase and auto-merge all allowed; `delete_branch_on_merge=true` |
| Practice | coordinator count (not re-run here) | 48 of last 49 merged PRs are merge commits; 1 squash (#146); no direct non-merge commit on main since PR #1 |
| `publish-branch.sh` | script header and `scripts/README.md` | pushes branch and opens PR against `main`; fast-forward only; refuses when it would need a force or the branch has nothing new; only pushes if a PR is already open |
| Bless command | `crates/mct-corpus/src/lib.rs` (`BLESS_ENV`), `scripts/unix/corpus-report.sh` | `MCT_BLESS=1 cargo test -p <crate> --test corpus`; wrapper `scripts/unix/corpus-report.sh <lang> --bless [--update-progress]`. CONTRIBUTING.md review checklist already names `corpus-report.sh <lang> --bless --update-progress` |

Already documented elsewhere (link, do not repeat): check.sh / pr-body.sh / pr-status.sh / pr-conflicts.sh in `scripts/README.md` and `rules.md`; CI jobs in `CONTRIBUTING.md`; "open an issue first" for LanguageParser / schema / MCP signature in `CONTRIBUTING.md` and `rules.md`; corpus review checklist in `CONTRIBUTING.md`; lead owns integration, pushes and PRs in `rules.md`.

## Draft section (proposal only; [DECIDE] marks an undecided rule)

> ### Integrating into `main`
> - Open PRs with `scripts/unix/publish-branch.sh` after `scripts/unix/check.sh` (committed).
> - Branch protection requires the four-check set above (committed, descriptive).
> - Run `scripts/unix/pr-conflicts.sh` before opening when other PRs are open; rebase or merge `origin/main` before opening; do not open umbrella PRs that repeat commits of other PRs. [DECIDE-7: rebase vs merge of main into the branch; the existing wording in `scripts/README.md` speaks of merging and a normal fast-forward push]
> - Merge method: [DECIDE-1].
> - Auto-merge: [DECIDE-2].
> - Docs-only changes: [DECIDE-3].
> - Who merges / reviews: [DECIDE-4].
> - Policy location: [DECIDE-5].
> - `feat`/`fix`/`perf` PRs under `crates/` need a test that fails without the change: [DECIDE-6]. (Only the corpus case, "small parser bugs ... with a test that fails before the fix", is already in CONTRIBUTING.md.)
> - `internal/corpus-progress.md`: [DECIDE-8].

## Decision list

### DECIDE-1 Merge commit vs squash vs rebase
- Evidence: all three allowed; 48/49 recent merges are merge commits; `delete_branch_on_merge=true`; no linear-history rule; rebase/squash would rewrite the branch commits that `rules.md` and PM docs treat as identified revisions.
- Options: (a) merge commit; (b) squash; (c) allow either, per PR.
- Recommendation: (a) as the documented default, because it matches practice and preserves reviewed commit hashes; allow exceptions with a stated reason.
- Consequence: (a) keeps history noisy but traceable; (b) loses per-commit hashes and conflicts with checkpoint records; documenting a default does not change repository settings (disabling methods is a separate admin action).

### DECIDE-2 Auto-merge
- Evidence: `allow_auto_merge=true`; required checks make it safe from red merges; with `strict=false` and `enforce_admins=false` a stale branch can still merge.
- Options: allow; allow only after explicit approval; forbid.
- Recommendation: do not state it as policy yet. If adopted, tie it to explicit user authorization per PR (PROJECT_MANAGER.md step 5 already requires authorization to merge).
- Consequence: auto-merge executes the merge without a human present, so approval must precede enabling it.

### DECIDE-3 Direct docs push to main
- Evidence: no direct non-merge commit on main since PR #1; `enforce_admins=false` means an admin could push, but no PR-required rule exists to forbid or allow it formally. Old proposal `80c638c` recommended it; not adopted.
- Options: all changes via PR; docs-only exception for the owner.
- Recommendation: all changes via PR (matches 100% observed practice).
- Consequence: an exception needs a precise definition of "docs" (paths) and a record of who may use it.

### DECIDE-4 Who may merge / required reviews
- Evidence: no required reviews in protection; `rules.md` says the lead owns integration, pushes and PRs unless the user delegates; PM step 5 requires user authorization.
- Options: keep as is (convention only); enable "1 required approving review" in branch protection; owner-only merge.
- Recommendation: keep the existing convention; do not write "one approving review required" anywhere. Enabling the setting is a repository admin decision for the user, and a solo-owner repo cannot self-approve.
- Consequence: enabling required reviews would block the owner's own PRs unless admins bypass (`enforce_admins=false` currently allows it).

### DECIDE-5 Policy location
- Options: PROJECT_MANAGER.md "Integration and completion" (agent workflow, where the committed paragraph is); CONTRIBUTING.md (human contributors, already holds CI and corpus checklists); `project-management/DECISIONS.md` as a PM-DEC entry recording the choices; `rules.md` (no: it must not carry process policy).
- Recommendation: contributor-facing rules (merge method, PR hygiene, tests) in CONTRIBUTING.md; agent-facing integration in PROJECT_MANAGER.md; record each accepted decision as a PM-DEC in DECISIONS.md. CONTRIBUTING.md was outside this task's write scope, so it is not edited.
- Consequence: two homes risk duplication; each rule should live in exactly one and be linked from the other.

### DECIDE-6 Failing-test requirement for every `crates/` feat/fix/perf PR
- Evidence: not enforced by CI; only stated for corpus parser fixes. `perf` PRs and refactor-like feats often cannot have a test that fails before.
- Options: mandatory; required with a written waiver; recommended only.
- Recommendation: do not make it mandatory policy. If adopted, "for `fix` PRs, include a regression test that fails without the change, or state why not in the PR body".
- Consequence: mandatory wording will be violated by legitimate PRs and weaken trust in the other rules.

### DECIDE-7 Updating the branch with main
- Evidence: `scripts/README.md` conflict guidance says preserve both sides, regenerate snapshots from combined code, run `check.sh` before a normal fast-forward push; PM says never rewrite another contributor's published branch; `strict=false`.
- Options: merge main into branch; rebase own unpublished branch; either.
- Recommendation: leave to the existing PM rule (own unpublished branch may be rebased; published branches are not rewritten).
- Consequence: low; mostly already covered, so no new text needed.

### DECIDE-8 `internal/corpus-progress.md` ownership and bless line
- Evidence: the file has no ownership statement; CONTRIBUTING.md already tells corpus PR authors to run `corpus-report.sh <lang> --bless --update-progress`, so authors currently edit their own row. `repo_ledgers.rs` tests table consistency in CI and `corpus-progress-check.sh` checks it against GitHub. The earlier "only the coordinator edits it" is not current practice or documented.
- Options: each corpus PR edits its own row (status quo); coordinator-only; coordinator-only for the summary line.
- Recommendation: keep status quo; add at most one line in CONTRIBUTING.md: on conflicts in `expected.snap` or the progress table, do not hand-merge, rerun `scripts/unix/corpus-report.sh <lang> --bless --update-progress` on the combined code (equivalent to `MCT_BLESS=1 cargo test -p <crate> --test corpus`).
- Consequence: coordinator-only ownership would serialize every corpus PR through one writer and contradict the existing checklist.

## Home for the committed text
`PROJECT_MANAGER.md` was chosen because it is in scope and already owns "Integration and completion". CONTRIBUTING.md is likely the better home for contributor-facing rules (DECIDE-5) but was not edited.
