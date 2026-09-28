# Query cache (issue #16)

`search_symbols` and `hybrid_search` keep an in-process cache of **rankings**:
which symbols a query resolved to, in which order. A repeated query that is
still valid skips the ranking work and, when this session was already sent
that exact response, is answered with one line instead of the full list.
Implementation: `crates/mct-mcp-server/src/cache.rs`; wiring in
`server.rs` (`search_symbols`, `hybrid_search`, `cached_reply`).

Priorities, in order: correctness and freshness, then token reduction, then
latency, then hit rate. A false hit is worse than a miss, so every doubt is a
miss.

## Exact vs semantic

| | Exact | Semantic |
|---|---|---|
| Tools | `search_symbols`, `hybrid_search` | `hybrid_search` with a semantic side only (`alpha > 0`, a model available, not a `"quoted"` exact phrase) |
| Lookup | SHA-256 key → `HashMap` probe | cosine similarity of the query embedding against earlier queries cached under the same key minus the query text |
| Match | identical normalised query (whitespace runs collapsed; case kept — it steers `alpha` routing) | similarity ≥ threshold (**0.95** by default) **and** best − second-best ≥ margin (**0.02**) |
| Extra cost | none | none: the query embedding is the one the search needs anyway, reused on a miss |

`search_symbols` is lexical by contract, so it never gets a semantic hit: two
paraphrases can mean the same thing and still match different names.

### The exact key

