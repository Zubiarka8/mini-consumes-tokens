# ADR-031: Vault publication note by note under an OS lock

## Status
Proposed (2026-11-08). Second block of the vault and web performance plan (branch `perf/vault-publication`). Supersedes the publication and writer-lock parts of [ADR-013](013-obsidian-vault-location.md); vault location, ownership and the manifest format are unchanged.

## Context
ADR-013 published the vault by copying the whole next tree into a staging directory and swapping it in with two renames (`out -> previous`, `next -> out`), under a lock file created with `create_new` and removed by `Drop`. Measured on `main` 589d1cc:
- Readers saw notes disappear between the two renames: 1,102 missing reads out of 1,145 per 20 publications (300-file fixture).
- The lock file outlived a stopped writer: after SIGINT of `web` (10/10) and SIGKILL (1/1), every later publication failed until a person deleted it.
- A second writer failed instead of waiting: 19/20 rounds of a concurrent `reindex` and `obsidian` ended in `File exists (os error 17)`.
- `web` published twice per change: once from the watcher's post-reindex hook, once from the snapshot reload (4 lock warnings in 6 changes).
- One changed source file rewrote all 20,205 files of a 20,204-note vault.

## Decision
- **Lock.** Writers take an OS advisory lock (`File::try_lock`) on `<vault>.writer.lock`, in this process or another, and poll for up to 120 s before failing with a `Locked` error. The OS releases the lock when the process exits, however it exits. The lock file stays on disk.
- **Older versions.** While it publishes, a writer also creates the legacy `<vault>.lock` exclusively, with a marker text, and removes it when done, so older binaries keep failing closed. A legacy file holding the marker was left by a stopped writer of this version and is replaced. Any other legacy file (older versions leave it empty) fails with a message saying to delete it if no mct process is running; it is never removed on age alone.
- **Note by note.** Only notes whose file does not already hold the new text are written, each to a temporary file in its own directory (`.mct-obsidian-*.tmp`) renamed over the note. Removed notes are deleted. The manifest is written the same way, last. Unchanged notes and unmanaged files are not touched.
- **Recovery.** After an interruption the manifest still describes the previous notes. A note whose file already holds its new text is accepted as written by that interrupted publication, not as a user edit. Leftover temporary files are deleted by the next publication.
- **One publisher per change in `web`.** A snapshot reload no longer publishes: the process that reindexed published the vault from its own post-reindex hook. The first snapshot still publishes.

## Consequences
- Measured after the change: 0 missing reads in 5 runs of 20 publications; 0 of 100 concurrent `reindex`/`obsidian` pairs failed; SIGINT of `web` while publishing blocked 0 of 10 later publications; one changed source file rewrote 4 files (3 notes and the manifest) instead of 20,205, and the one-change publication took 6,921 ms p50 instead of 12,066 ms (release, 20,204 notes). The remaining time is building the notes, addressed separately (`direct_file_imports`).
- Every note is old or new at any instant, never missing. During a publication the tree may mix old and new notes; the manifest switches last.
- Stopped writers no longer block later ones, and concurrent writers wait instead of failing.
- Work is proportional to the changed notes, plus reading every managed note to verify it was not edited (unchanged from ADR-013).
- Personal files are no longer copied, so their identity and timestamps are kept.
- A user file named `.mct-obsidian-*.tmp` inside the vault is treated as a leftover and deleted.
- Windows: renaming over a file another process holds open without delete sharing fails; the publication stops with the manifest unchanged and the next one finishes it.
- An older binary that is publishing when this version starts is not detected unless it has created its lock file; mixed versions can then publish at the same time. Restart old MCP servers after upgrading.
- No new runtime dependency or schema change. Rollback: revert; the manifest and paths are unchanged.

## Alternatives rejected
- Keeping the staged tree and swapping directories atomically (`renamex_np RENAME_SWAP`, `renameat2 RENAME_EXCHANGE`): platform-specific, unavailable on Windows, and still copies the whole tree.
- Stealing the legacy lock after an age: a slow live writer would be overrun.
- Hard links from the staging tree: an editor of the old note would modify the staged copy.

## Verification
- `crates/mct-obsidian/src/manifest.rs` tests: `a_reader_never_sees_a_note_missing_during_publication`, `one_changed_note_leaves_the_other_files_untouched`, `a_stopped_writer_does_not_block_the_next_one` (child process stopped with SIGKILL and SIGINT), `concurrent_publishers_wait_for_each_other`, `a_writer_that_cannot_get_the_lock_in_time_fails_with_a_clear_error`, `a_legacy_lock_left_by_this_version_is_replaced_but_a_foreign_one_is_not`, `a_failed_publication_keeps_the_old_manifest_and_the_next_one_recovers`, `a_leftover_temporary_note_is_removed_by_the_next_publication`. The first three fail on `main`.
- `crates/mct-cli/src/web/tests.rs`: `a_snapshot_reload_leaves_the_vault_to_the_reindexer` (fails on `main`).
