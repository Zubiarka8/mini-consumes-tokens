# Context budget benchmark

Three kinds of figures, never mixed:

- **Estimated tokens** — `EstimatedCounter` (the budget's own heuristic) and
  `chars/4` (the repository's historical heuristic). Both are heuristics,
  disagree with each other (the first is about 1.4× the second here) and
  neither is a tokenizer.
- **Measured by a model tokenizer** — *not measured*. No Claude tokenizer is
  published; `examples/token_calibration.rs` compares the estimate with the
  official `count_tokens` endpoint, but needs `ANTHROPIC_API_KEY` and was not
  run (no key). Its plumbing was checked against a local stub only; the stub's
  numbers are meaningless and are not reported.
- **Billed tokens** — never measured.

Latency is a single run and noisy. Not measured at all: tool-catalog tokens
attributable to this change (the `mct-eval` total moved from ~11143 to ~11433
with the `hybrid_search_like` arguments and descriptions; not isolated),
whole-conversation tokens, memory, and agent behaviour with a real model.

## 1. Same scenarios, before and after the stabilization

`cargo run -p mct-mcp-server --example budget_benchmark --release -- <repo-root>`
(over a temporary copy of `crates/mct-index/src` and `docs/01-architecture/adrs`,
TOON default). "Before" is the code of the first implementation, re-run on
2026-10-21 immediately before the changes (identical to the first run).
`EstimatedCounter` tokens:

| task | mode | before | after | evidence before → after |
|---|---|---|---|---|
| locate a function | default | 140 | 140 | yes → yes |
| locate a function | overview | 166 | 140 | yes → yes |
| context pack | default | 1775 | 1775 | yes → yes |
| context pack | detailed | 1805 | 1775 | yes → yes |
| context pack | summary | 890 | 881 | yes → yes |
| context pack | overview | 483 | 478 | yes → yes |
| context pack | detailed, max_tokens=600 | 504 | 489 | yes → yes |
| ADR section | default | 1056 | 1056 | yes → yes |
| ADR section | summary | 94 | 86 | yes → yes |
| ADR section | overview | 88 | 84 | yes → yes |
| find a decision | default (snippet_lines=8) | 1767 | 1767 | yes → yes |
| find a decision | overview, max_tokens=300, one call | 282 | 266 | **NO → NO** |
| escalate | one default call | 1775 | 1775 | yes → yes |
| escalate | overview then detailed (2 calls) | 2288 | 2253 | yes → yes |

Added after the first run (no "before" row exists):

| task | mode | calls | ~tokens (EstimatedCounter) | evidence |
|---|---|---|---|---|
| find a decision | overview, max_tokens=300, follow `next: offset=` | 2 | 416 | yes |
| escalate | `detailed`, max_tokens=1200, one call | 1 | 1184 | yes |

Reading it:

- The `locate a function` regression is gone (now identical to the default).
  Cause: a footer on a page that lost nothing.
- `find a decision` at `max_tokens=300` **still misses the evidence in one
  call** and that is by design: three or more ADRs share the heading, the
  labelled one ranks last and four rows fit. What changed is that the answer now
  says `partial, 4 of 6 seen; next: offset=4`, and following it finds the
  evidence in 2 calls for 416 estimated tokens, against 1767 for the default
  call.
- `overview → detailed` is still more expensive than one call (2253 vs 1775,
  +27%): the second call re-sends what the first sent. It is not removed, it
  is priced (`next: detailed~N` in the footer) and avoided by asking once
  (`detailed` with a budget: 1184 for the same evidence).
- Before the leftover-budget pass, `detailed` with `max_tokens=1200` would have
  returned about 490 tokens (the budget between "whole source" and "signature"
  was stranded); it now uses 1184 of 1200.

## 2. Labelled retrieval cases

`cargo run -p mct-mcp-server --example retrieval_eval --release`; asserted by
`cargo test -p mct-mcp-server --test retrieval_quality`. A generated fixture
(4 Rust files, a test, 3 ADRs, a note with WikiLinks) and 12 cases with
labelled evidence: function location (search and hybrid), ambiguous symbol
(search and pack, with a forbidden wrong definition), references, dependencies,
a case that needs source, a Markdown section, an ADR lookup among identical
headings, mixed code + docs, a WikiLink dependency, and a query with no
evidence. A case passes only if **every** labelled evidence is in the answers
and no forbidden one is. A *silent miss* is missing evidence in answers that
carry no marker (`Budget:`, `omitted`, `No row fits`, …).

| strategy | cases | satisfied | mean recall | silent misses | calls | ~tokens (EstimatedCounter) | ~tokens (chars/4) |
|---|---|---|---|---|---|---|---|
| default (unchanged tools) | 12 | 12 | 1.00 | 0 | 12 | 1723 | 1212 |
| overview | 12 | 10 | 0.88 | 0 | 12 | 1267 | 888 |
| summary | 12 | 10 | 0.88 | 0 | 12 | 1751 | 1219 |
| detailed + budget (one call) | 12 | 12 | 1.00 | 0 | 12 | 2079 | 1469 |
| overview + small budget, follow `next` (searches only) | 6 | 6 | 1.00 | 0 | 9 | 574 | 380 |
| overview then detailed (packs only) | 6 | 6 | 1.00 | 0 | 12 | 2170 | 1552 |

Per-case rows (with precision of search rows, signalled/silent flags) are
printed by the example.

What it shows, and what it does not:

- `overview` is cheaper (−26%) and loses exactly the two cases whose evidence is
  source text (`needs-the-body`, `markdown-section`); both are signalled, and
  escalating recovers them. It is not a free saving.
- `summary` is **not** cheaper than the default on this set (1751 vs 1723):
  default search pages carry no snippets, `summary` adds a line per hit.
- `detailed + budget` costs more than the default because it returns more
  (snippets) and a footer; it is the strategy that keeps recall at 1.00 with a
  bounded size, not one that saves.
- Following `next` on the six search cases costs 574 estimated tokens in 9
  calls, against 410 in 6 calls for the unchanged tools on the same cases: on
  this small fixture the default pages are already tiny, so the budgeted path
  is **more** expensive here. It reached all evidence, and every strategy had 0
  silent misses; the no-evidence case says so in all of them. The saving seen
  in section 1 (416 vs 1767) comes from the large default page of the real ADR
  corpus, not from this set.
- Limits: 12 cases on a tiny generated fixture, one run, heuristic token
  counts. Recall@K here is "labelled evidence is in the answers" (K = the page
  or budget); it is not a statistically meaningful benchmark and says nothing
  about large repositories. Calls inside macro arguments are not indexed, which
  the fixture avoids on purpose.

## 3. Later work

Catalog growth, the fixed cost of a budgeted page (37 estimated tokens), the
57-case extended set and the adaptive-selection simulation are in
`benchmarks/adaptive-detail.md`.

Real repositories and an Obsidian vault: `benchmarks/real-repos.md`.
