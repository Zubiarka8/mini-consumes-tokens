# Error tracking

This directory tracks project MCP errors, observed indexing failures and documented parser limitations over time. Include connection and query failures, invalid tool arguments, and errors corrected during the same session. Notify the user when an MCP error occurs.

- [Registry](REGISTRY.md): stable IDs, current status, evidence and the next verification step.
- [History](HISTORY.md): dated observations and status changes. Append new observations; preserve earlier ones.

## Updating the records

1. Run the project's `get_indexing_status` MCP tool or `mct-cli --root . status`. Record the observation date, checkout revision, index timestamp and relevant diagnostics. An index snapshot may predate the checkout or local changes.
2. Separate unexpected failures from deliberately malformed test fixtures. Expected rejection is not a parser defect. Do not hide unexpected failures by excluding files.
3. For each MCP error, record the tool/operation, relevant arguments without secrets, exact diagnostic and impact. For argument errors, inspect `get_tool_schema` and retry with the documented arguments. Reproduce unexpected parse failures with `scripts/unix/parse-probe.sh <file>` (Windows: `scripts/windows/parse-probe.ps1`). Record the actual outcome, including command failures. Distinguish caller mistakes, invalid input, parser rejection of valid input, and infrastructure failures only when evidence supports the classification.
4. Update the existing ID for the same problem. Add a new ID for a distinct problem. Include the file/symbol, impact, first/last observation, status, evidence, next action, and an issue/PR reference when available.
5. Append a dated entry to `HISTORY.md`, including errors fixed during the same session. Record fixes and their verification revision. Mark a record resolved only after the failing operation succeeds; for indexing failures, also verify that a refreshed index no longer reports the unexpected failure. A missing file or an unverified branch fix is not proof of resolution.

Statuses: `OPEN` (observed unexpected failure), `KNOWN_LIMITATION` (documented unsupported behavior), `EXPECTED` (intentional rejection), `RESOLVED` (verified correction).

Documentation evidence and current reproductions must be labeled separately. Linked issue/PR states are not live status unless checked. Keep records in English and omit secrets, private logs and machine-specific paths. This is a manually maintained registry; no scheduled refresh is installed.

## Local incident log

Besides the versioned records above, `mct-cli`, `mct-mcp-server` and the dashboard keep a local history of occurrences in `<project root>/.mct-index/errors/`, one file per occurrence. It is not versioned (`.mct-index/` is ignored) and does not replace the registry: use it as evidence when you update an ID.

- **Name:** `YYYYMMDDTHHMMSS.ffffffZ--<operation>--<kind>--<id>.log`, for example `20261119T153045.123456Z--reindex--syntax-error--a7f2c01d9b3e.log`. The time is UTC, `operation` and `kind` are lowercase `[a-z0-9-]`, and `id` is 12 hex digits.
- **Content:** `date`, `operation`, `kind`, `path` and `line` when known, and `detail`. The diagnostic has control characters removed, is length-limited, shows the project root as `<root>` and the home directory as `~`, and has known secret shapes (`password=`, `token:`, `Authorization:`, `bearer …`) redacted. Arguments, request bodies and review notes are never recorded.
- **When it is written:** only if `.mct-index/` already exists as a real directory. A failure before the first `init` keeps its usual diagnostic and creates nothing. `errors/` is created on the first record.
- **Never fatal:** writing is best effort. If it fails (for example a regular file sits where `errors/` should be, or `errors/` is a symlink, which is never followed), the operation's output, exit code, MCP response and HTTP status and body are unchanged and the failure is not logged again. A record is written to a temporary file and published with a hard link, so it is complete and never overwrites another record. A filesystem without hard links records nothing.
- **Kinds:**
  - Index (`reindex`): `syntax-error`, `read-failure`, `post-reindex-hook`. A syntax error is recorded once per distinct stored diagnostic: the same invalid file re-parsed on later runs adds nothing, a different diagnostic is a new record, and breaking the file again after it recovered records again. A content change that yields the same `line N: message` is not told apart. A read failure and a hook failure are recorded on every occurrence. Unsupported languages are informational and not recorded.
  - MCP (`tools-call`, `batch`): `invalid-params`, `internal-error`, `not-ready`, `tool-error`, for a direct call and for each failing batch sub-query, even when the batch itself answers. Sub-queries skipped by the byte budget are not recorded. Background failures use the operations `startup-index`, `auto-reindex` and `catch-up-reindex` with kind `reindex-failed`.
  - CLI: `cli-error`, with the subcommand as the operation.
  - Dashboard (`dashboard-review`, `dashboard-ignore`): `review-conflict`, `review-source-stale`, `review-save-failed`, `ignore-conflict`.
- **Indexing:** `.mct-index` is a built-in exclusion, so the indexer and the file watcher ignore `errors/`; writing a record never triggers a reindex.

Delete the files freely; nothing reads them back.
