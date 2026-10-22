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
