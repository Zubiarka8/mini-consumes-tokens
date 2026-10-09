# Intent extraction and tool preselection (issue #19)

## Evaluation set and scoring

Fixed before any extraction rule or threshold was written:
`crates/mct-mcp-server/tests/fixtures/intent_eval.toon`, 42 held-out queries.
Development examples (the unit tests in `crates/mct-mcp-server/src/intent.rs`)
are a separate set; rules and thresholds are tuned on those only.

Each case lists `need`: required groups separated by `;`, each group a set of
acceptable tools separated by `|`. An empty `need` means no reliable
selection exists. `forbid` lists tools that must not be delivered (negated
requests such as "don't reindex").

Per selector, over the same queries and the same catalog:

| Metric | Definition |
|---|---|
| top-1 | Cases with a non-empty `need` whose first selected tool belongs to some group. Not defined for an unordered selection (the full catalog). |
| recall | Groups with at least one acceptable tool in the delivered set, over all groups. |
| precision | Mean over cases with a non-empty `need` of (delivered tools that belong to some group) / (delivered tools). |
| no-selection correct | Cases with an empty `need` where the selector fell back to the full catalog instead of guessing. |
| forbidden delivered | Cases where a `forbid` tool is in the delivered set. |
| payload tokens | Query + rendered intent + JSON of the delivered tools as `tools/list` serializes them. Approximate (see below). |
| latency | Mean preprocessing time per query. |

Delivering a tool only puts its schema in front of the model; it never
executes it or grants anything.

## Results

Base commit 5956a76 (`origin/main`), catalog of 18 tools from
`MctServer::tool_catalog()`, release build, Apple Silicon (macOS), one run.
Reproduce:

```sh
cargo test --release -p mct-mcp-server --test intent_eval -- --nocapture
# with the embedding comparison (bge-small-en-v1.5, local, no network once cached;
# a fresh worktree needs FASTEMBED_CACHE_DIR pointing at an existing model cache):
cargo test --release -p mct-mcp-server --features semantic --test intent_eval -- --include-ignored --nocapture
```

```
selectors[4]{selector,top1,recall,precision,no_selection_ok,forbidden_delivered,fallbacks,approx_tokens,bytes,ms_per_query}:
  full_catalog,n/a,1.000,0.081,5/5,2,42,330865,1012123,0.001
  rules,0.971,1.000,0.938,4/5,0,6,74884,229707,0.010
  rules+embeddings,0.944,0.976,0.935,2/5,0,3,53251,163618,1.484
  embedding_top3,0.297,0.634,0.234,0/5,1,0,62080,188929,12.994
```

- `full_catalog` is today's flow: every tool schema with every request.
- `rules` (the default): payload approx. tokens −77.4 % (330,865 → 74,884
  over the 42 queries), precision 0.081 → 0.938, recall unchanged at 1.000,
  top-1 33/34, no negated tool delivered. Fallback on 6/42 queries.
- `rules+embeddings` (floor 0.60, set on development queries only): fewer
  tokens only because it guesses on requests that have no reliable answer
  (3/5 wrong), and it lost one required tool (e41). **Not recommended**; the
  fallback stays opt-in (pass `ToolVectors` to `preselect`). Embedding the
  catalog costs ~270 ms once, then ~1.5 ms per query.
- `embedding_top3` approximates issue #11's proposed design (top-3 tools by
  query/description similarity) with the same model: top-1 0.297, recall
  0.634. Description similarity alone does not identify the needed tool here.

Held-out misses of `rules`: e04 ("list everything X calls" — `list` ranks
`list_symbols` before `find_calls`; both are delivered) and e33 ("Write a poem
about Rust" — the capitalized language name is read as a symbol name, so the
lookup alternatives are offered instead of the full catalog). They are left
unfixed so the figures above stay a held-out measurement.

## Limitations

- Token counts are approximate (`approx_tokens`: a word run or a punctuation
  character is one token); no model tokenizer is a workspace dependency.
- Selection accuracy is measured on the delivered tool set, not on a model's
  actual tool choice: no model is called.
- Rules are English-only. The same author wrote the development examples and
  the evaluation set; the set was committed before the rules, but shared
  phrasing habits remain possible.
- Not integrated into a request path: the server never receives the user's
  query before the model. The consumer is issue #11 (Tool Attention), which
  needs a query-carrying entry point — a new or changed MCP tool signature,
  which rules.md treats as a cross-cutting change.
