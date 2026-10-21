# Adaptive detail selection: extended evaluation and offline simulation

Figures are heuristic tokens (`EstimatedCounter`, and `chars/4` where shown),
over `text` output. Neither is a tokenizer; billed tokens are not measured; the
calibration against the model's `count_tokens` is still pending (no key).
Latency is in-process, the median of three runs, and noisy.

Reproduce: `cargo run -p mct-mcp-server --example adaptive_eval --release`
and `--example catalog_report --release`; validity is asserted by
`cargo test -p mct-mcp-server --test extended_eval`.

## 1. Tool catalog (22 tools)

"Before" is the live catalog with the `detail` / `max_tokens` properties
removed from three tools and the three `PRE` sentences put back to `HEAD`, so
other edits of the working tree are excluded.

| | chars/4 | EstimatedCounter | mct-eval count |
|---|---|---|---|
| whole catalog | 8367 → 8671 (+304) | 13944 → 14467 (+523, +3.7%) | 10994 → 11410 (+416) |

- All of it is in six JSON-schema properties on three tools (`search_symbols_like`,
  `hybrid_search_like`, `build_context_pack`): `detail` +61..85, `max_tokens`
  +95..96 each (EstimatedCounter). Descriptions grew by 0: `PRE` is not part of
  the catalog description (`WHEN | NOT | TAGS`).
- `get_tool_schema` (on demand) returns `PRE`: +25..26 per tool.
- No other tool changed. `mct-eval`'s `baseline.json` says 11143; the live
  "before" is 11017; the 126 gap is other working-tree changes, not this one.
- Transmitted per flow: a client that loads every schema pays the +523 once per
  session whatever it does; a client that loads schemas on demand pays +159
  (pack) or +182 (search) only in flows that call those tools, and nothing for
  `find_callers` and the other 19 tools.
- Arithmetic on measured figures: a pack task that needs only locations drops
  from 1775 to 478 tokens (-1297), which pays the +159 schema in the first use.
  A plain search that keeps default arguments saves nothing and pays +182: the
  schema growth is pure cost for callers that never send the new arguments.
- The properties were not shortened: the level descriptions are what lets a
  caller choose a level. Removing them would trade information for a number.

## 2. Fixed cost of a budgeted page

Measured on `search_symbols_like` / overview: header 16 + footer 21 = 37
estimated tokens per page, rows about 20 each. With `max_tokens=90` a page
holds two rows (about half useful); at 150, five rows. This is why following
`next` with a tiny budget cost 574 against 410 tokens in the 12-case set:
overhead per page, not a defect of the default path. Do not set a search
budget below about 150.

## 3. Extended set

57 cases over a generated 33-file corpus (Rust, ADRs, guides, notes with
WikiLinks): exact symbols 8, ambiguous 6, references/dependencies 8,
small/large functions 6, Markdown sections 6, ADR/docs 6, mixed 5, no evidence 4,
numerous results 4, budgets too small 4. Evidence is a `path:line` computed
from the generated text or an exact corpus string; a case passes only if all
of it is present and nothing forbidden is. 29 Dev / 28 Eval, alternating
inside each category in the order written.

Disclosures:
- Three mixed cases were relabelled after the first run because the query did
  not contain the words of the expected name (`logging` is not in `log_event`;
  `cache` is not a prefix of `caching`; the search does not stem). They were
  unfair for every strategy alike.
