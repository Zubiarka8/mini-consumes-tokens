# ROADMAP — mini-consumes-tokens (post-0.1.0)

> **Historical / point-in-time document**, frozen at commit `9ca9ab5` (2026-09-13, pre-0.2.0). Several candidates proposed below have since shipped (e.g. Kotlin, Markdown, `list_symbols`) — `CHANGELOG.md` is the current source of truth for what's actually released. Kept for planning history, not as a live status page.

Analysis and planning only; no product code was written to produce this
document. It is based on a full review of `checklist.md`, `CHANGELOG.md`,
`README.md`, `RELEASING.md`, `CONTRIBUTING.md`, and the `mct-core`/`mct-index`
schema, indexer, and queries as they stood in version 0.1.0 (commit `9ca9ab5`,
2026-09-13).

Effort estimates use **one project work session** as the unit, based on prior
work such as the Java+C# session, the C++/Go session, and Clippy enforcement.
Small means part of a session or a short, bounded session. Medium means one
full session, comparable to adding a language. Large means multiple sessions
or a session at significant risk of expanding unless scoped in advance.

---

## Candidatos evaluados

### 1. Type resolution through real LSP integration

**What:** Replace or augment the current AST/name-based heuristics with real
type resolution through a language server or equivalent library for each
language. Today, `find_symbol`, `find_references`, and `find_calls` query by
name (`WHERE name = ?1`) and cannot distinguish same-named symbols in separate
scopes; see `mct-index/src/queries.rs`.

**Why it matters:** If two classes in a real repository define `run()`,
`find_callers("run")` silently combines their callers. More symbols mean more
name collisions, so this gets worse as a repository grows. LSP is also the
explicit dependency for Go's proposed `find_implementations` (candidate #3)
and could improve C# and C++ `extends`/`implements` accuracy.

**Effort:** Large. There is no single protocol shared by all languages:
`rust-analyzer`, `pyright`/`pylsp`, `gopls`, `clangd`, and others have distinct
startup and process-lifecycle needs. Decide between persistent child
processes and in-process resolver libraries where available. This likely
requires a sequence of sessions by language or protocol family.

**Risk and dependencies:** This affects the `core` model: symbols would need
resolved type/scope identity beyond `name` and `parent`, and the index schema
may need a column or table. Every existing language must be revalidated. This
has the widest blast radius in this roadmap.

**Blocking:** No for current use. This is the project's open decision #1 and
does not block a completion criterion.

### 2. Secret exclusion patterns — Ruby, PHP, and Swift

