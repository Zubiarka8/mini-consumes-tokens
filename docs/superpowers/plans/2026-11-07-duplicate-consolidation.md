# Duplicate consolidation implementation plan

**Goal:** Resolve genuine repetition in the supplied 25 groups and classify the
remaining groups from their current implementation, independently of old reviews.

**Architecture:** Keep language-specific syntax and fixtures local. Share test
assertions in mct-corpus, fixture construction in server test support, and
grammar-neutral child traversal in mct-tree-sitter. Preserve public contracts.

**Acceptance:** Preserve fixture inputs, expected values, traversal order,
anonymous versus named children, depth increments and isolated temporary roots.
No dependency, schema or parser trait changes. Commits and PR publication were
authorized separately after implementation and classification.

## Steps

- [x] Establish existing parse, corpus and index integration test results.
- [x] Group 1: share inheritance relation assertions, keeping symbol checks local.
- [x] Groups 2 and 25: share child traversal, preserving named/all selection.
- [x] Groups 5 and 24: use derived Default for private walker initialization.
- [x] Group 4: share cache project construction with caller-supplied billing source.
- [x] Group 8: share migration setup and postconditions, preserving SQL/version/count.
- [x] Group 9: share single-language coverage assertions.
- [x] Group 12: share class/method/call assertions with explicit class names.
- [x] Group 15: share indexed symbol assertions with explicit expected metadata.
- [x] Group 16: share sweep measurement and threshold assertion, keeping query sets.
- [x] Group 17: share relation ownership range assertions.
- [x] Classify other groups as false-positive (different semantics) or ignored
  (intentional initialization/adapters with already centralized implementation).
- [x] Run affected tests, strict Clippy, formatting and diff checks; update reviews
  through the dashboard API using fresh revisions and record individual reasons.

## Review focus

Do not merge distinct migration expectations, drop anonymous AST tokens, weaken
assertions, combine cache fixtures' source contents, or inherit a resolution
across changed group membership. Inspect full current members before saving.

## Verification

Baseline: 424 passed, 15 existing ignored cases. After the extractions: 797
successful test executions across focused suites and full affected parser crates.
Strict all-target/all-feature Clippy, cargo fmt --check and git diff --check pass.
Restored three imports inside the optional semantic-audit model benchmark so the
all-feature check compiles; no model download or ignored benchmark was run.

Twelve supplied groups were refactored. Eight no longer appear. Four still match
by normalized setup/adapter shape after their logic moved to shared helpers;
record the resolution and ignore only that remaining intentional repetition.
Eight independent semantic cases are false positives; five existing public
adapter groups are intentionally ignored. Exact decisions and reasons are in
the dashboard review history and target/duplicate-review.md.

## Next supplied page: 25 groups numbered 15–48

- [x] Preserve detector fixtures and classify them as intentionally ignored.
- [x] Group 23: share variable-symbol assertions; retain Bash export and
  PowerShell scope-stripping inputs and failure messages.
- [x] Group 33: share default-inclusion path checks using existing exclusion
  test support; retain every Laravel and Claude lookalike path.
- [x] Review all other bodies and preserve independent mappings, grammar rules,
  fixture expectation tables, and trivial test constructors where extraction
  would add coupling or obscure differing contracts.
- [x] Verify changed suites and strict Clippy; save fresh states and individual
  evidence for all 25 supplied identities through the dashboard API.


Next-page verification: 38 baseline and 38 post-change tests pass, along with
strict all-target/all-feature scoped Clippy, formatter and diff checks. Groups
23 and 33 no longer appear. Nine groups are false positives and fourteen are
intentionally ignored with individual evidence; no detector fixtures changed.

## Publication verification

The complete pending cleanup and dashboard changes were reviewed and separated
into commits by responsibility before PR publication. On macOS with rustc 1.98.1:

- `scripts/unix/check.sh`: formatting passed; 2,139 workspace tests passed,
  zero failed and 30 existing cases ignored; strict all-target/all-feature
  Clippy passed; the evaluation gate passed with accuracy 1.000 and no regressions.
- Dashboard `npm test`: all 45 tests passed.
- `git diff --check`: passed.

No detector fixtures or dependency files changed. Review states and their history
remain in the local dashboard index, outside version control. Cross-platform CI
with the repository's pinned Rust toolchain still needs to run on the PR.
