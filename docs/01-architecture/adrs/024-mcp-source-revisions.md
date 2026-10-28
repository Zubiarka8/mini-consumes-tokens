# ADR-024: Verify source revisions before rendering MCP ranges

## Status

Accepted for local implementation. Completes the source verification behavior
of #236 on MCP. Snapshot consistency depends on ADR-021 (#234).

## Context

Indexed ranges describe the bytes parsed by the indexer. Reading a modified
file and applying old ranges can present unrelated code as a symbol definition.
The web endpoint already refuses such source; MCP must enforce the same rule.

## Decision

Read the file once with the existing repository reader, compare its Git blob
hash with the indexed file hash, and render only those same verified bytes.
Reuse this check for skeletons, snippets and context-pack source/signatures.
Missing hashes are unverifiable. Optional source is omitted while indexed
metadata remains available. Skeleton requests return an explicit reindex error.
Use ADR-021 read snapshots for the symbol lookup and byte verification together.

## Consequences

Source retrieval hashes the file it already reads. No dependency, new size cap,
review store or source rewrite is introduced. Files can change immediately
after the read; the result still describes the verified bytes at read time.

## Verification

`tests/source_revision.rs` covers unchanged/reindexed source, shifted and
same-line edits, missing/binary data, restoration, independently stale related
files, the existing size limit and cached rankings.
