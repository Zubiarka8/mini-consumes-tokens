# ADR-006: Module dependency graph tool

## Status
Accepted (2026-10-13). Issue: [#28](https://github.com/Zubiarka8/mini-consumes-tokens/issues/28). Adds the MCP tool `get_dependency_graph`. No schema migration: it reads the existing `Imports` relations and the resolution of [ADR-002](002-qualified-relation-resolution.md).

## Context
- `impact_analysis` is symbol-level: it lists callers, references and tests of one symbol. An agent planning a refactor also asks a coarser question: which files does this module depend on, who depends on it, and is there an import cycle?
- Answering it today means chaining `find_references` per import or reading files, which costs the tokens this project exists to save.
- Import relations already exist in the index, and relation resolution is computed on read, so each import is `resolved`, `ambiguous` or `unresolved`.

## Decision

### Model
- **The unit is the file.** A node is an indexed file. A `module` that names a directory expands to the indexed files under it. Symbol-level detail stays in `impact_analysis`.
- **Edges only from RESOLVED imports.** `importer -> imported` exists when an `Imports` relation resolves to exactly one file. An `ambiguous` or `unresolved` import never becomes an edge, so the graph states nothing the index cannot prove. This is the rule the other relation tools already follow past hop 1.
- **External imports are listed separately.** Imports that resolve to no indexed file (third-party, std, unknown) are reported in an `external` section, with the importing file. They are neither nodes nor edges.

### Traversal and cycles
- A breadth-first walk from the root follows imports out to `depth` hops. Each node reports its hop depth (the shortest distance).
- `depth` defaults to 2 and is clamped to the project's maximum query depth, like the other multi-hop tools. A larger request is clamped, not rejected.
- Cycles are found with an iterative strongly-connected-component pass (Tarjan, with an explicit stack, no recursion), so a deep import chain cannot overflow the stack on adversarial repository input. Each component with more than one file, or one file that imports itself, is one reported cycle. No cycle prints `none`.
- The standard `limit` caps the rows. The answer sets `truncated` when it cut nodes, edges or externals.

### Output
- `format` is `toon` by default, or `text`. There is no JSON format.
- `export` is optional: `mermaid` renders a `graph LR` block, `dot` renders a Graphviz `digraph`. The export draws the same nodes and edges as the answer and is rendered after truncation, so a diagram never shows more than the table.

### Relationship to other tools
- `impact_analysis`: symbol-level blast radius. `get_dependency_graph`: module-level coupling. Neither calls the other.
- It joins `batch` and the tool categories like any read-only query tool.

## Consequences
- One call replaces a chain of import lookups and gives cycles that no other tool reports.
- Languages whose imports the resolver cannot map to a file show those imports as `external`. The graph under-reports edges, never over-reports them. Better resolution lifts coverage without changing the contract.
- A directory root can fan out widely, so `limit` and `truncated` matter there.
- The catalog grows by one tool: `tools.ttc`, `ttc::KNOWN_TOOL_NAMES`, the README tool count and `crates/mct-eval/baseline.json` follow it.

## Alternatives considered
- **Symbol-level graph.** Duplicates `impact_analysis` and is much larger.
- **Edges from ambiguous imports too.** Shows more, but states relations the index cannot prove.
- **Recursive cycle detection.** Simpler, but depth is unbounded on repository input.
