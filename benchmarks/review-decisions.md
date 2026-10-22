# Review decisions: overhead benchmark

Measures what shared review decisions cost, in parts that are never added
together:

- **query response**: a default `find_dead_code` call (latency, bytes,
  estimated tokens, and a SHA-256 proving the bytes are unchanged);
- **catalog**: the `find_dead_code` tool entry and the whole `tools/list`
  catalog, paid once per session by clients that load every schema;
- **review data**: the scan, the review keys, reading the store files, parsing
  and resolving them;
- **memory**: peak resident memory of one load, in its own process;
- **CLI**: `duplicates` with the cache and with `--reviewed`.

Design: `docs/superpowers/specs/2026-10-22-shared-review-decisions-design.md`
(§12); the decision is recorded in ADR-019.

## Methodology

- Release build; one machine; the dataset is an untracked copy of the
  repository at the base commit (`e68aec9`), so no developer index is used.
- `review_overhead` runs 5 untimed warmup calls, then 50 timed ones; the
  table reports p50 and p95. Figures quoted as **medians of three separate
  process runs**, with the observed range.
- Peak memory: `/usr/bin/time -l` on a process that only loads a prepared
  store (`--load`), so the file generation is not counted.
- Tokens are the project's `EstimatedCounter`, not a model tokenizer.
- Latency is in-process (no stdio transport). No CPU isolation: p95 includes
  scheduler noise, and one of three query runs was about 25% slower than the
  other two. Gates use p50 medians.

```sh
B=<scratch directory>
git -C <checkout> archive e68aec9 | tar -x -C "$B"
cargo build --release -p mct-cli --bin mct-cli
cargo build --release -p mct-mcp-server --example review_overhead
target/release/mct-cli --root "$B" init
target/release/examples/review_overhead --root "$B" --iterations 50
# memory, one store per process
target/release/examples/review_overhead --root "$B" --prepare c3 100000 <dir>
/usr/bin/time -l target/release/examples/review_overhead --root "$B" --load <dir>
```

## Environment

Apple M4, macOS 27.0, rustc 1.98.1. Dataset: 926 indexed files, 202
dead-code candidates.

## Baseline (P0, base `e68aec9`, one run)

| format | p50 µs | p95 µs | bytes | EstimatedCounter | SHA-256 |
|---|---|---|---|---|---|
| toon | 39204 | 42604 | 4387 | 1714 | `92c7c8d17a5ae9405ec56672dd5e76b96c0ba16a16f3ce80d61d152c52380386` |
| text | 38278 | 41227 | 2508 | 749 | `dcb1bf9f878a417c31371f850147ebafd34046768c673180e7bd58f38e77b0a6` |

| catalog entry | bytes | EstimatedCounter | SHA-256 |
|---|---|---|---|
| find_dead_code | 1334 | 551 | `acc66c0689abd225b08b658db5eed24b2c3a4d9d97177df48686705dcd80cb4c` |
| all 22 tools | 34769 | 14490 | `39ca32ff5b81e57304112ffda824803d5571268e043b21d77cff3a76f50529e3` |

## Current build (`bc3133e`, median of three runs)

### Query response and catalog

| format | p50 median µs | runs µs | bytes | SHA-256 |
|---|---|---|---|---|
| toon | 41533 | 39834, 51692, 41533 | 4387 | identical to baseline |
| text | 39549 | 39101, 52995, 39549 | 2508 | identical to baseline |

The catalog is unchanged for the default call (same SHA-256 for
`find_dead_code` and for all 22 tools). The projected `reviewed` property
adds 1334 → 1576 bytes and 551 → 645 estimated tokens (+94, +17%), which is
+0.65% of the catalog. It is projected, not published.

### Review data

Scan (already run by `find_dead_code`): median 40.7 ms (40.2–41.8).

Review keys for all 202 candidates: median 3.1 ms (2.8–4.0), of which the
file-hash query is 0.4 ms and the identity pass 2.4 ms.

The identity pass was 3.5 ms before commit `e3c7c85`. Its `digest` formatted
every byte with `format!`, allocating 32 strings per key; a lookup table gives
the same bytes. The A2 golden keys in the web tests still match.

Store load, median of three runs (`read` is reading the files without parsing;
`load` includes it; `resolve` is `Reviews::resolve` on the 202 candidates):

