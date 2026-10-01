# Design: qualified identity and relation resolution

Date: 2026-10-01

Status: proposed for issues #97 and #98; implementation not started

## Goal

Make relation queries and graph walks follow the symbol named by a source
reference, rather than every definition that happens to share its final name.
Preserve enough source context to distinguish a unique target from an
ambiguous or external target, and never present an arbitrary name match as a
resolved dependency. Markdown note identity from #98 uses the same path-scoped
identity contract.

## Current failure mode

Parsers store only `SymbolRelation.to_name`. Several parsers discard all but
the final component of a qualified expression (`A::run` becomes `run`). The
index queries relations by that string, and traversal uses names as graph
nodes. Consequently `A::run` can lead to `B::run`, and identical names in
different languages or fixtures can be shown as dependencies. The former
best-effort `relations.to_symbol_id` column was removed because it selected an
arbitrary same-name row; restoring that behavior is not a resolution fix.

## Decisions

1. **A resolved edge points to one symbol row.** Within an index snapshot,
   `symbols.id` is the authoritative endpoint for a uniquely resolved
   relation. A relation with zero or multiple candidates has no resolved
   endpoint. Traversal and impact analysis use symbol IDs, never names, as
   visited graph nodes.
2. **Qualified identity is inspectable and path-scoped.** Results expose the
   target's `language`, normalized repository-relative `path`, `kind`,
   qualified parent chain, name, and declaration location. The row ID is an
   efficient snapshot-local key, not a durable identity across a rebuild.
   Source-stable consumers use the descriptive tuple. Overloads with no
   sufficient type information remain ambiguous; line/column can identify a
   declaration in the current snapshot but must not be used to claim semantic
   overload resolution.
3. **Keep raw syntax as evidence.** `SymbolRelation` retains the original
   target spelling and adds structured qualification/context rather than
   replacing it with a normalized guess. A language parser may provide
   receiver/type/module qualifiers and import bindings when its AST supports
   them. Missing qualification means “unqualified”, not “project-wide”.
4. **Resolution is explicit.** Persist `resolved`, `ambiguous`, `external`,
   or `unresolved` status. `resolved` requires exactly one candidate after
   applying language, module/import, lexical scope, and containing-type
   constraints available for that relation kind. `ambiguous` stores all
   remaining candidate IDs. `external` is only used when parser/import
   evidence proves the target is outside the indexed repository; no match by
   itself is not proof of an external dependency and remains `unresolved`.
   `unresolved` also covers unsupported language semantics and incomplete
   indexing.
5. **Do not infer cross-language edges from spelling.** A candidate from a
   different language is eligible only when an explicit relation or
   language-specific import/export mapping establishes that connection. The
   default candidate set is restricted to the source language.
