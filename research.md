# Research — token efficiency and MCP accuracy

> **Status**: analysis, not implementation. No product code was modified to
> produce this document.
> **Date**: 2026-09-18 · **Branch**: `refactor/mct-rename` · **Base commit**: `abcb478`
>
> Related repository documents:
> - `ISSUES_PENDING.md` — three product bugs already captured by `#[ignore]`
>   tests. They are **referenced**, not reopened, here.
> - `ROADMAP.md` — frozen at `9ca9ab5` (pre-0.2.0). Candidate #10
>   ("large-scale performance") listed three unmeasured hypotheses. This
>   document **measures two and corrects one** (see §C1).

---

## 1. Method

All figures are measured, not estimated. They were obtained by sending
JSON-RPC directly to the actual server binary:

```sh
call() {
  printf '%s\n' \
    '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"p","version":"0"}}}' \
    '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
    "$1" | ./target/debug/mct-mcp-server.exe --root . 2>/dev/null | tail -n 1
}
call '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' | wc -c
```

Conversion used throughout: **~4 bytes ≈ 1 token** (the same heuristic as
`benchmarks/token-benchmark.md`). This is not a real tokenizer; it indicates
the order of magnitude, which is the point here.

Index state at measurement time: **663 files, 5,544 symbols**.

---

## 2. Executive summary

| # | Finding | Measurement | Impact | Effort |
|---|----------|--------|---------|----------|
| A4 | Entire `.claude/worktrees/` tree is indexed, duplicating the repository | 3,652 of 5,544 symbols (65.9%) are duplicate noise | **High** | Small |
| A2 | `get_project_overview` with no arguments | **167,864 B ≈ 42,000 tokens** in one call | **High** | Small |
| A3 | `get_indexing_status` | 21,113 B ≈ 5,278 tokens; 20,425 B (97%) are dependency dumps | **High** | Small |
| B1 | Relations resolved only by global name | `find_callers("main")` returns PHP and Python, no Rust | **High** | Medium |
| C1 | `WalkDir` does not prune directories | Walks `target/` (45,220 files) and `.git/` (1,725) on every reindex | **High** | Small |
| A1 | Fixed `tools/list` cost per session | 12,873 B ≈ 3,218 tokens before the first query | Medium | Medium |
| B5 | File/directory heuristic breaks on dots | `list_symbols(".claude")` says "No symbols found" despite 3,652 symbols inside | Medium | Trivial |
| B3 | FTS5 table maintained by triggers but **never queried** | Write cost on every reindex; no query benefit | Medium | Medium |
| B2 | `to_symbol_id` is written (one SQL query per relation) and never read | N queries per file with no consumer | Medium | Small |
| C9 | Watcher monitors the entire root, including `target/` | One `cargo build` triggers thousands of events | Medium | Small |
| D1 | Obsidian vault is invisible to `get_project_overview` | `overview("docs")` = 18 modules, **100% "(no top-level symbols)"** | Medium | Medium |

---

## 3. Block A — Token usage

### A1. Fixed startup cost: 3,218 tokens before the first query

`tools/list` returns **12,873 bytes**. Measured breakdown by tool:

| Tool | `description` (B) | `inputSchema` (B) | Total (B) |
|---|---|---|---|
| `list_symbols` | 708 | 1.149 | 1.949 |
| `impact_analysis` | 593 | 1.024 | 1.710 |
| `find_callers` | 468 | 1.098 | 1.658 |
| `find_references` | 381 | 1.093 | 1.582 |
| `get_project_overview` | 609 | 793 | 1.515 |
| `find_calls` | 233 | 1.095 | 1.418 |
| `get_file_skeleton` | 840 | 418 | 1.383 |
| `find_symbol` | 366 | 481 | 923 |
| `reindex` | 543 | 284 | 897 |
| `get_indexing_status` | 434 | 36 | 552 |

Two specific observations:

1. **Three nearly identical schemas:** `find_calls`, `find_callers`, and
   `find_references` share the same four parameters
   share the same four parameters (`function`/`symbol`, `limit`, `offset`,
   `depth`) and long descriptions for `depth` and `offset`: **3,286 B (~820
   tokens)** of near-duplicate schema. Shorten those parameter descriptions
   to one line and link to tool documentation to save ~500–600 tokens without
   losing actionable information.
