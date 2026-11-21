use rusqlite_migration::{Migrations, M};

/// The clause of migration 13 (ADR-026). A macro so the migration and its
/// enumeration test (`schema/module_shadow_tests.rs`) share one literal.
macro_rules! shadowed_module_predicate {
    () => {
        "AND NOT (s.kind = 'module' AND r.kind <> 'imports' AND r.target_kind IS NULL
                       AND EXISTS (SELECT 1 FROM symbols o
                                   WHERE o.file_id = s.file_id AND o.name = s.name
                                     AND o.parent IS s.parent AND o.kind <> 'module'))"
    };
}

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
        // makes `café` match `cafe`. No embeddings for literals.
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
        // Qualified relation resolution (issue #97). The parser's evidence
        // about a relation's target (`mct_core::RelationTarget`) is stored
        // on the row; which symbols it may denote is *derived* by the
        // `relation_candidates` view, never stored, so a candidate can't
        // outlive a reindex of the file that declared it and nothing is ever
        // backfilled by name (the dropped `to_symbol_id` above was exactly
        // that). A candidate shares the relation's name and the language
        // the evidence names (the source file's own by default), and
        // satisfies every qualifier/path/module constraint; a call never
        // targets a whole-file module and a receiver (`member`) call only a
        // method. One candidate = resolved (unless `member`: the receiver's
        // type is unproven, so ambiguous), several = ambiguous, none =
        // unresolved, unless the parser proved `external`.
        //
        // `targets_parsed` is 0 on every pre-existing row: those were written
        // without evidence, so they read as unresolved until reparsed, which
        // emptying every `content_hash` forces on the next reindex.
        M::up(
            r#"
            ALTER TABLE relations ADD COLUMN qualifier TEXT;
            ALTER TABLE relations ADD COLUMN target_path TEXT;
            ALTER TABLE relations ADD COLUMN target_language TEXT;
            ALTER TABLE relations ADD COLUMN external INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE relations ADD COLUMN targets_parsed INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE relations ADD COLUMN module TEXT;
            ALTER TABLE relations ADD COLUMN member INTEGER NOT NULL DEFAULT 0;

            CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.name = r.to_name
                JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<')
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND NOT (r.kind = 'calls' AND s.kind = 'module');

            UPDATE files SET content_hash = '';
            "#,
        ),
        // Markdown notes as graph nodes (issue #98). Two further constraints
        // a parser may put on a relation's target: its `kind` (a note link
        // names the note's `module`, not a same-named heading) and a
        // `module` its file must declare (`[[beta#Shared]]` reaches `Shared`
        // only in the file whose note symbol is `beta`). Both are NULL on
        // existing rows, which keeps their candidates unchanged. The view is
        // recreated with the extra predicates; every file is reparsed so
        // notes get their module symbol and path-aware targets.
        M::up(
            r#"
            ALTER TABLE relations ADD COLUMN target_kind TEXT;
            ALTER TABLE relations ADD COLUMN target_module TEXT;

            DROP VIEW relation_candidates;
            CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.name = r.to_name
                JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<')
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND m.name = r.target_module))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module');

            UPDATE files SET content_hash = '';
            "#,
        ),
        // Parser-declared aliases (issue #135). Start empty: legacy relation
        // names cannot prove alias declarations. Reparse once after migration.
        //
        // The view keeps migration 10's single-SELECT shape and only widens
        // the target lookup: a `module` relation without an explicit path
        // also matches symbols declaring `to_name` as an alias, and a
        // `target_module` scope also matches a note by alias. Every other
        // predicate is unchanged, so an ambiguous scope still yields its
        // same-named sections, never the notes.
        //
        // Query-plan notes (measured with `relation_scale`): the lookup is a
        // correlated `IN` (UNION ALL: `IN` already deduplicates), because a
        // compound view is materialized under the `COUNT(*) ... WHERE
        // relation_id = r.id` lookups and an `OR` join was ~30x slower. The
        // `r.to_name IN (s.name, aliases of s)` term is implied by the lookup
        // but lets a `symbol_id = ?` query reach `relations` through
        // `idx_relations_to_name` instead of scanning every relation, and
        // `CROSS JOIN` keeps `sf` after `s`, so a `relation_id = ?` query
        // never walks `idx_files_language` per relation.
        M::up(
            r#"
            CREATE TABLE symbol_aliases (
                symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                alias TEXT NOT NULL,
                PRIMARY KEY (symbol_id, alias)
            );
            CREATE INDEX idx_symbol_aliases_alias ON symbol_aliases(alias);

            DROP VIEW relation_candidates;
            CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<')
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module');

            UPDATE files SET content_hash = '';
            "#,
        ),
        // LuaRocks detail (issue #267): everything a rockspec declares, kept
        // apart from `dependencies`, which stays the one-row-per-name summary
        // every ecosystem shares (and which CLI and web read). One row per
        // entry in declaration order: repeated names, scopes, constraints,
        // platform tables and module maps all survive, and each section
        // records whether it was read completely. Purely additive: an
        // existing index gets the table empty and fills it on the next
        // reindex, which re-reads every manifest.
        M::up(
            r#"
            CREATE TABLE rockspec_items (
                id                 INTEGER PRIMARY KEY,
                manifest_path      TEXT NOT NULL,
                position           INTEGER NOT NULL,
                kind               TEXT NOT NULL,
                section            TEXT,
                scope              TEXT,
                platform           TEXT,
                namespace          TEXT,
                name               TEXT NOT NULL,
                version_constraint TEXT,
                path               TEXT,
                native             INTEGER NOT NULL DEFAULT 0,
                source             TEXT,
                state              TEXT NOT NULL,
                detail             TEXT,
                UNIQUE(manifest_path, position)
            );
            CREATE INDEX idx_rockspec_items_name ON rockspec_items(name);
            "#,
        ),
        // A file's `module` symbol shares its stem with the type declared in
        // it (PHP, Java, C#, TS), so a reference that no syntax pins to a
        // kind listed both and read as `ambiguous among 2` (issue #272, ADR-026).
        // The module is dropped from such a relation only when the same file
        // declares a same-named, same-parent symbol that is not a module:
        // that symbol satisfies every predicate the module did, so the
        // relation keeps a candidate. Every file is reparsed, as in the
        // earlier view changes.
        M::up(concat!(
            r#"
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<')
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  "#,
            shadowed_module_predicate!(),
            r#";

            UPDATE files SET content_hash = '';
            "#,
        )),
        // Possible framework entry points (issue #273, ADR-027). A parser may
        // declare that a framework could load a symbol by convention although
        // no source line names it; the rows belong to the symbol, so a
        // reparse or deletion of the declaring file replaces or removes them
        // through the cascade. The optional requirement (a manifest and the
        // package it must declare) is applied when a hint is read, against
        // `dependencies`, so a change to the manifest alone changes what is
        // shown. Every file is reparsed to declare its hints.
        M::up(
            r#"
            CREATE TABLE symbol_entry_points (
                symbol_id          INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                framework          TEXT NOT NULL,
                rule               TEXT NOT NULL,
                evidence           TEXT NOT NULL,
                requires_manifest  TEXT,
                requires_package   TEXT,
                requires_min_major INTEGER,
                PRIMARY KEY (symbol_id, framework, rule)
            );

            UPDATE files SET content_hash = '';
            "#,
        ),
        // A variable declared inside a function is reachable only from that
        // function's file (issue #320 D4, ADR-035): a Lua `speed()` in
        // `hud.lua` never resolves to a `local speed` of `physics.lua`. The
        // parent must be proven a function or method of the same file — a
        // same-named container there (a class, module, struct…) keeps the
        // variable a candidate from anywhere; a variable, constant or field
        // can't contain one. File-level variables and a global first
        // assigned in a function (no parent) are unaffected. No reparse: the
        // view reads only columns every index already has.
        M::up(concat!(
            r#"
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<')
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  "#,
            shadowed_module_predicate!(),
            r#";
            "#,
        )),
        // Declaration details (issue #321, ADR-034): a parser may record what
        // the syntax proves about a declaration — its header as written,
        // parameters, return type, visibility, modifiers and generics. Sparse:
        // a symbol without a row has no known details, and a NULL column is
        // "unknown". The rows belong to the symbol, so a reparse or deletion
        // of its file replaces or removes them through the cascade. Every
        // file is reparsed so parsers that emit details can record them.
        M::up(
            r#"
            CREATE TABLE symbol_details (
                symbol_id        INTEGER PRIMARY KEY REFERENCES symbols(id) ON DELETE CASCADE,
                signature        TEXT,
                return_type      TEXT,
                return_source    TEXT,
                visibility       TEXT,
                modifiers        TEXT,
                generics         TEXT,
                parameters_known INTEGER NOT NULL DEFAULT 0
            ) WITHOUT ROWID;

            CREATE TABLE symbol_parameters (
                symbol_id    INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                position     INTEGER NOT NULL,
                name         TEXT,
                type_text    TEXT,
                type_source  TEXT,
                default_text TEXT,
                kind         TEXT NOT NULL,
                PRIMARY KEY (symbol_id, position)
            ) WITHOUT ROWID;

            UPDATE files SET content_hash = '';
            "#,
        ),
        // A parser may rule out a field as a relation's target (issue #412):
        // `TABLE.items()` on a receiver of unproven type can reach a method
        // or a callable, but a same-named field of an unrelated class is only
        // a name coincidence. `not_field` is 0 on existing rows, which keeps
        // their candidates; every file is reparsed so parsers that set it
        // can.
        M::up(concat!(
            r#"
            ALTER TABLE relations ADD COLUMN not_field INTEGER NOT NULL DEFAULT 0;
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<')
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  "#,
            shadowed_module_predicate!(),
            r#";
            UPDATE files SET content_hash = '';
            "#,
        )),
        // A call qualified by a type also reaches a method an ancestor
        // declares (issue #479): each supertype is the target of an
        // `extends`/`implements` relation stored for a type of that name and
        // language, walked up to 8 levels, and the nearest level that
        // declares a member of that name wins (an override shadows what it
        // inherits; the type's own members are level 0). A `target_module`
        // then pins the qualifier type, not the ancestor (a PHP `$this->m()`
        // reaches a trait of another namespace). Stored rows are unchanged,
        // so nothing is reparsed.
        M::up(concat!(
            r#"
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // A method reached through an inline module and its type
        // (`inner::Thing::make()`, issue #478): the method's parent is the
        // type, so the module also matches when the type is declared in
        // that module. The parser's `target_path` still pins the file, so
        // another file's same-named module is no candidate. Stored rows are
        // unchanged, so nothing is reparsed.
        M::up(concat!(
            r#"
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // Declared join points (issue #566): preserve literal framework entry
        // points separately from call relations and reparse existing files.
        M::up(
            r#"
            CREATE TABLE join_points (
                symbol_id INTEGER NOT NULL REFERENCES symbols(id) ON DELETE CASCADE,
                kind      TEXT NOT NULL,
                value     TEXT NOT NULL,
                role      TEXT NOT NULL CHECK (role IN ('declares', 'uses')),
                PRIMARY KEY (symbol_id, kind, value, role)
            ) WITHOUT ROWID;

            UPDATE files SET content_hash = '';
            "#,
        ),
        // Matching join points across files (issue #561, ADR-038): a use
        // keeps where it is written, so the relation made from it points at
        // the exact line, and a relation made by that pass is marked with
        // its join kind so the next run can replace it. Every file is
        // reparsed so parsers that now extract uses and axum or Express
        // routes can record them.
        M::up(
            r#"
            ALTER TABLE join_points ADD COLUMN line INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE join_points ADD COLUMN column INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE join_points ADD COLUMN byte_len INTEGER NOT NULL DEFAULT 0;
            ALTER TABLE relations ADD COLUMN join_kind TEXT;
            CREATE INDEX idx_join_points_match ON join_points(kind, value, role);
            CREATE INDEX idx_relations_join ON relations(join_kind) WHERE join_kind IS NOT NULL;

            UPDATE files SET content_hash = '';
            "#,
        ),
        // A supertype in C# and C++ is a type (issue #388): a constructor
        // spelled like its class, or a same-named function or field, is no
        // candidate of an `extends`/`implements` relation. Other languages keep
        // them (a JS class extends a constructor function, a Python class a
        // variable). Stored rows are unchanged, so nothing is reparsed.
        M::up(concat!(
            r#"
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND NOT (r.kind IN ('extends', 'implements')
                           AND sf.language IN ('csharp', 'cpp')
                           AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // A function declared inside another function or method is reachable
        // by name only from its own file (issue #376, ADR-040): another file's
        // relation does not get it as a candidate. Containment is by span: the
        // enclosing one starts first and ends on a later line, or both lie on
        // one line and its bytes cover the inner one (a minified bundle). A
        // span that ends on the same later line is ambiguous and kept. Stored
        // rows are unchanged, so nothing is reparsed.
        M::up(concat!(
            r#"
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND NOT (r.kind IN ('extends', 'implements')
                           AND sf.language IN ('csharp', 'cpp')
                           AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  AND (sf.id = cf.id
                       OR s.kind <> 'function'
                       OR s.end_line IS NULL
                       OR NOT EXISTS (
                           SELECT 1 FROM symbols p
                           WHERE p.file_id = s.file_id
                             AND p.id <> s.id
                             AND p.kind IN ('function', 'method')
                             AND (p.line < s.line
                                  OR (p.line = s.line AND p.column < s.column))
                             AND (p.end_line > s.end_line
                                  OR (p.line = s.line AND p.end_line = s.line
                                      AND s.end_line = s.line
                                      AND p.column + p.byte_len >= s.column + s.byte_len))))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // A name the source writes with no receiver never reaches a member of
        // a type in JS/TS (issue #395, ADR-041): `f()`, `new Repo()`,
        // `<Button />`, a type `Paid` or `import { f }` names a top-level
        // binding, so a method or field is no candidate. `top_level` is 0 on
        // existing rows, which keeps their candidates; every file is reparsed
        // so the JS/TS parser can set it.
        M::up(concat!(
            r#"
            ALTER TABLE relations ADD COLUMN top_level INTEGER NOT NULL DEFAULT 0;
            DROP VIEW relation_candidates;
            UPDATE files SET content_hash = '';
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.top_level = 0 OR s.kind NOT IN ('method', 'field'))
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND NOT (r.kind IN ('extends', 'implements')
                           AND sf.language IN ('csharp', 'cpp')
                           AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  AND (sf.id = cf.id
                       OR s.kind <> 'function'
                       OR s.end_line IS NULL
                       OR NOT EXISTS (
                           SELECT 1 FROM symbols p
                           WHERE p.file_id = s.file_id
                             AND p.id <> s.id
                             AND p.kind IN ('function', 'method')
                             AND (p.line < s.line
                                  OR (p.line = s.line AND p.column < s.column))
                             AND (p.end_line > s.end_line
                                  OR (p.line = s.line AND p.end_line = s.line
                                      AND s.end_line = s.line
                                      AND p.column + p.byte_len >= s.column + s.byte_len))))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // A parser may mark a symbol that only its own file can name (issue
        // #446, ADR-042): a C# `using Alias = T;` is visible in its file or
        // namespace block only, so another file's relation does not get it as
        // a candidate. `file_scoped` is 0 on existing rows; every file is
        // reparsed so parsers that mark symbols can.
        M::up(concat!(
            r#"
            ALTER TABLE symbols ADD COLUMN file_scoped INTEGER NOT NULL DEFAULT 0;
            DROP VIEW relation_candidates;
            UPDATE files SET content_hash = '';
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0)
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.top_level = 0 OR s.kind NOT IN ('method', 'field'))
                  AND (s.file_scoped = 0 OR sf.id = cf.id)
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND NOT (r.kind IN ('extends', 'implements')
                           AND sf.language IN ('csharp', 'cpp')
                           AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  AND (sf.id = cf.id
                       OR s.kind <> 'function'
                       OR s.end_line IS NULL
                       OR NOT EXISTS (
                           SELECT 1 FROM symbols p
                           WHERE p.file_id = s.file_id
                             AND p.id <> s.id
                             AND p.kind IN ('function', 'method')
                             AND (p.line < s.line
                                  OR (p.line = s.line AND p.column < s.column))
                             AND (p.end_line > s.end_line
                                  OR (p.line = s.line AND p.end_line = s.line
                                      AND s.end_line = s.line
                                      AND p.column + p.byte_len >= s.column + s.byte_len))))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // Cargo package names and literal TypeScript aliases add module evidence (#374, #563).
        M::up(concat!(
            r#"
            CREATE TABLE cargo_packages (
                manifest_path TEXT PRIMARY KEY,
                directory TEXT NOT NULL,
                crate_name TEXT NOT NULL
            );
            CREATE INDEX idx_cargo_packages_name ON cargo_packages(crate_name);
            CREATE TABLE ts_configs (manifest_path TEXT PRIMARY KEY, directory TEXT NOT NULL, error TEXT);
            CREATE TABLE ts_paths (
                manifest_path TEXT NOT NULL REFERENCES ts_configs(manifest_path) ON DELETE CASCADE,
                position INTEGER NOT NULL,
                pattern TEXT NOT NULL,
                target TEXT NOT NULL,
                PRIMARY KEY (manifest_path, position)
            );
            CREATE TABLE ts_config_parents (
                manifest_path TEXT NOT NULL REFERENCES ts_configs(manifest_path) ON DELETE CASCADE,
                parent_path TEXT NOT NULL,
                PRIMARY KEY (manifest_path, parent_path)
            );
            CREATE INDEX idx_ts_config_parents_parent ON ts_config_parents(parent_path);
            CREATE VIEW ts_alias_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id AND cf.language = 'javascript_typescript'
                JOIN ts_configs config ON (config.directory = '' OR substr(cf.relative_path, 1, length(config.directory) + 1) = config.directory || '/')
                JOIN ts_paths mapping ON mapping.manifest_path = config.manifest_path
                JOIN files sf ON sf.language = 'javascript_typescript'
                JOIN symbols s ON s.file_id = sf.id AND s.kind = 'module'
                WHERE r.targets_parsed = 1 AND r.external = 0 AND r.kind = 'imports'
                  AND r.target_kind = 'module' AND substr(r.module, 1, 2) = '@/'
                  AND NOT EXISTS (SELECT 1 FROM ts_configs nearer
                      WHERE length(nearer.directory) > length(config.directory)
                        AND (config.directory = '' OR substr(nearer.directory, 1, length(config.directory) + 1) = config.directory || '/')
                        AND substr(cf.relative_path, 1, length(nearer.directory) + 1) = nearer.directory || '/')
                  AND substr(r.module, 1, instr(mapping.pattern, '*') - 1) = substr(mapping.pattern, 1, instr(mapping.pattern, '*') - 1)
                  AND substr(r.module, length(r.module) - length(mapping.pattern) + instr(mapping.pattern, '*') + 1) = substr(mapping.pattern, instr(mapping.pattern, '*') + 1)
                  AND length(r.module) >= length(mapping.pattern) - 1
                  AND sf.relative_path IN (
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)),
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.ts',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.tsx',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.js',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.jsx',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '/index.ts',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '/index.js'
                  );
            UPDATE files SET content_hash = '' WHERE language = 'javascript_typescript';
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                    UNION ALL
                    SELECT symbol_id FROM ts_alias_candidates
                    WHERE relation_id = r.id AND substr(r.module, 1, 2) = '@/'
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND ((substr(r.module, 1, 2) = '@/'
                        AND s.id IN (SELECT symbol_id FROM ts_alias_candidates
                                     WHERE relation_id = r.id))
                  OR (r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL OR sf.relative_path = r.target_path)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0
                       OR (cf.language = 'rust' AND EXISTS (
                           SELECT 1 FROM cargo_packages cp
                           WHERE cp.crate_name = r.module
                             AND (cp.directory = '' OR substr(sf.relative_path, 1, length(cp.directory) + 1) = cp.directory || '/')
                             AND NOT EXISTS (SELECT 1 FROM cargo_packages nested
                                 WHERE length(nested.directory) > length(cp.directory)
                                   AND (cp.directory = '' OR substr(nested.directory, 1, length(cp.directory) + 1) = cp.directory || '/')
                                   AND substr(sf.relative_path, 1, length(nested.directory) + 1) = nested.directory || '/')
                       )))
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.top_level = 0 OR s.kind NOT IN ('method', 'field'))
                  AND (s.file_scoped = 0 OR sf.id = cf.id)
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND NOT (r.kind IN ('extends', 'implements')
                           AND sf.language IN ('csharp', 'cpp')
                           AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  AND (sf.id = cf.id
                       OR s.kind <> 'function'
                       OR s.end_line IS NULL
                       OR NOT EXISTS (
                           SELECT 1 FROM symbols p
                           WHERE p.file_id = s.file_id
                             AND p.id <> s.id
                             AND p.kind IN ('function', 'method')
                             AND (p.line < s.line
                                  OR (p.line = s.line AND p.column < s.column))
                             AND (p.end_line > s.end_line
                                  OR (p.line = s.line AND p.end_line = s.line
                                      AND s.end_line = s.line
                                      AND p.column + p.byte_len >= s.column + s.byte_len))))))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // A root-relative web URL resolves under the web root its app's
        // framework declares, not always `public/` (#567, ADR-044).
        M::up(concat!(
            r#"
            ALTER TABLE relations ADD COLUMN web_app TEXT;
            CREATE TABLE web_roots (
                manifest_path TEXT PRIMARY KEY,
                directory TEXT NOT NULL,
                root TEXT NOT NULL
            );
            CREATE INDEX idx_web_roots_directory ON web_roots(directory);
            UPDATE files SET content_hash = '' WHERE language IN ('html', 'css');
            DROP VIEW relation_candidates;
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                    UNION ALL
                    SELECT symbol_id FROM ts_alias_candidates
                    WHERE relation_id = r.id AND substr(r.module, 1, 2) = '@/'
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND ((substr(r.module, 1, 2) = '@/'
                        AND s.id IN (SELECT symbol_id FROM ts_alias_candidates
                                     WHERE relation_id = r.id))
                  OR (r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL
                       OR sf.relative_path = CASE
                           WHEN r.web_app IS NULL THEN r.target_path
                           ELSE COALESCE(
                               (SELECT CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || w.root || '/'
                                FROM (SELECT wr.root, 2 AS rank FROM web_roots wr
                                      WHERE wr.directory = r.web_app
                                      UNION ALL
                                      SELECT 'static', 1 FROM files cfg
                                      WHERE cfg.relative_path IN (
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.js',
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.ts',
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.mjs',
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.cjs')) w
                                ORDER BY w.rank DESC LIMIT 1)
                               || substr(r.target_path, length(CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END) + 8),
                               r.target_path)
                       END)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0
                       OR (cf.language = 'rust' AND EXISTS (
                           SELECT 1 FROM cargo_packages cp
                           WHERE cp.crate_name = r.module
                             AND (cp.directory = '' OR substr(sf.relative_path, 1, length(cp.directory) + 1) = cp.directory || '/')
                             AND NOT EXISTS (SELECT 1 FROM cargo_packages nested
                                 WHERE length(nested.directory) > length(cp.directory)
                                   AND (cp.directory = '' OR substr(nested.directory, 1, length(cp.directory) + 1) = cp.directory || '/')
                                   AND substr(sf.relative_path, 1, length(nested.directory) + 1) = nested.directory || '/')
                       )))
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.top_level = 0 OR s.kind NOT IN ('method', 'field'))
                  AND (s.file_scoped = 0 OR sf.id = cf.id)
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND NOT (r.kind IN ('extends', 'implements')
                           AND sf.language IN ('csharp', 'cpp')
                           AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  AND (sf.id = cf.id
                       OR s.kind <> 'function'
                       OR s.end_line IS NULL
                       OR NOT EXISTS (
                           SELECT 1 FROM symbols p
                           WHERE p.file_id = s.file_id
                             AND p.id <> s.id
                             AND p.kind IN ('function', 'method')
                             AND (p.line < s.line
                                  OR (p.line = s.line AND p.column < s.column))
                             AND (p.end_line > s.end_line
                                  OR (p.line = s.line AND p.end_line = s.line
                                      AND s.end_line = s.line
                                      AND p.column + p.byte_len >= s.column + s.byte_len))))))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
        // A `#…` import resolves through the nearest package.json
        // `imports` map (#563, ADR-045).
        M::up(concat!(
            r#"
            CREATE TABLE package_manifests (
                manifest_path TEXT PRIMARY KEY,
                directory TEXT NOT NULL
            );
            CREATE TABLE package_imports (
                manifest_path TEXT NOT NULL,
                position INTEGER NOT NULL,
                pattern TEXT NOT NULL,
                target TEXT NOT NULL,
                PRIMARY KEY (manifest_path, position)
            );
            DROP VIEW relation_candidates;
            DROP VIEW ts_alias_candidates;
            CREATE VIEW ts_alias_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id AND cf.language = 'javascript_typescript'
                JOIN ts_configs config ON (config.directory = '' OR substr(cf.relative_path, 1, length(config.directory) + 1) = config.directory || '/')
                JOIN ts_paths mapping ON mapping.manifest_path = config.manifest_path
                JOIN files sf ON sf.language = 'javascript_typescript'
                JOIN symbols s ON s.file_id = sf.id AND s.kind = 'module'
                WHERE r.targets_parsed = 1 AND r.external = 0 AND r.kind = 'imports'
                  AND r.target_kind = 'module' AND substr(r.module, 1, 2) = '@/'
                  AND NOT EXISTS (SELECT 1 FROM ts_configs nearer
                      WHERE length(nearer.directory) > length(config.directory)
                        AND (config.directory = '' OR substr(nearer.directory, 1, length(config.directory) + 1) = config.directory || '/')
                        AND substr(cf.relative_path, 1, length(nearer.directory) + 1) = nearer.directory || '/')
                  AND substr(r.module, 1, instr(mapping.pattern, '*') - 1) = substr(mapping.pattern, 1, instr(mapping.pattern, '*') - 1)
                  AND substr(r.module, length(r.module) - length(mapping.pattern) + instr(mapping.pattern, '*') + 1) = substr(mapping.pattern, instr(mapping.pattern, '*') + 1)
                  AND length(r.module) >= length(mapping.pattern) - 1
                  AND sf.relative_path IN (
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)),
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.ts',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.tsx',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.js',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '.jsx',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '/index.ts',
                      replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) || '/index.js'
                  )
                UNION ALL
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id AND cf.language = 'javascript_typescript'
                JOIN package_manifests package ON (package.directory = '' OR substr(cf.relative_path, 1, length(package.directory) + 1) = package.directory || '/')
                JOIN package_imports mapping ON mapping.manifest_path = package.manifest_path
                JOIN files sf ON sf.language = 'javascript_typescript'
                JOIN symbols s ON s.file_id = sf.id AND s.kind = 'module'
                WHERE r.targets_parsed = 1 AND r.external = 0 AND r.kind = 'imports'
                  AND r.target_kind = 'module' AND substr(r.module, 1, 1) = '#'
                  AND NOT EXISTS (SELECT 1 FROM package_manifests nearer
                      WHERE length(nearer.directory) > length(package.directory)
                        AND (package.directory = '' OR substr(nearer.directory, 1, length(package.directory) + 1) = package.directory || '/')
                        AND substr(cf.relative_path, 1, length(nearer.directory) + 1) = nearer.directory || '/')
                  AND ((instr(mapping.pattern, '*') = 0 AND r.module = mapping.pattern)
                       OR (instr(mapping.pattern, '*') > 0
                           AND substr(r.module, 1, instr(mapping.pattern, '*') - 1) = substr(mapping.pattern, 1, instr(mapping.pattern, '*') - 1)
                           AND substr(r.module, length(r.module) - length(mapping.pattern) + instr(mapping.pattern, '*') + 1) = substr(mapping.pattern, instr(mapping.pattern, '*') + 1)
                           AND length(r.module) >= length(mapping.pattern)))
                  AND sf.relative_path IN (
                      (CASE WHEN instr(mapping.pattern, '*') = 0 THEN mapping.target ELSE replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) END),
                      (CASE WHEN instr(mapping.pattern, '*') = 0 THEN mapping.target ELSE replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) END) || '.ts',
                      (CASE WHEN instr(mapping.pattern, '*') = 0 THEN mapping.target ELSE replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) END) || '.tsx',
                      (CASE WHEN instr(mapping.pattern, '*') = 0 THEN mapping.target ELSE replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) END) || '.js',
                      (CASE WHEN instr(mapping.pattern, '*') = 0 THEN mapping.target ELSE replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) END) || '.jsx',
                      (CASE WHEN instr(mapping.pattern, '*') = 0 THEN mapping.target ELSE replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) END) || '/index.ts',
                      (CASE WHEN instr(mapping.pattern, '*') = 0 THEN mapping.target ELSE replace(mapping.target, '*', substr(r.module, instr(mapping.pattern, '*'), length(r.module) - length(mapping.pattern) + 1)) END) || '/index.js'
                  );
            UPDATE files SET content_hash = '' WHERE language = 'javascript_typescript';
            "#,
            r#"CREATE VIEW relation_candidates AS
                SELECT r.id AS relation_id, s.id AS symbol_id
                FROM relations r
                JOIN symbols caller ON caller.id = r.from_symbol_id
                JOIN files cf ON cf.id = caller.file_id
                JOIN symbols s ON s.id IN (
                    SELECT id FROM symbols WHERE name = r.to_name
                    UNION ALL
                    SELECT symbol_id FROM symbol_aliases
                    WHERE alias = r.to_name AND r.target_kind = 'module'
                      AND r.target_path IS NULL
                    UNION ALL
                    SELECT symbol_id FROM ts_alias_candidates
                    WHERE relation_id = r.id AND (substr(r.module, 1, 2) = '@/' OR substr(r.module, 1, 1) = '#')
                )
                CROSS JOIN files sf ON sf.id = s.file_id
                WHERE r.targets_parsed = 1
                  AND (((substr(r.module, 1, 2) = '@/' OR substr(r.module, 1, 1) = '#')
                        AND s.id IN (SELECT symbol_id FROM ts_alias_candidates
                                     WHERE relation_id = r.id))
                  OR (r.to_name IN (SELECT s.name
                                    UNION ALL
                                    SELECT alias FROM symbol_aliases
                                    WHERE symbol_id = s.id)
                  AND r.external = 0
                  AND sf.language = COALESCE(r.target_language, cf.language)
                  AND (r.qualifier IS NULL
                       OR s.parent = r.qualifier
                       OR substr(s.parent, 1, length(r.qualifier) + 1) = r.qualifier || '<'
                       OR (EXISTS (SELECT 1 FROM relations sr
                                       WHERE sr.to_name = s.parent
                                         AND sr.kind IN ('extends', 'implements'))
                           AND NOT EXISTS (SELECT 1 FROM symbols own
                                           JOIN files ownf ON ownf.id = own.file_id
                                           WHERE own.name = r.to_name
                                             AND own.parent = r.qualifier
                                             AND ownf.language = sf.language)
                           AND s.parent IN (
                           WITH RECURSIVE ancestors(name, depth) AS (
                               SELECT r.qualifier, 0
                               UNION
                               SELECT ir.to_name, a.depth + 1
                               FROM ancestors a
                               JOIN symbols t ON t.name = a.name
                               JOIN files tf ON tf.id = t.file_id
                                            AND tf.language = sf.language
                               JOIN relations ir ON ir.from_symbol_id = t.id
                                                AND ir.kind IN ('extends', 'implements')
                               WHERE a.depth < 8
                           )
                           SELECT a.name FROM ancestors a
                           WHERE a.depth > 0
                             AND NOT EXISTS (SELECT 1 FROM ancestors b
                                             JOIN symbols own ON own.parent = b.name
                                                             AND own.name = r.to_name
                                             JOIN files ownf ON ownf.id = own.file_id
                                                            AND ownf.language = sf.language
                                             WHERE b.depth < a.depth))))
                  AND (r.target_path IS NULL
                       OR sf.relative_path = CASE
                           WHEN r.web_app IS NULL THEN r.target_path
                           ELSE COALESCE(
                               (SELECT CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || w.root || '/'
                                FROM (SELECT wr.root, 2 AS rank FROM web_roots wr
                                      WHERE wr.directory = r.web_app
                                      UNION ALL
                                      SELECT 'static', 1 FROM files cfg
                                      WHERE cfg.relative_path IN (
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.js',
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.ts',
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.mjs',
                                          CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END || 'svelte.config.cjs')) w
                                ORDER BY w.rank DESC LIMIT 1)
                               || substr(r.target_path, length(CASE WHEN r.web_app = '' THEN '' ELSE r.web_app || '/' END) + 8),
                               r.target_path)
                       END)
                  AND (r.module IS NULL
                       OR s.parent = r.module
                       OR EXISTS (SELECT 1 FROM symbols pt
                                  WHERE pt.file_id = s.file_id AND pt.name = s.parent
                                    AND pt.parent = r.module)
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '/') > 0
                       OR instr('/' || replace(sf.relative_path, '-', '_'), '/' || r.module || '.') > 0
                       OR (cf.language = 'rust' AND EXISTS (
                           SELECT 1 FROM cargo_packages cp
                           WHERE cp.crate_name = r.module
                             AND (cp.directory = '' OR substr(sf.relative_path, 1, length(cp.directory) + 1) = cp.directory || '/')
                             AND NOT EXISTS (SELECT 1 FROM cargo_packages nested
                                 WHERE length(nested.directory) > length(cp.directory)
                                   AND (cp.directory = '' OR substr(nested.directory, 1, length(cp.directory) + 1) = cp.directory || '/')
                                   AND substr(sf.relative_path, 1, length(nested.directory) + 1) = nested.directory || '/')
                       )))
                  AND (r.member = 0 OR s.kind = 'method')
                  AND (r.not_field = 0 OR s.kind <> 'field')
                  AND (r.top_level = 0 OR s.kind NOT IN ('method', 'field'))
                  AND (s.file_scoped = 0 OR sf.id = cf.id)
                  AND (r.target_kind IS NULL OR s.kind = r.target_kind)
                  AND (r.target_module IS NULL
                       OR EXISTS (SELECT 1 FROM symbols m
                                  WHERE m.file_id = s.file_id AND m.kind = 'module'
                                    AND (m.name = r.target_module
                                         OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                    WHERE a.symbol_id = m.id
                                                      AND a.alias = r.target_module)))
                       OR (s.parent IS NOT r.qualifier
                           AND EXISTS (SELECT 1 FROM symbols qt
                                       JOIN symbols m ON m.file_id = qt.file_id
                                                     AND m.kind = 'module'
                                       WHERE qt.name = r.qualifier
                                         AND qt.parent IS NULL
                                         AND (m.name = r.target_module
                                              OR EXISTS (SELECT 1 FROM symbol_aliases a
                                                         WHERE a.symbol_id = m.id
                                                           AND a.alias = r.target_module)))))
                  AND NOT (r.kind = 'calls' AND s.kind = 'module')
                  AND NOT (r.kind IN ('extends', 'implements')
                           AND sf.language IN ('csharp', 'cpp')
                           AND s.kind IN ('method', 'function', 'field', 'variable', 'constant'))
                  AND (sf.id = cf.id
                       OR s.kind <> 'variable'
                       OR s.parent IS NULL
                       OR NOT EXISTS (SELECT 1 FROM symbols p
                                      WHERE p.file_id = s.file_id AND p.name = s.parent
                                        AND p.kind IN ('function', 'method'))
                       OR EXISTS (SELECT 1 FROM symbols p
                                  WHERE p.file_id = s.file_id AND p.name = s.parent
                                    AND p.kind NOT IN ('function', 'method', 'variable',
                                                       'constant', 'field')))
                  AND (sf.id = cf.id
                       OR s.kind <> 'function'
                       OR s.end_line IS NULL
                       OR NOT EXISTS (
                           SELECT 1 FROM symbols p
                           WHERE p.file_id = s.file_id
                             AND p.id <> s.id
                             AND p.kind IN ('function', 'method')
                             AND (p.line < s.line
                                  OR (p.line = s.line AND p.column < s.column))
                             AND (p.end_line > s.end_line
                                  OR (p.line = s.line AND p.end_line = s.line
                                      AND s.end_line = s.line
                                      AND p.column + p.byte_len >= s.column + s.byte_len))))))
                  "#,
            shadowed_module_predicate!(),
            ";",
        )),
    ])
}

