# Contributing

## Adding a new language

This is the extension point most contributions will touch, so it gets its own checklist:

1. **New crate.** `crates/ccm-lang-<name>`, depending on `ccm-core` (path dependency) and `tree-sitter-<name>` (pin an exact version).
2. **Implement `ccm_core::LanguageParser`:**
   - `language_id()` — a stable lowercase identifier (`"go"`, `"kotlin"`), stored in the index's `language` column.
   - `file_extensions()` — extensions this parser owns, without the leading dot. Two parsers can never claim the same extension (`LanguageRegistry::register` panics on conflict — a startup-time configuration error, not something repo content can trigger).
   - `parse()` — must never execute, `eval`, or otherwise run any part of the input; parsing is purely AST-based. A syntax error in the input is a `ParseError::Syntax` return value, never a panic — this method runs over arbitrary third-party source.
3. **No `unwrap()`/`panic!`** on any path that processes file content from the indexed repo. `.unwrap_or_default()` / early-return on `Option`/`Result` instead.
4. **Register it** in exactly two places — nowhere else changes:
   - `ccm-mcp-server/src/registry.rs::build_registry`
   - `ccm-cli/src/main.rs::build_registry`
5. **Tests** in `crates/ccm-lang-<name>/tests/parse.rs`:
   - A function-and-call extraction test.
   - At least one test covering the language's distinctive idiomatic syntax (generics for C#/Java, templates for C++, decorators for Python — whatever the equivalent is for your language).
   - A syntax-error case asserting `ParseError::Syntax`, not a panic.
6. **Update docs:** the language table in `README.md` and the "Cobertura de lenguajes" section in `checklist.md`.
7. **Run the workspace test suite** (`cargo test --workspace`) and fix any regressions before opening a PR.

## Changing the `LanguageParser` trait, the SQLite schema, or an already-published MCP tool signature

These are cross-cutting: the trait is implemented by every language crate, the schema is shared by every language's data, and a published tool signature is part of the contract Claude Code agents rely on. Open an issue describing the change before sending a PR — these need explicit sign-off, not a drive-by PR.

## Code style

- No `unwrap()`/`panic!` on repo-input processing paths, in any crate.
- Single responsibility per crate: `ccm-core` doesn't know about any specific language or about SQLite; a `ccm-lang-*` crate doesn't know about SQLite or MCP.
- Comment the *why*, not the *what* — non-obvious constraints, not a restatement of the code.

## Commit messages

`type(scope): short description` — types: `feat` / `fix` / `refactor` / `docs` / `test` / `chore` / `perf`. Mark breaking changes explicitly.