2. Tool descriptions are long **by design** and prevent agents from choosing
   the wrong tool. Trim *repeated parameter descriptions*, not the tool
   descriptions themselves.

**Recommendation:** target `tools/list` ≤ 9,000 B (~2,250 tokens). Add a test
that fails when the payload exceeds this budget, as
`docs/03-performance/limits-spec.md` does for product limits.

### A2. `get_project_overview` costs ~42,000 tokens by default

Measured in this repository:

| Llamada | Bytes | ~Tokens |
|---|---|---|
| `get_project_overview({})` | 167.864 | ~42.000 |
| `get_project_overview({include_relations:false})` | 45.760 | ~11.440 |
| `get_project_overview({path:"crates"})` | 164.294 | ~41.000 |

The tool description literally says *"TOKEN-SAVING project digest"* and
*"the cheapest way to get oriented"*. A call with no arguments consumes more
context than reading 40 full files.

Causes, all in `crates/mct-mcp-server/src/server.rs:533-600` and
`crates/mct-mcp-server/src/format.rs:overview`:

- `include_relations` defaults to **`true`** (`unwrap_or(true)`), and each
  symbol emits callers as sub-lines. This is 73% of the payload.
- `max_symbols_per_module` (default 8) limits symbols **per module**, but
  there is no cap on module count (225 here) or total bytes.
- Modules without symbols still emit a `(no top-level symbols)` line (see D1).

**Recommendations:**
1. Default `include_relations` to `false`. This one-line change has the best
   impact-to-effort ratio in the report.
2. Add a **byte budget** (for example, 24,000 B), truncate by module, and
   declare truncation in the header: `(120 of 225 modules shown — pass
   `path` to narrow)`. `limit` counts rows, not bytes; a byte budget actually
   protects context.
3. Sort modules by symbol density before truncation so the most informative
   content is retained.

### A3. `get_indexing_status`: dependency dumps make up 97% of the payload

21,113 B ≈ 5,278 tokens, of which **20,425 B** are the `Dependencies
detected` section: every workspace `Cargo.toml` and all its dependencies,
one per line. More than 30 manifests repeat the same 20 `mct-lang-*` entries.

This diagnostic tool ("is the index fresh?") costs more than most actual
queries.

**Recommendation:** summarize by default
(`Dependencies: 34 manifests, 312 declared (18 unique external)`) and expose
details through an explicit `verbose: true` parameter or, preferably, move
them to `mct-cli --root . status --deps`, where a person would inspect them.

### A4. The index contains two complete copies of the repository

`.claude/worktrees/` contains two agent worktrees
(`agent-a91ea5ed15bdbe91b`, `agent-af80f4d6cd52dd3bb`), each containing the
entire `crates/` and `docs/` trees.

Measurements:

- `list_symbols(".claude/worktrees")` → **3,652 symbols**, **65.9%** of the
  index's 5,544.
- `find_references("reindex")` → 234 hits, **156 (67%)** of them in
  `.claude/worktrees/`.
- `find_symbol("reindex")` → **12 definitions**, where there should be about 4.

This is not only token noise; it is **accuracy noise**. Two-thirds of the
results come from entries the user cannot edit and an agent could mistake for
the real code.

`.gitignore` already excludes `.claude/` (line 19) and `.mct-index/` (line 4),
but **the indexer does not read `.gitignore` at all**, despite the project
already depending on `git2`.

**Recommendation (the highest-value item in this report):**
1. Immediate: add `**/.claude{,/**}` and `**/.claude-index{,/**}` to
   `DEFAULT_EXCLUDE_PATTERNS` (`crates/mct-index/src/exclude.rs:11-37`).
2. Structural: honor `.gitignore` through `git2` (already a dependency), with
   a `--no-gitignore` flag to disable it. A Git-ignored file is not project
   source by definition.

### A5. `limit` counts rows, not bytes

`paginate()` (`format.rs:29-34`) limits the number of items. No tool accounts
for how much context it returns. `list_symbols("crates", limit: 500)` returns
50,101 B ≈ 12,500 tokens with no warning.

**Recommendation:** add a shared byte budget in `format.rs`, applied after
`paginate()`, that truncates and reports this through the existing
`truncation_note`. One change point can serve all tools.

### A6. Pure-noise output lines

- `(no top-level symbols)` for every empty module in the overview.
- `list_symbols` alignment padding (`{:<name_width$}`) emits spaces that cost
  tokens without adding information; two fixed spaces are sufficient.

