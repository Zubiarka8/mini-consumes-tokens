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
- Peak RSS is about 2 GB in both trees; the lock change does not move it (the batch size does, see Memory).

## Memory

Measured on this repository (7,235 symbols at the time), debug test profile,
`cargo test ... --features semantic`, RSS sampled with `ps -o rss=` every
100 ms from a thread of the test process (temporary probe, not committed).
One cold `hybrid_search` on a freshly indexed in-memory index, then a 3 s
settle, then one warm query.

| Part | RSS |
|---|---|
| Empty process | ~10 MB |
| With the index | 44 MB |
| After model load (cold call start) | ~596 MB (earlier measurement, not re-sampled here) |
| Vectors, 7.2k x 384 x f32 | ~11 MB (computed, not the growth) |

Each row below is one run; the second table repeats the whole series
(same binary, run back to back). "Max" is the sampled maximum, "final" the
RSS right after the cold call.

| Embedding batch | Max RSS run 1 / run 2 | Final RSS run 1 / run 2 | Cold call run 1 / run 2 | Warm query |
|---|---|---|---|---|
| 256 (old) | 2,471 / 2,243 MB | 2,414 / 2,076 MB | 82.2 / 75.6 s | 112 / 111 ms |
| 256 (old, repeats) | 2,130 / 2,219 MB | 2,130 / 2,126 MB | 75.4 / 82.1 s | 110 / 118 ms |
| 128 | 1,484 / 1,483 MB | 1,482 / 1,481 MB | 70.7 / 69.8 s | 111 / 111 ms |
| 64 | 851 / 856 MB | 851 / 856 MB | 73.1 / 66.9 s | 111 / 111 ms |
| 32 (new) | 659 / 665 MB | 659 / 665 MB | 66.3 / 61.3 s | 109 / 111 ms |
| 32 (repeats) | 585 / 665 MB | 585 / 665 MB | 66.0 / 66.5 s | 111 / 113 ms |
| 16 | 454 MB | 454 MB | 62.7 s | 117 ms |

Other experiments (same series):

| Experiment | Max RSS | Final / settled RSS | Cold call | Reading |
|---|---|---|---|---|
| ORT CPU arena off (`ep::CPU::with_arena_allocator(false)`), batch 256 | 2,695 / 2,799 MB | 2,474 / 2,179 MB final | 78.1 / 78.6 s | not lower than arena on |
| arena off, batch 32 | 1,411 / 1,413 MB | 1,411 / 1,342 MB | 63.0 / 65.3 s | higher than arena on at batch 32 (~660 MB) |
| model loads in the process | 1 (`load` ran once per server) | | | not loaded twice |

What the numbers support:

- **Batch size is the variable.** RSS falls roughly in proportion to the
  batch (2.1-2.4 GB at 256, 1.5 GB at 128, 0.85 GB at 64, 0.66 GB at 32,
  0.45 GB at 16), the series is monotonic in both runs, and the cold call did
  not get slower (it was 61-67 s at 32 against 75-82 s at 256, but the machine
  was shared; read that as "not slower", not as a speed-up).
  fastembed pads every batch to its longest text and keeps the batch's
  per-token output until it pools it, so a larger batch means larger live
  buffers. I did not isolate which buffer (tokenizer, ONNX intermediates or
  output tensors) dominates.
- **Hypothesis "the ONNX Runtime CPU arena is not returned": not confirmed.**
  Turning the arena off did not lower RSS at batch 256 and raised it at batch
  32, so the arena is not the cause. The system allocator holding freed
  memory was not tested.
- **Hypothesis "the model is loaded or kept more than once": discarded.** One
  load per server.
- The ONNX memory-pattern option is not exposed by fastembed 7.1.0 (only
  `with_session_config` string entries and execution providers), so it was not
  tested.
- The 2.49 GB peak is the same thing as the ~2.1-2.4 GB plateau at batch 256:
  samples at the end of the embedding were 2.2-2.5 GB across runs and the
  later settle moved it by up to 0.6 GB in either direction, so the original
  "peak above plateau" gap is within run-to-run spread here.

Change: `EMBED_BATCH` in `crates/mct-index/src/semantic.rs` went from 256 to 32
(it also sets the rows per committed transaction). Guard:
`pending_embeddings_split_into_small_batches`.

Final code, full `real_model` test under `/usr/bin/time -l` (whole process,
including the second server and the 8-way burst), load average ~11:
cold quiet 62.3 s, cold with probe 73.8 s, worst ordinary call 310 ms
(240,101 samples), warm median 100.3 ms, burst of 8 wall 0.75 s, peak RSS
698 MB (Pair B above: 2.17 GB, measured at a different load, so it is
indicative only).

## Limitations

- **Real-model numbers are from one machine and one corpus.** Other processes
  shared the CPU; repeat the command above on a quiet machine before quoting a
  figure.
- The worst ordinary call (~310 ms) was attributed to the commit of one
  256-symbol batch. With 32-symbol batches it is still 310 ms, so that
  attribution does not hold; the cause was not isolated.
- Memory figures: one machine, shared CPU (load average 5-12), debug test
  profile, a single sampler at 100 ms (a shorter spike could be missed), two
  runs per point. Compare only the rows measured back to back.
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