#[cfg(test)]
mod module_shadow_tests;

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::migrations;

    #[test]
    fn join_point_migration_queues_existing_files_for_reparse(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 19)?;
        conn.execute(
            "INSERT INTO files (relative_path, language, content_hash, last_indexed_at)
             VALUES ('api.rs', 'rust', 'old-hash', 0)",
            [],
        )?;
        migrations().to_latest(&mut conn)?;
        let hash: String = conn.query_row("SELECT content_hash FROM files", [], |r| r.get(0))?;
        let points: u32 = conn.query_row("SELECT COUNT(*) FROM join_points", [], |r| r.get(0))?;
        assert!(hash.is_empty());
        assert_eq!(points, 0);
        Ok(())
    }

    #[test]
    fn alias_migration_reparses_without_inventing_legacy_aliases(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 10)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at)
                 VALUES (1, 'Other.md', 'markdown', 'old-hash', 0);
             INSERT INTO symbols (id, file_id, name, kind, line, column, byte_len)
                 VALUES (1, 1, 'Other', 'module', 1, 1, 1), (2, 1, 'Alpha', 'element', 2, 1, 1);
             INSERT INTO relations (from_symbol_id, kind, to_name, line, column, byte_len)
                 VALUES (1, 'references', 'alias:Alpha', 1, 1, 0),
                        (1, 'references', 'Alpha', 3, 1, 1);",
        )?;
        migrations().to_latest(&mut conn)?;
        let aliases: i64 =
            conn.query_row("SELECT COUNT(*) FROM symbol_aliases", [], |r| r.get(0))?;
        let candidates: i64 =
            conn.query_row("SELECT COUNT(*) FROM relation_candidates", [], |r| r.get(0))?;
        assert_eq!(aliases, 0);
        assert_eq!(candidates, 0);
        let hash: String = conn.query_row("SELECT content_hash FROM files", [], |r| r.get(0))?;
        assert_eq!(hash, "");
        conn.execute("UPDATE files SET content_hash = 'reparsed'", [])?;
        migrations().to_latest(&mut conn)?;
        let hash: String = conn.query_row("SELECT content_hash FROM files", [], |r| r.get(0))?;
        assert_eq!(hash, "reparsed", "hash invalidation runs only once");
        Ok(())
    }

    /// The literals migration forces a full re-parse of an existing index.
    #[test]
    fn literals_migration_invalidates_every_content_hash() -> Result<(), Box<dyn std::error::Error>>
    {
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

    /// Declaration details (ADR-034): migrating an index that already holds
    /// data keeps every row, creates both empty tables and forces one reparse
    /// so parsers can record details.
    #[test]
    fn symbol_details_migration_keeps_rows_and_forces_one_reparse(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 15)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at)
                 VALUES (1, 'src/lib.rs', 'rust', 'abc123', 0);
             INSERT INTO symbols (id, file_id, name, kind, line, column, byte_len)
                 VALUES (1, 1, 'run', 'function', 1, 1, 1);",
        )?;
        migrations().to_latest(&mut conn)?;
        let count = |sql: &str| -> rusqlite::Result<i64> { conn.query_row(sql, [], |r| r.get(0)) };
        assert_eq!(count("SELECT COUNT(*) FROM symbols")?, 1);
        assert_eq!(count("SELECT COUNT(*) FROM symbol_details")?, 0);
        assert_eq!(count("SELECT COUNT(*) FROM symbol_parameters")?, 0);
        let hash: String = conn.query_row("SELECT content_hash FROM files", [], |r| r.get(0))?;
        assert_eq!(hash, "");
        Ok(())
    }

    fn assert_legacy_migration(
        version: usize,
        seed: &str,
        expected_candidates: i64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, version)?;
        conn.execute_batch(seed)?;
        migrations().to_latest(&mut conn)?;
        let candidates: i64 =
            conn.query_row("SELECT COUNT(*) FROM relation_candidates", [], |r| r.get(0))?;
        assert_eq!(candidates, expected_candidates);
        let hash: String = conn.query_row("SELECT content_hash FROM files", [], |r| r.get(0))?;
        assert_eq!(hash, "");
        Ok(())
    }

    /// A relation written before resolution existed is never resolved by
    /// name after the migration — not even to the only same-named symbol —
    /// and its file is queued for the reparse that records real evidence.
    #[test]
    fn resolution_migration_leaves_legacy_relations_unresolved(
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_legacy_migration(
            8,
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at)
                 VALUES (1, 'src/lib.rs', 'rust', 'abc123', 0);
             INSERT INTO symbols (id, file_id, name, kind, line, column, byte_len)
                 VALUES (1, 1, 'main', 'function', 1, 1, 1), (2, 1, 'helper', 'function', 2, 1, 1);
             INSERT INTO relations (from_symbol_id, kind, to_name, line, column, byte_len)
                 VALUES (1, 'calls', 'helper', 1, 1, 1);",
            0,
        )
    }

    /// Migration 15 (ADR-035): a variable declared inside a function is a
    /// candidate only for a relation from its own file. A file-level
    /// variable, a global first assigned in a function (no parent) and a
    /// variable under a same-named container stay candidates from anywhere;
    /// a same-named variable is no container. Applied to an index that already holds the data, with no
    /// reparse: the view reads only columns the index already has.
    #[test]
    fn function_local_variable_migration_scopes_candidates_to_their_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 14)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
                 (1, 'lua/physics.lua', 'lua', 'h1', 0),
                 (2, 'lua/hud.lua', 'lua', 'h2', 0);
             INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len) VALUES
                 (1, 1, 'step', 'function', NULL, 1, 1, 1),
                 (2, 1, 'speed', 'variable', 'step', 2, 1, 1),
                 (3, 1, 'gravity', 'variable', NULL, 3, 1, 1),
                 (4, 1, 'Body', 'class', NULL, 4, 1, 1),
                 (5, 1, 'Body', 'function', NULL, 5, 1, 1),
                 (6, 1, 'mass', 'variable', 'Body', 6, 1, 1),
                 (7, 2, 'draw', 'function', NULL, 1, 1, 1),
                 (8, 1, 'remaining', 'function', NULL, 7, 1, 1),
                 (9, 1, 'now', 'variable', 'remaining', 8, 1, 1),
                 (10, 1, 'remaining', 'variable', 'step', 9, 1, 1);
             INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                    targets_parsed) VALUES
                 (1, 7, 'calls', 'speed', 4, 1, 1, 1),
                 (2, 1, 'references', 'speed', 3, 1, 1, 1),
                 (3, 7, 'references', 'gravity', 5, 1, 1, 1),
                 (4, 7, 'references', 'mass', 6, 1, 1, 1),
                 (5, 7, 'calls', 'now', 7, 1, 1, 1);",
        )?;
        migrations().to_version(&mut conn, 15)?;
        let candidates = |relation: i64| -> rusqlite::Result<i64> {
            conn.query_row(
                "SELECT COUNT(*) FROM relation_candidates WHERE relation_id = ?1",
                [relation],
                |r| r.get(0),
            )
        };
        assert_eq!(candidates(1)?, 0, "another file's function-local `speed`");
        assert_eq!(candidates(2)?, 1, "the same file's `speed`");
        assert_eq!(candidates(3)?, 1, "a file-level variable");
        assert_eq!(candidates(4)?, 1, "a parent that is not only a function");
        assert_eq!(
            candidates(5)?,
            0,
            "a same-named variable can't be the parent of `now`"
        );
        let hash: String =
            conn.query_row("SELECT content_hash FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        assert_eq!(hash, "h1", "no reparse needed");
        Ok(())
    }

    /// ADR-036: an existing row keeps its field candidate and every file is
    /// queued for reparse; a row with `not_field` keeps every other kind.
    #[test]
    fn not_field_migration_rules_out_only_fields() -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 16)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
                 (1, 'shop/page.py', 'python', 'h1', 0),
                 (2, 'shop/report.py', 'python', 'h2', 0);
             INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len) VALUES
                 (1, 1, 'Page', 'class', NULL, 1, 1, 1),
                 (2, 1, 'items', 'field', 'Page', 2, 1, 1),
                 (3, 1, 'Cart', 'class', NULL, 3, 1, 1),
                 (4, 1, 'items', 'method', 'Cart', 4, 1, 1),
                 (5, 2, 'report', 'function', NULL, 1, 1, 1);
             INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                    targets_parsed) VALUES
                 (1, 5, 'calls', 'items', 2, 1, 1, 1);",
        )?;
        migrations().to_version(&mut conn, 17)?;
        let candidates = |conn: &Connection| -> rusqlite::Result<Vec<i64>> {
            let mut stmt = conn.prepare(
                "SELECT symbol_id FROM relation_candidates WHERE relation_id = 1 ORDER BY 1",
            )?;
            let ids = stmt.query_map([], |r| r.get(0))?.collect();
            ids
        };
        assert_eq!(candidates(&conn)?, [2, 4], "an existing row is unchanged");
        let hash: String =
            conn.query_row("SELECT content_hash FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        assert_eq!(hash, "", "every file is reparsed");
        conn.execute("UPDATE relations SET not_field = 1 WHERE id = 1", [])?;
        assert_eq!(
            candidates(&conn)?,
            [4],
            "the field is ruled out, the method kept"
        );
        Ok(())
    }

    /// The note-graph migration keeps every existing relation's candidates
    /// (the new constraints are NULL there) and queues a reparse so notes get
    /// their module symbols and path-aware targets.
    #[test]
    fn note_graph_migration_invalidates_hashes_and_constrains_nothing_retroactively(
    ) -> Result<(), Box<dyn std::error::Error>> {
        assert_legacy_migration(
            9,
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at)
                 VALUES (1, 'a.md', 'markdown', 'abc123', 0);
             INSERT INTO symbols (id, file_id, name, kind, line, column, byte_len)
                 VALUES (1, 1, 'A', 'element', 1, 1, 1), (2, 1, 'B', 'element', 2, 1, 1);
             INSERT INTO relations (from_symbol_id, kind, to_name, line, column, byte_len, targets_parsed)
                 VALUES (1, 'references', 'B', 1, 1, 1, 1);",
            1,
        )
    }

    /// Migration 18 (ADR-037): a type-qualified relation also reaches a
    /// method of an ancestor the index records for that type, unless the
    /// type or a nearer ancestor declares a member of that name. Applied to an index
    /// that already holds the data, with no reparse.
    #[test]
    fn inherited_member_migration_reaches_only_a_recorded_ancestor(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 17)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
                 (1, 'Repo.cs', 'csharp', 'h1', 0),
                 (2, 'Repo.java', 'java', 'h2', 0);
             INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len) VALUES
                 (1, 1, 'Base', 'class', NULL, 1, 1, 1),
                 (2, 1, 'Save', 'method', 'Base', 2, 1, 1),
                 (3, 1, 'Find', 'method', 'Base', 3, 1, 1),
                 (4, 1, 'Child', 'class', NULL, 4, 1, 1),
                 (5, 1, 'Find', 'method', 'Child', 5, 1, 1),
                 (6, 1, 'Old', 'method', 'Grand', 6, 1, 1),
                 (7, 2, 'Other', 'class', NULL, 1, 1, 1);
             INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                    targets_parsed, qualifier) VALUES
                 (1, 4, 'extends', 'Base', 4, 1, 1, 1, NULL),
                 (2, 7, 'extends', 'Base', 1, 1, 1, 1, NULL),
                 (3, 5, 'calls', 'Save', 5, 1, 1, 1, 'Child'),
                 (4, 5, 'calls', 'Find', 5, 1, 1, 1, 'Child'),
                 (5, 5, 'calls', 'Old', 5, 1, 1, 1, 'Child'),
                 (6, 7, 'calls', 'Save', 1, 1, 1, 1, 'Other');",
        )?;
        let candidates = |conn: &Connection, relation: i64| -> rusqlite::Result<Vec<i64>> {
            let mut stmt = conn.prepare(
                "SELECT symbol_id FROM relation_candidates WHERE relation_id = ?1
                 ORDER BY symbol_id",
            )?;
            let ids = stmt
                .query_map([relation], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>();
            ids
        };
        assert!(candidates(&conn, 3)?.is_empty(), "before: no candidate");
        migrations().to_version(&mut conn, 18)?;
        assert_eq!(candidates(&conn, 3)?, [2], "Base's Save, inherited");
        assert_eq!(
            candidates(&conn, 4)?,
            [5],
            "Child's own Find shadows Base's"
        );
        assert!(
            candidates(&conn, 5)?.is_empty(),
            "Grand is no recorded ancestor"
        );
        assert!(
            candidates(&conn, 6)?.is_empty(),
            "a Java type's supertype is no C# method's"
        );
        let hash: String =
            conn.query_row("SELECT content_hash FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        assert_eq!(hash, "h1", "no reparse needed");
        Ok(())
    }

    /// Migration 22 (issue #388): in C# and C++ a supertype relation names a
    /// type, so a constructor or function spelled like the class is no
    /// candidate. Other languages keep non-type candidates: a JS class may
    /// extend a constructor function, a Python class a variable holding one.
    #[test]
    fn file_scoped_migration_offers_a_symbol_only_to_its_own_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 24)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
                 (1, 'Aliases.cs', 'csharp', 'h1', 0),
                 (2, 'Program.cs', 'csharp', 'h2', 0);
             INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len) VALUES
                 (1, 1, 'Money', 'type_alias', NULL, 1, 1, 1),
                 (2, 1, 'Ledger', 'class', NULL, 2, 1, 1),
                 (3, 2, 'Run', 'method', NULL, 1, 1, 1);
             INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                    targets_parsed) VALUES
                 (1, 3, 'calls', 'Money', 1, 1, 1, 1),
                 (2, 2, 'references', 'Money', 2, 1, 1, 1);",
        )?;
        let candidates = |conn: &Connection, relation: i64| -> rusqlite::Result<Vec<i64>> {
            conn.prepare("SELECT symbol_id FROM relation_candidates WHERE relation_id = ?1")?
                .query_map([relation], |r| r.get(0))?
                .collect()
        };
        migrations().to_version(&mut conn, 25)?;
        assert_eq!(
            candidates(&conn, 1)?,
            [1],
            "an existing row is not file-scoped"
        );
        conn.execute("UPDATE symbols SET file_scoped = 1 WHERE id = 1", [])?;
        let empty: [i64; 0] = [];
        assert_eq!(candidates(&conn, 1)?, empty, "another file never names it");
        assert_eq!(candidates(&conn, 2)?, [1], "its own file does");
        let hash: String =
            conn.query_row("SELECT content_hash FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        assert_eq!(hash, "", "every file is reparsed");
        Ok(())
    }

    #[test]
    fn top_level_migration_drops_type_members_of_bare_names(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 23)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
                 (1, 'models.ts', 'javascript_typescript', 'h1', 0),
                 (2, 'app.ts', 'javascript_typescript', 'h2', 0);
             INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len) VALUES
                 (1, 1, 'format', 'method', 'Money', 1, 1, 1),
                 (2, 1, 'format', 'field', 'Bridge', 2, 1, 1),
                 (3, 1, 'format', 'function', NULL, 3, 1, 1),
                 (4, 2, 'main', 'function', NULL, 1, 1, 1);
             INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                    targets_parsed) VALUES
                 (1, 4, 'calls', 'format', 2, 1, 1, 1),
                 (2, 4, 'calls', 'format', 3, 1, 1, 1);",
        )?;
        let candidates = |conn: &Connection, relation: i64| -> rusqlite::Result<Vec<i64>> {
            conn.prepare(
                "SELECT symbol_id FROM relation_candidates WHERE relation_id = ?1
                 ORDER BY symbol_id",
            )?
            .query_map([relation], |r| r.get(0))?
            .collect()
        };
        migrations().to_version(&mut conn, 24)?;
        assert_eq!(
            candidates(&conn, 1)?,
            [1, 2, 3],
            "an existing row keeps them"
        );
        conn.execute("UPDATE relations SET top_level = 1 WHERE id = 1", [])?;
        assert_eq!(candidates(&conn, 1)?, [3], "a bare name: the function only");
        assert_eq!(
            candidates(&conn, 2)?,
            [1, 2, 3],
            "a member access keeps members"
        );
        let hash: String =
            conn.query_row("SELECT content_hash FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        assert_eq!(hash, "", "every file is reparsed");
        Ok(())
    }

    #[test]
    fn nested_function_migration_scopes_candidates_to_their_file(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 22)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
                 (1, 'lib.js', 'javascript', 'h1', 0),
                 (2, 'bundle.js', 'javascript', 'h2', 0),
                 (3, 'app.js', 'javascript', 'h3', 0);
             INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len,
                                  end_line) VALUES
                 (1, 1, 'outer', 'function', NULL, 1, 1, 100, 10),
                 (2, 1, 'inner', 'function', NULL, 2, 3, 10, 3),
                 (3, 1, 'top', 'function', NULL, 12, 1, 30, 14),
                 (4, 1, 'last', 'function', NULL, 20, 1, 40, 25),
                 (5, 1, 'tail', 'function', NULL, 24, 3, 9, 25),
                 (6, 1, 'legacy', 'function', NULL, 5, 3, 9, NULL),
                 (7, 2, 'wrap', 'function', NULL, 1, 1, 500, 1),
                 (8, 2, 'em', 'function', NULL, 1, 50, 20, 1),
                 (9, 2, 'a', 'function', NULL, 1, 600, 10, 1),
                 (10, 2, 'b', 'function', NULL, 1, 620, 10, 1),
                 (11, 3, 'main', 'function', NULL, 1, 1, 50, 9);
             INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                    targets_parsed) VALUES
                 (1, 11, 'calls', 'inner', 2, 1, 1, 1),
                 (2, 11, 'calls', 'em', 3, 1, 1, 1),
                 (3, 11, 'calls', 'a', 4, 1, 1, 1),
                 (4, 11, 'calls', 'b', 5, 1, 1, 1),
                 (5, 11, 'calls', 'top', 6, 1, 1, 1),
                 (6, 11, 'calls', 'tail', 7, 1, 1, 1),
                 (7, 11, 'calls', 'legacy', 8, 1, 1, 1),
                 (8, 1, 'calls', 'inner', 4, 3, 1, 1);",
        )?;
        let candidates = |conn: &Connection, relation: i64| -> rusqlite::Result<Vec<i64>> {
            conn.prepare("SELECT symbol_id FROM relation_candidates WHERE relation_id = ?1")?
                .query_map([relation], |r| r.get(0))?
                .collect()
        };
        assert_eq!(
            candidates(&conn, 1)?,
            [2],
            "before: a nested function is offered"
        );
        migrations().to_version(&mut conn, 23)?;
        let empty: [i64; 0] = [];
        assert_eq!(candidates(&conn, 1)?, empty, "nested, from another file");
        assert_eq!(candidates(&conn, 2)?, empty, "nested on one minified line");
        assert_eq!(candidates(&conn, 3)?, [9], "a one-line sibling stays");
        assert_eq!(candidates(&conn, 4)?, [10], "a one-line sibling stays");
        assert_eq!(candidates(&conn, 5)?, [3], "a top-level function stays");
        assert_eq!(candidates(&conn, 6)?, [5], "same end line: ambiguous, kept");
        assert_eq!(candidates(&conn, 7)?, [6], "no end line: kept");
        assert_eq!(candidates(&conn, 8)?, [2], "its own file keeps it");
        let hash: String =
            conn.query_row("SELECT content_hash FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        assert_eq!(hash, "h1", "no reparse needed");
        Ok(())
    }

    #[test]
    fn supertype_migration_drops_non_type_candidates_in_csharp_and_cpp(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut conn = Connection::open_in_memory()?;
        crate::search::register_functions(&conn)?;
        migrations().to_version(&mut conn, 21)?;
        conn.execute_batch(
            "INSERT INTO files (id, relative_path, language, content_hash, last_indexed_at) VALUES
                 (1, 'Repo.cs', 'csharp', 'h1', 0),
                 (2, 'widget.cpp', 'cpp', 'h2', 0),
                 (3, 'base.js', 'javascript', 'h3', 0);
             INSERT INTO symbols (id, file_id, name, kind, parent, line, column, byte_len) VALUES
                 (1, 1, 'Repo', 'class', NULL, 1, 1, 1),
                 (2, 1, 'Repo', 'method', 'Repo', 2, 1, 1),
                 (3, 1, 'Stock', 'class', NULL, 3, 1, 1),
                 (4, 1, 'Stock', 'method', 'Stock', 4, 1, 1),
                 (5, 2, 'Widget', 'class', NULL, 1, 1, 1),
                 (6, 2, 'Widget', 'method', 'Widget', 2, 1, 1),
                 (7, 2, 'Button', 'class', NULL, 3, 1, 1),
                 (8, 3, 'Base', 'function', NULL, 1, 1, 1),
                 (9, 3, 'Child', 'class', NULL, 2, 1, 1);
             INSERT INTO relations (id, from_symbol_id, kind, to_name, line, column, byte_len,
                                    targets_parsed) VALUES
                 (1, 3, 'extends', 'Repo', 3, 1, 1, 1),
                 (2, 7, 'extends', 'Widget', 3, 1, 1, 1),
                 (3, 9, 'extends', 'Base', 2, 1, 1, 1),
                 (4, 4, 'calls', 'Repo', 4, 1, 1, 1);",
        )?;
        let candidates = |conn: &Connection, relation: i64| -> rusqlite::Result<Vec<i64>> {
            let mut stmt = conn.prepare(
                "SELECT symbol_id FROM relation_candidates WHERE relation_id = ?1
                 ORDER BY symbol_id",
            )?;
            let ids = stmt
                .query_map([relation], |r| r.get(0))?
                .collect::<rusqlite::Result<Vec<i64>>>();
            ids
        };
        assert_eq!(
            candidates(&conn, 1)?,
            [1, 2],
            "before: class and constructor"
        );
        migrations().to_version(&mut conn, 22)?;
        assert_eq!(candidates(&conn, 1)?, [1], "C#: the class only");
        assert_eq!(candidates(&conn, 2)?, [5], "C++: the class only");
        assert_eq!(
            candidates(&conn, 3)?,
            [8],
            "JS: a constructor function stays"
        );
        assert_eq!(
            candidates(&conn, 4)?,
            [1, 2],
            "a call keeps the constructor"
        );
        let hash: String =
            conn.query_row("SELECT content_hash FROM files WHERE id = 1", [], |r| {
                r.get(0)
            })?;
        assert_eq!(hash, "h1", "no reparse needed");
        Ok(())
    }
}
