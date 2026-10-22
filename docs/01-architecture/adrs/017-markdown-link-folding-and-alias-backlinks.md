# ADR-017: Case-folded Markdown link resolution and alias-aware references

## Status
Proposed (2026-10-21). Nothing in this ADR is implemented. It needs an approval of the three open questions below before code starts. It adds schema migration 12, which [ADR-005](005-markdown-alias-storage.md) (migration 11) allows only with a new ADR.

## Context
A functional audit of the Obsidian integration (temporary copy of the `obsidian-help` vault `en/` at `cd6c9f3c`, plus 10 controlled notes; run with `mct-cli init`, SQLite opened read-only, tools called through `mct-cli call`) found two defects that matter to anyone with a real vault.

1. **Links differ from their target only by case stay unresolved.** `[[internal links]]` does not reach the note `Internal links`. Obsidian resolves these. In the audit copy, 53 bare note links were unresolved (51 in the original vault; the rest come from my test notes); **44 of them have exactly one target if case is ignored, none becomes ambiguous, none has a non-ASCII spelling**, and 9 stay unresolved (genuinely broken). No two notes differ only by case (the colliding stems are same-name notes in different folders). ADR-005 chose exact, case-sensitive comparison on purpose and listed case folding as out of scope; this ADR is that follow-up and must change stems, headings and aliases together, as ADR-005 warned.
2. **`find_references` ignores aliases.** The view `relation_candidates` already resolves `[[Zeta Alias]]` to the note `Alias Target`, but the tools match relations by *spelling* (`r.to_name = ?1`, `queries.rs`, "Hits are matched by spelling"). Observed:
   - `find_references("Alias Target")` returned 7 hits; the links written as `[[Zeta Alias]]` and `[[ZA]]` (resolved to that note in the database) are missing: backlinks are incomplete.
   - `find_references("Zeta Alias")` returned nothing, because the start set is built from symbols *named* `Zeta Alias` and none exists.
   - `find_symbol("Zeta Alias")` says "not found" (search finds it).

Not in scope here (each is a separate decision): shortest-path links (`[[a/Dup Name]]`), standard `[text](Note.md)` links, `#^block` and nested `#A#B` anchors, the generated atlas content, the Summary signature of notes with frontmatter, and the ambiguous-versus-unresolved wording in context packs.

## Decision

### D1. A second, case-folded resolution tier for Markdown targets
Resolution stays computed on read from `relation_candidates`. For a relation whose source and target are Markdown (`target_kind` `module` or `element`):
1. **Tier 1 (unchanged):** exact, case-sensitive match on name, alias or `target_module`. If it yields at least one candidate, those are the candidates. Every link that resolves or is ambiguous today keeps the same candidates.
2. **Tier 2 (new):** only when tier 1 yields none, match on the folded form. One candidate is `resolved`, several are `ambiguous`, none `unresolved`.

