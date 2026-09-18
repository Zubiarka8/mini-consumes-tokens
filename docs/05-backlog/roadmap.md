# Backlog

## Deferred from `ccm-lang-md` Phase 2
See spec `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md`.
- Anchor-aware resolution of `[[Page#Heading]]` against the target file's real heading symbol.
- Tags/links inside list items, blockquotes, tables.
- Code-span exclusion for `#tag`/`[[...]]` (needs an inline-grammar reparse).
- Exact-span relation locations instead of block-granular.

## Token-efficiency improvements
- **BFS query cache in SQLite.** Cache results of expensive multi-hop traversals (`find_references`/`find_calls`/`find_callers`/`impact_analysis` with `depth` > 1) keyed by `(symbol_id, tool, depth, offset)`, invalidated whenever `reindex` runs. Avoids recomputing the same graph walk if an agent repeats a similar query within a session. **Touches the SQLite schema** (new cache table) — per `CLAUDE.md`'s cross-cutting rule, this needs an issue opened first, not a drive-by PR.
- **Agent-maintained Obsidian session note.** At the close of a task, the agent appends a short note (decisions made, why, what's left) to a per-session or per-topic page in this vault, so the next session starts by reading one note instead of reconstructing context from `git log`/conversation history. Doesn't touch `crates/` or the MCP tool surface — pure docs workflow, no cross-cutting constraint.

## Open project decisions
- HTML template-engine handling (Django/Jinja2 embedded in `.html`) — undecided, see `internal/checklist.md`.
- PHP-specific secret patterns (`wp-config.php`, `config/database.php`) — named, not implemented.

## Related
[[00-index]]

#backlog
