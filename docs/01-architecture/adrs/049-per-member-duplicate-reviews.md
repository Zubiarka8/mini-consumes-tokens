# ADR-049: Per-member review of duplicate groups

## Status
Proposed (2026-11-21). Issue: [#263](https://github.com/Zubiarka8/mini-consumes-tokens/issues/263). Extends [ADR-019](019-shared-review-decisions.md).

The maintainer answered the issue's open questions on 2026-11-21:
- The group shows the state of its most pending member, plus a count.
- A group note stays next to the per-file notes.
- A membership change keeps the decisions of surviving members for re-evaluation, and a new member starts pending.

## Decision
- **Kind:** `duplicate-member` is a new finding kind.
  - **Identity:** `duplicate-member-id-v1` hashes language, path, symbol name, symbol kind, and the ordinal among same-named members of that file. It names the member alone, so it survives any change of the group.
  - **Fingerprint:** `duplicate-member-fp-v1` adds the group's fingerprint. Any change to the group's code or members shows the member's decision for re-evaluation instead of applying it.
  - Letters (A, B, …) stay a display concern.
- **Inheritance, not conversion:** a member's candidates are its own rows plus its group's candidates from every source (`C3`, `B2`, `A2`, `A1`). The usual order picks the winner, latest first.
  - A group decision saved later applies to every member; this is the "apply to all" action.
  - A member decision saved later overrides it for that member only.
  - Decisions saved before this change therefore appear on every member, with their history. No file is rewritten.
  - A member that disagrees with its group is not a conflict; only sources disagreeing at the winner's own level are.
- **Format:** `review-decisions.toon` is written as version 4 only once it holds a `duplicate-member` row or event.
  - A store with only group decisions stays version 3, which older binaries keep reading.
  - Version 3 still rejects member rows.
  - An older binary reports a version 4 file as written by a newer version: it hides nothing and writes nothing.
- **Summary:** `summarize(members)` gives the label of the most pending member, in this order: `needs-reevaluation`, `unreviewed`, `in-review`, `resolved-but-detected`, `confirmed`, `dismissed`, `false-positive`. It also gives how many members have a final decision, and `Suppressed` only when every member is suppressed.
- **Consumers:** `find_duplicate_code` and `mct duplicates --reviewed` hide a group only when every member is hidden, and show the summary label.
  - `find_duplicate_code` adds `reviewed_members` to the group row and a per-member `review` column.

## Not done here
- The dashboard's file selector and per-member saves. Until a surface writes member rows, every member takes its group's decision, so every surface still agrees.

## Verification
- `reviews::member_tests` (8 tests):
  - keys;
  - inheritance with history;
  - one member decided alone;
  - latest-wins between group and member;
  - a membership change;
  - the summary and hiding rule;
  - version 3/4/5 handling;
  - an `A1` group decision on every member.
- `mct-mcp-server/tests/duplicate_code.rs` › `a_group_is_hidden_only_when_every_member_is`. It failed before the change.
