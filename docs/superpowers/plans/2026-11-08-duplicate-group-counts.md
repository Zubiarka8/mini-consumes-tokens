# Duplicate group counts and presentation implementation plan

**Goal:** Distinguish member locations from unique files and make every preview omission explicit.

**Architecture:** Preserve detector membership, review identities, ranking and scope enum values. Extend dashboard rows additively; keep list metadata bounded and retrieve complete members through the existing detail endpoint. Use the existing DOM and translation tables.

**Constraints:** Work only in this worktree, branch from main, no commits/pushes/PR publication. Repository content in English; Spanish and English UI literals are required exceptions. No Chrome automation.

- [x] Reproduce groups #1, #3 and #6 against a real localhost API; inspect source through explicit-root mct queries.
- [x] Add failing API tests: same-file counts, all names through six members, explicit omissions beyond six, complete detail and stable identity on reindex.
- [x] Add failing UI tests: pluralized locations/files, visible scope, preview notice and complete copy template.
- [x] Extend duplicate_row without changing grouping or review keys. Remove frontend count reconstruction; use backend counts.
- [x] Check nested whole-function groups with generated production-parser examples; assert nonoverlapping occurrences within each group and deterministic reindexing.
- [x] Hunt variants, inspect path containment and response bounds, measure duplicate_row on synthetic large groups, review full diff and document the API.
- [x] Run requested fmt, Clippy, Rust tests and dashboard npm tests; build and relaunch dashboard; inspect using a non-Chrome surface if available.
- [x] Save evidence, differential review and an English PR draft; report any remaining limits.

**Review focus:** More than six members, repeated names and paths, same-line disjoint functions, stored decisions/numbers, root-level scope semantics. No new test dependency is needed: generated bounded-domain cases exercise invariants.


**User steering:** Render all list identifiers as English words; retain original
identifiers in metadata, location panels and AI exports. Replace repeated raw
names with one visual label while preserving every occurrence and its counts.
Use smaller wrapping labels and a secondary metadata line. Extend the same
server-count rule to the React comparison status bar; regenerate its committed
bundle with the existing build.
