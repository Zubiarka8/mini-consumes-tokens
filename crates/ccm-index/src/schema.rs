use rusqlite_migration::{Migrations, M};

/// Versioned schema migrations, applied in order by [`crate::Index::open`].
/// One schema for every language: symbols/relations carry a `language`
/// column rather than living in per-language tables, so adding a language
/// plugin never requires a migration.
pub fn migrations() -> Migrations<'static> {
    Migrations::new(vec![M::up(
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
    )])
}
