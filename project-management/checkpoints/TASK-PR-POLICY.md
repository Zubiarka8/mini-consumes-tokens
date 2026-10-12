# TASK-PR-POLICY checkpoint

- Branch: `docs/pr-integration-policy-draft`; base `a3e10b0f450a93107061152feb862a1d242820a1` (origin/main).
- Status: REVIEW. Nothing pushed, no PR, no merge.
- Policy home: `PROJECT_MANAGER.md`, "Integration and completion" > "Pull requests and `main`". It owns integration; `rules.md` only points to it and `CONTRIBUTING.md` holds contributor/CI/corpus detail.
- Superseded: `TASK-PR-POLICY-DRAFT.md` (undecided options) is removed; its decisions are now settled.

## Approved decisions (user)
Merge commit by default, no squash by default; no auto-merge; every change including docs via PR, no direct push to `main`; review before integration is project policy; no universal fail-first test rule, but regression coverage appropriate to the change.

## GitHub facts observed 2026-10-12 (`gh api`, read-only)
- `branches/main/protection`: required checks `rustfmt`, `build + test + clippy (ubuntu-latest|macos-latest|windows-latest)`, `strict=false`; no `required_pull_request_reviews` key (no approval required); `enforce_admins=false`; `required_linear_history=false`; `allow_force_pushes=false`; `allow_deletions=false`; `required_conversation_resolution=false`; `required_signatures=false`.
- `rulesets`: `[]`.
- Repo: `allow_merge_commit`, `allow_squash_merge`, `allow_rebase_merge`, `allow_auto_merge`, `delete_branch_on_merge` all true; `allow_update_branch=false`.
- History: last 100 merged PRs, 98 merge commits, 2 single-parent (#146 squash; `9a90982`); 0 of 100 have a GitHub review recorded. Latest non-merge commit on first-parent `origin/main` is #146 (2026-10-08); earlier ones date from 2026-09-17.

## Evaluated, no new rule (already owned elsewhere or no source)
- Rebase before merge: `PROJECT_MANAGER.md` (branch isolation, step 3) and `scripts/README.md` conflict guidance already cover it.
- Umbrella PRs: no source defines a rule; `scripts/README.md` only says to close a redundant PR.
- `internal/corpus-progress.md`: the file itself says each corpus PR updates its own row; no coordinator-only rule exists.
- `MCT_BLESS=1`: documented in `crates/mct-corpus/src/lib.rs`, `scripts/unix/corpus-report.sh`, `CONTRIBUTING.md` checklist.

## Open for the user
Enabling required PR/review or disabling squash/rebase/auto-merge is a GitHub admin action not performed here. The policy relies on convention until then.