SHA-256 over length-prefixed fields (so adjacent fields can't collide):

- cache format version and the server's crate version;
- repository identity — the canonical project root;
- tool name and tool cache version (`search_symbols/1`, `hybrid_search/1`);
- normalised query;
- ranking parameters: `path`, `language`, and for `hybrid_search` the
  effective `alpha` (after clamping/auto-routing);
- search configuration: `RRF_K`, `SEMANTIC_CANDIDATES`, `EXACT_PHRASE_BOOST`,
  `EMBEDDING_TEXT_VERSION`;
- embedding model id (`fastembed/bge-small-en-v1.5`, …) or none for a lexical
  ranking.

Presentation parameters — `limit`/`top_k`, `offset`, `snippet_lines`,
`format` — are **not** in the key: a hit is rendered with the call's own, so
page 2 of a cached query reuses its ranking. The repository *state* is not in
the key either; it is validated on every lookup (below), which is what lets an
unrelated change keep an entry instead of orphaning it.

## What an entry stores

The normalised query; its L2-normalised embedding (semantic entries); the
compatibility digest (key minus query); the repository state it was computed
at; the full ranked `SymbolHit` list (path, name, kind, language, line, column,
end line, parent, level); and a `result_fingerprint`. No source text and no
rendered response.

## Validation — before any hit

1. **Compatible**: exact key equal, or for a semantic candidate the
   compatibility digest equal (same tool/version, parameters, config and
   **embedding model** — vectors of different models are never compared).
2. **Similarity** ≥ threshold (semantic only), else not even a candidate.
3. **Not ambiguous** (semantic only): best − runner-up < margin →
   `ambiguous_match`, miss. `0.973 / 0.812` is a clear candidate,
   `0.973 / 0.969` is a miss.
4. **Result still exists**: every ranked symbol is looked up again by
   `(path, name, kind, line, column)` in one query → missing →
   `stale_rejection`.
5. **Fingerprint matches**: SHA-256 over each ranked symbol's *current* row
   plus the *current* content hash of every file holding one → different →
   `fingerprint_mismatch`.
6. **Repository state**: SHA-256 over every indexed file's
   `(path, content_hash)` (memoised on SQLite's `total_changes()` +
   `PRAGMA data_version`, so an idle lookup doesn't re-read the table). Equal
   → hit. Different (something *else* changed) → the ranking is recomputed
   from the stored query and stored vector — no re-embedding — and compared
   symbol by symbol: identical → hit, entry re-stamped with the new state;
   different → `repository_mismatch`, miss.

Any error on the way (a failed query, an unavailable model) is a miss. A
refused entry is dropped.

### Why it never shows stale code

The cache holds `query → ranked symbol identities`, never source. A hit is
rendered exactly like a miss: symbol rows from the current index, snippets
read from the files on disk *now*. If a file changed on disk and the index
hasn't caught up yet, a hit shows today's source — the same thing an uncached
call shows (`tests/query_cache.rs::a_hit_never_shows_stale_source`).

The one-line reply is only used when the freshly rendered response is
**byte-identical** (SHA-256) to one already returned in this session; any
difference — new source, other page, other format — sends the full text.

## Invalidation

Granular, by referenced file and symbol:

- editing, deleting or renaming anything in a file a result points into
  invalidates that result (steps 4–5), and only results that point into it;
- a change elsewhere (step 6) keeps the entry when the recomputed ranking is
  unchanged — e.g. a function body edited in an unrelated file;
- a change elsewhere that *does* alter the ranking — a new symbol matching the
  query, or entering the semantic top-`SEMANTIC_CANDIDATES` — is caught by
  the comparison and misses.

Why not skip the recomputation for "unrelated" files: lexical ranking uses
BM25 statistics over the whole corpus, and the semantic side ranks every
symbol, so no file is provably irrelevant to a ranking. Recomputing (without
the embedding, the expensive part) and comparing is the honest check.

## Reply and token saving

| Situation | Reply |
|---|---|
| Miss | the tool's normal output, unchanged |
| Hit, response already sent this session (`reference` mode) | `cache: unchanged since your identical earlier call (cache:false resends)` or `cache: same as your earlier \`<query>\` (sim 0.990); cache:false resends` |
| Hit, not sent yet, or `full` mode | the full re-rendered output; a semantic hit adds a last line naming the query whose ranking it reuses and its similarity |

`cache: false` on the tool call bypasses the lookup and always returns the
full text — for an agent whose context no longer holds the earlier response.

## Configuration

Environment variables of `mct-mcp-server` (e.g. under `env` in `.mcp.json`):

| Variable | Default | Meaning |
|---|---|---|
| `MCT_CACHE` | `on` | `off` disables every cache path: tools behave exactly as without a cache |
| `MCT_SEMANTIC_CACHE` | `on` | `off` keeps the exact cache only |
| `MCT_CACHE_THRESHOLD` | `0.95` | cosine similarity a semantic candidate needs; clamped to `0.5..=1.0` |
| `MCT_CACHE_MARGIN` | `0.02` | minimum best − second-best similarity; below it the match is ambiguous |
| `MCT_CACHE_RESPONSE` | `reference` | `full` always re-sends full responses (latency saving only) |
| `MCT_CACHE_MAX_ENTRIES` | `256` | entries kept; least recently used evicted |
| `MCT_EMBEDDING_MODEL` | `bge-small-en-v1.5` | the model (see README); part of every semantic key |

An unparsable value is logged and replaced by its default. Per call:
`cache: false`. Programmatically: `ServerOptions { cache: CacheConfig, .. }`
with `MctServer::with_options` (`mct-eval` runs with the cache disabled, so
its timed repeats measure the tools, not the cache).

## Metrics

`MctServer::cache_stats()`, and appended to `get_indexing_status` once the
cache has answered anything:

`exact_hit`, `exact_miss`, `semantic_candidate`, `semantic_hit`,
`semantic_miss`, `stale_rejection`, `fingerprint_mismatch`,
`repository_mismatch`, `ambiguous_match`, `reference_responses`, and
count/mean/max of `lookup_latency` (whole lookup incl. validation, excl.
embedding), `embedding_latency` and `validation_latency`.

## Benchmarks

Functional tests: `crates/mct-mcp-server/tests/query_cache.rs` (22 cases,
deterministic fake embedder with per-test query vectors), unit tests in
`cache.rs`. Latency and tokens: a separate benchmark,

```sh
cargo run --release -p mct-mcp-server --example cache_benchmark [-- <root>]
cargo run --release -p mct-mcp-server --features semantic --example cache_benchmark
```

which fails if the exact p95 lookup is ≥ 10 ms or a path's reduction is
< 90%. Token reduction is `1 − cached / uncached` (`chars / 4`), measured
separately per path over valid hits only. On this repository (Apple Silicon,
release build, 2026-09-28):

| Path | Hits | Uncached → cached tokens | Reduction | Cache lookup p50 / p95 |
|---|---|---|---|---|
| Exact (`search_symbols` + lexical `hybrid_search`, 10 queries) | 20/20 | 3 628 → 360 | **90.1%** | 0.14 / 0.86 ms |
| Semantic, bag-of-words embedder (6 paraphrase pairs) | 6/6 | 1 672 → 129 | **92.3%** | 1.9 / 2.4 ms |
| Semantic, `bge-small-en-v1.5` (`--features semantic`, same 6 pairs) | 6/6 | 1 668 → 129 | **92.3%** | 2.5 / 2.5 ms (+ query embedding: mean 6.8 ms) |

With the real model a semantic hit's end-to-end call was p50 7.8 ms against
14 ms uncached — the query embedding dominates, and it is paid either way; the
uncached p95 (31.7 s) is the first call embedding the whole repository.

The saving is a fixed ~20-token line against whatever the full response
costs, so it grows with the response: ~90% for a default 10-hit page without
snippets, more with `snippet_lines`. A response already shorter than the
reference line is sent in full.

## Limitations

- **Per session, in memory.** The cache lives in the server process — one MCP
  session — and is never persisted: no schema migration, no stale state
  across restarts, but also no hits across sessions.
- **Search tools only.** The graph tools (`find_*`, `impact_analysis`,
  `build_context_pack`…) are not cached; they are single indexed queries
  already.
- **Latency gain is modest for lexical search** (already sub-millisecond on
  this repo); the win there is tokens. For `hybrid_search` an exact hit also
  skips the query embedding.
- **A reference reply assumes the agent still has the earlier response.** If
  its context was compacted, `cache: false` (or `MCT_CACHE_RESPONSE=full`)
  gets the full text.
- **Unrelated changes cost one recomputation** per surviving entry (lexical
  ranking + stored-vector semantic ranking), and on a small repository every
  file tends to be referenced by an unscoped `hybrid_search` result, so any
  edit invalidates it.
- **Semantic similarity is the model's.** The threshold and margin guard
  against loose matches, but whether two phrasings clear 0.95 is up to the
  embedding model; the cache errs toward missing.