**Correction recorded at the time:** PHP already had its own crate
(`mct-lang-php`; see candidate #5), but the `wp-config.php` and
`config/database.php` secret patterns remained unimplemented. An earlier
`checklist.md` statement that no PHP secret conventions had been identified
contradicted this entry; it was corrected there under open decision #2.

**What:** Add Ruby patterns (`config/master.key`,
`config/credentials.yml.enc`), PHP patterns (`wp-config.php`,
`config/database.php`; `.env` is already covered generically), and Swift
patterns (`GoogleService-Info.plist`, credential-bearing `*.xcconfig`) to
`mct-index/src/exclude.rs`.

**Why it matters:** Path exclusions work without a registered `LanguageParser`
or a language crate, because `mct-index` can exclude a path without parsing
the file. This work is independent of adding Ruby, PHP, or Swift support.

**Effort:** Small. Add constants to `DEFAULT_EXCLUDE_PATTERNS` and tests in
`mct-index/tests/exclude.rs`.

**Risk and dependencies:** None; no `core` or language crate changes.

**Blocking:** No. Open decision #2 was deferred, not active work.

### 3. `find_implementations` for Go

**What:** Add a tool or existing-tool parameter that returns the Go structs
structurally implementing a named interface by matching method sets.

**Why it matters:** Without this, users cannot ask which types implement an
interface without searching manually.

**Effort:** Medium with a local heuristic that compares method names but does
not resolve parameter types; this risks false positives and negatives when
unrelated structs share method names. Small if real type resolution already
exists.

**Risk and dependencies:** Explicitly depends on decision #1. Choose either a
heuristic implementation with known error margins or wait for LSP; doing both
would duplicate the work.

**Blocking:** No. This is open decision #3.

### 4. Structural HTML, CSS, and JSX indexing

**What:** Extend the `core` model with symbol and relation kinds (for example,
`Element`, `StyleRule`, `Renders`, and `StylesTarget`) so HTML, CSS, and JSX
form a graph, rather than indexing only embedded code through the generic
walker.

**Why it matters:** Frontend questions such as which component renders a
button or which CSS rules affect a class cannot currently be answered without
reading or searching `.html`, `.css`, or `.tsx` files. This is a visible gap
for modern web stacks.

**Effort:** Large. This changes `mct-core`; every language crate and every
exhaustive mapping of symbol/relation kinds must be reviewed and revalidated.
Do not combine it with another `core` change such as LSP.

**Risk and dependencies:** High. Unlike adding a language crate, this is a
cross-cutting model change. The earlier scope decision is recorded in
`checklist.md`; logical JS/TS indexing works without it.

**Blocking:** No.

### 5. Additional languages: Kotlin, Swift, and Ruby

PHP was removed from this candidate after `mct-lang-php` was implemented.
The PHP secret patterns in candidate #2 remain an independent task.

**What:** Add `mct-lang-kotlin`, `mct-lang-swift`, and `mct-lang-ruby` using
the established `LanguageParser` workflow: register each crate in
`mct-mcp-server` and `mct-cli`, and add fixtures, fuzzing, and benchmarks.

**Why it matters:** At the time of this roadmap, these languages were listed
in `KNOWN_PENDING_LANGUAGES`; `get_indexing_status` could report them as
pending instead of failing silently. Kotlin and Swift were likely high-demand
targets, and production Ruby/Rails repositories could benefit as well.

**Effort:** Medium per language, comparable to other language additions.
**Risk and dependencies:** Architecturally low; the `LanguageParser` trait
does not require `core` or `index` changes. Add secret exclusions before
indexing repositories that may contain credentials, as a recommended order
rather than a hard dependency.

**Blocking:** No.

### 6. `find_types(kind?)`

**What:** List symbols by one of the six existing type kinds (`Class`,
`Struct`, `Interface`, `Enum`, `Trait`, or `TypeAlias`). Prefer an optional
`kind` parameter on `find_symbol` over a new tool.

**Why it matters:** Users exploring an unfamiliar repository often need to
list its types before they know an exact name, for example, all interfaces in
a module.

**Effort:** Small. `symbols.kind` is already stored as text; add a filtered
query and tests without changing the schema. **Risk and dependencies:** None;
no `core` or language changes. Reusing a tool also avoids exceeding the
project's stated range of 6–8 tools.

**Blocking:** No.

### 7. `find_tests(symbol?)`

**What:** Expose the `test`/`test_*` name heuristic currently embedded in
`impact_analysis` (`mct-mcp-server/src/server.rs`) so users can list tests
related to a symbol without requesting the full impact analysis.

**Why it matters:** Users who only want to know which tests cover a function
currently pay for the callers/references included in `impact_analysis`.

**Effort:** Small. Extract and reuse the existing tested heuristic.
**Risk and dependencies:** Low technical risk. Product risk: this would be
tool #8, at the project's stated tool-count limit. If both `find_tests` and
`find_types` are pursued, make `find_types` a parameter on an existing tool.

**Blocking:** No.

### 8. `find_dead_code()`

**What:** List symbols with no incoming relations as candidates for dead code.

**Why it matters:** This supports repository cleanup without requiring
repeated manual searches for each symbol.

**Effort:** Small for an initial query against the existing schema, with no
`core` or `index` changes. However, no incoming relation does not prove code
is unused: `main`, framework entry points called from outside the indexed
repository, public library APIs, and trait/interface implementations invoked
through dynamic dispatch may all appear unused. Consider a small filtering
pass before presenting the result as trustworthy.

**Risk and dependencies:** No technical dependencies. High product risk if
false positives make users distrust the tool. Evaluate it on a real
repository before exposing it.

**Blocking:** No.

### 9. `semantic_search(query)`

**What:** Search symbols by meaning using embeddings, in addition to exact-name
and lexical FTS5 search.

**Why it matters:** Users cannot find a function such as
`computeTotalWithVat` from a description like “where is the tax-inclusive
price calculated?” unless they use a text search.

**Effort:** Large. This adds embedding generation, vector storage, and a
product decision about local inference versus an API; it is more than adding
a query over the current schema.

**Risk and dependencies:** An API conflicts with the README's “no network
calls by default — zero telemetry” promise unless explicitly opt-in. Local
embeddings avoid network access but add a substantial inference runtime and
binary size. The user must make an explicit product decision before this can
be estimated precisely.

**Blocking:** No. Defer indefinitely unless explicitly requested; see
“Deferred for now.”

### 10. Large-repository performance research

This is a measurement task, not a proposed optimization: generate or obtain
a synthetic repository with hundreds of thousands of files, then measure
initial indexing time, SQLite size, and query latency before users report a
scaling problem.

The 0.1.0 code review identified three unconfirmed hypotheses to measure:

1. `reindex()` walks the entire tree and reads every non-excluded file into
   memory to hash it, including unchanged files. I/O may therefore scale with
   repository size rather than the diff.
2. Parsing is sequential, so initial indexing does not use multiple cores.
3. `write_parsed_file` runs one SQL lookup per relation. This may be costly at
   high relation counts. `LIMIT 1` without deterministic tie-breaking also
   makes same-named-symbol resolution more arbitrary; that is a correctness
   concern related to candidate #1, separate from performance.

**Effort:** Medium for building a realistic fixture and measuring these
hypotheses. Estimate fixes only after seeing results; potential changes include
parallel parsing or caching file metadata to avoid rereading unchanged
content.

**Risk and dependencies:** Low for research; evaluate any resulting product
change separately using measurements.

**Blocking:** No. This was preventive work, not a response to a user-reported
problem.

### 11. Local token-savings observability

Consider exposing a token-savings estimate or post-session report to users.
Today, measurements exist only in development benchmarks such as
`benchmarks/token-benchmark.md` and `mct-cli/examples/token_benchmark.rs`.

**Why it matters:** Users cannot verify savings in their own repositories and
usage patterns; they can only rely on generic benchmark results.

**Effort:** Small for a `get_token_savings_estimate()` tool that compares
responses from the active MCP session with estimated `Read`/`Grep` costs using
the existing benchmark method. This requires a bounded, in-memory usage
counter, not changes to `core` or `index`. Medium for a persistent,
post-session report with its own storage.

**Risk and dependencies:** Product risk. An MCP tool would compete for the
6–8-tool budget; consider a `mct-cli --root . stats` command instead, since
users may want these results outside an agent workflow.

**Blocking:** No.

### 12. Minor technical debt noted during review

These items were found while reviewing the full checklist and README. None
blocks current work; they were grouped here for tracking:

- **Uneven integration fixtures:** the six newer language crates (Java, C#,
  JS/TS, C++, Go, and the polyglot fixture) have end-to-end tests through the
  real `mct-index`; Rust and Python have parser-level tests only. Small effort:
  add one integration fixture for each using the established pattern.
- **Incomplete token benchmarks for Rust and Python:** newer languages have
  documented benchmarks, but these two do not. The overall requirement of
  benchmarking at least three languages is already met, so this is not a
  blocker. Small effort.
- **Stale README benchmark/status claims:** at the time, the README still
  said the token benchmark was planned even though it had been run for five
  languages, and it said seven languages were implemented while the table
  counted eight plus Lua. These were documentation inconsistencies, not new
  product features.
- **Actual crates.io/GitHub release:** release configuration was prepared in
  `RELEASING.md` and `.github/workflows/release.yml`, but deliberately not
  executed. Publishing requires the user's decision.

---

## Orden sugerido

This is a reasoned sequence, not a schedule. Start with low-cost work that
does not change `core`; reserve isolated sessions for `core` changes; measure
before committing to work that cannot yet be estimated reliably.

1. Fix the README's stale status and benchmark statements (candidate #12).
2. Add Ruby/PHP/Swift secret exclusions (candidate #2); small and independent
   of `core`.
3. Add `find_types` as a `find_symbol` parameter (candidate #6).
4. Evaluate `find_tests` alongside #6, deciding which feature should be a
   separate tool while respecting the 6–8-tool limit.
5. Measure large-repository performance (candidate #10), ideally alongside
   the smaller work above. Use results to decide whether indexing needs
   optimization before large-repository usage grows.
6. Add Kotlin or Swift first (candidate #5) based on demand. This can proceed
   while the larger decisions mature.
7. Consider `find_dead_code` (candidate #8) after performance measurements;
   they may show whether the query needs an index or other adjustment.
8. Decide whether token-savings observability belongs in `mct-cli` rather
   than an MCP tool (candidate #11).
9. Handle LSP (candidate #1) and structural HTML/CSS/JSX (candidate #4) in
   separate, isolated sessions. Both change `core` and require revalidating
   every existing language crate; the user should choose which comes first.
10. Implement Go `find_implementations` (candidate #3) after LSP, or as an
    isolated medium-sized heuristic if the user chooses not to wait.
11. Consider semantic search (candidate #9) only after an explicit product
    decision about the “no network calls by default” promise.

---

## Deferred for now

- **External embedding APIs for `semantic_search`:** not a default, because
  they violate the README's no-network/zero-telemetry promise. Reconsider a
  local model only if a real user need justifies its inference runtime and
  binary size.
- **Per-language SQLite tables:** the project uses one schema with a
  `language` column. Splitting tables would not address the three performance
  hypotheses above, so those measurements alone do not justify reopening the
  design.
- **Parallel indexing as a committed optimization:** do not adopt `rayon`
  before measuring. Revisit parallelism after candidate #10 provides data.
- **A scope heuristic instead of `LIMIT 1`:** do not create a separate
  candidate. Same-file or same-directory preference is partial type
  resolution and belongs within open decision #1; evaluating it separately
  would duplicate that design discussion.
