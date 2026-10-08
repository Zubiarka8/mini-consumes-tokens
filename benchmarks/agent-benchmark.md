# Agent benchmark: Claude Code with and without the MCP server

`token-benchmark.md` and `quality-eval.md` measure single tool responses, in characters or approximate tokens. This benchmark measures what a session really costs. Claude Code runs the same task end to end with and without `mct-mcp-server`, and the token usage and cost come from Claude Code's own accounting. The main metric is **cost per correctly completed task**: a cheap run that gets the answer wrong does not count as a saving.

Harness: `benchmarks/agent/agent_bench.py` (Python stdlib only). Tasks: `benchmarks/agent/tasks.json`.

## Scenarios

| Id | Setup |
|---|---|
| `A-no-mcp` | Built-in tools only (`Read, Grep, Glob, Edit, Write, Bash`), no MCP server |
| `B-mcp` | Same tools, plus `mct-mcp-server` over stdio. All tool schemas are loaded up front (`ENABLE_TOOL_SEARCH=false`) |
| `C-mcp-deferred-catalog` | Same as B, but Claude Code defers MCP tool schemas and loads them on demand through tool search (`ENABLE_TOOL_SEARCH=true`) |

C is the catalog optimisation that works today without changing the server, and it happens on the client side. The server has no slimmer catalog yet: the `inputSchema` trimming proposed after issue #14 is not implemented. Once it exists, build it and compare `B` runs that use `--server <slim binary>` against the default build.

## Tasks

The tasks run against this repository at the commit pinned in `tasks.json`. They cover these categories:

1. locate a function;
2. list every call site;
3. explain an implementation;
4. assess the impact of a signature change;
5. locate an injected bug without editing;
6. fix an injected bug so the tests pass.

Tasks 1–5 are graded on the final `ANSWER:` line with the regular expressions in `tasks.json`. This uses the `expect`/`absent` scoring of `mct-eval`: a wrong hit zeroes the score. Task 5 also fails if the agent edits the file. Task 6 is graded by running `cargo test -p mct-core --lib`, and fails if the `#[cfg(test)]` block was edited.

