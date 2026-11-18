# ADR-038: Incremental CI planner, final gate and recovery path

## Status
Proposed (2026-11-17). The planner runs in observation mode only. Selection is not enforced, and no required check changes without the maintainer's approval. Details and evidence are in [ci-cd.md](../ci-cd.md).

## Context
- On 2026-11-17, the last 100 `CI` runs show 46 PR runs and 12 push runs to `main` as `failure`, plus 22 PR runs `cancelled`. Two causes explain most of that:
  1. `cargo-audit` failed in 79 of 100 runs on RUSTSEC-2026-0544 (`libz-sys` 1.1.31, fixed in 1.1.33). The advisory has no CVSS score, so the `severity_threshold = "high"` filter keeps it and the gate fails. This is a fail-safe working as designed.
  2. Every push to `main` queued 18 macOS fuzz jobs on a pool of 5 macOS runners. The median queue was 45 min and the longest 138 min, and 15 jobs were never given a runner. The required `build + test + clippy (macos-latest)` of pull requests waited behind them, with a median queue of 11.9 min and a maximum of 95 min. Of its 24 cancellations, 17 came from a newer push to the same PR during that wait, and 2 jobs never got a runner.
- Each pull request runs the full workspace on three OSes, whatever it changes. The sampled merges #520 and #508 each change one `mct-lang-*` crate, which reaches 6 of 26 workspace crates.
- Branch protection requires `rustfmt` and the three `build + test + clippy` checks by name. A required check that is skipped counts as passing on GitHub.

## Decision
- **Planner in `scripts/ci/`.** Standard-library Python with no project build. It runs `git diff` against the merge base, builds the crate graph with `cargo metadata --no-deps --locked`, takes the reverse transitive closure and classifies risk R0 to R4 from `ci_policy.toml`, then emits a GitHub matrix. An unknown path, a missing base commit or a failed `cargo metadata` selects the full suite.
- **Observation first.** The `plan` job writes its decision to the run summary. `build-test` keeps running everything until enough plans match full runs (see "Activation" in ci-cd.md).
- **One final gate.** The `ci-gate` job runs with `if: always()` and fails unless every job in `[gate].required` ended in `success`. A required job that was skipped, cancelled or missing blocks the gate. Once it has a track record, the maintainer can make `CI gate` the single required check.
- **Fuzzing out of the push path.** `ci-nightly.yml` runs Linux fuzzing on each push to `main` and every night, and macOS fuzzing only nightly or on demand, with `max-parallel: 4`.
- **Recovery without a circular dependency.** A push to `recovery/**` runs the whole `ci.yml` on that commit, through `workflow_call`. Once the workflow is on `main`, a manual `workflow_dispatch` classifies a failed run and reruns only the infrastructure failures, within a bounded number of attempts. It never pushes, tags or edits checks.

## Consequences
- A change to `scripts/ci/`, `.github/` or `Cargo.lock` is R4 and always runs the full suite, so a policy edit is tested by the CI it changes.
- The `ci-scripts` job tests the planner and runs a checksum-pinned `actionlint`.
- Rollback: revert the commit. Branch protection is unchanged, so the old required checks keep working.
