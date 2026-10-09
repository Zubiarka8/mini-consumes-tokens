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
