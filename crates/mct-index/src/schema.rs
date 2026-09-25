use rusqlite_migration::{Migrations, M};

/// Versioned schema migrations, applied in order by [`crate::Index::open`].
/// One schema for every language: symbols/relations carry a `language`
/// column rather than living in per-language tables, so adding a language
/// plugin never requires a migration.
pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        M::up(
            r#"
            CREATE TABLE files (
                id              INTEGER PRIMARY KEY,
                relative_path   TEXT NOT NULL UNIQUE,
                language        TEXT NOT NULL,
                content_hash    TEXT NOT NULL,
                last_indexed_at INTEGER NOT NULL
            );
            CREATE INDEX idx_files_language ON files(language);

            CREATE TABLE symbols (
                id       INTEGER PRIMARY KEY,
                file_id  INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                name     TEXT NOT NULL,
                kind     TEXT NOT NULL,
                parent   TEXT,
                line     INTEGER NOT NULL,
                column   INTEGER NOT NULL,
                byte_len INTEGER NOT NULL
            );
            CREATE INDEX idx_symbols_name ON symbols(name);
            CREATE INDEX idx_symbols_file ON symbols(file_id);

            CREATE TABLE relations (
                id             INTEGER PRIMARY KEY,
                from_symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                kind           TEXT NOT NULL,
                to_name        TEXT NOT NULL,
                to_symbol_id   INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
                line           INTEGER NOT NULL,
                column         INTEGER NOT NULL,
                byte_len       INTEGER NOT NULL
            );
            CREATE INDEX idx_relations_from ON relations(from_symbol_id);
            CREATE INDEX idx_relations_to_name ON relations(to_name);
            CREATE INDEX idx_relations_kind ON relations(kind);

            CREATE VIRTUAL TABLE symbols_fts USING fts5(
                name,
                content = 'symbols',
                content_rowid = 'id'
            );
            CREATE TRIGGER symbols_ai AFTER INSERT ON symbols BEGIN
                INSERT INTO symbols_fts(rowid, name) VALUES (new.id, new.name);
            END;
            CREATE TRIGGER symbols_ad AFTER DELETE ON symbols BEGIN
                INSERT INTO symbols_fts(symbols_fts, rowid, name) VALUES ('delete', old.id, old.name);
            END;
            CREATE TRIGGER symbols_au AFTER UPDATE ON symbols BEGIN
                INSERT INTO symbols_fts(symbols_fts, rowid, name) VALUES ('delete', old.id, old.name);
                INSERT INTO symbols_fts(rowid, name) VALUES (new.id, new.name);
            END;

            -- Files skipped during indexing: either no LanguageParser is
            -- registered for the extension, or the file failed to parse. These
            -- are distinct causes surfaced separately by get_indexing_status.
            CREATE TABLE index_issues (
                id            INTEGER PRIMARY KEY,
                relative_path TEXT NOT NULL,
                issue_kind    TEXT NOT NULL,
                detail        TEXT NOT NULL,
                detected_at   INTEGER NOT NULL,
                UNIQUE(relative_path, issue_kind)
            );

            CREATE TABLE index_meta (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        ),
        // Added when mct-lang-bash/mct-lang-powershell landed, alongside a
        // manifest scanner (see mct-index/src/manifests.rs) that is
        // deliberately NOT a LanguageParser — Cargo.toml/package.json/
        // requirements.txt/go.mod aren't source code to symbol-index, just
        // metadata about what a repo depends on. A separate migration, not
        // folded into the one above, so an existing `.mct-index/index.sqlite3`
        // gets this table added instead of needing a fresh index.
        M::up(
            r#"
            CREATE TABLE dependencies (
                id            INTEGER PRIMARY KEY,
                manifest_path TEXT NOT NULL,
                language      TEXT NOT NULL,
                name          TEXT NOT NULL,
                version       TEXT,
                UNIQUE(manifest_path, name)
            );
            CREATE INDEX idx_dependencies_manifest ON dependencies(manifest_path);
            CREATE INDEX idx_dependencies_name ON dependencies(name);
            "#,
        ),
        // Added for `list_symbols`: the 1-based end line of a symbol's node
        // (e.g. the closing brace of a function/class body), when the
        // `LanguageParser` that produced it populates `Location::end_line`.
        // Nullable, and added via ALTER TABLE rather than folded into the
        // schema above, so an existing `.mct-index/index.sqlite3` just
        // gets the column added (as NULL for every already-indexed row)
        // instead of needing a fresh index — those rows read back as `None`
        // until the next reindex repopulates them from a parser that sets it.
        M::up("ALTER TABLE symbols ADD COLUMN end_line INTEGER;"),
        // `to_symbol_id` was a best-effort FK, resolved by an unscoped
        // `SELECT id FROM symbols WHERE name = ?1 LIMIT 1` on every relation
        // written during reindex — non-deterministic on a duplicate name
        // (whichever row SQLite happened to return first) and never
        // backfilled retroactively. Nothing ever read it: every relation
        // query (`find_references`/`find_calls`/`find_callers`/
        // `impact_analysis`) resolves purely on the `to_name` string column,
        // narrowed by the `path`/`language` scope filters added in Fase 2
        // instead. See investigacion.md §B2 and issue #35.
        M::up("ALTER TABLE relations DROP COLUMN to_symbol_id;"),
        // Writer-declared depth of a symbol, where the language has one (a
        // Markdown ATX heading's `#`..`######`) — see `SymbolRecord::level`
        // in `mct-core`. Nullable and additive like `end_line` above: an
        // existing index gets the column added as NULL for every
        // already-indexed row, repopulated by the parser that sets it on
        // the next reindex. Every `LanguageParser` besides `mct-lang-md`
        // leaves it NULL.
        M::up("ALTER TABLE symbols ADD COLUMN level INTEGER;"),
    ])
}
