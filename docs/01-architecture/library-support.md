# Library and framework support

Library support has three independent responsibilities: detect declared packages, index application source, and capture relationships introduced by a framework. A supported language does not imply complete framework coverage or an index of installed third-party source.

## Implementation map

| Responsibility | Entry point | Implementation |
| --- | --- | --- |
| Recognize dependency manifests and deduplicate package names | [`manifests.rs`](../../crates/mct-index/src/manifests.rs) | Private format modules under [`manifests/`](../../crates/mct-index/src/manifests) |
| Read JS/TS packages from `package.json` | `manifests::parse_manifest` | [`manifests/javascript.rs`](../../crates/mct-index/src/manifests/javascript.rs) |
| Read Python packages from `requirements.txt` | `manifests::parse_manifest` | [`manifests/python.rs`](../../crates/mct-index/src/manifests/python.rs) |
| Select a JS/TS/TSX grammar and validate syntax | [`mct-lang-js-ts/src/lib.rs`](../../crates/mct-lang-js-ts/src/lib.rs) | The public `JsTsParser`; source traversal in [`walker.rs`](../../crates/mct-lang-js-ts/src/walker.rs) |
| Extract JS/TS imports and CommonJS require paths | `walker::Walker` | [`walker/imports.rs`](../../crates/mct-lang-js-ts/src/walker/imports.rs) |
| Extract JS/TS exports and CommonJS assignments | `walker::Walker` | [`walker/exports.rs`](../../crates/mct-lang-js-ts/src/walker/exports.rs) |
| Extract Python source symbols and relations | [`mct-lang-python/src/lib.rs`](../../crates/mct-lang-python/src/lib.rs) | `PythonParser` and its AST walker |
| Extract CSS selectors and imports | [`mct-lang-css/src/lib.rs`](../../crates/mct-lang-css/src/lib.rs) | `CssParser` and its selector helpers |

Rust and Go manifests have their own private `manifests/rust.rs` and `manifests/go.rs` modules. The indexer owns persistence; format modules return `ManifestDependency` values and do not access SQLite. Language parsers implement `LanguageParser` and do not read package manifests or install packages. All JS/TS traversal modules share one private walker state; module boundaries do not change symbol ownership or extraction order.

## Coverage and scope

The authoritative framework coverage table is in [CONTRIBUTING.md](../../CONTRIBUTING.md#frameworklibrary-coverage-beyond-the-language-table); detailed fixture findings are in [corpus progress](../../internal/corpus-progress.md). Update those records when behavior changes rather than maintaining competing coverage tables.

- JS/TS dependencies include the four `package.json` dependency sections. React component logic is parsed, but JSX render relationships remain a documented gap.
- Python manifest support currently covers `requirements.txt`; `pyproject.toml`, Poetry and Pipenv manifests are not implemented. Flask and Django corpus scenarios exercise application code, not framework internals.
- CSS framework packages can be declared in `package.json` and detected by the JS ecosystem manifest parser. CSS extraction handles selectors and imports, not a browser's cascade or computed styles; Tailwind grammar limitations are documented separately.
- Declared versions are kept as written. There is no package installation, lockfile dependency resolution, or automatic external-source indexing. `node_modules/` and `vendor/` are built-in exclusions.

The Python corpus directory `project/library/` models a lending library (books, members and loans); it is application-domain code, not a directory of third-party packages. Fixture directories describe realistic applications and should retain their import relationships when moved or renamed.

## Library directory layout

Use the English plural `libraries` and the established lowercase package name:

```text
crates/
  mct-lang-js-ts/src/libraries/react/
  mct-lang-python/src/libraries/django/
  mct-lang-python/src/libraries/flask/
  mct-lang-css/src/libraries/bootstrap/
  mct-lang-css/src/libraries/tailwind/
```

Each directory currently holds one `mod.rs`: a module doc stating its scope, then the tests. The `libraries` modules are enabled with `#[cfg(test)]`: they group seven existing parser regression tests by library, while production extraction continues to use the shared language parser. This organization adds no new framework behavior. A folder's existence does not mean production support; this page is the scope record, so do not add per-library README files that repeat it.

These tests read the corpus fixtures under `tests/corpus/project/`, but they are unit tests of the library target, not part of the `corpus` test binary. `corpus-report.sh`/`corpus-report.ps1` therefore run both `--test corpus` and `--lib libraries`; a new library module needs no script change.

The React tests cover TSX component logic and hook calls. Django and Flask tests cover application classes, decorators, imports and calls. Bootstrap and Tailwind tests cover application CSS selectors and imports, including assertions documenting unsupported constructs. These are parser tests against source examples, not tests running the frameworks.

Keep the shared grammar and AST traversal outside library directories. Add production code to a library directory only when a concrete framework pattern needs specialized extraction; connect that code explicitly to the parser and test it. Do not create empty adapters or duplicate language behavior to populate a directory. Keep multi-file fixtures, corpus snapshots and index integration in `tests/`; moving those fixtures can change import relationships and expected locations.

## Extending support safely

1. Choose the responsibility being changed. A new dependency file belongs in the appropriate manifest-format module; shared source-language behavior belongs in its language parser, and library-specific behavior and focused regressions belong under `src/libraries/<name>/`.
2. Keep detection and dispatch in `manifests.rs` and deduplication in the shared entry point. Do not make the indexer understand npm, pip or framework syntax.
3. Verify a format change with manifest unit tests and [`mct-index/tests/manifests.rs`](../../crates/mct-index/tests/manifests.rs). Verify source extraction with the parser's parse, corpus and index-integration tests.
4. Keep existing corpus snapshots unchanged for a behavior-preserving refactor. Add focused regressions for behavior changes and document any intentional snapshot changes.
5. Follow the repository's issue-first rule for changes to core symbol/relation contracts, SQLite schema or published MCP signatures.

Useful focused checks:

```sh
scripts/unix/check.sh -p mct-index
scripts/unix/check.sh -p mct-lang-js-ts
scripts/unix/check.sh -p mct-lang-python
scripts/unix/check.sh -p mct-lang-css
```

Windows uses the equivalent `scripts/windows/check.ps1 -p <crate>`. Existing passing tests demonstrate their covered patterns, not complete framework support.
