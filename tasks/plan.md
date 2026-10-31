# Markdown indexing audit plan

## Scope

Complete the Markdown entries in `mct-falta.md` against the parser, index and
duplicate-analysis implementation. Preserve existing changes and avoid schema,
published tool signature and `LanguageParser` changes.

## Ordered work

1. Verify Markdown symbols, relations, literals and incremental indexing with
   focused parser and index tests. Correct reproducible defects.
2. Design and implement duplicate detection for sections and paragraphs using
   accurate source ranges and language-owned tokenization. Preserve existing
   Rust and Python results and handle overlapping candidates explicitly.
3. Exercise duplicate detection through the real index and user-facing entry
   points, including edits, deletions, stale files, exclusions and Unicode.
4. Update `mct-falta.md` with verified results and remaining limitations; run
   focused tests, workspace checks and review the diff.

## Risks

Paragraphs are not indexed symbols. Duplicate analysis currently selects only
functions and methods, so a token source alone cannot enable this feature.
The design must avoid inventing code-symbol semantics for prose and must not
silently report unsupported modes as supported.
