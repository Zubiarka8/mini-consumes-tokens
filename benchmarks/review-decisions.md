# Review decisions: overhead benchmark

Measures what shared review decisions cost, kept in separate parts that are
never added together:

- **query response**: a default `find_dead_code` call (latency, bytes,
  estimated tokens, and a SHA-256 that proves the bytes are unchanged);
- **catalog**: the `find_dead_code` tool entry and the whole `tools/list`
  catalog, paid once per session by clients that load every schema;
- **review data**: reading and resolving review files (added with the
  domain module).

Design: `docs/superpowers/specs/2026-10-22-shared-review-decisions-design.md`
(§12).

## How to reproduce

```sh
B=<scratch directory>
git -C <checkout> archive e68aec9 | tar -x -C "$B"
cargo build --release -p mct-cli --bin mct-cli
cargo build --release -p mct-mcp-server --example review_overhead
target/release/mct-cli --root "$B" init
target/release/examples/review_overhead --root "$B" --iterations 50
```

The dataset is an untracked copy of the repository at the base commit, so no
developer index is used. Tokens are the project's `EstimatedCounter`
heuristic, not a model tokenizer. Latency is wall time of the in-process tool
call (no stdio transport), on one machine, without isolation from other load.

## Baseline (P0, base `e68aec9`)

Environment: Apple M4, macOS 27.0, rustc 1.98.1, release build.
Dataset: 926 indexed files, 202 dead-code candidates; 50 iterations.

### Query response: default `find_dead_code`

| format | p50 µs | p95 µs | bytes | EstimatedCounter | SHA-256 |
|---|---|---|---|---|---|
| toon | 39204 | 42604 | 4387 | 1714 | `92c7c8d17a5ae9405ec56672dd5e76b96c0ba16a16f3ce80d61d152c52380386` |
| text | 38278 | 41227 | 2508 | 749 | `dcb1bf9f878a417c31371f850147ebafd34046768c673180e7bd58f38e77b0a6` |

### Catalog

| entry | bytes | EstimatedCounter | SHA-256 |
|---|---|---|---|
| find_dead_code | 1334 | 551 | `acc66c0689abd225b08b658db5eed24b2c3a4d9d97177df48686705dcd80cb4c` |
| all 22 tools | 34769 | 14490 | `39ca32ff5b81e57304112ffda824803d5571268e043b21d77cff3a76f50529e3` |

The `scripts/unix/check.sh` run on the same base reported the `mct-eval`
catalog at ~11433 tokens; that is `mct-eval`'s own word/punctuation count,
a different heuristic from `EstimatedCounter`.

Limitations: one machine and one dataset; no CPU isolation, so p95 includes
scheduler noise. The gates in the design compare p50 values.

## After P1 (shared review domain, no consumer wired)

Same environment and dataset; 20 iterations.

### Query response and catalog: unchanged

Both response SHA-256 values and both catalog SHA-256 values are identical to
the baseline; p50 38756–40213 µs against 38278–39204 µs (run noise).

### Projected catalog cost of D3 (not published)

| entry | bytes | EstimatedCounter |
|---|---|---|
| `find_dead_code` today | 1334 | 551 |
| with an optional `reviewed` property | 1576 | 645 (+94, +17%) |
| whole catalog today | 34769 | 14490 (the property adds 0.65%) |

The listed description holds WHEN/NOT/TAGS only, so the `tools.ttc`
PRE/EFFECTS text reaches clients through `get_tool_schema`, not `tools/list`.

### Review data

Dead-code scan (already part of every `find_dead_code` call): p50 38755 µs.
Review keys on top of it for all 202 candidates (file hashes + identities):
p50 4032 µs, p95 4136 µs.

Synthetic stores: `rows` current rows and as many history rows (cut to the
32 MiB bound for `C3`, 50,000 for `B2`), about 10% matching real candidates.

