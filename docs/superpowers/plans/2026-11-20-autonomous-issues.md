# Autonomous issues implementation plan

**Goal:** Complete the independently actionable open issues using the existing project architecture.

**Architecture:** Keep each change within its owning crate and preserve review identity, index evidence, and existing user files. Deliver and verify each independently testable slice before proceeding.

**Tech stack:** Rust workspace, SQLite, TypeScript, JavaScript, Node, Cargo.

**Specifications:** GitHub issues #18, #24, #188, #260, #263, #374, #395, #560, #563 and #567.

## Constraints

- Do not commit, push, modify GitHub issues, or overwrite another worktree.
- Preserve unrelated local changes and agent configuration.
- Do not add speculative resolution edges or expose secrets.
- Record an issue number and move on if one error blocks progress for 20 minutes.

## Tasks

1. #188: bound reference lifetime, report recoveries, include useful results, and verify cache behavior and token targets. Investigate client and graph-cache limits separately.
2. #260: preserve drafts across refreshed revisions, include saved decisions in every AI brief, compare #242 regressions, and run focused UI tests.
3. #560: reuse the existing skill templates, implement safe registration and detection, verify current vendor paths, then integrate per-agent MCP configuration and prompts.
4. #567: compare remaining resolution cases with current code; add supported cases and regression tests without guessing the web root.
5. #374 and #563: map literal project configuration to conservative module evidence. Implement and test one ecosystem at a time, including stale configuration and ambiguous or missing targets.
6. #263: design stable per-member identity and review migration, then implement storage, dashboard, filters, MCP agreement, and migration tests.
7. #24: run reproducible token measurements on pinned public repositories with a real tokenizer and document commands and results.
8. #18: establish a repeatable tool-selection baseline, improve descriptions, and measure against the stated target.
9. #395: inspect the existing implementation and avoid duplicating the active PR.

## Review focus

- Conflicting review writes must not overwrite a newer decision.
- Edited skill files and unrelated configuration must survive registration.
- Alias matches must stay unresolved when evidence is ambiguous or external.
- Index changes must not leave stale results after configuration edits.
- Benchmarks must distinguish missing answers from token savings.

### #260: reference-branch comparison

The backup PR #242 tested same-name dead-code siblings, conflicting decisions,
review visibility in MCP/CLI, and changed duplicate membership against its old
SQLite store. Current coverage includes `a_homonym_added_above_never_inherits_the_decision_of_its_twin`
(`crates/mct-index/src/reviews/tests.rs`), scoped relation identity and review
compatibility tests, plus HTTP revision checks. The draft-revision regression
added here covers an unsaved note when newer props arrive before the save:
the first save conflicts and retains the draft; the reviewer can then rebase
and save. AI-copy tests cover the saved state, prior state and note. The old
branch's SQLite implementation is not copied into the TOON store.