The ground truth for tasks 2 and 4 comes from the compiler, not from the MCP server. Each method was marked `#[deprecated]` in a scratch copy, and every warning from `cargo check --workspace --all-targets` was recorded. This matters for interpreting results: at this commit, `find_callers for_extension` returns 3 of the 5 real call sites. It misses `crates/mct-cli/src/main.rs:731` and `:732`, which are inside `assert_eq!` arguments (see [Calls inside macros](#calls-inside-macros)). Task 2 can therefore show the MCP server misleading the agent.

## Calls inside macros

tree-sitter does not parse a macro's arguments: `assert_eq!(cli.for_extension(e), ..)` reaches the Rust extractor as a flat `token_tree`. The extractor (`crates/mct-lang-rust/src/lib.rs`, the `token_tree` and `identifier` arms) handles that token stream on purpose, and narrowly:

- A free function of the **same file** used inside a macro (`params![helper(x)]`) is recorded as `References`, never `Calls`, because a token stream cannot tell a call from a value use.
- An identifier right after `.` or `::` is skipped, so a **method call** inside a macro leaves no relation at all.
- A function from **another file** inside a macro leaves no relation at all, as for any foreign name in value position.

So this is a deliberate extractor limitation forced by the parser, not an index bug: the index stores what the extractor gives it. Effect on the tools at commit `c06ab5c`:

| Symbol | Real sites | `find_callers` | `find_references` | `impact_analysis` |
|---|---|---|---|---|
| `LanguageRegistry::for_extension` (method) | 5 calls | 3 | 3 | 3 callers, 3 refs |
| `prose_literal` (same-file free fn) | 2 plain calls + 12 inside test macros + 1 re-export | 2 | 15 | 2 callers, 15 refs, **0 likely tests** |

Calls inside standard macros (`assert!`, `assert_eq!`, `format!`, `vec!`, `println!`) are real calls and should appear. Consequences for an agent: `find_callers` and `impact_analysis` under-report call sites and affected tests, and nothing in the output says results may be incomplete, so an agent that trusts them gives an incomplete answer (task 2) or underestimates a change's blast radius. `find_dead_code` can also flag a method used only inside macros.

Regression test: `a_method_call_inside_a_macro_token_tree_leaves_a_relation` in `crates/mct-lang-rust/tests/parse.rs`, `#[ignore]`d until fixed (`cargo test -p mct-lang-rust --test parse -- --include-ignored` reproduces it).

Recommended fix, not implemented: inside a `token_tree`, treat `identifier` + `(`-delimited `token_tree` as a call candidate, including after `.` (method) and `::` (path), and record it as `References` with the method name. Stricter: re-parse the token tree as an expression list for a fixed allow-list of standard macros and emit real `Calls`. Impact: higher recall for `find_references`/`impact_analysis` and test detection; some false positives inside DSL macros (`quote!`, `html!`) with the heuristic, none with the allow-list; a few more relation rows in the index. Keep `find_callers` strict unless the allow-list version is chosen.

## Isolation

- **Fresh snapshot per run.** Every run gets its own `git archive` of the pinned commit in a new temporary directory, so the snapshot has no index, no `target/` and no edits left by an earlier run. The MCP server builds its index from scratch inside that snapshot.
- **No agent instruction files.** `CLAUDE.md`, `AGENTS.md`, `rules.md`, `.claude/`, `.codex/`, `.agents/` and `.mcp.json` are removed from the snapshot. Scenario A is therefore not told to use tools it does not have, and none of the repository's own hooks run.
- **No user configuration.** Runs use `--setting-sources project`, `--strict-mcp-config` and `--no-session-persistence`, so user settings, plugins, hooks, `~/.claude/CLAUDE.md` and other MCP servers stay out of the comparison.
- **Contamination is verified, not assumed.** The `system/init` event of each run is recorded, and a run is flagged as contaminated when:
  - it lists an unexpected MCP server or a plugin that is not built into Claude Code;
  - a C run lacks `ToolSearch`, or an A/B run has it;
  - a B/C run's `mct` server is not connected;
  - an A run has `mct` tools.
- **Same conditions everywhere.** Model, effort, `--max-turns` and the prompt are identical across scenarios. The order of scenarios rotates per task and repetition.
- **Shared on purpose: Anthropic's prompt cache.** It is account-wide and cannot be isolated: a run can read a prefix cached by an earlier run. Rotating the order spreads this effect across scenarios. The report shows cache writes and cache reads separately, and also *total input*, which is the sum of input, cache-write and cache-read tokens and does not depend on cache hits.
- **Shared on purpose: the Cargo registry cache.** It is shared in `~/.cargo`, and builds run with `--offline`. Each snapshot compiles into its own `target/`.

## Metrics

All metrics come from Claude Code's `stream-json` output. No metric is estimated from bytes.

| Metric | Source |
|---|---|
| Input, cache-write, cache-read and output tokens | `usage` field of the `result` event |
| Cost (USD) | `total_cost_usd`. This is Claude Code's estimate, and it is notional on a subscription (see `apiKeySource` in `run.json`) |
| Turns, duration | `num_turns`, `duration_ms` |
| Tool calls, by name and `mct` calls | `tool_use` blocks |
| Errors | `tool_result` blocks with `is_error`, plus `permission_denials` |
| Retries | `system/api_retry` events |
| Success, score | The task's grader |

The report gives mean ± sample standard deviation per scenario and per task, the success rate, and the **cost per successful task**: total cost divided by the number of successes. It also lists every B/C run in which the MCP server was connected but never called.

## Running it

Prerequisites: an authenticated `claude` CLI and a release build of the server from the commit under test.

```sh
cargo build --release -p mct-mcp-server --locked
python3 benchmarks/agent/agent_bench.py selftest   # graders and setup; no model calls
python3 benchmarks/agent/agent_bench.py run --dry-run   # prints every claude command; no model calls
```

The following commands consume model usage. Do not run them without authorization.

```sh
# Pilot: 6 tasks x 3 scenarios x 1 repetition = 18 runs
python3 benchmarks/agent/agent_bench.py run --reps 1 --max-budget-usd 1.0
# Full run: 5 repetitions = 90 runs
python3 benchmarks/agent/agent_bench.py run --reps 5 --max-budget-usd 1.0
python3 benchmarks/agent/agent_bench.py report target/agent-bench/<timestamp>
```

`--max-budget-usd` is Claude Code's per-run spending cap, so the hard ceiling of a run set is runs × cap. Use `--task` and `--scenario` (both repeatable) to run a subset. Output goes to `target/agent-bench/<timestamp>/<scenario>/<task>/rep<n>/`: `stream.jsonl` holds the full transcript and `run.json` holds the metrics and the grading.

## Results

### Pilot, 2026-10-08: one task, one run per scenario

`run --task find-call-sites --reps 1 --max-budget-usd 1.0`, Claude Code 2.1.294, `claude-sonnet-5-5`, effort `medium`. **One run per scenario gives no statistical evidence**; it only validates the harness.

| Scenario | Input | Cache write | Cache read | Total input | First request input | Output | Cost estimate (USD) | API requests | Tool calls | Time (s) | Result |
|---|---|---|---|---|---|---|---|---|---|---|---|
| A-no-mcp | 8 | 5,373 | 28,751 | 34,132 | 7,457 | 844 | 0.0357 | 4 | 3 (Grep, Bash denied, Read) | 16.0 | 5/5 |
| B-mcp | 6 | 19,582 | 36,581 | 56,169 | 17,768 | 597 | 0.0916 | 3 | 3 (find_callers, Grep, Bash) | 7.5 | 5/5 |
| C (invalid, ran as B) | 6 | 5,103 | 50,971 | 56,080 | 17,767 | 745 | 0.0381 | 3 | 3 (find_callers, Grep, Read) | 7.4 | 5/5 |
| C, rerun after the fix | 6 | 5,218 | 23,946 | 29,170 | 9,009 | 649 | 0.0322 | 3 | 2 (Grep, Bash) | 8.3 | 5/5 |

What the pilot showed:

- **Scenario C did not defer anything.** `--tools` drops the built-in `ToolSearch` tool unless it is listed, and without it Claude Code loads every MCP schema up front. C's first request was the same size as B's. Fixed: C now adds `ToolSearch`, and a C run without it is flagged as contaminated. In the rerun, deferral worked: the first request fell to 9.0k (1.5k above A). That run never loaded or called an `mct` tool, so it measures only the cost of an idle deferred catalog, not the MCP's benefit.
- **The MCP catalog costs about 10.3k input tokens per API request** (first request 17.8k with the server against 7.5k without). B made one request fewer than A, which did not make up for it: +65% total input.
- **The MCP answer was incomplete and the agent knew to check it.** `find_callers` returned 3 of 5 sites in B and C; both runs also ran `Grep` in the same turn and answered 5/5. On this task the MCP call added tokens and no accuracy.
- **The cost estimate follows the prompt cache, not the scenario.** C reused 14.4k tokens of prompt prefix that B had written seconds before, so its cost was 58% lower than B's for the same token total. Compare total input and output tokens; read cost only across many rotated runs.
- **A lost a turn to the harness.** A compound `cd ...; sed ...` Bash command was denied by the allow-list. `Bash(cd:*)` is now allowed.
- **Isolation held.** No MCP server other than `mct` in B/C and none in A; `apiKeySource: none`; no hooks; the three plugins loaded were Claude Code's built-in ones, the same in every scenario (no longer flagged). Each run left an empty auto-memory folder under `~/.claude/projects/`; they were removed.
- **Billing guard.** The harness refuses to start unless the logged-in account reports extra usage off, strips API-key variables from each run's environment, and stops at the first run whose `rate_limit_event` reports overage use or whose `init` reports an API key. No pilot run tripped it.

## Known limits

- **Small task set.** There are six tasks on one Rust repository, so the results do not generalise to every repository or language.
- **Few repetitions.** Five repetitions only show variability; they do not give tight confidence intervals. Treat a difference as real only when it is larger than the spread of both scenarios and holds across most tasks.
- **Cost depends on run order.** The prompt cache is account-wide, so a run's cost estimate depends on what ran just before it. Token totals do not.
- **Keyword grading is a floor, not proof.** Task 3 now asks for `quantity operator threshold` conditions joined by `AND`/`OR` and rejects `OR`, but it still cannot check that the conditions apply per token; read its recorded answers. Path tasks reject any listed `.rs` path that no expected pattern accounts for, so an over-inclusive list fails.
- **Answer-line grading.** Grading reads only the `ANSWER:` line. A run that is correct but badly formatted counts as a failure, and that cost is real.
- **Task 6 test check.** The grader only checks that the test block is unchanged. It does not stop the agent from weakening the library in some other way that still passes those tests.
