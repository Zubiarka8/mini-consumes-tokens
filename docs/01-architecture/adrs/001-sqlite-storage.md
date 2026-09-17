# ADR-001: SQLite as the index storage engine

## Status
Accepted

## Context
The index must store symbols/relations for any supported language, be queryable with joins (parent/child, cross-file relations), and work identically on Linux/macOS/Windows with zero external services.

## Decision
Use SQLite via `rusqlite` (WAL mode, FTS5), one schema shared by every language — a `language` column on `files`, not per-language tables (see [[overview]]). Tables: `files`, `symbols`, `relations`, `symbols_fts`, `index_issues`, `index_meta`, `dependencies`.

## Consequences
- No server process, no network calls, single-file portability.
- A single shared schema means adding a language never touches storage — only a new `LanguageParser` impl.
- Schema changes are cross-cutting (every language crate + the MCP tool contract) — require an issue first, not a drive-by PR.

#adr #architecture
