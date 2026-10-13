# Checkpoint: depgraph-cycles (issue #28)

- Implemented `find_cycles` in `crates/mct-index/src/cycles.rs` with an iterative Tarjan SCC (O(V+E), no recursion).
- Returns SCCs with more than one node plus self-loop nodes; members sorted, cycles sorted.
- Tests cover: empty, no cycle, 2-cycle, 3-cycle, self-loop, disjoint cycles, cycle plus tail, overlapping cycles, duplicate edges, edge reordering, 100_000-node chain and ring.
- Signature unchanged.
