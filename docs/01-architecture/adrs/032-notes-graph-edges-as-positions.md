# ADR-032: Notes graph edges as node positions

## Status
Accepted (2026-11-08).

## Context
- `GET api/notes-graph` sent each edge as `{"from": <note path>, "to": <note path>}`. Note paths of a generated vault are long (a directory map is `directories/<name>_<64-hex hash>.md`), so on a 20,204-note vault edges were 68 % of the default response (3,157,297 bytes for 5,000 nodes) and of `all=1` (15,611,258 bytes).
- The only consumer is the dashboard's notes page, served by the same binary as the API. `graph.js` draws an edge from `{from, to}` node ids; it does not read the API directly.

## Decision
- Each edge is `[from, to]`: the positions of its two notes in that response's `nodes` array.
- Node fields, `all=1`, `total`, `truncated` and `edgeKinds` are unchanged.
- The notes page turns the pairs back into `{from, to}` ids before calling `graphView`, so the graph component and the dependency graph keep their format.

## Consequences
- 20,204-note vault, release, n=5: the default response is 1,074,656 bytes (was 3,157,297) and is served in 18.6 ms p50 (was 27.6); `all=1` is 4,547,828 bytes (was 15,611,258), 40.7 ms (was 86.3).
- An edge means nothing without the `nodes` array of the same response. A client written against the old format reads no edges; there is no such client in this repository.

## Alternatives rejected
- Dropping node fields the graph does not read (`tags`) or can derive (`orphan`): about 4 % smaller, far from the 2 MB target.
- gzip `Content-Encoding`: 724 KB on the wire with no format change, but a new dependency and per-request compression, and the browser still parses 3.1 MB.

## Verification
- `crates/mct-cli/src/web/tests.rs`: `notes_graph_edges_are_pairs_of_node_positions`.
- `crates/mct-cli/tests/dashboard/interactions.test.mjs`: the Obsidian graph draws edges sent as positions in the node list.
