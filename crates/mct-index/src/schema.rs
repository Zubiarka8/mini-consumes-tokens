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
        // `search_symbols` (issue #58): each symbol's name split into
        // lowercase words at camelCase/PascalCase/snake_case/kebab-case/
        // acronym boundaries (`HTTPServer` → `http server httpserver`), in a
        // standalone FTS5 table keyed by `symbols.id`. The words come from
        // `mct_split_words`, a Rust scalar function (`search.rs`) registered
        // on the connection before migrations run — SQL alone can't split
        // camelCase. That also means only this project's own software can
        // write `symbols` from here on, which CLAUDE.md already requires.
        // Backfilled here so an existing index is searchable without a
        // forced reindex; `symbols_fts` above is left untouched, so
        // `find_symbol`'s prefix mode is unchanged.
        M::up(
            r#"
            CREATE VIRTUAL TABLE symbol_words_fts USING fts5(words);
            INSERT INTO symbol_words_fts(rowid, words)
                SELECT id, mct_split_words(name) FROM symbols;
            CREATE TRIGGER symbol_words_ai AFTER INSERT ON symbols BEGIN
                INSERT INTO symbol_words_fts(rowid, words) VALUES (new.id, mct_split_words(new.name));
            END;
            CREATE TRIGGER symbol_words_ad AFTER DELETE ON symbols BEGIN
                DELETE FROM symbol_words_fts WHERE rowid = old.id;
            END;
            CREATE TRIGGER symbol_words_au AFTER UPDATE OF name ON symbols BEGIN
                UPDATE symbol_words_fts SET words = mct_split_words(new.name) WHERE rowid = old.id;
            END;
            "#,
        ),
        // `hybrid_search` (issue #61): one L2-normalised embedding vector
        // per symbol, as little-endian f32s, tagged with the model that
        // produced it (vectors from different models are not comparable).
        // Not backfilled — vectors come from an embedding model outside
        // SQLite, filled lazily by `semantic::refresh_embeddings`. Dropped
        // with the symbol via the FK cascade (`foreign_keys` is ON on every
        // connection), and on an in-place rename by the trigger, so a stale
        // vector never outlives the name it was computed from.
        M::up(
            r#"
            CREATE TABLE symbol_embeddings (
                symbol_id INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
                model     TEXT NOT NULL,
                vector    BLOB NOT NULL
            );
            CREATE INDEX idx_symbol_embeddings_model ON symbol_embeddings(model);
            CREATE TRIGGER symbol_embeddings_au AFTER UPDATE OF name, kind, parent ON symbols BEGIN
                DELETE FROM symbol_embeddings WHERE symbol_id = old.id;
            END;
            "#,
        ),
        // Prose string literals (`ParsedFile::literals`), so a quoted
        // `hybrid_search` finds an error/log message's file and line.
        // `symbol_id` is the innermost symbol enclosing the literal, resolved
        // at write time. `literals_fts` is an external-content table (the
        // text is stored once, in `literals`), kept in sync by the triggers,
        // which also fire for the `files` FK cascade. `detail=full` keeps
        // token positions, which phrase queries need; `remove_diacritics`
        // makes `conexión` match `conexion`. No embeddings for literals.
        //
        // Emptying every `content_hash` makes the next reindex re-parse every
        // file: the incremental skip compares git blob hashes, so an
        // unchanged file would otherwise never get its literals until a
        // `reindex --force`.
        M::up(
            r#"
            CREATE TABLE literals (
                id        INTEGER PRIMARY KEY,
                file_id   INTEGER NOT NULL REFERENCES files(id) ON DELETE CASCADE,
                symbol_id INTEGER REFERENCES symbols(id) ON DELETE SET NULL,
                line      INTEGER NOT NULL,
                text      TEXT NOT NULL
            );
            CREATE INDEX idx_literals_file ON literals(file_id);
            CREATE INDEX idx_literals_symbol ON literals(symbol_id);

            CREATE VIRTUAL TABLE literals_fts USING fts5(
                text,
                content = 'literals',
                content_rowid = 'id',
                detail = full,
                tokenize = 'unicode61 remove_diacritics 1'
            );
            CREATE TRIGGER literals_ai AFTER INSERT ON literals BEGIN
                INSERT INTO literals_fts(rowid, text) VALUES (new.id, new.text);
            END;
            CREATE TRIGGER literals_ad AFTER DELETE ON literals BEGIN
                INSERT INTO literals_fts(literals_fts, rowid, text) VALUES ('delete', old.id, old.text);
            END;
            CREATE TRIGGER literals_au AFTER UPDATE OF text ON literals BEGIN
                INSERT INTO literals_fts(literals_fts, rowid, text) VALUES ('delete', old.id, old.text);
                INSERT INTO literals_fts(rowid, text) VALUES (new.id, new.text);
            END;

            UPDATE files SET content_hash = '';
            "#,
        ),
    ])
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::migrations;

    /// The literals migration forces a full re-parse of an existing index.
    #[test]
    fn literals_migration_invalidates_every_content_hash() -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        let before_literals = 7;
        migrations().to_version(&mut conn, before_literals)?;
        conn.execute(
            "INSERT INTO files (relative_path, language, content_hash, last_indexed_at)
             VALUES ('src/lib.rs', 'rust', 'abc123', 0)",
            [],
        )?;
        migrations().to_latest(&mut conn)?;
        let hash: String = conn.query_row("SELECT content_hash FROM files", [], |r| r.get(0))?;
        assert_eq!(hash, "");
        Ok(())
    }
}
