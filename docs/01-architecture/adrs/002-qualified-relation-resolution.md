# ADR-002: Qualified relation resolution

## Status
Accepted — first slice of issue #97 (design: `docs/superpowers/specs/2026-10-01-qualified-relation-identity-design.md`). Issue #98 (Markdown note identity) builds on this contract.

## Context
Relations stored only `to_name`, and every query and walk joined on it, so `A::run` reached `B::run` and a Rust `Ok(..)` was shown as a C# `Ok` method (F02). The old `to_symbol_id` column picked an arbitrary same-named row and was dropped for that reason.

## Decision
- **Parser evidence, additive.** `mct_core::RelationTarget { relation, qualifier, path, language, external }`, attached by index through `ParsedFile::relation_targets` (sparse; built with `..Default::default()`, so no parser has to change). A relation with no entry is unqualified. Set a field only when the source proves it.
- **Candidates.** Definitions named `to_name` in `language` (default: the source file's own), whose `parent` equals `qualifier` (or `qualifier<…>`), declared in `path` when set; a call never targets a whole-file `module`. Exactly one = `resolved`, several = `ambiguous` (all kept, none picked), none = `unresolved`; `external` only when the parser set it. No candidate is never proof of external.
- **Derived, not stored.** Evidence is persisted on `relations` (`qualifier`, `target_path`, `target_language`, `external`, `targets_parsed`); candidates come from the `relation_candidates` view. A resolution therefore can't outlive the symbols it was computed from across incremental/forced reindex, deletion or rename, and nothing is backfilled by name. This deviates from the design's stored `to_symbol_id` + candidate table; revisit only if query latency demands it, by materialising the same view.
- **Legacy rows.** The migration leaves pre-existing rows `targets_parsed = 0` (always unresolved) and empties every `content_hash` so the next reindex reparses them.
- **Identity.** `SymbolHit::id` is the snapshot-local row id; the source-stable identity is `(language, relative_path, kind, parent, name, line)`. `RelationHit` adds `relation_id`, `from_symbol_id`, `resolution`, `target_id` (only when resolved) and `candidate_count`; `Index::relation_candidates` lists the candidates. `to_name` stays the source spelling.
- **Walks.** Hop 1 of `find_calls/find_callers/find_references` is still the name query (inputs unchanged). Later hops walk symbol ids: forward only along resolved callees, backward only from referrers whose relation resolved to the node; ambiguous relations are reported where found and never expanded. `build_context_pack` walks from the packed definitions' ids, lists only resolved callees/dependencies as related symbols, puts ambiguous ones in a separate "Uncertain relations" section with candidates, and names unresolved/external ones in the footer.
- **Output.** Text relation lines append ` (ambiguous: N)`, ` (unresolved)` or ` (external)`; a resolved line is unchanged. TOON adds `resolution` and `candidates` columns.

## Contract for parsers (including #98)
A parser that can prove more fills `RelationTarget`: a type/container name in `qualifier`, the declaring file in `path` (repository-relative, forward slashes, exactly as stored in `files.relative_path`), another language only when the source names it. Markdown: a wikilink resolved to a note sets `path` to that note's path (or leaves it unset when only a basename is known, which stays ambiguous on collisions); a `[[Note#Heading]]` link sets `path` to the note and names the heading, so a same-named heading in another note is never a candidate. Never set `external` on absence.

## Consequences
- Rust records `Type::f`, `Self::f`, `self.f()` (impl type), same-file free-function calls (`path`) and `std`/`core`/`alloc` paths (external); HTML id/class references target `css`; XAML handlers target `csharp` in the `.xaml.cs` code-behind. Other parsers are unqualified until enriched: same-language unique names resolve, the rest are ambiguous or unresolved.
- `fan_in_counts`, `reference_counts` (`find_dead_code`, `get_project_overview`) still count by name.
- No receiver-type inference, overload selection or import-alias resolution yet.

#adr #architecture
