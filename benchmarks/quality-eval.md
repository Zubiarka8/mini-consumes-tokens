# Quality evaluation (`mct-eval`)

Continuous quality evaluation of the MCP tools (issue #21): a fixed suite of tool calls, scored on every PR against a checked-in baseline and re-run monthly as a report.

- **Suite:** `crates/mct-eval/suite.json` — one case per tool call, run in-process against the versioned `crates/mct-mcp-server/tests/fixtures/omni-app` polyglot fixture through the same dispatcher `batch` uses, so each call returns exactly what an MCP client would get. Every read-only tool is covered (definitions, callers, callees, references, impact, search, structure, dead code, `batch`).
- **Baseline:** `crates/mct-eval/baseline.json` — each case's accuracy, reciprocal rank and tokens, plus the tool catalog's tokens.
- **Run it:** `cargo run -p mct-eval` (add `--verbose` for every tool output, `--json`/`--markdown <path>` to save the report).

## Metrics

| Metric | How it is measured | Gate |
|---|---|---|
| Accuracy | Fraction of a case's `expect` strings found in the output; 0 if any `absent` string (a wrong hit) shows up | Any drop below the baseline, per case |
| MRR | For `ranked` cases, 1 / rank of `expect[0]` among the output's `path:line` hit lines | Any drop, per case |
| Success rate | The call returned a non-error result | Every case must succeed |
| Tokens | The JSON-RPC request + response as it crosses the transport, same approximate tokenizer as `mct-mcp-server/tests/batch.rs`; also the `tools/list` catalog every session pays for | More than +10% (+8 tokens slack) per case or for the catalog |
| Latency | p50/p95 over `--iterations` timed calls per case, after one warm-up; also the fixture's index build time | Per-case p95 under an absolute budget (500 ms, `--latency-budget-ms`) |

Accuracy, MRR, success and tokens are deterministic on a fixture, so they are gated tightly. Latency depends on the machine and build profile (the PR gate runs a debug build on three OSes), so it is only gated by an absolute budget and otherwise reported for trend.

## Where it runs

- **Every PR (early regression detection):** `crates/mct-eval/tests/regression.rs` is part of `cargo test --workspace` in `ci.yml`'s `build-test` job. A regression fails the job, and the message names each case that regressed.
- **Monthly report:** `.github/workflows/quality-report.yml` runs on the 1st of every month (or on demand). It re-runs the suite in a release build with 20 iterations, then publishes the Markdown report as the job summary and the JSON + Markdown as an artifact kept for 90 days. The JSON includes every tool's output, so a drift can be diagnosed from the artifact alone. The job fails if `main` has drifted from the baseline.

## Changing the baseline

A change that moves a number on purpose refreshes the baseline in the same PR:

```sh
cargo run -p mct-eval -- --write-baseline
```

Examples: a new case, a better ranking, or a deliberate change to an output format. The regression gate also lists improvements, meaning cases that score better or cost fewer tokens than the baseline, as a prompt to refresh it and lock the gain in.

## Known gap it recorded

`exact-case name among many partial matches` has a reciprocal rank of 0.5. For the query `Ledger`, `search_symbols` ranks the Go `module ledger` (a case-insensitive exact match) above `struct Ledger` (the exact-case match). The case is kept at its current value so a fix shows up as an improvement.