---

## 4. Block B — MCP accuracy and quality

### B1. Relations resolve by global name, causing cross-language false positives

Reproducible example in this repository:

```
find_callers("main") →
  crates/mct-lang-php/tests/fixtures/billing-app/run.php:11 [php]    run  --calls--> main
  crates/mct-lang-python/tests/fixtures/billing-app/main.py:10 [python] main --calls--> main
```

No Rust results; two results from another language's fixtures. Queries
(`crates/mct-index/src/queries.rs:140-177`) use `WHERE r.to_name = ?1` with no
file, directory, or language condition.

With common names (`new`, `run`, `get`, `parse`, `main`, `location`), this can
become unusable rather than merely imprecise, and token cost grows with the
number of false positives.

**Recommendations** (in increasing order of cost):
1. **Optional scope filters** for `find_symbol`, `find_calls`,
   `find_callers`, `find_references`, and `impact_analysis`: `path` (prefix)
   and `language`. `list_symbols` already supports them; the others do not.
   This is the cheapest accuracy improvement and **does not change the schema**;
   it adds only `AND f.relative_path LIKE ?` / `AND f.language = ?`.
2. **Proximity-based tie-breaking**: same file → same directory → same
   language → global. `ROADMAP.md` includes this in decision #1 (LSP) rather
   than as a separate candidate. That decision is still pending, so item 1
   should not wait for it.

### B2. `to_symbol_id` is written once per relation and never read

`crates/mct-index/src/indexer.rs:337-343` runs this query **for every
relation**, inside the transaction:

```sql
SELECT id FROM symbols WHERE name = ?1 LIMIT 1
```

and stores its result in `relations.to_symbol_id`. A repository-wide search
confirmed that **no query reads it**. All five queries in `queries.rs` match
by `to_name`. Therefore:

- indexing cost: one extra SQL query per relation, without `prepare_cached`;
- query benefit: none;
- `LIMIT 1` without `ORDER BY` picks an **arbitrary** same-name symbol, so
  reading it later would be incorrect.

**Recommendation:** choose one option instead of leaving it halfway implemented.
- **Use it:** resolve with deterministic tie-breaking (B1.2) and make
  `find_references`/`find_callers` prefer non-null `to_symbol_id`. This turns
  B1 from a heuristic into actual resolution.
- **Remove it:** migrate away the column and query to recover indexing speed.

Note: a code comment already acknowledges that reindexing does not backfill
this column, so the stored value currently depends on **indexing order** and
is not deterministic across machines.

### B3. FTS5 is set up but never queried

`crates/mct-index/src/schema.rs:47-62` creates `symbols_fts` (an FTS5 virtual
table over `symbols.name`) and **three triggers** (`symbols_ai`, `symbols_ad`,
`symbols_au`). Since `write_parsed_file` runs `DELETE FROM symbols WHERE
file_id = ?` followed by N `INSERT`s, every file reindex triggers a full
delete-and-insert cycle in the FTS index.

But `find_symbol` uses `WHERE s.name = ?1` — **exact matching**. No tool
supports prefix, fuzzy, or case-insensitive search.

Direct token consequence: when an agent does not know the exact name, it falls
back to `list_symbols` → read → retry, the very cost this project aims to
avoid.

**Recommendation:** use the infrastructure already being maintained. Add a
`fuzzy: true` mode (or `match: "exact" | "prefix" | "fuzzy"` parameter) to
`find_symbol` backed by `symbols_fts`. This needs one query; the infrastructure
already exists. If it will not be used, **remove the table and triggers**;
today they add write cost only.

### B4. No relation tool accepts a scope

This is B1's concrete action item, listed separately: five tools lack
`path`/`language`. For a polyglot repository — a core README use case — this is
the most visible accuracy gap.

### B5. Any directory containing a dot is treated as a file

`crates/mct-index/src/queries.rs:89` and `server.rs:329` share this heuristic:

```rust
let is_file = path.rsplit('/').next().unwrap_or(path).contains('.');
```

Thus `list_symbols(".claude")` generates `WHERE f.relative_path = '.claude'`
and returns `No symbols found under '.claude'` — **despite 3,652 symbols
inside**. The same occurs for `.github`, `.cargo`, `v1.2/`, and `my.module/`.

