# Report: docs-changelog

## Researched
- `git log v0.2.0..HEAD` (27 commits): the release script, `mct-cli update`, the web/3D foundation (all already in `[Unreleased]`) and the `website/` series (not recorded).
- `RELEASING.md` against `rules.md` "Release version files" and `scripts/unix/release.sh`.
- Every `crates/`, `benchmarks/`, `internal/`, `scripts/` path in `benchmarks/*.md`, checked for existence.

## Findings
| File | Claim | Evidence | Action |
|---|---|---|---|
| CHANGELOG.md | Unreleased lists release.sh, `mct-cli update`, web/3D | commits 1eb1358, 034ea97, 7b0b17c | none |
| CHANGELOG.md | `website/` (landing + docs site) not recorded | f22dac4 and 20+ later website commits; `website/README.md` | added an Added entry |
| RELEASING.md | Pre-flight mentions only the README badge | rules.md lists 6 version spots; `release.sh bump` only edits Cargo.toml, README badge, lockfiles | added manual spots (README sentence, `--version` example, `site/index.html` eyebrow, `git grep`) |
| RELEASING.md | `release.sh` steps, 5 archives, dated CHANGELOG check | match `release.sh` (`changelog_ok`, 5 assets, tag from origin/main) | none |
| benchmarks/agent-benchmark.md | `main.rs:731`/`:732` miss | true at pinned commit c06ab5c; at HEAD those lines are different code | clarified that the line numbers are as of c06ab5c |
| benchmarks/*.md | referenced files and commands | all paths exist; `c06ab5c` exists | none |
| site/index.html (out of scope) | eyebrow says `v0.1.0`, workspace is 0.2.0 | `site/index.html:88` | flagged only |

## Changes
- CHANGELOG.md: one Added entry for `website/`.
- RELEASING.md: one pre-flight item expanded.
- benchmarks/agent-benchmark.md: one clarification.

## Unverified
- All numeric figures in `benchmarks/*.md` (character counts, token costs, latencies, accuracy): not re-run.
- Whether `cargo run -p mct-cli --example token_benchmark` and the other cargo commands still produce the documented numbers: not executed.
- Website description in the CHANGELOG is based on `website/README.md` and commit subjects, not on a build.

## Follow-ups
- Update the `site/index.html` eyebrow (stale at v0.1.0); owner decision, outside this scope.
- Consider extending `release.sh bump` to cover the manual version spots.
