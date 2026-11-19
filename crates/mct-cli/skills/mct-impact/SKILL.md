---
name: mct-impact
description: "Blast radius of changing a symbol: callers, references and covering tests. Use before changing or removing a function, method or type."
---
Use when: before you change or remove a symbol and need to know what breaks. One composite call replaces separate caller, reference and test searches.
Returns: counts of direct callers, total references and likely affected tests, then three tables: the likely tests, the direct callers and all references. The context pack adds the definition and related symbols.
1. Call impact_analysis with symbol=<arg>, depth=2.
2. Call build_context_pack on the same symbol.
3. Report callers by crate, the tests that cover it, and any ambiguous caller without guessing.
4. End with a verdict: what breaks if the signature changes, which tests to run first, and what stays safe. Say plainly when no test covers it.
No MCP client? Run `mct-cli --root . call --tool impact_analysis --args '{"symbol":"<arg>"}'`.
Do not read whole files.
