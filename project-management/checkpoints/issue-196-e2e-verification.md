# issue-196-e2e-verification checkpoint

- Branch: `claude/issue-196-e2e-verification`; base `562633a` (origin/main). Nothing pushed.
- Status: REVIEW. Black-box tests for `mct-mcp-server --reindex-delay <VALUE>` written, all `#[ignore = "needs issue-196-server-delay-flag"]`.
- Files: `crates/mct-mcp-server/tests/reindex_delay_e2e.rs`, `scripts/unix/reindex-delay-smoke.sh` (runs the ignored tests).
- Tests: 500ms delay indexes after the delay and not before; `off` never auto-updates; `0s`/`abc`/`-5s`/`10x` rejected naming flag and value; conflict with `--reindex-debounce-ms` rejected (clap "cannot be used with").
- On this base the 4 tests fail for the right reason: clap reports `unexpected argument '--reindex-delay'`.
- Remaining: after integrating the server flag, remove the four `#[ignore]` attributes and run `scripts/unix/reindex-delay-smoke.sh`; adjust the exact error wording assertions if the server's message differs.