| format | rows | file bytes | load p50 µs | load p95 µs | resolve p50 µs | hidden |
|---|---|---|---|---|---|---|
| c3 | 0 | 124 | 95 | 107 | 85 | 0 |
| c3 | 1000 | 342630 | 881 | 918 | 160 | 100 |
| c3 | 10000 | 3423802 | 8623 | 10670 | 633 | 202 |
| c3 | 100000 | 33554388 | 103050 | 111589 | 4891 | 202 |
| a2 | 0 | 80 | 95 | 107 | 79 | 0 |
| a2 | 1000 | 186586 | 599 | 623 | 145 | 100 |
| a2 | 10000 | 1863758 | 5741 | 5825 | 558 | 202 |
| a2 | 100000 | 18633760 | 83033 | 90597 | 4286 | 202 |
| b2 | 0 | 123 | 95 | 109 | 79 | 0 |
| b2 | 1000 | 320229 | 808 | 847 | 157 | 100 |
| b2 | 10000 | 3200333 | 8541 | 9212 | 618 | 202 |
| b2 | 100000 | 24000331 | 133257 | 210386 | 2577 | 202 |
| all | 0 | 327 | 135 | 147 | 80 | 0 |
| all | 1000 | 849445 | 2174 | 2240 | 291 | 100 |
| all | 10000 | 8487893 | 27841 | 32263 | 1867 | 202 |
| all | 100000 | 76188479 | 341260 | 421856 | 12231 | 202 |

Peak RSS of one load + resolve, in a process that did not generate the files
(`/usr/bin/time -l`): no review files 23.2 MB; `C3` at 100,000 rows 121.6 MB
(+98 MB); all three formats at 100,000 rows 255.1 MB (+232 MB).

### Against the gates (§12)

| gate | measured | result |
|---|---|---|
| `reviewed` omitted: identical bytes, p50 within 2% | identical SHA-256; p50 within run noise | met |
| 1,000 rows: ≤ +5 ms or ≤ +10% p50 | keys 4.0 + load 0.9 + resolve 0.2 ≈ +5.1 ms (+13%) | missed by ~0.1 ms |
| 100,000 rows: ≤ +50 ms p50 | `C3` ≈ +112 ms; all three ≈ +357 ms | missed |
| 100,000 rows: ≤ +64 MiB RSS | `C3` +98 MB; all three +232 MB | missed |
| false suppression after edits and reindex | measured in P2/P4 with the consumers | pending |

The 1,000-row figure is the MCP/CLI cost of one call with `reviewed` set.
The 100,000-row misses come from parsing: about 1 µs and 0.5 KB per row,
mostly 64-character hex keys held as heap strings. Options, not yet applied:
binary 32-byte keys, and the per-file parse cache that §7.9 reserves for a
long-running MCP server. Capacity finding: at about 170 bytes per `C3` row,
the 32 MiB file bound, not the 100,000-row bound, limits a store to roughly
190,000 rows in total; a save beyond it is refused, nothing is dropped.

## After P2 and P4 (web and CLI wired; MCP unchanged)

Same environment and dataset; 50 iterations, machine otherwise idle.

| format | p50 µs | p95 µs | bytes | EstimatedCounter | SHA-256 |
|---|---|---|---|---|---|
| toon | 39421 | 47903 | 4387 | 1714 | `92c7c8d17a5ae9405ec56672dd5e76b96c0ba16a16f3ce80d61d152c52380386` |
| text | 39256 | 41149 | 2508 | 749 | `dcb1bf9f878a417c31371f850147ebafd34046768c673180e7bd58f38e77b0a6` |

Response and catalog SHA-256 values are identical to the baseline. p50 is
+0.6% (TOON) and +2.6% (text) against the baseline run; across the five runs
made on this machine the default p50 ranged from 38.3 to 40.2 ms, so the
difference is within run-to-run noise, and the default `find_dead_code` path
runs the same code as at the base (`find_dead_code_candidates` keeps its
filter; only its predicate moved into a shared function).

Review keys: p50 3989 µs. `C3` load at 1,000 rows 881 µs, at 100,000 rows
98535 µs; all three formats at 100,000 rows 293871 µs. The gate results of
the P1 section are unchanged: 1,000 rows ≈ +5.0 ms (at the 5 ms gate),
100,000 rows and memory still miss.
