# ADR-038: Declared join points as possible entry points

## Status

Proposed (2026-11-19). Issues: [#561](https://github.com/Zubiarka8/mini-consumes-tokens/issues/561) and [#566](https://github.com/Zubiarka8/mini-consumes-tokens/issues/566).

## Context

A framework or an external client can invoke a handler without leaving an indexed call. A declared route is evidence of a possible entry point even when no client in the repository uses it. The convention hints in ADR-027 have different semantics: they annotate candidates and do not exclude them.

## Proposed decision

- A join point records a kind, a normalized literal value, a declaration or use role, and the exact symbol in the same parsed file. The initial kind is an HTTP endpoint whose value contains a method and path.
- Persist join points separately from prose literals and convention hints. File replacement removes the old points with their symbols. A migration must request a reparse of existing files.
- A declaration is evidence of a possible entry point, not proof that the symbol executes. A dead-code scan excludes the associated symbol and reports its reason separately. A missing matching use does not change this decision.
- Preserve ADR-027 hints and their existing candidate behavior. Resolve a declaration to its exact symbol, never to every symbol sharing its name.
- The first HTTP extractor handles only explicitly identifiable route declarations and literal paths. Computed paths and ambiguous handler identities produce no declaration. Other frameworks and join kinds need their own evidence-backed extractors.
- Cross-language matching, normalization of parameterized paths, and new relation edges remain in #561. This decision does not change existing relation resolution.

## Verification

Exercise persistence and reindexing, handler identity, unmatched declarations, ordinary unreferenced functions, convention hints, and the dead-code report. Record candidate counts and removed candidates before and after on this repository.

The initial extractor recognizes qualified Actix Web method attributes such
as `#[actix_web::get("/api/notes")]`. Migration 20 stores their declarations
and invalidates existing content hashes.

Forced reindex of the same `69296cc1` repository snapshot before and after
the change produced 122 dead-code candidates in both runs. The CSV reports
were identical. The snapshot had no matching route declarations. The MCP
fixture exercises the removal of two handlers from the candidate list.
