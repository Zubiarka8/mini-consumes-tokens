# ADR-007: On-demand duplicate function report

## Status
Proposed (spike, issue #207). The Stage 8 evaluation below is recorded; acceptance is pending maintainer review. Adds the `mct-cli duplicates` subcommand. No schema migration, no MCP tool, and no change to `LanguageParser`.

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
- **A new `TokenSource` trait in `mct-core`**, next to but separate from `LanguageParser`. It turns one file's text into a stream of non-comment tokens, each with a kind (keyword, identifier, literal, punctuation, or a synthetic block start or end), its text, its line range and its byte range, plus the line ranges of any syntax errors. A language crate opts in by implementing it; a language without an implementation is simply not covered.
- **Tokens come from the tree-sitter tree, not from text.** `mct-tree-sitter` holds one generic, iterative walker (`tokenize_tree`, over a `TreeCursor`, so nesting depth costs heap, never stack). Each language crate supplies a `TokenClasses` table read off its grammar's `node-types.json`: `trivia_kinds` (comments, Python line continuations; whole subtrees dropped), `literal_kinds` (one token per node, children not walked), `identifier_kinds`, `keyword_kinds` for keyword tokens that are not purely alphabetic (Rust's `macro_rules!`), and `block_kinds` for blocks that only layout delimits. Every other leaf made only of letters and underscores is a keyword; any other leaf is punctuation. `ERROR` and `MISSING` nodes are reported as `error_lines`, and a candidate overlapping one is skipped. Nothing in the walker is specific to a language.
- **Layout-only blocks get synthetic markers.** Python's indent and dedent tokens are hidden in the tree-sitter tree, so leaves alone lose block structure: `total = total * row` inside an `if` and the same line after it gave the same sequence, and the first version grouped two such functions although they behave differently. The walker therefore brackets every node of a `block_kinds` kind with a zero-width, empty-text `BlockStart` and `BlockEnd` token. Python lists `block`; Rust lists nothing, because its blocks have `{` and `}` tokens of their own. Indentation width still produces nothing, so a one-line suite (`if x: y`) matches its indented form.
- **Notable classifications.** Rust `primitive_type` (`i32`, `str`) is an identifier; `self`, `super`, `crate`, `mut` are keywords; a lifetime is `'` plus an identifier. Python `True`, `False`, `None` and `...` are literals, and `self`/`cls` are identifiers.
- **`mct-index` stays grammar-free.** The caller builds a `TokenSourceRegistry` (`mct_languages::build_token_sources()`) and passes it to the analysis. The index crate gains no dependency on any grammar and no language knowledge. Registering a language is one line next to its parser registration.
- **`LanguageParser`, the schema and the MCP server do not change.** Indexing never tokenizes, so a token source costs nothing unless the report is requested.

### Normalization and grouping
- Type-2 clones only. Within a candidate, comments and whitespace are dropped, every identifier becomes `$ID`, every literal becomes `$LIT`, and keywords, operators, structural punctuation and block markers are kept as they are.
- Only the candidate's own block markers are kept. Lines cannot tell them apart from the markers of a block that encloses the candidate: a class body starts on its first method's first line and ends on its last method's last line. A marker before the candidate's first source token is dropped, and so is any marker that does not pair with another inside the candidate. Without this rule, re-measuring pydantic-core lost two real Python groups whose members sat at the start or end of a class.
- A candidate must have at least `min_lines` lines that contain a source token and at least `min_tokens` source tokens, counted before normalization. Block markers are not source text and count toward neither threshold.
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
- **Literal values and literal types are ignored.** Two functions that differ only in a constant group together, and their behavior may differ. This includes constants of different types: `1` and `"a"` are both `$LIT`.
- **Renaming is not checked for consistency.** Every identifier becomes the same `$ID`, independently of the others, so `a + a` and `a + b` normalize alike, and two functions whose data flow differs can group.
- **A file the indexer could not parse is invisible.** The indexer stores no symbol for a file it fails to parse, so the report has no candidate there and does not list it as skipped. The report can say `status: complete` while such a file is left out. `mct-cli status` lists those files. The `parse-error` skip only covers syntax errors inside files that were indexed.
- **Rust macro bodies are token trees.** The grammar splits a macro invocation's body into identifiers, literals and punctuation without parsing it as code, and macro expansion is not modelled. In `macro_rules!`, fragment specifiers (`expr`, `ident`) read as keywords and `$x` as an identifier.
- **Python f-string interpolations collapse into one literal.** The expressions inside `{}` are not compared token by token.
- **Docstrings weigh as lines.** A multi-line string literal counts every line it spans, as the issue's line rule requires, so a long Python docstring can lift a short function over `min_lines` (in pydantic-core, a 43-line group has 119 tokens). It is still one `$LIT` token, so a docstring present in one function and absent in another keeps them apart.
- **The analysis enumerates indexed files, not the disk.** A file deleted or edited since indexing is skipped as stale; it never silently disappears or groups on old content. `reindex` before running for a complete report.
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
Measured on 2026-10-15 on macOS (Apple Silicon) with a release build of `mct-cli` (`cargo build --release -p mct-cli`) at `3f70db8`, default thresholds (6 lines, 40 tokens). Times are wall-clock and memory is the maximum resident set size, both from `/usr/bin/time -l` around one run; they are single observations, not benchmarks.

