# Library/framework module template

Use this template for a framework or library pattern implemented on top of an
existing language parser. Replace `<language>` and `<library>` throughout.
The folder groups focused tests; it is not a plugin API, and creating it alone
does not enable framework support. The scope and user-visible coverage belong
in [library-support.md](../01-architecture/library-support.md) and the
framework coverage table in [CONTRIBUTING.md](../../CONTRIBUTING.md#frameworklibrary-coverage-beyond-the-language-table).

## 1. Decide what is changing

- **Manifest recognition** (for example, another dependency file): implement
  the format in `crates/mct-index/src/manifests/` and dispatch it through
  `manifests.rs`. Return `ManifestDependency` values; do not add SQLite access
  to the format module.
- **Language-wide syntax or relations**: change the existing parser's shared
  walker. Keep behavior that applies to the language out of a library module.
- **Framework-specific source behavior**: make a focused change in the
  language parser/walker and add regression tests under
  `crates/mct-lang-<language>/src/libraries/<library>/`.
- **Coverage documentation only**: update the authoritative table and scope
  page; do not add an empty module or imply that the framework is supported.

If the change requires a new relation/symbol kind, schema change, or published
MCP signature change, stop and follow the issue-first rule in
[CONTRIBUTING.md](../../CONTRIBUTING.md#changing-the-languageparser-trait-the-sqlite-schema-or-an-already-published-mcp-tool-signature).

## 2. Copy this module shape

```text
crates/mct-lang-<language>/src/libraries/<library>/mod.rs
```

Declare the module in that crate's existing `src/libraries/mod.rs` (`mod <library>;`;
the whole `libraries` module is already compiled for tests only). Keep each module small and focused.
Do not add a per-library README: the implementation map and coverage table are
the canonical documentation.

```rust
//! <Library> source-pattern regressions through the shared <Language> parser.

use mct_core::{RelationKind, SymbolKind};

use crate::libraries::{corpus, parent};

#[test]
fn library_application_patterns_are_indexed() {
    let c = corpus();

    // Replace these fixture values and expected line with a real scenario.
    let relation = c.relation(
        "path/to/app.ext",
        "owner_symbol",
        RelationKind::References,
        "framework_symbol",
    );
    assert_eq!(relation.line, 12);

    let component = c.symbol("path/to/app.ext", "owner_symbol", SymbolKind::Function);
    assert_eq!(component.kind, SymbolKind::Function);
    assert_eq!(parent("path/to/app.ext", "owner_symbol", SymbolKind::Function), None);
}
```

The `corpus`/`parent` helpers shown above exist today only in
`crates/mct-lang-python/src/libraries/mod.rs` (which also has `lines`); the
CSS crate defines `corpus` and `rules`, and the JS/TS crate defines none and
parses inline in each module (see `libraries/react/mod.rs`). Import the helpers
your crate really has, or add a small private one to its `libraries/mod.rs`
modelled on the Python/CSS `corpus()` (`Corpus::load` with
`env!("CARGO_MANIFEST_DIR")`). This skeleton is intentionally expected to
fail until its example path, names, kind, parent and source line are replaced
with values from the new fixture. Assert behavior in application-owned fixtures, not framework
internals. If the behavior is better expressed as a small parser unit test, use
the local parsing helper and assert the exact symbols/relations and their
owners.

## 3. Define the extraction contract

Before changing code, write down a few representative source forms and the
expected indexed result. Framework support normally reuses the language-neutral
model from `mct-core/src/symbol.rs`; do not add a `ReactComponent`, `Route`, or
other framework-specific enum variant for a one-framework feature. Map source
constructs to existing kinds and relation kinds, or document why they remain
ordinary language syntax.

| Source pattern | Expected model decision |
| --- | --- |
| Framework-declared class/function/component | Existing `SymbolKind` (often `Class`, `Function` or `Method`); preserve the source name and enclosing type parent. |
| Route, decorator, component tag, hook, template/include, CSS selector | Existing `RelationKind` (`References`, `Calls`, `Imports`, etc.) only when that relation is actually represented by source syntax. State the relation owner and source location. |
| Package declared in a manifest | `ManifestDependency` with package name and version as written; this is not a parsed source symbol. |
| External framework target | `RelationTarget.external` only when the source gives positive evidence it is outside the indexed repository. A missing local definition alone is not proof. |

For each pattern, answer: what exact syntax is recognized; which symbol owns
the relation (module, component, function, method); what `to_name` is emitted;
what line is reported; whether qualification is preserved; what happens for
aliases, nested/member names, intrinsics, duplicate declarations, and invalid
syntax; and what is intentionally out of scope. Add `RelationTarget` evidence
only if syntax proves it. Do not claim a framework relationship merely
because an ordinary `Calls` or `References` relation exists.

## 4. Choose tests at the right levels

Use the lowest level that proves each behavior, plus an end-to-end level when
the feature crosses the index boundary:

1. **Focused parser regression** in `src/libraries/<library>/mod.rs` (or the
   language crate's `tests/parse.rs`): small valid source, exact symbol kind
   and parent, relation kind/target/owner and location. Add negative assertions
   for syntax that must not become a framework relation (for example intrinsic
   HTML tags, closing tags, comments or string contents), and test variants
   such as nested scopes, aliases and qualified names when relevant.
2. **Corpus test** in `tests/corpus.rs` and a realistic scenario under
   `tests/corpus/project/`: exercise several files that import/call one
   another, with exact cross-file assertions using `Corpus::relation` and
   `Corpus::symbol`. If a behavior changes snapshot output, inspect every
   changed line and ensure the golden file records only intended extraction.
3. **Index integration** where the feature must be visible through queries:
   check the indexed `language`, symbol kind/parent, relation query, dependency
   rows for manifest work, and absence of parse issues. Use the neighboring
   crate's `tests/index_integration.rs` pattern.
4. **Manifest tests** for a new dependency format: unit cases for valid
   sections, optional/missing fields, duplicate package names, versions,
   comments/blank lines as applicable, malformed input, and platform/runtime
   dependencies that should be excluded. Add integration coverage in
   `crates/mct-index/tests/manifests.rs` to prove file-name detection and
   persisted dependency output.

Tests may use `unwrap`/`expect` for test setup, but production parsing of
repository input must handle missing AST fields and malformed manifests without
panicking. For a known bug, use an issue-linked ignored test rather than
asserting incorrect output as correct. For an intentional limit, label it
`Known limit:` in the assertion/comment and link the parser module docs.

## 5. Write the fixture and regression first

1. Find the existing application scenario under
   `crates/mct-lang-<language>/tests/corpus/project/`. Extend it when it is
   already the right scenario; otherwise create one small, realistic scenario.
2. Use application-owned code and name the external framework/library and
   version in a short fixture comment. Do not copy framework source or generated
   bundles into the corpus.
3. Assert only relationships the index is meant to expose. A dependency being
   detected does not mean its source is parsed; an import, call, or CSS selector
   does not imply framework-specific resolution.
4. For a bug fix, make the regression fail before the parser change. For a
   documented limitation, label the assertion/comment `Known limit:` and point
   to the parser module documentation that explains the boundary.
5. If a corpus snapshot changes, inspect the complete diff and update it only
   for the intended behavior. Keep fixture size and malformed-input rules from
   the corpus section of `CONTRIBUTING.md`.

## 6. Implement and document

- Put production extraction in the existing parser/walker, then connect any
  specialized helper explicitly. Keep AST traversal and language-wide behavior
  shared.
- For any recursive AST walk, enforce `mct_core::MAX_TRAVERSAL_DEPTH`; use
  safe node-text access and optional-child handling. Never execute/evaluate
  project code, load the installed framework, or infer a relation from a name
  match alone.
- Add focused tests in `src/libraries/<library>/mod.rs`; add cross-file or
  index-level assertions to the corpus/integration tests where appropriate.
- Update `docs/01-architecture/library-support.md` if the implementation map,
  layout, or scope changed.
- Update the framework coverage table in `CONTRIBUTING.md` in the same change.
  State **Supported**, **Partial**, or **Not supported**, what is actually
  extracted, and the most important known gap. Avoid claiming full support from
  one regression test.

## 7. Verify

Run the checks for the affected crate (the repository's wrapper prints a short
summary):

```sh
scripts/unix/check.sh -p mct-index
scripts/unix/check.sh -p mct-lang-<language>
```

Run only the relevant command when the change affects only one area. For a
manifest format, also run `mct-index` manifest integration tests. For parser
behavior, run that parser's unit, corpus, and index integration tests as
appropriate. Windows uses `scripts/windows/check.ps1 -p <crate>`.

Before opening a PR, confirm the documentation describes the shipped behavior,
fixtures contain no vendored/generated code, and tests do not encode a known
bug as expected behavior.