- The harness obeys the answer's own notices (`next: offset=`, `pass a higher
  limit`, `raise source_lines`) for every strategy, the unchanged tools included.
- The auto rule (section 4) was written before the first run and not changed
  afterwards, so Dev/Eval is a consistency check, not a tuned-then-held-out test.
- The corpus is synthetic; 57 cases is not a statistical sample.

All tasks (the unchanged tools cannot take a budget, so they pay for the four
budget cases):

| strategy | tasks | satisfied | silent misses | mean recall | calls | calls/task | ~tokens (Est) | ~tokens/satisfied | ~tokens (chars/4) | p50 µs | p95 µs |
|---|---|---|---|---|---|---|---|---|---|---|---|
| default | 57 | 56 | 0 | 1.00 | 61 | 1.07 | 16579 | 296 | 11781 | 183 | 5352 |
| detailed, direct | 57 | 57 | 0 | 1.00 | 72 | 1.26 | 20213 | 354 | 14417 | 800 | 5887 |
| summary | 57 | 44 | 0 | 0.78 | 71 | 1.25 | 12908 | 293 | 9037 | 692 | 5720 |
| overview, escalate if missing | 57 | 57 | 0 | 1.00 | 84 | 1.47 | 14472 | 253 | 10388 | 692 | 9443 |
| detailed + budget, follow next | 57 | 53 | 0 | 0.94 | 75 | 1.32 | 12217 | 230 | 8670 | 1504 | 6524 |
| auto (query-only rules) | 57 | 57 | 0 | 1.00 | 78 | 1.37 | 14077 | 246 | 10103 | 683 | 10303 |
| intent rules (caller knows its intent) | 57 | 57 | 0 | 1.00 | 71 | 1.25 | 12509 | 219 | 8932 | 680 | 5564 |

Tasks without a budget of their own (53):

| strategy | tasks | satisfied | silent misses | mean recall | calls | calls/task | ~tokens (Est) | ~tokens/satisfied | ~tokens (chars/4) | p50 µs | p95 µs |
|---|---|---|---|---|---|---|---|---|---|---|---|
| default | 53 | 53 | 0 | 1.00 | 55 | 1.04 | 13517 | 255 | 9582 | 132 | 5352 |
| detailed, direct | 53 | 53 | 0 | 1.00 | 55 | 1.04 | 18445 | 348 | 13184 | 630 | 5221 |
| summary | 53 | 40 | 0 | 0.76 | 54 | 1.02 | 11144 | 278 | 7809 | 611 | 5675 |
| overview, escalate if missing | 53 | 53 | 0 | 1.00 | 68 | 1.28 | 12861 | 242 | 9253 | 684 | 9443 |
| detailed + budget, follow next | 53 | 49 | 0 | 0.93 | 58 | 1.09 | 10449 | 213 | 7437 | 1462 | 6105 |
| auto (query-only rules) | 53 | 53 | 0 | 1.00 | 62 | 1.17 | 12466 | 235 | 8968 | 605 | 10303 |
| intent rules (caller knows its intent) | 53 | 53 | 0 | 1.00 | 55 | 1.04 | 10898 | 205 | 7797 | 595 | 5410 |

Per category, satisfied / tasks (estimated tokens):

| category | default | detailed, direct | summary | overview, escalate if missing | detailed + budget, follow next | auto (query-only rules) | intent rules (caller knows its intent) |
|---|---|---|---|---|---|---|---|
| ADR and documentation | 6/6 (827) | 6/6 (1496) | 5/6 (980) | 6/6 (902) | 6/6 (1453) | 6/6 (822) | 6/6 (822) |
| ambiguous symbols | 6/6 (1450) | 6/6 (2369) | 4/6 (1478) | 6/6 (1576) | 6/6 (1522) | 6/6 (1576) | 6/6 (1264) |
| exact symbols | 8/8 (1292) | 8/8 (1756) | 8/8 (1062) | 8/8 (686) | 8/8 (1172) | 8/8 (686) | 8/8 (686) |
| insufficient budget | 3/4 (3062) | 4/4 (1768) | 4/4 (1764) | 4/4 (1611) | 4/4 (1768) | 4/4 (1611) | 4/4 (1611) |
| markdown sections | 6/6 (610) | 6/6 (668) | 1/6 (550) | 6/6 (1017) | 6/6 (722) | 6/6 (610) | 6/6 (610) |
| mixed code and documentation | 5/5 (400) | 5/5 (1425) | 5/5 (641) | 5/5 (400) | 5/5 (879) | 5/5 (400) | 5/5 (400) |
| no evidence | 4/4 (103) | 4/4 (103) | 4/4 (103) | 4/4 (103) | 4/4 (103) | 4/4 (103) | 4/4 (103) |
| numerous results | 4/4 (1025) | 4/4 (2792) | 4/4 (1515) | 4/4 (845) | 4/4 (1131) | 4/4 (845) | 4/4 (845) |
| references and dependencies | 8/8 (3422) | 8/8 (3422) | 8/8 (2848) | 8/8 (1780) | 7/8 (2006) | 8/8 (1872) | 8/8 (1780) |
| small and large functions | 6/6 (4388) | 6/6 (4414) | 1/6 (1967) | 6/6 (5552) | 3/6 (1461) | 6/6 (5552) | 6/6 (4388) |

Dev and Eval separately:

| strategy | tasks | satisfied | silent misses | mean recall | calls | calls/task | ~tokens (Est) | ~tokens/satisfied | ~tokens (chars/4) | p50 µs | p95 µs |
|---|---|---|---|---|---|---|---|---|---|---|---|
| default | 29 | 28 | 0 | 1.00 | 32 | 1.10 | 10086 | 360 | 7194 | 236 | 10411 |
| detailed, direct | 29 | 29 | 0 | 1.00 | 34 | 1.17 | 11692 | 403 | 8361 | 800 | 13775 |
| summary | 29 | 21 | 0 | 0.74 | 33 | 1.14 | 7152 | 340 | 5025 | 692 | 5696 |
| overview, escalate if missing | 29 | 29 | 0 | 1.00 | 41 | 1.41 | 8801 | 303 | 6336 | 684 | 13714 |
| detailed + budget, follow next | 29 | 26 | 0 | 0.91 | 37 | 1.28 | 6303 | 242 | 4474 | 1541 | 7263 |
| auto (query-only rules) | 29 | 29 | 0 | 1.00 | 38 | 1.31 | 8604 | 296 | 6195 | 683 | 14096 |
| intent rules (caller knows its intent) | 29 | 29 | 0 | 1.00 | 33 | 1.14 | 7447 | 256 | 5331 | 680 | 10364 |

| strategy | tasks | satisfied | silent misses | mean recall | calls | calls/task | ~tokens (Est) | ~tokens/satisfied | ~tokens (chars/4) | p50 µs | p95 µs |
|---|---|---|---|---|---|---|---|---|---|---|---|
| default | 28 | 28 | 0 | 1.00 | 29 | 1.04 | 6493 | 231 | 4587 | 132 | 5113 |
| detailed, direct | 28 | 28 | 0 | 1.00 | 38 | 1.36 | 8521 | 304 | 6056 | 1126 | 5887 |
| summary | 28 | 23 | 0 | 0.82 | 38 | 1.36 | 5756 | 250 | 4012 | 1545 | 6305 |
| overview, escalate if missing | 28 | 28 | 0 | 1.00 | 43 | 1.54 | 5671 | 202 | 4052 | 1540 | 6483 |
| detailed + budget, follow next | 28 | 27 | 0 | 0.96 | 38 | 1.36 | 5914 | 219 | 4196 | 1504 | 6524 |
| auto (query-only rules) | 28 | 28 | 0 | 1.00 | 40 | 1.43 | 5473 | 195 | 3908 | 1518 | 6358 |
| intent rules (caller knows its intent) | 28 | 28 | 0 | 1.00 | 38 | 1.36 | 5062 | 180 | 3601 | 1132 | 5245 |

Where every unsatisfied case is signalled, none is silent (0 silent misses in
all strategies). Not satisfied: `summary` on the 13 tasks that need source or
section text; `detailed + budget` on four packs whose budget (350) is below
what the evidence costs (a pack has no `next: offset`, only the priced levels);
the unchanged tools on `budget-below-minimum` (they cannot honor a budget).

## 4. Offline adaptive simulation

Strategies: unchanged tools; `detailed` direct; `summary`; `overview` then
`detailed` if the evidence is missing; `detailed` inside a budget following
`next`; **auto**, a rule using only what a server sees (tool, query text, path:
`.md` pack -> `detailed`, everything else -> `overview`, then `detailed` if
missing); **intent**, the level chosen from what the task wants, which only the
caller knows.

Pack tasks (no budget), break-even share of source-needing packs above which a
strategy costs more than the unchanged tool (this set has 52%):

| strategy | saved per location/relations pack | extra per source/section pack | break-even share of source-needing packs | share in this set |
|---|---|---|---|---|
| overview, escalate if missing | 203 | +151 | 57% | 52% |
| auto (query-only rules) | 195 | +114 | 63% | 52% |
| intent rules (caller knows its intent) | 203 | +0 | never loses | 52% |

Reading (tasks without a budget, tokens vs the unchanged tools, 13517):

- Starting at `overview` and escalating: 12861 (-4.9%), 0.24 more calls/task.
- Auto rule: 12466 (-7.8%), 0.13 more calls/task, p95 about twice (extra call).
  Its own contribution over plain "overview, then escalate" is 395 tokens (3%
  of the set); the rest comes from starting at `overview`, which a caller can
  already do.
- Intent: 10898 (-19%), no extra calls: the saving is real when the caller
  knows what it wants.
- Auto's break-even (63%) is close to this set's mix (52%): the gain depends on
  how many tasks need source, which no query-only rule can see.

## 5. Decision

**Server-side adaptive selection is not integrated.** The default behavior is
unchanged, as required, and the measured benefit does not justify a new
`detail` value: a query-only rule adds about 3% over what callers get today by
starting at `overview`; its gain is fragile to the task mix; it doubles p95 per
task through extra calls; and the larger gain (-19%) needs the caller's intent,
which a server cannot infer. The guidance in README, ADR-016 and
`benchmarks/context-budget.md` is what the data supports: start at `overview`
for where/who-calls, ask once for `detailed` when you need source or section
text. The criterion for revisiting: a rule that beats "overview, then
escalate" by a margin larger than the task-mix sensitivity above, on a corpus
that is not generated.
