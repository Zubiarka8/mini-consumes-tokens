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
