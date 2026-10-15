# ADR-007: On-demand duplicate function report

## Status
Proposed (spike, issue #207). Draft: the sections marked `TODO(coordinator)` are filled in after the Stage 8 measurements and manual review, and the status moves to Accepted or Rejected then. Adds the `mct-cli duplicates` subcommand. No schema migration, no MCP tool, and no change to `LanguageParser`.

## Context
- Duplicated logic is documented but never reported. `first_error` is identical in `mct-lang-rust`, `mct-lang-python` and `mct-lang-cpp`, and language registrations repeat between `mct-cli` and `mct-mcp-server`. `find_dead_code` finds symbols nobody references. Nothing finds code that exists twice.
- The index stores symbols with a line range. `SymbolRecord` keeps neither the body's tokens nor a fingerprint, and `LanguageParser::parse` returns symbols and relations, not tokens.
- `index_file` skips a file whose content hash is unchanged. An analysis that ran only on changed files could miss a pair whose other copy did not change, so the analysis must look at every candidate each time it runs.
- Phase 0 of the issue forbids changing parsers, traits, the SQLite schema or published MCP signatures, forbids running during `reindex`, and forbids persisting groups or fingerprints. A tokenizer that is a regex over source is also ruled out; the token source must be a parser or a reliable lexer.
- The first question is therefore whether any language can be tokenized reliably without touching `LanguageParser`.

## Decision

### Shape
- **On demand only.** `mct-cli --root <project> duplicates [--min-lines N] [--min-tokens N]` opens the existing index, analyzes the current files and prints a report. It never reindexes, never writes, and never creates an index for a project that has none. Defaults are 6 lines and 40 tokens.
- **Read-only analysis in `mct-index`.** `find_duplicate_groups(index, token_sources, options)` selects indexed `function` and `method` symbols, reads each file through `read_repository_file` and its limits, and returns a `DuplicateReport`. One candidate's failure becomes a skipped candidate, not an error. Only a failure to query the index fails the call.
- **The report is TOON.** There is no JSON output. The header states that every group is a candidate for human review and that identical structure does not prove redundant or equivalent behavior. It carries complete or partial status, the options used, supported and unsupported languages, how generated code is handled, totals, skipped counts for every reason, and for every group the exact `path:start-end` ranges to inspect with a bounded excerpt per member.

### Token source
- **A new `TokenSource` trait in `mct-core`**, next to but separate from `LanguageParser`. It turns one file's text into a stream of non-comment tokens, each with a kind (keyword, identifier, literal or punctuation), its text, its line range and its byte range, plus the line ranges of any syntax errors. A language crate opts in by implementing it; a language without an implementation is simply not covered.
- **Tokens come from the tree-sitter tree, not from text.** `mct-tree-sitter` holds one generic walker over the leaves of a tree. Each language crate supplies a small per-grammar table that says which node kinds are comments, which are literals, which are identifiers, and which keywords the grammar exposes. Nothing in the walker is specific to a language.
- **`mct-index` stays grammar-free.** The caller builds a `TokenSourceRegistry` (`mct_languages::build_token_sources()`) and passes it to the analysis. The index crate gains no dependency on any grammar and no language knowledge. Registering a language is one line next to its parser registration.
- **`LanguageParser`, the schema and the MCP server do not change.** Indexing never tokenizes, so a token source costs nothing unless the report is requested.

### Normalization and grouping
- Type-2 clones only. Within a candidate, comments and whitespace are dropped, every identifier becomes `$ID`, every literal becomes `$LIT`, and keywords, operators and structural punctuation are kept as they are.
- A candidate must have at least `min_lines` lines that contain a token and at least `min_tokens` tokens, counted before normalization.
- Candidates are grouped by equality of the normalized sequence, and only inside one language. Two functions in different languages never share a group. A hash may speed up the comparison but is not an identifier and not part of the output contract.
- A group has at least two members. Groups are ordered by `line_count x member_count` descending, ties by the first member's path and start line, and members by path and start line. The same tree, index and options give the same report.

### Languages
- **Supported in Phase 0: Rust and Python.** Both grammars distinguish keywords, identifiers, literals and comments as distinct node kinds, and the indexed ranges map onto lines.
- **All other languages are excluded in Phase 0.** Their functions are not candidates and are not counted as skipped; the report lists the indexed languages that have function symbols but no token source under `unsupported_languages`. Adding a language is adding its table and one registration; it is not a design change.

### Generated code
- Phase 0 detects no generated code. There is no reliable existing marker, and the issue forbids adding filename or content heuristics for this spike. Generated files are left out only when the project's existing exclusion rules already keep them out of the index (built-in exclusions, `.mctignore`, `@import-gitignore`). The report states this in a `generated_code` line so that a reader knows generated duplicates may be present.

### Skipped candidates
- A candidate is skipped, with a reason and a short detail but never file contents, when the file cannot be read within the repository limits (`read-error`), the token source fails or the candidate overlaps a syntax error (`parse-error`), the file changed since it was indexed (`stale-location`), or its indexed range is empty, inverted, missing its end or outside the file (`invalid-range`). Any skip marks the report partial. The CLI prints counts for all four reasons, including zero.

## Consequences
- The first version answers "what is repeated in the languages we can tokenize" without a migration, a new MCP tool or a parser change, and it costs nothing at index time.
- The analysis re-reads and re-tokenizes every candidate on each run. Its cost grows with the number of function and method symbols in the supported languages, not with the size of the whole repository. Measured runtime and memory are recorded in "Evaluation (Stage 8)".
- Nothing is persisted, so no staleness bug and no migration can be introduced. The price is that an agent cannot ask for duplicates cheaply from inside a session.
- `mct-core` gains one trait and a few types; `mct-tree-sitter` gains one generic walker; each opted-in language crate gains a table and an `impl`. A language crate without a token source is untouched.
- The report is a list of leads. Reading the ranges is what decides whether two functions should be merged.

## Alternatives considered
- **A regex or hand-written lexer over the source.** Rejected by the issue and unreliable: it cannot tell a keyword from an identifier in every context, mishandles nested comments, raw strings and macros, and would need one lexer per language.
- **Extend `LanguageParser` to emit tokens.** Cross-cutting: every language crate and every test of the trait would change, and indexing would pay for tokens it does not use. Deferred until the report proves useful (see the conditions below).
- **Compute fingerprints at index time and store them in SQLite.** Needs a schema migration, and a migration can invalidate all content hashes. It also reintroduces staleness: a fingerprint of a file whose other copy changed is wrong until both are reindexed. Out of scope for the spike.
- **Hash AST subtrees instead of token sequences.** Catches structure that differs in layout but not in shape, but it ties the result to each grammar's node names, so the same function would group differently across grammar versions, and a subtree hash cannot report a token or line count that matches the thresholds the issue defines.
- **Group across languages.** Rejected by the issue: identical normalized text in two languages rarely means the same thing.
- **Detect Type-3 (near-miss) clones.** Out of scope; it needs similarity scoring, which makes the output depend on a tuned threshold.

## Known limitations
- **Ranges are line-based.** A candidate covers whole lines from the indexed start line to the end line, so tokens on the first or last line that sit outside the function's own node (for example an attribute or a closing brace of an enclosing block on the same line) are counted with it.
- **Nested functions count inside the outer one.** A function defined inside another is a candidate on its own and also contributes its tokens to the outer function, so an outer and an inner pair can both appear.
- **Literal values are ignored.** Two functions that differ only in a constant (`1` vs `2`, `"a"` vs `"b"`) group together, and their behavior may differ.
- **Rust macro bodies are raw token trees.** Tokens inside a macro invocation are not parsed by the grammar, so identifiers, keywords and literals there are classified from the token tree and may be less precise.
- **Python f-string interpolations collapse into one literal.** The expressions inside `{}` are not compared token by token.
- **No Type-3 detection.** One inserted or deleted token separates two otherwise identical functions into different groups.
- **Generated code is not detected.** See above.
- **Same shape is not same intent.** Small accessors, builders and trait method bodies often match by design.

## Conditions for a later phase
Each is a reason to open a separate design issue, not to extend this one.
- **Persistence.** The measured runtime or memory of an on-demand run is too large for routine use on the projects it is meant for, and a fingerprint can be kept consistent with `index_file`'s hash-skip and `reindex_paths`.
- **A parser-API phase.** Three or more further languages are worth supporting and their per-grammar tables repeat enough code that emitting tokens from `LanguageParser` would remove more duplication than it adds, or another analysis also needs tokens from the same pass.
- **An MCP tool.** The manual review shows enough useful matches that agents would call the report during a session. Publishing a tool changes the MCP contract.
- **Type-3 detection.** Reviewers report near-misses as the main missing signal, not as noise.

## Evaluation (Stage 8)
TODO(coordinator): fill in from the measured runs; do not estimate. Record only what was run.

### This repository
- TODO(coordinator): command used.
- TODO(coordinator): revision.
- TODO(coordinator): supported and unsupported languages as reported.
- TODO(coordinator): indexing time, separately from analysis time.
- TODO(coordinator): duplicate-analysis runtime.
- TODO(coordinator): approximate memory use and how it was estimated.
- TODO(coordinator): candidates analyzed, below threshold, skipped (per reason) and groups found.

### External project
- TODO(coordinator): project, source and revision, and how it was obtained and analyzed.
- TODO(coordinator): the same figures as above.

### Manual review
- TODO(coordinator): sample criteria (up to 20 groups, all of them if fewer exist).
- TODO(coordinator): per reviewed group, its language, the evidence used and the classification (useful structural candidate, coincidental match or false positive).
- TODO(coordinator): totals per classification. A small sample is not a false-positive rate.

### Outcome
- TODO(coordinator): update the Status line, and revisit "Conditions for a later phase" against the measurements.
