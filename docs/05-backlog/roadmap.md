# Backlog

## Deferred from `ccm-lang-md` Phase 2
See spec `docs/superpowers/specs/2026-09-17-obsidian-docs-vault-and-md-parser-design.md`.
- Anchor-aware resolution of `[[Page#Heading]]` against the target file's real heading symbol.
- Tags/links inside list items, blockquotes, tables.
- Code-span exclusion for `#tag`/`[[...]]` (needs an inline-grammar reparse).
- Exact-span relation locations instead of block-granular.

## Open project decisions
- HTML template-engine handling (Django/Jinja2 embedded in `.html`) — undecided, see `internal/checklist.md`.
- PHP-specific secret patterns (`wp-config.php`, `config/database.php`) — named, not implemented.

## Related
[[00-index]]

#backlog
