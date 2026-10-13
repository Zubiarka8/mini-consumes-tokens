# Checkpoint: depgraph-core (issue #28)

Status: done. `Index::dependency_graph` implemented in `crates/mct-index/src/dependency_graph.rs`; tests in `crates/mct-index/tests/dependency_graph.rs`.

- Module = indexed file; directory prefix = union of its files, reported as the prefix (trailing `/` trimmed).
- Edges: `imports` relations resolved to exactly one non-member candidate in another file. Ambiguous: ignored. External/unresolved: names go to `external` (sorted, deduplicated).
- BFS per hop, depth clamped to [1, MAX_QUERY_DEPTH], each file visited once; back-edges kept, self/same-module edges dropped. Edge depth = hop of the importing module + 1.
- `limit` caps nodes (min 1); edges to cut nodes are dropped and `truncated` is set.
- Unknown module: empty graph with `root` set and no nodes.
- Checks: `scripts/unix/check.sh -p mct-index` passes (fmt, 246 tests, clippy).
