---
name: mct-deps
description: "Import graph of a file or directory: what it imports, who imports it and import cycles. Use to judge how coupled a module is."
---
1. Call get_dependency_graph with module=<arg>. Raise depth for more hops, or pass export=mermaid for a diagram.
2. Report the imports, the importers and any cycle. List unresolved and third-party imports separately.
Do not read whole files.
