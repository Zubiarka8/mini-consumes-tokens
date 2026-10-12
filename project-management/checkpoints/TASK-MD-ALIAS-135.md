# TASK-MD-ALIAS-135 checkpoint

**Task / previous agent / status:** TASK-MD-ALIAS-135, Claude worker; status
`blocked` on decisions D1-D3 (implementation candidate committed, not pushed).

**Branch / worktree name / base commit / latest commit:**
`claude/md-alias-135`, worktree `mini-consumes-tokens-claude-md-alias-135`,
base `a3e10b0f450a93107061152feb862a1d242820a1` (origin/main). Latest
commit: resolve with `git rev-parse claude/md-alias-135` (this checkpoint is
the last commit; the implementation ends at `test(md): cover wikilink
resolution by frontmatter alias`).

**Objective and acceptance criteria:** recover the uncommitted issue #135
work (wikilink resolution by frontmatter alias), verify it, fix the
`relation_scale` regression, and commit a candidate without resolving the
open product decisions. Issue #135 criteria: unique alias resolves; shared
alias is `ambiguous` with every candidate; title-only match stays
`unresolved`; migration does not backfill by name; full and incremental
reindex agree; `alias:<a>` asserted in
`front_matter_tags_aliases_and_title_are_queryable`.

**Completed:**
- Recovered diff applied cleanly at the base (no touched file changed
  between the old base and `a3e10b0`).
- `feat(core)`: `ParsedFile::symbol_aliases` / `SymbolAlias`.
- `feat(index)`: migration 11 (`symbol_aliases` table) and the recovered
  view, committed as found.
- `perf(index)`: view rewritten; restores main's ambiguous-anchor behavior.
- `feat(md)`: Markdown parser declares `aliases` entries; crate docs and the
  note-graph spec updated.
- `test(md)`: alias tests, D2/D3 pinning tests, macOS temp-dir canonicalize.

**In progress / not started:** nothing in progress. Not started: an ADR or
issue comment for D1 (requires a decision); push/PR (not authorized).

**Changed paths:** `crates/mct-core/src/lib.rs`,
`crates/mct-lang-rust/src/lib.rs`, `crates/mct-index/src/schema.rs`,
`crates/mct-index/src/indexer.rs`, `crates/mct-lang-md/src/lib.rs`,
`crates/mct-lang-md/tests/note_graph.rs`,
`crates/mct-lang-md/tests/index_integration.rs`,
`crates/mct-lang-md/tests/fixtures/vault/daily/2026-09-18.md`,
`docs/superpowers/specs/2026-10-01-markdown-note-graph-design.md`, this file.
No uncommitted changes.

**Checks executed (macOS, same machine):**
- `scripts/unix/check.sh`: fmt ok; CI clippy ok (no warnings); mct-eval ok
  (accuracy 1.000); test step FAILED on one test (below; the script stops at
  the first failing test binary).
- `cargo test --workspace --no-fail-fast`: 149 test binaries, 1232 passed,
  1 failed, 24 ignored. Only failure:
  `mct-index/tests/assets.rs::an_index_from_before_assets_gains_them_on_a_normal_reindex`
  (`user_version` 11, expected 10 per ADR-003 D7). Caused by migration 11;
  passes on the base. Blocked on D1; the test was not modified.
- Base `a3e10b0`, `cargo test -p mct-index -p mct-lang-md`: all pass.
- `relation_scale`: debug suite base 33.6 s, recovered 145.9 s, final 41.5 s.
  Release suite base 15.4-21.3 s, recovered 65.8 s, final 19.2 s.

**Known problems and relevant decisions:**

D1 - alias storage shape and migration (issue #135 open question).
- Candidate: migration 11 adds `symbol_aliases(symbol_id, alias)` with
  `ON DELETE CASCADE`, an alias index, and recreates `relation_candidates`;
  starts empty and clears `content_hash` once to force a reparse.
- ADR-003 D7 says "No schema migration in any phase"; the ADR is scoped to
  the web/3D plan, but `crates/mct-index/tests/assets.rs` asserts
  `user_version == 10` with "Bump only with a new ADR", so in practice it
  gates every migration.
- Options: (a) approve migration 11 with a new ADR (or ADR-003 amendment)
  and bump the assertion to 11; (b) no migration: emit an extra `module`
  symbol per alias in the same file (duplicate symbols in listings and
  search; the view still needs a kind/name rule); (c) no migration: resolve
  through the existing `alias:<a>` relations from the note (still requires
  recreating the view, which is itself a migration, or resolving in Rust
  outside the view).
- Impact: a migration changes the count, so older binaries hit
  `CONNECTION_CLOSED` on a migrated index (`scripts/unix/reinstall.sh`).
- Affected tests: `assets.rs` D7 assertion,
  `schema::tests::alias_migration_reparses_without_inventing_legacy_aliases`.

D2 - `[[note#Section]]` when `note` is ambiguous.
- The recovered view resolved it to the notes; that broke main's
  `an_anchor_on_an_ambiguous_note_stays_ambiguous_not_first_match`.
- Candidate: keep main's behavior (ambiguous, listing the same-named
  sections), for basenames and shared aliases alike.
- Options: (a) sections (candidate, no test change); (b) notes (requires
  changing main's test and restoring a scope view, with a cost to check);
  (c) unresolved.
- Affected tests: `an_anchor_on_an_ambiguous_note_stays_ambiguous_not_first_match`,
  `an_anchor_through_a_shared_alias_lists_both_sections`.

D3 - alias comparison case sensitivity.
- Candidate: exact, case-sensitive (issue lists case folding out of scope).
- Options: (a) keep; (b) case-fold aliases only (inconsistent with stems and
  headings); (c) fold everywhere together with the slug decision.
- Affected test: `alias_matching_is_case_sensitive`.

**Dependencies / outstanding references:** none running. Scratch base
worktree (detached at `a3e10b0`) exists under the worker's scratchpad;
remove with `git worktree remove` when no longer needed.

**Files or symbols to inspect first:** migration 11 in
`crates/mct-index/src/schema.rs`; `write_parsed_file` in
`crates/mct-index/src/indexer.rs`; `push_frontmatter_relations` in
`crates/mct-lang-md/src/lib.rs`.

**Exact next action and resume condition:** the coordinator or user decides
D1-D3. If D1 = (a): write the ADR, then update the `assets.rs` assertion in
the same change and rerun `scripts/unix/check.sh`. Then review and open a PR.