6. **Markdown notes are symbols with path identity.** A note-level Module
   symbol's canonical identity is `(markdown, normalized vault-relative
   path, module)`; its database `symbols.id` is the snapshot-local endpoint.
   It is keyed by the normalized vault-relative path, not title or basename.
   A unique alias/path lookup may resolve a wikilink to that note;
   colliding basenames or aliases remain ambiguous. A heading's identity is
   scoped to its containing note path. `[[Page#Heading]]` resolves the page
   first, then the heading within that exact note; a same-named heading in
   another note is never a fallback. Path normalization and case sensitivity
   follow the repository filesystem/index policy and must not silently
   collapse two distinct paths.
7. **Queries preserve their existing inputs and raw target fields.** Existing
   `find_calls`, `find_callers`, `find_references`, and `impact_analysis`
   inputs remain unchanged. Their results add resolution status, exact target
   identity when resolved, and candidate identities when ambiguous. Existing
   `to_name` remains the source spelling for compatibility. `build_context_pack`
   treats only resolved edges as dependencies; it presents ambiguous and
   unresolved relations in a clearly separate section with candidate paths,
   rather than selecting one. No client should need to parse a formatted
   name to recover identity.
8. **Never backfill by name.** Additive migrations must not assign old
   relations to whichever matching symbol SQLite returns first. Mark legacy
   relations unresolved and invalidate file content hashes so the normal
   reindex path reparses relations. Until a file is reparsed, queries must
   describe its old relation resolution as unknown/unresolved.

## Proposed model and schema shape

The parser-side target should be a structured value, conceptually:

```rust
struct RelationTarget {
    spelling: String,              // exact source text or the parser's raw target
    name: String,                  // existing to_name compatibility value
    qualifier: Option<String>,     // e.g. A in A::run, receiver/type where known
    module: Option<String>,        // import/module context where known
}
```

The exact Rust API should be settled before changing `LanguageParser` and
`SymbolRelation`; all parser crates construct relations directly today, so a
field addition is a workspace-wide source change. Prefer a nested target
object over a growing list of optional fields, while retaining a simple
`to_name` accessor or equivalent during migration if that keeps existing
consumers straightforward.

For persistence, retain `relations.to_name` and add raw qualification plus a
nullable exact `to_symbol_id` foreign key and resolution status. Store
ambiguous candidate IDs in a child table keyed by `relation_id` and
`candidate_symbol_id`; candidates are evidence, not graph edges. Record a
resolution reason only if it can be stable and useful to explain why a
candidate set was narrowed. Recompute relation resolution transactionally
after all symbols for the repository are available, so cross-file targets are
not accidentally classified from a partial per-file insertion order.

This design needs an additive migration and a reindex of existing files; it
does not silently reinterpret existing rows. If migration count and data size
make a forced reindex unsuitable, stage the migration so legacy rows remain
explicitly unresolved until reparsed. The index schema remains shared by all
languages; language-specific candidate logic belongs behind parser-provided
metadata or a language resolver registry, not scattered language-name checks
in `mct-index` or `mct-mcp-server`.

## Resolution order

The resolver should be deterministic and conservative:

1. Restrict candidates to the target category/relation kind and source
   language unless explicit import/export evidence says otherwise.
2. Apply an explicit module/path/import binding, including aliases.
3. Apply a containing-type or receiver qualifier when the parser provides it.
4. Apply lexical scope for unqualified local references, then file/module
   scope where the language semantics support it.
5. Return resolved only for one remaining candidate. Return ambiguous with
   the remaining identities for multiple candidates. Return unresolved for no
   candidate unless there is positive evidence for `external`.

Do not use repository-wide first-match fallback. Do not claim method dispatch
resolution for dynamic dispatch, inferred receiver types, structural typing,
or overload selection unless a language resolver can prove it.

## Acceptance and regression fixtures

- `A::run` resolves to `A::run`, never `B::run`; an unqualified `run` with
  multiple valid candidates is ambiguous.
- Same-file homonyms, nested types, and overloaded methods do not produce an
  arbitrary resolved edge.
- `std::...` or a known imported external package is external only when
  parser/import evidence supports that classification; an unknown bare name
  remains unresolved.
- Same-name symbols in Rust and C# do not become connected by name alone.
- A graph walk from one resolved callee cannot enter a same-name unrelated
  definition; ambiguous candidates are reported but not traversed as if each
  were a confirmed dependency.
- Markdown note links resolve by normalized relative path or unique alias;
  duplicate basename/alias is ambiguous, and `Note#Heading` can target a
  heading only inside the resolved note.
- Incremental reindex, deletion, rename, and full reindex remove stale
  candidate rows and recompute statuses atomically.
- MCP output keeps `to_name`, adds machine-readable status/identity/candidate
  fields, and `build_context_pack` visibly separates uncertain relations.

## Rollout

Implement this in two compatible slices: (1) core relation target metadata,
schema migration, resolver, and index tests; (2) parser enrichment by
language, Markdown note symbols/links, MCP formatting, and end-to-end fixtures.
Keep all resolution conservative during the transition. Do not update the
quality baseline until the new adversarial fixtures and existing evaluation
suite pass and their expected behavior has been reviewed.

## Out of scope

- Compiler/LSP-backed type inference or dynamic dispatch resolution.
- A language-specific semantic type checker in `mct-index`.
- Treating every unmatched name as an external package.
- Replacing user-facing raw target spelling with a canonicalized string.
- Matching notes by title as a fallback when path/alias lookup is ambiguous.
