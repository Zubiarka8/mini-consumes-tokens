# Base template: new `mct-lang-*` crate

Every language crate starts from this base, so all of them offer the same
guarantees: a parser, a syntax-error test, a corpus, a fuzz harness and
registry wiring. Copy the layout below, then replace `<name>`/`<Name>`.
This is a prototype: it captures what the existing crates share (checked
against `go`, `bash`, `cpp`, `csharp`, `css`), not an exhaustive spec. The authoritative
checklist stays in [CONTRIBUTING.md](../../CONTRIBUTING.md#adding-a-new-language).

## Layout

```
crates/mct-lang-<name>/
  Cargo.toml                 deps: mct-core, mct-tree-sitter, tree-sitter, tree-sitter-<name>
  src/lib.rs                 <Name>Parser: impl LanguageParser
  tests/parse.rs             extraction, idiomatic syntax, syntax error
  tests/corpus.rs            mct-corpus harness (snapshot in tests/corpus/expected.snap)
  tests/corpus/malformed/    one broken file
  tests/corpus/project/      realistic multi-file project
  tests/index_integration.rs end-to-end through mct-index (optional for tiny grammars)
  tests/fixtures/            small sample repo
  fuzz/Cargo.toml            standalone workspace ([workspace] empty table)
  fuzz/.gitignore
  fuzz/fuzz_targets/parse_<name>.rs
```

## `Cargo.toml`

```toml
[package]
name = "mct-lang-<name>"
description = "<Name> language plugin (tree-sitter-<name>) for mini-consumes-tokens's LanguageParser trait."
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
keywords = ["mcp", "tree-sitter", "code-index"]
categories = ["development-tools", "parser-implementations"]

[dependencies]
mct-core.workspace = true
mct-tree-sitter.workspace = true
tree-sitter.workspace = true
tree-sitter-<name> = "<x.y.z you validated>"   # caret, not `=`; see ADR-004

[dev-dependencies]
mct-corpus.workspace = true
mct-index.workspace = true
```

## `src/lib.rs`

```rust
//! `LanguageParser` implementation for <Name>, via `tree-sitter-<name>`.

use mct_core::{LanguageParser, ParseError, ParsedFile, SourceFile, MAX_TRAVERSAL_DEPTH};
use mct_tree_sitter::first_error;
use tree_sitter::Parser;

pub struct <Name>Parser;

impl LanguageParser for <Name>Parser {
    fn language_id(&self) -> &'static str {
        "<name>"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["<ext>"]
    }

    fn parse(&self, file: &SourceFile) -> Result<ParsedFile, ParseError> {
        let mut parser = Parser::new();
        // Grammar is compiled in; this only fails on an ABI mismatch pinned in Cargo.lock.
        if parser.set_language(&tree_sitter_<name>::LANGUAGE.into()).is_err() {
            return Err(ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "grammar ABI mismatch".to_string(),
            });
        }

        let tree = parser
            .parse(&file.contents, None)
            .ok_or_else(|| ParseError::Syntax {
                path: file.relative_path.clone(),
                line: 1,
                message: "tree-sitter produced no parse tree".to_string(),
            })?;

        let root = tree.root_node();
        if root.has_error() {
            let line = first_error(root).unwrap_or(root).start_position().row as u32 + 1;
            return Err(ParseError::Syntax {
                path: file.relative_path.clone(),
                line,
                message: "syntax error".to_string(),
            });
        }

        // Placeholder: walk the AST here, threading a `depth` counter that
        // stops at MAX_TRAVERSAL_DEPTH (see docs/02-crates/parsers/overview.md),
        // and return the symbols and relations found.
        let _ = MAX_TRAVERSAL_DEPTH;
        todo!("emit SymbolRecord / SymbolRelation for <name>")
    }
}
```

Replace the `todo!` before merging: a shipped parser must not panic on repo input.

## Mapping the AST to the shared index model

Before writing walker code, make a small mapping table from grammar nodes to
the existing shared model. Use the enums in `crates/mct-core/src/symbol.rs`;
do not invent a language-specific symbol or relation variant.

| Source construct | Shared representation | Questions to settle |
| --- | --- | --- |
| File/module | `SymbolKind::Module` (when this language models file scope) | What stable name does the file get? Does the path include directories? |
| Named declaration | Closest `SymbolKind` (`Function`, `Class`, `Struct`, `Interface`, `Enum`, `Trait`, `TypeAlias`, `Variable`, `Constant`, `Field`, etc.) | Which AST node owns the full definition range? Are anonymous declarations skipped? |
| Nested declaration | `SymbolRecord.parent` | Is the parent the enclosing type only, or a true lexical parent? Match established parsers' semantics and test them. |
| Call / import / inheritance / implementation / use | `RelationKind::Calls`, `Imports`, `Extends`, `Implements`, `References` | Which source construct proves this relation, and which enclosing symbol owns it? |

Each `SymbolRecord` needs a unique file-local `id` (`SymbolId`), exact name,
kind, `Location` and optional parent. Locations are 1-based line/column;
definition locations should cover the whole declaration and populate
`end_line` when available. Each `SymbolRelation.from` must point to an emitted
symbol in that same `ParsedFile`; its location is the relation site, and
`to_name` is the spelling present in source. Use a file/module symbol as owner
for top-level relations so they are not dropped. Keep symbol IDs stable within
one parse result, not across files or indexing runs.

`ParsedFile` also has optional `relation_targets` for evidence that makes a
target more precise (qualifier, module, path, language, kind, external).
Populate a field only when the syntax proves it; never guess resolution. Leave
the vector empty when the relation is unqualified. `literals` is for filtered
prose strings only when the language needs semantic literal search; use the
shared `LiteralCollector` and its caps rather than collecting every string.

### Walker design

Keep parsing and walking separate: parse the source, return a structured
`ParseError::Syntax` for invalid syntax, then walk the tree into a result
builder. A walker commonly owns the source text, `symbols`, `relations`, a
`next_id`, and context for the current relation owner and enclosing type.
Centralize `push_symbol` and `push_relation` so IDs, empty names, locations,
and ownership are handled consistently. For each recursive descent, increment
`depth` and return at `MAX_TRAVERSAL_DEPTH`; do not use an unbounded recursive
walk on repository input. Be careful that nested functions change relation
ownership and that class/type nesting controls `parent` independently.

Only use `unwrap`/`expect` for invariants independent of repository text (for
example, a statically compiled grammar ABI, with a documented reason). Missing
AST fields, invalid UTF-8 node slices, and optional children must be handled
with safe defaults or skipped nodes. Parsing must never execute macros,
scripts, build hooks, or framework code. Document non-obvious ownership and
grammar-specific exceptions in the parser's module docs.

## `tests/parse.rs`

```rust
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_<name>::<Name>Parser;

fn parse(src: &str) -> mct_core::ParsedFile {
    <Name>Parser
        .parse(&SourceFile {
            relative_path: "sample.<ext>".to_string(),
            contents: src.to_string(),
        })
        .expect("valid source should parse")
}

#[test]
fn extracts_function_and_call() {
    let parsed = parse("<minimal source with helper declaration and call>");
    let helper = parsed.symbols.iter().find(|s| s.name == "helper").unwrap();
    assert_eq!(helper.kind, mct_core::SymbolKind::Function);
    assert!(parsed.relations.iter().any(|r| {
        r.kind == mct_core::RelationKind::Calls && r.to_name == "helper"
    }));
}

#[test]
fn idiomatic_syntax_is_extracted() {
    // Assert the relevant kind, parent, relation kind/target, and source line.
    // Choose a distinctive construct: generics, decorators, macros, etc.
}

#[test]
fn syntax_error_is_a_value_not_a_panic() {
    let result = <Name>Parser.parse(&SourceFile {
        relative_path: "broken.<ext>".to_string(),
        contents: "<input that the grammar rejects>".to_string(),
    });
    assert!(matches!(result, Err(mct_core::ParseError::Syntax { .. })));
}
```

Replace the placeholder source with valid syntax for the grammar. Do not stop
at “parse succeeds”: tests should assert the names, kinds, parents, relation
owners, target names, and important locations. Include cases for nested scopes
and same-named constructs when they affect ownership. Add tests for imports,
inheritance/implementation, references, module names, and relation-target
evidence only where the language supports them. Add explicit edge cases for
anonymous declarations, Unicode identifiers, empty files, comments/strings
that resemble code, and malformed or deeply nested input where relevant.

### Test layers to include

1. **Parser unit tests** (`tests/parse.rs`): small examples pin exact AST to
   model behavior. Include valid extraction, idiomatic syntax, ownership,
   relation/location correctness, syntax errors, and any known grammar edge
   cases. Test behavior, not private helper implementation.
2. **Corpus tests** (`tests/corpus.rs`): use
   `mct_corpus::standard_tests!(<Name>Parser)` and add focused assertions for
   realistic multi-file application code, cross-file imports/calls, definition
   ranges, malformed fixtures, and documented limitations. Keep fixtures
   application-owned, representative, and within the size rules in
   `CONTRIBUTING.md`; never paste vendored framework code.
3. **Index integration tests** (`tests/index_integration.rs`): prove extension
   dispatch and the parse → SQLite → query round trip for representative
   symbols, relations and language ID. This is required when the parser has
   behavior that cannot be covered by the shared registry test; follow a
   neighboring crate's fixture pattern rather than marking it optional by
   default.
4. **Registry tests**: ensure a representative extension resolves to this
   parser through `mct-languages`, and that the language id is distinct
   from the others. The checker runs the shared registry crate's tests.
5. **Fuzzing**: the fuzz target must call the public parser on arbitrary valid
   UTF-8 and safely ignore parse errors. CI must run the harness.

For syntax errors, assert the returned `ParseError::Syntax` path and line when
stable, not just that a panic did not occur. If an upstream grammar accepts a
malformed sample, record the limitation and choose a sample it actually
rejects. Corpus tests must not mark a known parser bug as expected output:
fix it, use an issue-linked ignored assertion, or document a true design limit.

**Do not guess the broken input.** Many grammars accept malformed text. Markdown
accepted its whole malformed corpus and only rejected a NUL byte, so a guessed
input made the test fail. Confirm the input first with
`mct-cli probe broken.<ext>` (it prints `FAIL <file>:<line>: syntax error`), then
put that exact input in the test.

## `fuzz/`

`fuzz/Cargo.toml` and `fuzz/fuzz_targets/parse_<name>.rs` are copied from an
existing crate (for example `crates/mct-lang-go/fuzz/`), replacing the names.
The target calls `<Name>Parser.parse` on arbitrary UTF-8 bytes and ignores the result.

```rust
#![no_main]

use mct_core::{LanguageParser, SourceFile};
use mct_lang_<name>::<Name>Parser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(contents) = std::str::from_utf8(data) else {
        return;
    };
    let file = SourceFile {
        relative_path: "fuzz.<ext>".to_string(),
        contents: contents.to_string(),
    };
    let _ = <Name>Parser.parse(&file);
});
```

## Wiring (required)

These are the places the base does not cover by itself. The check script
treats items 1–4 as required; item 5 is a warning:

1. Root `Cargo.toml`: add `"crates/mct-lang-<name>"` to `members` and `mct-lang-<name> = { path = ... }` to `[workspace.dependencies]`.
2. `crates/mct-languages/Cargo.toml`: dependency on the crate.
3. `crates/mct-languages/src/lib.rs::build_registry`: register `<Name>Parser`, and add a representative extension to that file's test. The CLI, the MCP server and `mct-eval` all use this registry.
4. `.github/workflows/ci.yml`: add `- mct-lang-<name>` to the `fuzz-smoke` matrix.
5. `README.md` supported-languages line and a row in `internal/checklist.md`.
6. `docs/00-system/00-index.md` extension→crate table; update the parser
   overview or language-specific parser page if the language has meaningful
   limitations, special ownership rules, or security-sensitive behavior.
   Add the language's column and checklist to
   [symbol coverage](../02-crates/parsers/symbol-coverage.md), from a probe
   of each construct, not from what the walker is meant to do.
7. Any required file-extension/configuration lists elsewhere in the repo,
   such as ignore defaults or language-specific scripts; search for the
   extension and crate suffix before finishing. Avoid changing unrelated
   `mct-core`, MCP, CLI or index contracts.

## Verify

```sh
scripts/unix/new-language-check.sh <name>
```

It reports every wiring item above as `ok`, `MISS` (required) or `warn`, then
runs the crate's tests and the `mct-cli` registry tests. A new crate is done
when it reports no `MISS` and no `FAIL`. Fuzz harness, fuzz target, CI matrix
entry and syntax-error test are all `MISS` when absent, so a crate cannot skip
them silently.

The checker cannot assess extraction quality, ownership, ranges, completeness
of corpus assertions, or whether documentation claims match behavior. Review
those manually using the mapping table and test layers above. Also run the
workspace's normal quality gate when practical; a focused checker run is not a
substitute for CI.

## Known gaps in the existing crates

None of the 17 crates lacks a fuzz harness or a syntax-error test any more.
