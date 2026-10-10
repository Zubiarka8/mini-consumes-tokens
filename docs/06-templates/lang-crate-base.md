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
tree-sitter-<name> = "<exact version>"

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
    // one small sample: assert a symbol and a call relation
    let _ = parse("<minimal valid source>");
}

#[test]
fn idiomatic_syntax_is_extracted() {
    // one case for the language's distinctive construct (generics, decorators, macros...)
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

## Verify

```sh
scripts/unix/new-language-check.sh <name>
```

It reports every wiring item above as `ok`, `MISS` (required) or `warn`, then
runs the crate's tests and the `mct-cli` registry tests. A new crate is done
when it reports no `MISS` and no `FAIL`. Fuzz harness, fuzz target, CI matrix
entry and syntax-error test are all `MISS` when absent, so a crate cannot skip
them silently.

## Known gaps in the existing crates

None of the 17 crates lacks a fuzz harness or a syntax-error test any more.