**Recommendation:** ask the filesystem, not the path string —
`index.root().join(path).is_dir()`, using the current heuristic only as
a fallback for deleted paths. This two-line change removes an entire class
of silent "not found" results.

### B6. `looks_like_test_name` is redundant and too broad

```rust
lower.starts_with("test_") || lower.starts_with("test")
```

The first condition is subsumed by the second. `starts_with("test")` also
matches `testimonial`, `tester`, and `testament`. It misses conventions in
most of the 16 supported languages: Rust's `#[test]`, `*_test.go`,
`*Test.java`, and JS `describe`/`it`.

**Recommendation:** also inspect the file path (`tests/`, `__tests__/`,
`*_test.*`, `*Test.*`), a more reliable signal already stored in the database.

### B7. Previously documented bugs — do not reopen here

`ISSUES_PENDING.md` covers these with `#[ignore]` tests that serve as
specifications:

1. `get_file_skeleton` / `get_project_overview` miss Go, C#, Bash, and
   PowerShell (4 of 16 languages) because they use `parent.is_none()` to
   define "top-level".
2. `.lua` files are discarded without a diagnostic.
3. Seven gaps in the Obsidian note graph.

Item #1 also has a token cost not noted there: these four languages consume
module lines in the overview without contributing any symbols.

---

## 5. Block C — SQLite and indexing cost

### C1. `WalkDir` does not prune directories: 47,000 entries per reindex

`crates/mct-index/src/indexer.rs:88` uses `WalkDir::new(&root).into_iter()`
without `filter_entry`. Exclusions are applied **to files after descending**.
In this repository, that means walking:

- `target/` → **45.220 archivos**
- `.git/` → **1.725 archivos**

Before checking exclusions, `path.canonicalize()` is called (line 97): one
syscall per entry, about 47,000 per reindex, including every entry under
`target/`.

**Correction to `ROADMAP.md` #10, hypothesis 1:** it suggested that every
non-excluded file was read in full for hashing, even when unchanged. This is
true **only for files with a registered parser**: `registry.for_extension()`
(line 136) runs **before** `std::fs::read` (line 155). The actual cost is not
reading the whole repository, but **walking and calling `canonicalize()` on
47,000 entries under `target/` and `.git/`**.

**Recommendation:** use `WalkDir::filter_entry(|e| !excluded_dir(e))` to prune
directories and move `canonicalize()` after the exclusion check. This cuts
about 98% of syscalls per reindex in this repository.

### C2. Exclusion configuration is not wired up

`ExcludeSet::new(extra_patterns)` accepts user patterns, and its docstring
promises *"lets a project widen it via configuration"*. However, the whole
codebase only calls `ExcludeSet::default()`:

- `crates/mct-cli/src/main.rs:85`
- `crates/mct-mcp-server/src/main.rs:44` y `:67`

**No configuration path exists.** A user with a monorepo cannot configure
additional exclusions without recompiling.

**Recommendation:** read `exclude = [...]` from `.mct/config.toml` or an
existing file's `[mct]` section. `toml` is already an `mct-index` dependency.
This is the same solution as §A4, viewed from another angle.

### C3. N+1 queries in multi-hop BFS

`crates/mct-index/src/traversal.rs:21-56` runs one query per **node per
level**, each through `conn.prepare()` (`queries.rs:180`) rather than
`prepare_cached()`. `find_callers(depth: 3)` on a symbol with fan-in 40 can
compile dozens of identical SQL statements.