Folding is `str::to_lowercase()` in Rust (Unicode, not SQLite's ASCII-only `lower()`), computed when a file is indexed and stored in new columns:
- `symbols.name_fold TEXT` (Markdown `module` and `element` rows only; `NULL` for every other language),
- `relations.to_name_fold TEXT` and `relations.module_fold TEXT` (Markdown rows only),
- `symbol_aliases.alias_fold TEXT`,

with partial indexes on the non-null values (`idx_symbols_name_fold`, `idx_symbol_aliases_fold`). Other languages stay case-sensitive and pay nothing. Path links keep exact comparison (separate decision). Whitespace, Unicode normalization (NFC/NFD) and Obsidian's heading-text normalization are not part of this tier.

### D2. References are defined by resolution, not by spelling, for Markdown
`find_references(name)` and the backlink part of `build_context_pack` return a relation when either:
- `r.to_name = name` (today's rule, kept so no existing hit disappears), or
- `relation_candidates` contains a symbol that the query denotes. The start set is built from symbols named `name` **plus** note symbols that declare `name` as an alias (and, through D1, symbols whose folded name equals the folded query when the exact name has no symbol).

Hits are deduplicated by relation id. The hit still shows the spelling as written (`to`), its `resolution`, `candidates` and `targets`, so an agent sees that `[[Zeta Alias]]` is a backlink of `Alias Target` and why. Only `module` targets use aliases, as in ADR-005. For code languages the alias table is empty and the fold columns are `NULL`, so their results are identical to today's.

`find_symbol` keeps its contract ("definitions named X"): an alias is not a name. When nothing is found and `name` is a declared alias, the "not found" text adds the note that declares it (``Zeta Alias`` is an alias of `_mct-cases/Alias Target.md`). No new tool and no new argument, so the MCP catalog size does not change.

### Migration 12
Adds the four columns and two partial indexes, recreates `relation_candidates` with tier 2, and sets every `content_hash` to `''` so the next `init`/`reindex` re-parses and fills the columns (no SQL backfill by name, the policy of ADR-005 and #97). An older binary stops on a migrated index with the "migration number too high" error and needs a reinstall; this is the same consequence ADR-005 recorded. `crates/mct-index/tests/assets.rs` moves from 11 to 12 and points here.

## Alternatives considered

| Option | Verdict |
|---|---|
| (a) Rust-folded columns, two-tier view, partial indexes (migration 12) | **Chosen.** Unicode-correct, indexed lookups, no change for other languages, existing resolutions cannot change because tier 1 wins. |
| (b) SQLite `lower()` (ASCII only) plus an expression index | Rejected as the primary design. Simpler (index and view only) and enough for the measured vault (0 non-ASCII spellings), but wrong for `[[Ñandú]]` / `[[ÉCOLE]]`. Kept as the fallback if (a) misses the performance gate. |
| (c) Fold in one tier (`fold(a) = fold(b)` for everything) | Rejected. A case-only difference would turn links that are `resolved` today into `ambiguous` when two notes differ by case, changing existing answers. |
| (d) Resolve in Rust after the query | Rejected. `relation_candidates` is read by `queries.rs`, `traversal.rs`, `dependency_graph.rs`, `indexer.rs` and the server; each would need the same logic. |
| (e) Make `find_symbol` return alias holders as definitions | Rejected. It breaks "definition(s) of X" and pollutes skeletons, overviews and counts (the reason ADR-005 rejected alias symbols). A hint on "not found" keeps the contract. |
| (f) New tool or argument for backlinks | Rejected. The catalog grows (+~3.8 % was the cost of `detail`/`max_tokens`) and agents already call `find_references`. |

## Consequences
- 44 of the 53 unresolved bare links of the audit vault resolve to exactly one note; 9 stay broken. Expected, to be re-measured with the same script.
- Backlinks of a note become complete for alias-spelled links, and a query by alias works.
- The view gets a `NOT EXISTS`-style second tier. Its cost is measured with `crates/mct-index/tests/relation_scale.rs` before and after; the gate is a regression of at most 10 % on the existing scale fixture, and no extra cost for non-Markdown relations. If missed, fall back to (b).
- A relation can change from `unresolved` to `resolved` after the migration (intended). Existing `resolved` and `ambiguous` outcomes must not change; this is a tested property.
- Obsidian parity for aliases is **not assumed**: to my recollection Obsidian does not resolve `[[Alias]]` links by alias (it does for link suggestions), while MCT does since ADR-005. D2 does not depend on that; the UI check in the audit guide settles it.
- Documentation: `COMMANDS.md` (find_references/find_symbol behaviour), README Obsidian section (resolution rules), CHANGELOG, ADR-005 gets a "see ADR-017" line (its case-folding exclusion is lifted for Markdown only).

## Open questions (need an answer before implementation)
1. **Fold scope.** Unicode `to_lowercase` (recommended) or ASCII only (option b, no new columns)?
2. **Migration.** Is a migration 12 (older binaries need a reinstall and a reindex) acceptable for this release candidate, or should this wait for the release after?
3. **Alias in `find_symbol`.** Hint on "not found" only (recommended) or also list the alias holder as a definition?

## Implementation plan
Order matters; each stage ends with its tests green and no commit without approval. Skills used: `test-driven-development`, `incremental-implementation`, `documentation-and-adrs`.

0. **Baseline (no code).** Save today's outputs on the audit vault (`obs/vault` script set) and the `relation_scale` timings; keep `benchmarks/real-repos` results as the code-language regression reference (identical tokens/calls/evidence expected afterwards).
1. **Red tests.**
   - `crates/mct-lang-md/tests/note_graph.rs`: case-only link resolves; exact beats folded (two notes `Foo`/`foo`: `[[Foo]]` stays resolved to `Foo`); folded tie is `ambiguous` with all candidates; heading `#section one`; Unicode (`[[école]]`→`École`); alias folded; `title` still never an alias; path links stay exact; a Rust symbol differing only by case stays unresolved.
   - `crates/mct-index/tests/`: `find_references` of a note lists alias-spelled and folded-spelled backlinks, once each; `find_references(alias)` works; non-Markdown fixtures return the same hits as before; after deleting/renaming the declaring note the hits disappear and nothing dangles.
   - `crates/mct-mcp-server/tests/`: `find_symbol(alias)` hint; `build_context_pack` of a note includes alias-spelled backlinks; argument schemas and catalog size unchanged.
2. **Migration 12** in `crates/mct-index/src/schema.rs`; fill the fold columns in `indexer.rs` for Markdown rows; bump `tests/assets.rs` to 12 referencing this ADR.
3. **View and queries.** Recreate `relation_candidates` with tier 2; extend the start set and predicate for References (`queries.rs`, `traversal.rs::scoped_start`); keep `RELATION_SELECT` shape.
4. **Server text.** Hint in `find_symbol`; docs (list above); set this ADR to Accepted.
5. **Validation.** `scripts/unix/check.sh`; `relation_scale` against the 10 % gate; re-run the audit (expect the 44 links resolved, 0 new ambiguous, 0 changed outcome for links already resolved or ambiguous); `real_repos` three runs identical to `real-repos.md` for click, cobra and ripgrep; Linux CI through the PR before any release.

Risks: view cost (gate and fallback above); a reindex is required after upgrading (message already exists for migration 11); tier 2 could surprise users who relied on a link staying broken (documented in CHANGELOG).