| | This repository | pydantic-core |
|---|---|---|
| Source | `git archive 3f70db8` (tracked files only, so untracked local folders are not analyzed) | `git clone --depth 1 https://github.com/pydantic/pydantic-core`, commit `383eb95a19433754c0cecf7025b50c26b6d97a36` (2026-01-07); read and indexed only, nothing built or executed |
| Commands | `mct-cli --root <dir> init`, then `mct-cli --root <dir> duplicates` | same |
| Supported languages | python, rust | python, rust |
| Unsupported, reported | bash, cpp, csharp, go, java, javascript_typescript, kotlin, lua, php, powershell | javascript_typescript |
| Indexing (`init`) | 5.01 s, 39.8 MB; 744 files, 13,584 symbols | 1.85 s, 27.0 MB; 253 files, 5,695 symbols |
| Duplicate analysis | 0.48 s, 31.7 MB | 0.49 s, 27.0 MB |
| Candidates analyzed | 3,712 (1,104 below threshold) | 4,024 (1,589 below threshold) |
| Skipped | 0 (complete) | 0 (complete) |
| Groups (members) | 147 (464), all Rust | 149 (355): 55 Rust, 94 Python |
| Groups entirely in tests, benches or examples | 110 | 90 |

The `first_error` copies that motivated the issue no longer exist: the function now lives once in `mct-tree-sitter`, and no group contains it.

**Re-measurement after the block-marker fix.** Black-box testing after this evaluation found the Python nesting false positive described under "Token source". After the fix, both projects were re-run with the same indexes and default thresholds:
- Both reports are identical to the ones above, group for group and member for member.
- Duplicate analysis takes 0.48 s and 31.7 MB on this repository and 0.50 s and 27.2 MB on pydantic-core (three runs each).
- The fix therefore changes no group at the default thresholds in either project, and the manual review below stands.
- The false positive was not in the reviewed sample. The sample's "0 false positives" says nothing about nesting-only matches.

### Manual review
Sample: the 10 highest-ranked groups of each project (20 in total), in report order. Evidence: the member list and excerpts of the report, plus reading the source of at least two members of every group. Classes: **useful** (a structural duplicate worth consolidating or parametrizing), **coincidental** (same shape by design or by API convention, not worth merging), **false positive** (members do not actually share their structure).

| Project | # | Language | Members | Classification and evidence |
|---|---|---|---|---|
| this repository | 1 | Rust | 14 | Useful: `push_symbol` repeated verbatim in 14 language crates' walkers. |
| this repository | 2 | Rust | 14 | Useful: `push_relation`, same 14-crate repetition. |
| this repository | 3 | Rust | 10 | Useful: `open_indexed` test helper in each `tests/index_integration.rs`. |
| this repository | 4 | Rust | 14 | Useful, low value: the 8-line `parse` helper of each `tests/parse.rs`. |
| this repository | 5 | Rust | 12 | Useful: `open_indexed` of each `tests/depgraph.rs`. |
| this repository | 6 | Rust | 3 | Useful: the `find_references`, `find_calls` and `find_callers` MCP handlers in `server.rs` differ only by argument name, relation direction and label. |
| this repository | 7 | Rust | 2 | Useful: `mcp_chars` copied between the two `token_benchmark` examples. |
| this repository | 8 | Rust | 3 | Useful: the same test-double `parse` in three `mct-index` test files. |
| this repository | 9 | Rust | 13 | Coincidental: one syntax-error test per language crate, required by the contributing checklist. |
| this repository | 10 | Rust | 9 | Useful: `uuid_like` copied into nine `mct-index` test files. |
| pydantic-core | 1 | Rust | 8 | Useful: `serialize_i8` … `serialize_u64` share one body that a macro could generate. |
| pydantic-core | 2 | Python | 3 | Coincidental: `list_schema`, `set_schema`, `frozenset_schema` public builders; the shared shape is the API convention. |
| pydantic-core | 3 | Python | 3 | Coincidental: `no_info_{before,after,wrap}_validator_function` builders. |
| pydantic-core | 4 | Python | 2 | Coincidental: `with_info_{before,wrap}_validator_function` builders. |
| pydantic-core | 5 | Python | 2 | Coincidental: `url_schema` and `multi_host_url_schema` builders. |
| pydantic-core | 6 | Python | 2 | Coincidental: `tuple_schema` and `arguments_schema` only share the count of keyword parameters forwarded to `_dict_not_none`. |
| pydantic-core | 7 | Python | 3 | Useful: `test_config_datetime`, `_date`, `_time` could be one parametrized test. |
| pydantic-core | 8 | Rust | 2 | Useful: `Url::py_new` and `MultiHostUrl::py_new` repeat the same validation setup. |
| pydantic-core | 9 | Rust | 2 | Useful: the two URL validators' `validate` bodies. |
| pydantic-core | 10 | Python | 3 | Useful, low value: three argument-validation tests differing only in data. |

Totals in this sample: 14 useful, 6 coincidental, 0 false positives. Coincidental matches cluster in thin API builders whose bodies only forward keyword arguments, where identifier normalization erases the only difference. This is a 20-group sample chosen by rank, not a false-positive rate.

### Outcome
- At least one language has a reliable token source without changing `LanguageParser`, so the Stage 1 gate passed; Rust and Python both do.
- On-demand analysis costs about half a second and under 32 MB on projects of this size, well within routine use, so the persistence condition above is not met.
- The report surfaced the parser-walker repetition the issue suspected but had not measured (`push_symbol` and `push_relation` in 14 crates each) and test-helper copies. Whether to accept this ADR, and whether an MCP tool or more languages are worth a follow-up issue, is the maintainer's decision.
