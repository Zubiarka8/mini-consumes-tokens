# Parser contract (`mct-lang-*`)

Every `mct-lang-*` crate implements `mct_core::LanguageParser`:

- `language_id() -> &'static str`
- `file_extensions() -> &'static [&'static str]`
- `parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError>`

## Rules
- **Pure AST walk.** Never executes or evaluates input — parsers run over arbitrary, potentially adversarial repo content.
- **No panics on repo input.** A syntax error returns `ParseError::Syntax`, never `unwrap()`/`panic!`/`expect()` on a path that processes file content.
- **Bounded recursion.** Every recursive walker function threads a `depth: u32` counter and stops at `mct_core::MAX_TRAVERSAL_DEPTH` (256) — see [[sec-001-php-stack-overflow]] for why this exists and [[limits-spec]] for the exact value.
- **Shared model, no new variants without a cross-cutting review.** Emit `SymbolRecord`/`SymbolRelation` using the existing `SymbolKind`/`RelationKind` enums; a new variant touches every crate's `match` arms plus `mct-index`'s SQL mapping — needs an issue first.
- **Registration is the only integration point.** Add one line each to `mct-mcp-server/src/registry.rs::build_registry` and `mct-cli/src/main.rs::build_registry`. Nothing else in `mct-core`/`mct-index`/`mct-mcp-server` changes.

Exceptions with real security/complexity implications get their own note (e.g. [[mct-lang-php]]); everything else is just a row in [[00-index]]'s extension→crate table.

#architecture #contract #crate
