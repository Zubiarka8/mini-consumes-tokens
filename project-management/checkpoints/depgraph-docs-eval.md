# Checkpoint: depgraph-docs-eval

Issue: #28 (`get_dependency_graph`). Branch: claude/depgraph-docs-eval (base feat/get-dependency-graph).

Done:
- README: tool count 20 -> 21 (KNOWN_TOOL_NAMES has 20, plus the new tool), toon tool list, assistant prompt hint, usage list, new "Module dependency graph" section with call, TOON, cycle and Mermaid examples.
- rules.md: tool table row and crate list entry.
- docs/01-architecture/mcp-protocol-spec.md: tool row, parameter paragraph and example.
- ADR-006: docs/01-architecture/adrs/006-dependency-graph-tool.md.
- crates/mct-eval/suite.json: one `get_dependency_graph` case on the omni-app fixture (frontend/apiClient.ts imports ./format).

Not done / for the coordinator:
- suite.json case not run (tool not implemented in this branch); re-run `cargo run -p mct-eval` after integration and refresh baseline.json if intended.
- CLAUDE.md has no tool table (it points at rules.md), so new-tool-check keeps its "CLAUDE.md tool table" warning.
- TOON examples are illustrative; align column names with the real output after integration.
