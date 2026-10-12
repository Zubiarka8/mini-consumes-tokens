# issue-196-cli-register-persistence

Slice of #196: `mct-cli mcp-register --reindex-delay <VALUE>`.

- Grammar: `off`, or positive `<N>ms|s|m` (normalised lowercase); invalid input errors and writes nothing.
- Written to the server entry args as `--reindex-delay <value>`.
- Re-running without the flag preserves an existing value; with the flag it replaces it (no duplicates).
- Checks: `scripts/unix/check.sh -p mct-cli` passed (fmt, 37 tests, clippy).
- Server flag and docs are handled by other slices.