| format | rows | file bytes | read µs | load µs | load runs µs | resolve µs | hidden |
|---|---|---|---|---|---|---|---|
| c3 | 0 | 124 | 22 | 98 | 96–134 | 77 | 0 |
| c3 | 1,000 | 342630 | 43 | 996 | 931–1267 | 161 | 100 |
| c3 | 10,000 | 3423802 | 203 | 8703 | 8639–15113 | 618 | 202 |
| c3 | 100,000 | 33554388 | 4945 | 101254 | 99817–152485 | 5341 | 202 |
| a2 | 0 | 80 | 23 | 98 | 95–102 | 77 | 0 |
| a2 | 1,000 | 186586 | 31 | 667 | 666–679 | 145 | 100 |
| a2 | 10,000 | 1863758 | 99 | 6605 | 6538–6796 | 548 | 202 |
| a2 | 100,000 | 18633760 | 1815 | 97462 | 89395–118785 | 4620 | 202 |
| b2 | 0 | 123 | 22 | 96 | 96–97 | 77 | 0 |
| b2 | 1,000 | 320229 | 36 | 837 | 821–856 | 159 | 100 |
| b2 | 10,000 | 3200333 | 162 | 10267 | 9581–10337 | 635 | 202 |
| b2 | 100,000 | 24000331 | 3337 | 121719 | 111141–128810 | 2785 | 202 |
| all | 0 | 327 | 58 | 139 | 139–143 | 77 | 0 |
| all | 1,000 | 849445 | 97 | 2260 | 2227–2294 | 289 | 100 |
| all | 10,000 | 8487893 | 472 | 25914 | 25513–28608 | 1663 | 202 |
| all | 100,000 | 76188479 | 10773 | 314300 | 297230–328931 | 13352 | 202 |

Where the time goes (100,000 rows, `c3`): reading the file is about 5 ms of
the 101 ms load; parsing and building the map is the rest. A sampling profile
of repeated loads (`sample`) showed no single hot function: the time is in the
parse loop itself (validation, one `String` per key, and `BTreeMap` inserts
that compare 64-character keys). Binary 32-byte keys would remove allocations
and shorten those comparisons, but this profile does not prove the gain, so
they are not implemented. The memory cost is more concrete: reading the whole
file into a buffer costs +33.8 MB by itself (measured separately in a Python
process), about a third of the 98 MB peak. Streaming the file instead of
buffering it is the first candidate to test.

### Peak memory (one load and resolve, separate process, three runs each)

| store | peak RSS | delta vs no store |
|---|---|---|
| no review files | 23.1 MB | — |
| `c3`, 100,000 rows | 121.5 MB | +98.4 MB |
| all three formats, 100,000 rows | 255.1 MB | +232.0 MB |

### CLI: `duplicates` with and without `--reviewed`

Seven runs each on the same project, after one warm-up run:

| mode | median ms | range ms |
|---|---|---|
| cached (default) | 130 | 114–155 |
| `--reviewed hide` (cache bypassed) | 903 | 867–915 |

The bypass costs about 0.77 s per call on this 926-file repository. It is kept
on purpose: a cached duplicate report can hide a decision made since it was
written, and no cache key is defined yet (see the design, §7.11).

## Against the gates (§12)

| gate | measured | result |
|---|---|---|
| `reviewed` omitted: identical bytes | SHA-256 identical for both formats and the catalog | met |
| `reviewed` omitted: p50 within 2% of baseline | medians +5.9% (toon) and +3.3% (text); one run in three was about 25% slower | **not shown**; needs a quiet re-measure |
| 1,000 rows: at most +5 ms | keys 3.1 + load 1.0 + resolve 0.16 ≈ +4.2 ms | met |
| 100,000 rows (`c3`): at most +50 ms | load 101 + resolve 5.3 ≈ +106 ms | **missed** (about 2.1 times the gate) |
| 100,000 rows: at most +64 MiB peak | `c3` +98.4 MB ≈ +93.9 MiB | **missed** |
| 100,000 rows, all formats | +328 ms | not gated; reported |
| false suppression after edits and reindex | covered by unit tests and the live tests; not benchmarked | see the test plan |

The gates are not relaxed. The 100,000-row misses have two candidate fixes,
neither implemented: streaming the store file (about −34 MB peak), and binary
keys or a parse cache for the long-running MCP server.

Streaming is not implemented, for two measured reasons. Reading the file is
5 ms of the 101 ms load, so streaming cannot close the latency gap (+106 ms
against +50 ms). The parser works on the whole text (`parse_canonical(&str)`),
so streaming means a different line source for every format: a large change
for a gain that, on the arithmetic above, could bring memory from 98.4 MB to
about 64.6 MB (61.6 MiB), an estimate that has not been measured. The
latency gate is the blocker, and it stays open.

### Locking and temporary files

Loading is untouched, so the load and default-query figures above still hold.
Save latency was not measured after this change; a save now adds one lock
call and one directory scan. The crash test still reports 150 kills
with the lock file left behind in 149 of them, and no kill blocks the next
save (`crates/mct-index/tests/review_crash.rs`).

## Corrections to earlier sections

The earlier P1 and P2/P4 figures came from different runs, one of them without
warm-up, and were not reproducible together. They are superseded by the table
above:

- P1 said `c3` at 100,000 rows cost about +112 ms, and all formats +357 ms; the
  table above gives +106 ms and +328 ms for the same operations.
- P1 said the 1,000-row cost was +5.1 ms, which was before commit `e3c7c85`;
  the current figure is +4.2 ms.
- The P2/P4 section gave `c3` at 100,000 rows as 98,535 µs of load, from one
  run; the medians above replace it.
