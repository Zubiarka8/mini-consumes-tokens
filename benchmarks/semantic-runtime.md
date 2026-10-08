# Semantic runtime audit (R03)

Status: **measured** with the real `bge-small-en-v1.5` model on this
repository. Compiled and run on `perf/semantic-r03`; the base for comparison is
`3732075`. Numbers are in [Measured](#measured).

## What was wrong

`hybrid_search` (`crates/mct-mcp-server/src/server.rs`) took the index mutex
first and held it while it:

1. loaded the embedding model (`SemanticModel::get`: ONNX session creation and,
   on first use, a model download);
2. embedded the query (ONNX inference, tens of milliseconds);
3. embedded every pending symbol (`Index::refresh_embeddings`, seconds to
   minutes after a first index or a large reindex);
4. also held the query cache's `std` mutex across step 2.

Every other tool queues on the same mutex, so a cold semantic query froze
`find_symbol`, `search_symbols`, `reindex` and the watcher for its whole
duration. Separately, `semantic_ranking` materialised a `SymbolHit` for every
embedded row, then full-sorted and truncated to 200 (`SEMANTIC_CANDIDATES`).

## What changed

| Step | Before | After |
|---|---|---|
| Model load | under the index mutex, on the async worker | no lock held, on a blocking thread that is awaited (not detached); a `OnceLock` builds it once, concurrent callers wait on the same cell, a failed load is remembered |
| Query embedding | under the index mutex and the cache mutex | no lock held, on a blocking thread that is awaited; the index mutex is re-taken afterwards and cache validation runs against the index as it is *then* |
| Pending-symbol refresh | under the index mutex, for the whole embedding | the index mutex is held only for the snapshot and for each batch's commit (one short transaction per 256-symbol batch); each batch embeds on a blocking thread with no lock |
| `semantic_ranking` | one `SymbolHit` per embedded row, full sort | bounded max-heap of `limit` entries; a row strictly worse than the worst kept is rejected before its hit is built |

Lock order, documented on `MctServer::semantic_model`: model and query
embedding first with no lock held, then the index mutex, then the cache mutex
(short, never held across an `.await` or model work). Errors: a model that is
missing or failed to load yields the lexical ranking with the reason on the
first output line (unchanged); a failed query embedding is a tool error when a
ranking would have used it (unchanged text, `hybrid_search failed: …`) and a
cache miss otherwise.

### Top-k equivalence

The heap orders by the old comparator (similarity descending via `total_cmp`,
then path, then line) plus the scan position, which is what the old *stable*
sort used for the remaining ties, so output is identical, not merely
tie-equivalent. Limit 0 returns early. Wrong-dimension blobs are skipped as
before. Pagination is unaffected: callers still receive the same ranked list
and page over the fused result.

### Snapshot / embed / commit

`Index::snapshot_pending_embeddings` → `PendingEmbeddings::embed_batch` →
`Index::commit_embedding_batch` (exported from `mct-index`). The server's
`refresh_pending_unlocked` runs them: lock → snapshot → unlock; per batch,
embed on a blocking thread with no lock, then lock → commit → unlock.
`refresh_embeddings` composes the same three steps on one connection, so the
library behaviour is unchanged.

`symbols.id` is a bare `INTEGER PRIMARY KEY` (no `AUTOINCREMENT`), so SQLite
may reuse a deleted symbol's rowid; the id alone is not an identity. The commit
therefore stores a vector only if the row still matches the snapshot on id,
name, kind, parent, line, end line, file path **and** the file's
`content_hash`. A symbol deleted, rewritten or re-hashed in between is
skipped and stays pending; nothing is resurrected. Covered by
`commit_skips_symbols_changed_since_the_snapshot` and, end to end through the
server, by `the_index_mutex_is_free_while_pending_symbols_embed`.

## Regressions added

`crates/mct-index/src/semantic.rs` (unit):
`top_k_equals_the_full_sort_for_every_limit` (ties on similarity, path, line
and scan order; zero and wrong-dimension vectors; limits 0, 1, k, > rows, a
language scope and a file scope),
`commit_skips_symbols_changed_since_the_snapshot`,
`refresh_is_incremental_and_rejects_a_foreign_model`.

`crates/mct-mcp-server/tests/semantic_audit.rs` (integration, fake embedder
whose query or symbol embedding blocks on a test-controlled gate; no sleeps):
`the_index_mutex_is_free_while_the_query_embedding_blocks` (lexical search, an
alpha-0 `hybrid_search` and a full reindex complete while a semantic query is
blocked in the model; the blocked query then ranks the new index and no vector
exists for the deleted symbol),
`the_index_mutex_is_free_while_pending_symbols_embed` (the same, but the
blocked call is the pending-symbol refresh; fails on `3732075` with
`ordinary queries or the reindex waited for the symbol embedder`),
`concurrent_semantic_queries_agree_and_do_not_deadlock`,
`lexical_only_queries_never_reach_the_model`.

## Measured

Environment: Apple M4 (10 cores), 16 GB RAM, macOS 27.0 (Darwin 27.0.0),
rustc 1.98.1, `cargo test` debug profile, `--features semantic`, model
`fastembed/bge-small-en-v1.5` (384 dimensions), corpus: this repository
(7,187 symbols on `3732075`, 7,231 on `perf/semantic-r03`: the branch adds
code). The machine was shared with other `mct-mcp-server` processes; the
1-minute load average ranged from about 9 to 154 during these runs, so
absolute times vary by roughly 2x between runs. Compare rows within one pair.

Command (the same test file runs against both trees):

```sh
cargo test --locked -p mct-mcp-server --features semantic --test semantic_audit --no-run
./target/debug/deps/semantic_audit-<hash> --ignored --nocapture real_model
```

- *cold quiet*: one first `hybrid_search` on a freshly indexed server, nothing
  else queued (load + embed every symbol + rank).
- *cold with probe*: the same cold call while one ordinary `search_symbols`
  loops; "worst" is the longest single ordinary call, "samples" how many
  completed.
- *warm*: median of 5 repeat `hybrid_search` calls on the warmed server.
- *burst*: 8 `hybrid_search` calls started together on the warmed server; wall
  is the whole burst, the range is each call's own latency.
- *peak RSS*: `/usr/bin/time -l` maximum resident set size of the whole test
  process (model, index and ONNX runtime together), not of `semantic_ranking`
  alone.

Pair B (final code, same load, measured back to back):

| Measurement | Before (`3732075`) | After (`perf/semantic-r03`) |
|---|---|---|
| Cold `hybrid_search`, quiet | 84.6 s | 83.9 s |
| Cold `hybrid_search`, with probe | 168.3 s | 87.1 s |
| Worst ordinary `search_symbols` during the cold call | 168.3 s (2 samples) | 318.6 ms (192,229 samples) |
| Warm `hybrid_search`, median of 5 | 170.6 ms | 104.1 ms |
| Burst of 8, wall (per call) | 1.51 s (0.21–1.51 s) | 0.75 s (0.10–0.75 s) |
| Peak RSS, whole test process | 2.32 GB | 2.17 GB |

Pair B's base run ended with the load average at 154, so its warm and burst
figures are the least reliable here.

Pair A (first version of this branch, measured at lower load; the refresh was
still under the index mutex, which is why the stall was still long):

| Measurement | Before (`3732075`) | After, refresh still under mutex |
|---|---|---|
| Cold `hybrid_search`, with probe | 53.2 s | 51.8 s |
| Worst ordinary `search_symbols` during the cold call | 53.2 s (2 samples) | 46.6 s (32,231 samples) |
| Warm `hybrid_search`, median of 5 | 80.9 ms | 52.3 ms |

Pair A's 46.6 s stall is the pending-symbol refresh holding the index mutex
for its whole duration: the first 5.3 s, which covers model load and query
embedding, ran at ~170 µs per ordinary query. That is why the refresh was
moved out of the lock.

### What the numbers say

- The lock fix is what the measurements justify: worst ordinary latency during
  a cold call fell from the whole call (53 s, 168 s) to 0.32 s, and the cold
  call itself is unchanged when nothing else runs (84.6 s vs 83.9 s).
- Warm queries got faster in both pairs (81 → 52 ms, 171 → 104 ms). Warm
  queries take no lock during embedding in either tree, so the gain comes from
  the top-k heap, not from the lock change. The two pairs ran at different
  loads, so treat it as indicative, not as a precise figure.
- Pair B's cold-with-probe (87 s) is about half its base (168 s), and the
  cold-quiet pair shows no change in the model path. I did not isolate why the
  probed call is that much faster; the probe's own CPU load and the machine's
  load both differ between the two runs, so this gap is not a measured effect
  of the lock change.
- Peak RSS is about 2 GB in both trees; the change does not move it.

## Limitations

- **Real-model numbers are from one machine and one corpus.** Other processes
  shared the CPU; repeat the command above on a quiet machine before quoting a
  figure.
- The worst ordinary call (318 ms) is the commit of one 256-symbol batch,
  which holds the index mutex for one short transaction. A smaller batch would
  shorten it at the cost of more transactions; not tuned.
- The probe test loops ordinary queries on a two-worker runtime, so its CPU
  competition with the ONNX embedding is part of the cold-with-probe figure.

## Reproduce

```sh
cargo test --locked -p mct-index --lib semantic
cargo test --locked -p mct-mcp-server --test semantic_audit
cargo clippy --locked -p mct-index -p mct-mcp-server --all-targets --all-features -- \
  -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic
# Real model (downloads bge-small-en-v1.5 into <root>/.mct-index/models or
# $FASTEMBED_CACHE_DIR on first use; no external API):
cargo test --locked -p mct-mcp-server --features semantic --test semantic_audit \
  -- --ignored --nocapture real_model
```
