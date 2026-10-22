# Context budget on real repositories

Question: do `detail` and `max_tokens` give a reproducible improvement on real
repositories and on an Obsidian vault, and what do they cost?

Everything below is **estimated** tokens. `chars/4` and `EstimatedCounter` are
heuristics, never a tokenizer, and they are reported side by side, never
mixed. No Anthropic key was available, so `examples/token_calibration.rs`
(official `count_tokens`) was **not run**: there are no measured tokens and no
billed tokens in this document. Latency is wall-clock in-process, warm, median
of 5.

## Corpus (pinned)

| repo | language | commit | files indexed | symbols | index time |
|---|---|---|---|---|---|
| [pallets/click](https://github.com/pallets/click) | Python | `2247b35e` | 136 | 2522 | 472 ms |
| [spf13/cobra](https://github.com/spf13/cobra) | Go | `adbc8813` | 54 | 845 | 184 ms |
| [BurntSushi/ripgrep](https://github.com/BurntSushi/ripgrep) | Rust | `3fce3b5b` | 136 | 3915 | 906 ms |
| [obsidianmd/obsidian-help](https://github.com/obsidianmd/obsidian-help) `en/` | Markdown, 176 of 179 notes with WikiLinks | `cd6c9f3c` | 235 | 2129 | 546 ms |

Three languages and three sizes (cobra smallest, ripgrep largest code base),
plus a real Obsidian vault. The indexed root is the checkout (for the vault,
its `en/` folder).

## Reproduce

```sh
mkdir -p "$REPOS" && cd "$REPOS"   # any empty directory
git clone --depth 1 https://github.com/pallets/click
git clone --depth 1 https://github.com/spf13/cobra
git clone --depth 1 https://github.com/BurntSushi/ripgrep
git clone --depth 1 https://github.com/obsidianmd/obsidian-help
# then check each HEAD equals the commit in benchmarks/real-repos/tasks.json
cargo run -p mct-mcp-server --example real_repos --release -- \
    "$REPOS" benchmarks/real-repos/tasks.json 5
cargo run -p mct-mcp-server --example catalog_report --release
```

Token counts, evidence and the number of calls are deterministic (four runs
were compared over all 112 task×strategy rows: identical). Only latency
varies: the mean of per-task medians moved by ~2% between three consecutive
runs (e.g. `ladder` 2298–2350 µs).

## Method

16 tasks (4 per repo) in `benchmarks/real-repos/tasks.json`. Evidence strings
were chosen with `grep` over the checkouts, **not** from MCT output. A task is
*satisfied* when every evidence string is in the answers and no forbidden one
is (ripgrep has several `matcher`; the wrong one is forbidden). A miss is
*silent* if no answer carries a truncation or omission marker (`Budget:`,
`omitted`, `No row fits`, `none fit`, `more line(s)`).

Kinds: locate (search, hybrid), pack (callers across files, a test caller, an
ambiguous symbol), Markdown section, WikiLink neighbourhood.

Strategies (same tools, same query; only `detail` / `max_tokens` change):

| strategy | meaning |
|---|---|
| default | arguments omitted: the pre-existing behaviour |
| overview / summary / detailed | `detail` only, no budget |
| detailed+budget | `detail=detailed`, `max_tokens` 1500 (packs) / 1600 (search) |
| overview+budget (follow next) | `overview`, `max_tokens=400`, follow `next: offset=` for searches |
| ladder | simulated agent policy: `overview`, then `summary`, then `detailed`, stopping at the first level whose answer holds the evidence |

`ladder` is a policy simulation with fixed arguments over labelled evidence;
it is **not** an adaptive server feature and none was added.

## Results

### Totals over the 16 tasks

| strategy | satisfied | silent misses | calls | ~tok chars/4 | ~tok Estimated | evidence recall |
|---|---|---|---|---|---|---|
| default | 16/16 | 0 | 16 | 7228 | 10367 | 1.00 |
| overview | 15/16 | 0 | 16 | 2820 (−61%) | 4290 (−59%) | 0.96 |
| summary | 16/16 | 0 | 16 | 5065 (−30%) | 7907 (−24%) | 1.00 |
| detailed | 16/16 | 0 | 16 | 9876 (+37%) | 14335 (+38%) | 1.00 |
| detailed+budget | 16/16 | 0 | 16 | 9445 (+31%) | 13599 (+31%) | 1.00 |
| overview+budget (follow next) | 14/16 | 0 | 16 | 2510 (−65%) | 3821 (−63%) | 0.92 |
| ladder | 16/16 | 0 | 17 | 3216 (−55%) | 4904 (−53%) | 1.00 |

### By kind (EstimatedCounter, summed over the tasks of the kind)

| kind (tasks) | default | overview | summary | detailed | ladder |
|---|---|---|---|---|---|
| locate (6) | 1166 | 1156 | 1851 | 4219 | 1156 |
| markdown (3) | 653 | 497 | 637 | 1568 | 497 |
| pack (6) | 7326 | 1952 | 4645 | 7326 | 2566 (7 calls) |
| wikilink (1) | 1222 | 685 | 774 | 1222 | 685 |

### Per-task misses (all signalled, none silent)

| task | strategy | evidence | what was missing |
|---|---|---|---|
| click-pack-resolve | overview | 1/2 | the caller in `shell_completion.py` (overview lists fewer relations) |
| click-pack-resolve | overview+budget | 1/2 | same |
| cobra-pack-test-caller | overview+budget | 1/2 | `command_test.go` caller dropped by the 400-token budget |

`ladder` recovered `click-pack-resolve` with one extra call: 845 estimated
tokens in 2 calls against 1015 for the default in 1 call.

## What the data says

- **Where it pays**: context packs. Default is 7326 estimated tokens for six
  packs; `overview` 1952 (−73%) with one signalled miss; the `ladder` policy
  2566 (−65%) with full recall and one extra call in total. ripgrep's
  `search_parallel` pack goes from 2286 to 313 estimated tokens (−86%) with the
  labelled evidence intact.
- **Where it does not**: *locate* tasks. Default search pages are already
  ~100–300 tokens; `overview` saves 10 tokens in total, `summary` costs +59%,
  `detailed` +262%. For a location task keep the defaults.
- **Obsidian**: Markdown sections and WikiLink neighbourhoods behave like code
  (−24% for sections, −44% for the WikiLink pack via `overview`), and the
  `[[Advanced formatting syntax]]` link target stayed visible in `overview`.
  Only one WikiLink task and three Markdown tasks: an indication, not a
  statistic.
- **Budget alone does not substitute for the ladder**: `overview+budget` with
  `max_tokens=400` is the cheapest strategy but loses 2 of 16 tasks (signalled).
- **No silent evidence loss** in 16 tasks × 7 strategies = 112 measurements.
  Every miss carries `Budget:` / `omitted`.
- **Latency**: warm, sub-millisecond for searches (35–1400 µs), 1.4–7.7 ms for
  packs. `detail=overview` is not faster than default: the per-call work is the
  same selection plus a footer (about 0.2 ms of overhead on searches, noise
  level on packs). It is a token saver, not a latency saver.

## Catalog cost

`catalog_report` (22 tools): the three touched tools grew, nothing else.

| | chars/4 | EstimatedCounter | mct-eval count |
|---|---|---|---|
| whole catalog | 8367 → 8671 (+304, +3.6%) | 13944 → 14467 (+523, +3.8%) | 10994 → 11410 (+416) |
| one touched tool (a client that loads schemas on demand) | +90…+107 | +159…+182 | +126…+145 |

A client that loads every schema up front pays +523 estimated tokens once per
session. Break-even is about **0.7 pack tasks per session**
(`523 / ((7326 − 2566) / 6)` with the ladder). A session made only of *locate*
tasks never recovers it. For on-demand schema loading the cost is ~+180 on the
one tool used.

## Compact instructions for Claude Code (proposal, not wired anywhere)

Evidence-based: the only task kind that saves is the context pack, and the
ladder is the policy that kept recall at 1.00. Paste into the project
`CLAUDE.md` if wanted:

```text
MCT detail levels (`detail`, `max_tokens` = ESTIMATED budget, not real tokens):
- Locating a symbol/heading/note: call search_symbols_like with defaults. No `detail`.
- Understanding or editing one symbol: build_context_pack with detail=overview
  first; call again with detail=detailed only if you need a body, or a caller/test
  the overview omitted. Sure you need the body: one call, detail=detailed, max_tokens=1500.
- Many equal hits (ADRs, headings): detail=overview + max_tokens=400, follow `next: offset=N`.
- A `Budget:` footer or "omitted" means PARTIAL: never conclude something is absent
  from a partial answer; follow `next:` or escalate.
- Markdown / Obsidian: same as code; overview shows sections and [[WikiLinks]], not text.
```

## Limitations (do not read the table as more than it is)

- 16 tasks, 4 repositories, one run per measurement, heuristic token counts.
  Recall here is "labelled evidence strings are in the answers"; several labels
  are file paths, which a header already contains, so only the tasks with a
  cross-file or test caller discriminate (`click-pack-*`, `cobra-pack-*`).
- Tasks target symbols picked by one person; no random sampling.
- A real model could behave differently (it may not escalate, or may escalate
  on its own); the `ladder` is a rule, not an observed agent.
- No end-to-end conversation tokens, no memory measurement, no measured or
  billed tokens.
- Only Python, Go, Rust and Markdown were evaluated; the product indexes
  more languages, which this benchmark does not cover.
- No server-side adaptive policy was integrated: the data does not justify one
  (ladder needs the agent's knowledge of what evidence it is missing).

## Known risks

- Linux CI has not run for this branch (it is not pushed); the watcher
  regression test was validated on macOS and in a local Linux container only.
- No measured or billed tokens: the estimator error against a model tokenizer
  is unknown (`examples/token_calibration.rs` needs an API key).
- The catalog grows by ~3.8% (estimated) for every client that loads all
  schemas, including sessions that never use `detail`.
- `overview` can omit a caller or test that `detailed` shows; the omission is
  signalled, so an agent that ignores the `Budget:` footer can still conclude
  wrongly. The instructions above exist for that reason.