The header comment knowingly accepts this cost ("an acceptable trade for the
repo sizes this project targets"), which is reasonable. Still,
`prepare_cached()` is a one-word change that avoids recompilation without
changing the architecture.

### C4. N+1 queries in `get_project_overview`

`server.rs:578-600`: for every truncated module, `find_callers` runs **once per
candidate symbol** to rank by fan-in, then **again for every retained symbol**
to fetch relations. With 225 modules × 8 symbols, that is over 1,800 queries,
each using `prepare()`.

**Recommendation:** use one aggregate query
(`SELECT to_name, COUNT(*) FROM relations WHERE kind='calls' GROUP BY to_name`)
for fan-in ranking: one query instead of about 900.

### C5. Pagination happens in memory, not in SQL

`find_*` fetches **all** rows and `paginate()` truncates afterward. In the
measured case, `find_references("reindex")` materializes 234 `RelationHit`
values and their strings to display 50.

This is defensible because the `234 reference(s)` header needs the true total,
but the better approach is `COUNT(*)` plus `LIMIT/OFFSET`: two cheap queries
instead of materializing every result.

### C6. SQLite indexes

Current state (`schema.rs`):

| Index | Assessment |
|---|---|
| `idx_symbols_name` | Appropriate; heavily used |
| `idx_symbols_file` | Appropriate |
| `idx_relations_to_name` | Appropriate, but incomplete |
| `idx_relations_from` | Appropriate |
| `idx_relations_kind` | **Low selectivity** — `kind` has five values; SQLite is unlikely to choose it, and it adds write cost to every insert |

`find_callers` filters by `r.to_name = ?1 AND r.kind = 'calls'`, and
`find_calls` by `caller.name = ?1 AND r.kind = 'calls'`.

**Recommendation:** replace `idx_relations_kind` with the composite
`(to_name, kind)` index, which serves both predicates and removes an
ineffective index. Compare with `EXPLAIN QUERY PLAN` before and after; without
measurement this is a hypothesis, not a fact.

### C7. SQLite connection hygiene

`Index::open` sets `journal_mode=WAL` and `foreign_keys=ON`, and nothing else.
Measured disk state at the time:

```
index.sqlite3       3,366,912 B
index.sqlite3-wal   5,038,792 B   ← the WAL is larger than the database
```

The WAL is never checkpointed. The following settings are also missing:

- `PRAGMA synchronous = NORMAL` — safe under WAL; avoids an fsync per commit.
- `PRAGMA wal_autocheckpoint` or `wal_checkpoint(TRUNCATE)` on close.
- `PRAGMA optimize` on close to recalculate query-planner statistics.
- `PRAGMA mmap_size` for reads.

The **`.claude-index/`** directory (1 MB database + 4.3 MB WAL) is also a
leftover from the `ccm` → `mct` rename. It is neither in `.gitignore` nor in
`DEFAULT_EXCLUDE_PATTERNS` (which excludes only `**/.mct-index`), so it is
walked on every reindex.

### C8. The watcher observes all of `target/`

`background.rs:48` calls `debouncer.watch(&root, RecursiveMode::Recursive)` on
the entire root, and the `ExcludeSet` filter is applied **after** receiving
events. A `cargo build` generates thousands of events under `target/` that
the debouncer processes only to discard them.

Worse, a relevant event triggers `reindex(force=false)`, which walks the
**entire** tree again (§C1). Editing one file costs a 47,000-entry walk.

**Recommendation:** register watches by subdirectory while skipping excluded
directories, or keep the recursive watch and filter prefixes before queuing
events. Longer term, incrementally reindex only notified paths instead of
walking the tree again.

---

## 6. Block D — Obsidian / Markdown

### D1. The vault is invisible to overview tools

Measured on `docs/` (18 notes):

```
get_project_overview({path:"docs"}) → 1.531 B
  docs/00-system/00-index.md:
    (no top-level symbols)
  docs/00-system/glossary.md:
    (no top-level symbols)
  ... all 18 are identical
```

**100% of the output is noise.** Two causes compound:

1. `mct-lang-md` indexes headings as `SymbolKind::Element`, which is **not in
   `OVERVIEW_KIND_ALLOWLIST`** in `server.rs`.
2. Nested headings have a `parent`, and the filter checks
   `e.parent.is_none()` — the same root cause as `ISSUES_PENDING` #1.

**Recommendation:** include `element` in the overview allowlist for Markdown
and apply the "top-level" fix from `ISSUES_PENDING` #1, which addresses both
cases.

### D2. No note symbol: the entire vault graph depends on H1 headings

This is `ISSUES_PENDING` #3 (gaps 3.4–3.7), whose analysis already identifies
the shared solution: **one symbol per note file, and resolve wikilinks against
the filename rather than the H1 text**.

The additional point here is token cost: without a note symbol,
`get_file_skeleton` and `get_project_overview` cannot summarize a note. The
only way for an agent to see a `.md` file's content is to **read the whole
file** — the cost this project aims to avoid. In a vault with hundreds of
Obsidian notes, MCP currently saves **zero** tokens.

### D3. YAML frontmatter is not indexed

`ISSUES_PENDING` 3.3: `tags: [daily, review]` in frontmatter produces no
relations, although inline `#standup` does. Since Obsidian templates use
frontmatter as the default way to add tags, a standard vault has no tag graph
at all.

### D4. Obsidian-specific proposal (after `ISSUES_PENDING` #3)

Once note symbols exist, the missing tool would be a vault equivalent of
`get_file_skeleton`:

```
get_note_context(note, depth = 1)
  → frontmatter (tags, aliases, properties)
  → heading outline with levels
  → outgoing links (links vs. embeds, resolved to paths)
  → backlinks
  → orphan notes connected within `depth` hops
```

This would answer in one call what currently requires reading N full notes.
**Do not implement before closing `ISSUES_PENDING` #3**: without note symbols
and path-based resolution, the tool would return incorrect data.

---

## 7. Suggested plan

Ordered by measured impact-to-effort ratio, not by topic area.

### Phase 1 — Immediate gains (one short session, no architecture risk)

1. Exclude `.claude/` and `.claude-index/` → **65.9% fewer indexed symbols**
   and 67% less relation-result noise. *(§A4)*
2. Default `include_relations` to `false` → **73% less output from
   `get_project_overview`** (167,864 B → 45,760 B). *(§A2)*
3. Summarize dependencies in `get_indexing_status` → **97% less output**. *(§A3)*
4. Use `is_dir()` instead of `contains('.')` for the file/directory heuristic. *(§B5)*
5. Use `WalkDir::filter_entry` and call `canonicalize()` after exclusions. *(§C1)*
6. Delete `.claude-index/` and add it to `.gitignore`. *(§C7)*

> Verification: rerun the measurements in §1 and confirm that
> `get_project_overview({})` falls below 24,000 B and
> `find_symbol("reindex")` returns 4 definitions instead of 12.

### Phase 2 — Accuracy (one session, no schema changes)

7. Add optional `path` and `language` to `find_symbol`, `find_calls`,
   `find_callers`, `find_references`, `impact_analysis`. *(§B1, §B4)*
8. Add a shared byte budget in `format.rs` and report truncation. *(§A5)*
9. Use `prepare_cached()` and one aggregate fan-in query for the overview. *(§C3, §C4)*
10. Use file paths for the test-detection heuristic. *(§B6)*

### Phase 3 — Decisions to make before implementation

11. **`to_symbol_id`: use it or remove it.** It adds cost without a consumer,
    and its value depends on indexing order. *(§B2)*
12. **FTS5: query it or remove it.** Three triggers maintain an unused table;
    querying it would provide low-cost prefix/fuzzy search. *(§B3)*
13. **Exclusion configuration** (`.mct/config.toml`) — determines how to
    determines how to resolve §A4 structurally. *(§C2)*
14. **Honor `.gitignore` with `git2`** — determines whether §A4 needs
    ongoing manual maintenance or can be handled automatically. *(§A4, §C2)*

### Phase 4 — Obsidian (depends on `ISSUES_PENDING` #3)

15. Close `ISSUES_PENDING` #3 (note symbols and path-based resolution).
16. Add `element` to the Markdown overview allowlist. *(§D1)*
17. Index YAML frontmatter. *(§D3)*
18. Only then evaluate `get_note_context`. *(§D4)*

---

## 8. What not to do

- **Do not add tools before Phase 1.** Each tool adds ~1,400 B to
  `tools/list` in **every** session (§A1). `get_note_context` is justified only
  after the vault produces correct data.
- **Do not parallelize indexing yet.** `ROADMAP.md` defers this pending
  measurement, and this report confirms that the measured bottleneck is
  walking `target/` (§C1), not parsing. Prune first, then measure again.
- **Do not partition the SQLite schema by language.** This solves none of this
  report's findings; `ROADMAP.md` deferred it for the same reason.
- **Do not add LSP type resolution to fix §B1.** Scope filters (Phase 2, item
  7) capture most of the benefit at a fraction of the cost and do not block
  roadmap decision #1 if pursued later.

---

## 9. Limitations of this research

- Token figures are `bytes / 4`, not actual tokenization. Orders of magnitude
  are reliable; exact percentages are approximate.
- All measurements are from **this** repository (663 files, two agent
  worktrees). No large monorepo was measured; `ROADMAP.md` #10 remains the
  follow-up for that.
- §C6 (SQLite indexes) is the only section based on code inspection without
  measurement. Run `EXPLAIN QUERY PLAN` before acting.
- The test suite was not run as part of this research; no product code was
  changed.
