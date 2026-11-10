# ADR-034: Declaration details are sparse parser evidence in their own tables

## Status
Proposed (2026-11-10). Issue: [#321](https://github.com/Zubiarka8/mini-consumes-tokens/issues/321). Adds schema migration 16 ([ADR-035](035-function-local-variable-scope.md) took 15).

## Context
- A symbol carries only its name, kind, parent, location and level. The signature MCP shows is recomputed from source by two duplicated helpers (`mct-mcp-server/src/format.rs::signature_line`, `mct-index/src/semantic.rs::signature_at`). It is the first line of the definition, so it is wrong for multi-line headers and for attributes placed before the definition.
- Receiver-type resolution (D5) only reads types inside one parser today. `Overrides` (D8) and inherited methods need parameter types and modifiers stored in the index.
- `SymbolRecord` has no `Default`, so a new field would break all 17 parsers. `ParsedFile` already grows through sparse vectors: `relation_targets`, `symbol_aliases` and `entry_points`.
- A guessed type becomes a wrong resolved edge downstream, as D1 showed.

## Decision
- `mct-core` gets `ParsedFile::symbol_details: Vec<SymbolDetail>`, sparse and keyed by `SymbolId`.
  - A missing entry means unknown, and so does a `None` field. `parameters_known` tells "no parameters" from "parameters not modelled".
  - An entry naming no symbol of the file is ignored, and so is any entry after the first for the same symbol.
  - Texts are collapsed to one line and capped on a char boundary: signature 512 bytes, types and generics 256, defaults 64. `SymbolDetail::normalized` is applied when stored.
- Migration 16 creates `symbol_details` (one row per symbol) and `symbol_parameters` (one row per parameter).
  - Both are `WITHOUT ROWID` and keyed by the symbol, with `ON DELETE CASCADE` like `symbol_entry_points`. A reparse or deletion of a file replaces or removes its rows with no extra code path.
  - Migration 16 forces one full reparse.
- Every type records its provenance: `syntax` or `doc` (JSDoc, PHPDoc, a docstring, a Lua annotation). There is no inferred variant: no parser may emit an inferred type until a later ADR says how.
- Details never assert a resolution. Resolution stays derived in `relation_candidates`, so details can only narrow candidates.
- MCP output does not change with this ADR. Making `build_context_pack` prefer the stored signature is a later step, checked against the token budget ([ADR-016](016-context-budget.md)). Any new MCP field needs its own issue.

## Alternatives considered
- **A JSON column on `symbols`.** Rejected: D5/D8 query "parameter `x` of F has type T", which would need `json_each` on every lookup.
- **A field on `SymbolRecord`.** Rejected: it breaks every parser at once.
- **A partial index on `symbol_parameters(type_text)`,** proposed in the design. Left out: nothing reads by type yet. It can be added together with the first query that needs it.

## Consequences
- Parsers adopt details one language at a time, with no cross-crate breakage. A parser that emits none leaves both tables empty for its files.
- One full reindex on upgrade. An index at migration 16 cannot be opened by an older binary; the recovery is deleting `.mct-index`. This is a one-way door per index.
- Size, reindex time and memory are measured in the PR of each parser that starts emitting details, never estimated.
- `docs/02-crates/parsers/symbol-coverage.md` gets a details column per language, which must match what each parser emits.

#adr
