---
name: mct-deps
description: "Import graph of a file or directory: what it imports, who imports it and import cycles. Use to judge how coupled a module is."
---
Use when: you want to know what a file or directory imports, who imports it, or whether it sits in an import cycle, for example before moving code between modules.
Returns: a nodes table with depth, an edges table (from, to), then the lines circular dependencies and external imports, and whether the result was truncated.
1. Call get_dependency_graph with module=<arg>. Raise depth for more hops, or pass export=mermaid for a diagram.
2. Report the imports, the importers and any cycle. List unresolved and third-party imports separately.
3. For a cycle, name the files that close it and which import is cheapest to cut.
No MCP client? Run `mct-cli --root . call --tool get_dependency_graph --args '{"module":"<arg>"}'`.
Do not read whole files.
