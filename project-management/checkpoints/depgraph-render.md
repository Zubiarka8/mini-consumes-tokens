# Checkpoint: depgraph-render

- Issue: #28 (`get_dependency_graph`), renderers only.
- Implemented `render_dot` and `render_mermaid` in `crates/mct-mcp-server/src/graph_render.rs`.
- DOT: quoted/escaped ids (backslash, quote, newline), root doublecircle, cycle nodes `color=red`, cycle edges red dashed.
- Mermaid: generated ids `n0..` in sorted-path order, entity-escaped labels, `classDef root`/`cycle`, `linkStyle` for cycle edges.
- Output is deterministic (sorted, de-duplicated nodes and edges).
- Tests: chain, cycles, hostile names, determinism, lone root.
