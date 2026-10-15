# Checkpoint: TASK-207-A (issue #207) - Rust and Python token sources

- Branch: `claude/207-a-token-sources`. Base: `bfbf4b3eb45bd97031f1818b2a9474335c2b01f2`.
- Status: completed. Last code commit: `e717f64`; this checkpoint is the commit after it.

## Done

- `mct-tree-sitter/src/tokens.rs`: grammar-agnostic `tokenize_tree` / `tokenize_source` and `TokenClasses`. Iterative `TreeCursor` walk (no recursion, so nesting depth costs heap, not stack; no depth cap, because a cap would silently drop tokens). Rules: trivia subtrees skipped; a literal-kind node is one `Literal` token over the whole node; every other leaf is one token (`Identifier` for a named leaf of an identifier kind, `Keyword` for a keyword kind or letters/underscores-only text, else `Punctuation`). Zero-width and whitespace-only leaves are skipped. `ERROR` nodes (outermost only), `MISSING` nodes, and skipped subtrees that hide an error go to `error_lines`, sorted and merged. A `MISSING` node at column 0 also implicates the line above. Byte slices use `source.get(..)`, never indexing.
- `mct-lang-rust` `RustTokenSource` (`language_id` `"rust"`), `mct-lang-python` `PythonTokenSource` (`"python"`), each a table over `tokenize_source`.
- `mct-languages::build_token_sources` registers both; test checks ids `["python","rust"]` and that each is a registered parser id.

## Node-kind tables (read from node-types.json and parsed probes)

Rust (tree-sitter-rust 0.24.2)
- trivia: `line_comment`, `block_comment` (doc comments are line comments with `doc_comment` children), `shebang`.
- literal: `string_literal` (byte and C strings too), `raw_string_literal`, `char_literal`, `integer_literal`, `float_literal`, `boolean_literal`. `-1` in patterns is `-` + literal.
- identifier: `identifier`, `type_identifier`, `field_identifier`, `shorthand_field_identifier`, `metavariable`, `primitive_type` (`i32` has a digit, so letters-only would call it punctuation; it is a type name).
- keyword_kinds: `macro_rules!` (one token containing `!`). Other keywords by the letters-only rule: `self`, `super`, `crate`, `mut`, `_`, `fn`, ...
- Lifetimes and labels: `'` punctuation + identifier.

Python (tree-sitter-python 0.25.0)
- trivia: `comment` (shebang too), `line_continuation`.
- literal: `string`, `integer`, `float`, `true`, `false`, `none`, `ellipsis`. `True`/`False`/`None`/`...` are Literals (decision), so `x = None` and `x = 0` share a shape.
- identifier: `identifier` (also `self`, `cls`, builtins, soft keywords used as names).
- everything alphabetic else is a Keyword (`def`, `async`, `lambda`, `not`, `in`, `pass`, ...).

## Checks (from the worktree, via --manifest-path)

- `cargo fmt --all --check`: pass.
- `cargo clippy -p mct-tree-sitter -p mct-lang-rust -p mct-lang-python -p mct-languages --all-targets --all-features -- -D warnings -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic`: pass, no warnings.
- `cargo test -p mct-tree-sitter -p mct-lang-rust -p mct-lang-python -p mct-languages`: 126 passed, 0 failed, 2 ignored (pre-existing). New: 11 mct-tree-sitter (6 new), 11 rust tokens, 12 python tokens, 1 mct-languages.
- Cargo.toml / Cargo.lock untouched.

## Known limitations

- Python f-string interpolations are inside the single `Literal`: identifiers in `{expr}` are not tokens.
- Rust macro bodies (`token_tree`) are tokenized as the grammar splits them; macro_rules fragment specifiers (`expr`, `ident`) come out as Keywords, and `$x` as an Identifier. Macro-expanded code is not analyzed.
- No depth cap in the walk (iterative), so a hostile file costs heap proportional to its tree size.
- Tokens inside `ERROR` nodes are emitted but unreliable; consumers must honor `error_lines`.
- Lifetime `'a` is two tokens (`'`, `a`).

## Next action

Coordinator integrates with the other workers (index-side duplicate analysis and CLI) using `build_token_sources()`.
