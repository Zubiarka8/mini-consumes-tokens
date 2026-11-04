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
- **Registration is the only integration point.** Add the dependency and one line to `mct-languages/src/lib.rs::build_registry`, the registry the CLI, the MCP server and `mct-eval` share. Nothing else in `mct-core`/`mct-index`/`mct-cli`/`mct-mcp-server` changes.

## Intentional duplication

Shared grammar-neutral helpers live in `mct-tree-sitter` (`first_error`, `location`, `module_location`, `find_child`, `find_named_child`). These stay copied in each `mct-lang-*` crate on purpose:

- `Walker::push_symbol` / `push_relation` (identical in 14 crates): each `Walker` owns its own `symbols`, `relations` and `next_id`, and a shared wrapper would only replace 19 lines with a 15-line forwarding method per crate. `mct-lang-md`, `-css` and `-rust` differ (extra `level`, `target` or return value) and are not copies.
- `unquote` (xml, xaml), `visit_children` and `new`: tied to one grammar's node kinds.
- Test helpers (`parse`, `open_indexed`, `syntax_error_is_reported_not_panicked`): each crate's tests stay independent of the others.

Exceptions with real security/complexity implications get their own note (e.g. [[mct-lang-php]]); everything else is just a row in [[00-index]]'s extension→crate table.

#architecture #contract #crate
