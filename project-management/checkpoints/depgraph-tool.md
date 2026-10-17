# Checkpoint: depgraph-tool

Branch claude/depgraph-tool. Adds the `get_dependency_graph` MCP tool (issue #28).

- server.rs: args, handler, `validate_module_path`, batch dispatch, "relations" category.
- format.rs: text and TOON renderings, "module not found / not indexed" for an empty graph.
- tools.ttc, ttc.rs: catalog entry (catalog byte budget still under 4300).
- tests/dependency_graph.rs: end-to-end fixture with an a -> b -> c -> a cycle.

Open: the end-to-end tests need the real `Index::dependency_graph`, `find_cycles` and `graph_render`
from the sibling branches. Docs (README count, CLAUDE.md table, protocol spec, mct-eval case) are left to the coordinator.
