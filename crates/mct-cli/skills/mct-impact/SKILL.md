---
name: mct-impact
description: "Blast radius of changing a symbol: callers, references and covering tests. Use before changing or removing a function, method or type."
---
1. Call impact_analysis with symbol=<arg>, depth=2.
2. Call build_context_pack on the same symbol.
3. Report callers by crate, the tests that cover it, and any ambiguous caller without guessing.
Do not read whole files.
