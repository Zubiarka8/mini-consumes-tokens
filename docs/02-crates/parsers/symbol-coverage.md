# Symbol coverage by language

> **Measurement status:** The coverage inventory, scores and examples below
> describe the parser probes at commit `71ec6d0` on 2026-10-19. They have not
> been fully rechecked against the current working tree, including unreleased
> branch changes. Confirm a specific construct against current parser tests
> before relying on an individual cell.

What each `mct-lang-*` parser puts in the index, construct by construct:
functions, methods, classes and the rest of object-oriented programming,
fields, constants, global and local variables, and the relations between
them. It also covers which features work for each language: symbol search,
callers and references, dead code, repeated code, the import graph, and the
rest. Each language has a checklist, so you can see how much of the
language it covers and what is missing.

Use this page:

- before relying on a tool result for a language, to know what can be in it.
  For example, `find_symbol` never returns a Python module variable or a Go
  struct field, because those are not indexed;
- when adding a language. The [language crate template](../../06-templates/lang-crate-base.md)
  asks for inventory entries and a checklist here;
- when changing a parser. Update the cells it changes in the same PR.

Other coverage records, and what each is for:

| Record | What it holds |
|---|---|
| This page | What each parser indexes, by construct, with a checklist per language |
| [`internal/checklist.md`](../../../internal/checklist.md), "Language coverage" | How each crate was built and decided, as a dated history |
| [`internal/corpus-progress.md`](../../../internal/corpus-progress.md) | Corpus size, counts, and parser bugs found while writing the corpus |
| [`CONTRIBUTING.md`](../../../CONTRIBUTING.md#frameworklibrary-coverage-beyond-the-language-table), framework table | Frameworks and libraries (React, Vue, Django, Tailwind…) |
| [`library-support.md`](../../01-architecture/library-support.md) | Where library and manifest support is implemented |

Framework and library coverage stays in the `CONTRIBUTING.md` table. It is
not repeated here, as `library-support.md` asks. Frameworks only add rows to
this page when they change what a language parser emits.

## How this was checked

Initial fixtures from 2026-10-17 were rechecked on 2026-10-19 against parser
sources at commit `71ec6d0` (branch `feat/207-token-twins`), using debug builds.
The review rebuilt `mct-cli` offline
against this checkout; all indexing used temporary roots outside it. For each language, a probe file wrote
the checked constructs with distinctive names (`p_local`,
`P_CONST`, `Circle::area`…). The probe was run through
`mct_languages::build_registry()`, the registry the CLI, the MCP server and
`mct-eval` share, and emitted symbols (kind, parent, name, line range) and relations were
read back. The review repeated the registry probe and cross-checked its
counts with the rebuilt CLI; unverified forms remain `[?]`. Corpus snapshots
are supporting examples, not proof of every syntax form.

Feature probes use a small multi-file project per programming language:
an imported helper, unused callables/types where applicable, duplicate functions,
and `"probe literal message"`. The entry calls the helper and, where present, a
method. Separate HTML/CSS and Markdown fixtures check selectors, note links,
embeds and prose literals.
The applicable tools below were run through `mct-cli call`, the same code
the MCP server runs, plus `mct-cli duplicates`. The review repeated these
checks in temporary projects. Python's duplicate fixture has 33 normalized
tokens: it produces a group with `--min-tokens 20`, not with the default 40.
The tool catalog was checked with `discover_tool_categories`; semantic
inference and attention-mode selection were not exercised.

Implementation facts are defined and recounted in the
[implementation inventory](#implementation-inventory). Source evidence is the
linked parser crate, its `src/` walker and `tests/parse.rs`/`tests/corpus.rs`;
checks for absent forms inspect traversal as well as positive probe output.
Feature gates are linked in [Features by language](#features-by-language).

The "Syntax it recognizes" tables list the grammar nodes each walker
matches. The "Why the gaps exist" tables take their causes from the crate
docs (`//!` comments), `internal/checklist.md`, `errors/REGISTRY.md` and the
probes.

Cells describe the checked forms, supported by parser traversal or probes.
`[?]` means no confirming probe or source evidence was established in this
review; its checklist item states the remaining uncertainty. It does not
mean the construct is missing. See [Re-checking a cell](#re-checking-a-cell)
to repeat a probe.

Legend:

| Mark | Meaning |
|---|---|
| ✅ | Indexed with the expected kind, name and parent |
| ⚠️ | Indexed, but with a different kind, without its parent, or only in some forms |
| ❌ | Not indexed: no symbol or no relation |
| — | Not applicable: the language has no such construct |
| [?] | Unverified; excluded from scores |

## What the index can hold

Read this first. Some gaps are not parser gaps: the shared model has no
place for the information.

A symbol row stores only **name, kind, parent, start line and column,
end line, byte length**, plus file identity and a heading `level` for Markdown (`symbols`
table in `crates/mct-index/src/schema.rs`, `SymbolRecord` in
`crates/mct-core/src/symbol.rs`). The kinds are `function`, `method`,
`class`, `struct`, `interface`, `enum`, `trait`, `type_alias`, `module`,
`variable`, `constant`, `field`, `element`, `rule` and a few web/3D kinds.
Relations are `calls`, `imports`, `extends`, `implements` and `references`,
from one symbol to a target name. Parser-supplied `RelationTarget` evidence
can also constrain resolution by language, parent, qualifier or import path;
ordinary graph queries are not all name-only. Dead-code counts are name-only.

Declaration details ([ADR-034](../../01-architecture/adrs/034-symbol-details.md))
add a separate, sparse store for the signature as written, the parameters
(name, type, default, kind), the return type, visibility, modifiers and
generics. Each parser fills it from syntax only. A language not in this
table emits none yet:

| Language | Symbols with details | Visibility mapping | Modifiers | Not covered |
|---|---|---|---|---|
| Rust | `fn` (free, method, trait, `extern` block), `struct`, `union`, `enum`, `trait`, `type`, associated `type`, `const`, `static` | `pub` → public, `pub(crate)` → crate, `pub(super)`/`pub(in …)` → restricted, `pub(self)` or no keyword → private; none recorded in a trait or `impl Trait for T`, where the trait decides | `async`, `const`, `unsafe`, `extern` (functions); `unsafe` (traits) | closures bound by `let`, enum variants, fields, macro-generated items; parameter defaults (Rust has none) |
| JS/TS | `function` (and generator), methods, functions or arrows assigned to `const`/`let`/`var` or to a class field, classes, interfaces, type aliases, enums, class fields | class members: `private`/`protected`/`public` keyword, `#name` → private, otherwise public (the language default); none outside a class | `async`, generator (`*`), `static`, `readonly`, `abstract`, `override`, `const` (enums) | CommonJS/`export default` function values, interface members, abstract method signatures, object-literal properties. Types come from TS annotations (`syntax`) or, without one, from a JSDoc `@param {T} name` / `@returns {T}` right above (`doc`); a destructured parameter has no name for JSDoc to match |
| Python | `def` (functions and methods), `class`, PEP 695 `type` | none: Python has no visibility syntax (`_name` is a convention) | `async`; `@staticmethod` → static, `@abstractmethod` → abstract, `@override`, `@final` (last name of the decorator) | docstring types, lambdas, assigned attributes. A method's first parameter is the receiver (`self`/`cls`, any name) unless `@staticmethod`; after `*` or `*args`, parameters are keyword-only |
| Java | methods, constructors, classes, records, interfaces, annotation types, enums, each declarator of a field declaration | `public`/`protected`/`private`; none → package, but public in an interface or annotation type (the language default) | `abstract`, `static`, `final`, `sealed`, `@Override` → override | record accessors (synthesized, not written), enum constants, `throws` (kept in the signature only). The explicit `Type this` parameter is the receiver; `T...` is variadic |
| Kotlin | `fun` (top-level, member, extension), classes, interfaces, objects, enum classes, member properties, `val`/`var` constructor parameters | `public`/`protected`/`private`/`internal`; none → public at top level or in a class (the language default), none recorded for a local declaration | `abstract`, `open`, `final`, `sealed`, `override`, `const`, `inline`, `external` | enum entries, inferred return and property types (an expression body without a written type has none). An extension's receiver type is the `this` receiver; `vararg` is variadic |
| C# | methods, constructors, destructors, operators, conversion operators, local functions, delegates, classes, structs, interfaces, records, enums, properties, indexers, events, each declarator of a field declaration | `public`/`private`/`protected`/`internal`, `file` → file_private, `protected internal`/`private protected` → restricted; none → public in an interface, private in a class/struct/record, internal in a namespace or at top level (the language defaults); none recorded for an explicit interface implementation, a destructor, a static constructor or a local function | `abstract`, `async`, `const`, `extern`, `sealed`, `static`, `override`, `virtual`, `readonly`, `unsafe` | enum members, accessor bodies, `var` types. An extension method's `this` parameter is the receiver; `params` is variadic; `ref`/`out`/`in` stay in the signature only |
| PHP | functions, methods (Laravel's anonymous migration `up`/`down` included), classes, interfaces, traits, enums, each element of a property declaration, promoted constructor parameters | `public`/`protected`/`private` (`private(set)` reads as its first word); none → public for a method or property (the language default); functions and types have none | `abstract`, `final`, `static`, `readonly` | closures assigned to a variable, enum cases, class constants, `@template` generics. A type the syntax leaves out comes from the PHPDoc block right above (`@param T $name`, `@return T`) as `doc`; a written type always wins. `...$x` is variadic; `&$x` stays in the signature only |
| C++ | function and method definitions and prototypes (out-of-line `Class::method` included), classes, structs, unions, enums, `using` aliases, `typedef`s, each declarator of a variable or field declaration; a templated one keeps its `template <…>` | the access label in force in the class body; none → private in a `class`, public in a `struct`/`union` (the language defaults); none recorded outside a class (C++ has linkage there) or for an out-of-line definition (its access is the declaration's) | `virtual`, `static`, `extern`, `inline`, `constexpr` → const, `override`, `final`, `= 0` → abstract | enumerators, lambdas, `auto` deduction (an `auto` return without `-> T` stays `auto`). A type is everything the declaration writes but the name (`const char*`, `int&`); C `...` is a variadic parameter with no name or type |
| Lua | `function` declarations (`local`, dotted, `T:m`), functions assigned to a name or field, functions in a module table's fields | none: Lua has no visibility syntax (`local` is scope, not access) | — (Lua has none) | classes, fields and variables (no declaration header), `@generic`, `@class`/`@field`. Lua writes no types: every type comes from the LuaLS `---` lines right above (`@param name type`, `@return type`, several `@return`s joined as one multiple return) as `doc`. `function T:m()` has the implicit `self` receiver; `...` is variadic |
| Bash | functions (`function f()` and `f()`), file-level variables (bare or under `readonly`/`export`/`declare`/`typeset`/`local`) | none: Bash has no access control (`export` passes a variable to child processes) | `readonly`, or a `-r` flag of `declare`/`typeset`, → readonly | parameters (a shell function declares none: it reads `$1`, `$@`, so they are recorded as unknown), types (Bash has none), a variable's value |
| PowerShell | functions and filters (header up to the body, inline parameter list included), file-level variables (the assignment target as written: `$Global:Limit`, `[int]$Max`) | none: PowerShell functions have no access modifiers (`Export-ModuleMember` is a runtime call) | — | parameters come from the inline list or the body's `param()` block; a `[type]` literal is the type, other attributes (`[Parameter(Mandatory)]`, validators) are not. A function with neither form has unknown parameters (it can read `$args`). Return types (`[OutputType()]` is advisory), classes and enums (not indexed) |
| Go | `func`, methods, interface methods, `type` (struct, interface, defined type, alias) | an upper-case first letter → public, otherwise package (the exported-name rule) | — (Go has none) | `const`/`var` (not indexed as symbols), struct fields, function literals. A result list is one return type as written: `(n int, err error)` |

A Rust receiver is stored with the type it stands for: `&mut self` is `&mut Self`; a Go receiver keeps its declared type (`*Stack[T]`). `a, b int` is two `int` parameters.
A destructuring or `_` parameter keeps its type but has no name.

Without such details (every language but those above), the index has no:

- visibility (`pub`, `private`, `protected`, `internal`);
- modifiers: `static`, `abstract`, `final`/`sealed`, `virtual`/`override`, `async`, `readonly`;
- parameter names and types, return types, or full signatures;
- generic type parameters and their constraints.

An `overrides` relation (which method overrides which, D8) exists for Rust
`impl Trait for T` methods, Java `@Override` methods, Kotlin `override fun`,
C#, TypeScript and C++ `override` methods, Python `typing.override`
methods, PHP `#[\Override]` methods, and Kotlin `override val`/`var`, C# `override` properties,
indexers and events and TypeScript `override` fields only. No language
records variable or field types.

Because of this, where details are missing:

- a `private` method and a `public` method look the same;
- an abstract class is a plain `class`;
- `override` is not linked to the method it overrides (except Rust, Java,
  Kotlin, C#, TypeScript, Python, C++ and PHP methods, above);
- overloads are separate rows with the same name and parent, with no
  signature to tell them apart.

Persisting these as structured symbol metadata needs a shared-model/schema
design, not necessarily new kind variants. Tools can already read signatures
and modifiers from source snippets without persisting them. A cross-cutting
contract/schema change needs an issue first (`CONTRIBUTING.md`, "Changing the `LanguageParser`
trait, the SQLite schema, or an already-published MCP tool signature").

## Coverage score

Each language gets two scores: one for the constructs it indexes, one for
the features that work on it. Every applicable check counts: ✅ is 1,
⚠️ is 0.5, ❌ is 0. Checks marked — or `[?]` are left out.

The construct score compares a language with its own checklist, not the
whole language specification. Do not rank languages by it: their constructs
and denominators differ. The feature score uses ten common checks, but
measures these fixtures and known limits, not general tool accuracy. Scores
round to the nearest whole percent, with halves rounded up.

| Language | Constructs | Checks | Features | Checks | Unverified |
|---|--:|--:|--:|--:|--:|
| Rust | 60% | 26 | 80% | 10 | 0 |
| Python | 68% | 28 | 85% | 10 | 0 |
| JS/TS | 61% | 32 | 80% | 10 | 1 |
| Java | 70% | 27 | 65% | 10 | 0 |
| Kotlin | 70% | 33 | 65% | 10 | 2 |
| C# | 76% | 34 | 55% | 10 | 0 |
| C++ | 77% | 28 | 50% | 10 | 0 |
| Go | 47% | 18 | 60% | 10 | 0 |
| PHP | 83% | 30 | 65% | 10 | 0 |
| Lua | 54% | 13 | 50% | 10 | 1 |
| Bash | 81% | 8 | 60% | 10 | 0 |
| PowerShell | 38% | 17 | 60% | 10 | 0 |

## Language by language

The [checklists](#checklist-by-language) are the canonical per-language
comparison: they define scored checks and explain partial results. The syntax
tables map source forms to grammar nodes. A ✅ confirms the stated forms, not
every possible use of a construct or complete semantic analysis.

| Cause | Meaning | What fixing it takes |
|---|---|---|
| Not implemented | Missing extraction or normalization, with no written decision to omit it | A parser change |
| By design | A cited, explicit scope or mapping decision | Revisit that decision |
| Model limit | The shared persisted model cannot represent the information | Shared-model/schema design and an issue first |
| Grammar | The grammar rejects or misreads code independently known to be valid | A grammar fix or parser recovery |
| Bug | Incorrect output, rather than absence alone | A parser fix with regression coverage |

### Implementation inventory

All paths below are relative to `crates/`. Grammar versions are the requirements
in each crate's `Cargo.toml`, also checked against `Cargo.lock`. Parser lines
count Rust source under `src/`, excluding `src/libraries/`. Test counts are
actual `#[test]` declarations in `src/` and direct `tests/*.rs` files,
including ignored tests. Fixture code, text inside strings and macro-generated
tests are excluded; these counts do not claim that tests passed. Corpus counts
include only `tests/corpus/project/`, excluding malformed inputs,
snapshots and metadata. They were recounted for this review.

| Language / crate | Grammar requirement | Id; extensions | Parser lines |
|---|---|---|--:|
| Rust / [`mct-lang-rust`](../../../crates/mct-lang-rust) | tree-sitter-rust 0.24.2 | `rust`; .rs | 1141 |
| Python / [`mct-lang-python`](../../../crates/mct-lang-python) | tree-sitter-python 0.25.0 | `python`; .py, .pyi | 701 |
| JS/TS / [`mct-lang-js-ts`](../../../crates/mct-lang-js-ts) | tree-sitter-javascript 0.25.0, tree-sitter-typescript 0.23.2 | `javascript_typescript`; .js, .jsx, .mjs, .cjs, .ts, .mts, .cts, .tsx | 1064 |
| Java / [`mct-lang-java`](../../../crates/mct-lang-java) | tree-sitter-java 0.23.5 | `java`; .java | 401 |
| Kotlin / [`mct-lang-kotlin`](../../../crates/mct-lang-kotlin) | tree-sitter-kotlin-ng 1.1.0 | `kotlin`; .kt, .kts | 539 |
| C# / [`mct-lang-csharp`](../../../crates/mct-lang-csharp) | tree-sitter-c-sharp 0.23.5 | `csharp`; .cs | 591 |
| C++ / [`mct-lang-cpp`](../../../crates/mct-lang-cpp) | tree-sitter-cpp 0.23.4 | `cpp`; .cpp, .cc, .cxx, .hpp, .hh, .h | 693 |
| Go / [`mct-lang-go`](../../../crates/mct-lang-go) | tree-sitter-go 0.25.0 | `go`; .go | 458 |
| PHP / [`mct-lang-php`](../../../crates/mct-lang-php) | tree-sitter-php 0.24.2 | `php`; .php | 854 |
| Lua / [`mct-lang-lua`](../../../crates/mct-lang-lua) | tree-sitter-lua 0.5.0 | `lua`; .lua | 484 |
| Bash / [`mct-lang-bash`](../../../crates/mct-lang-bash) | tree-sitter-bash 0.25.1 | `bash`; .sh, .bash | 471 |
| PowerShell / [`mct-lang-powershell`](../../../crates/mct-lang-powershell) | tree-sitter-powershell 0.26.4 | `powershell`; .ps1, .psm1 | 500 |
| HTML / [`mct-lang-html`](../../../crates/mct-lang-html) | tree-sitter-html 0.23.2 | `html`; .html, .htm | 412 |
| CSS / [`mct-lang-css`](../../../crates/mct-lang-css) | tree-sitter-css 0.25.0 | `css`; .css | 1041 |
| XML / [`mct-lang-xml`](../../../crates/mct-lang-xml) | tree-sitter-xml 0.7.0 | `xml`; .xml | 233 |
| XAML / [`mct-lang-xaml`](../../../crates/mct-lang-xaml) | tree-sitter-xml 0.7.0 | `xaml`; .xaml | 410 |
| Markdown / [`mct-lang-md`](../../../crates/mct-lang-md) | tree-sitter-md 0.5.3 | `markdown`; .md | 721 |

### Verification inventory

Tests are **parse / corpus / index integration / other**, where other includes
unit tests and all remaining integration files. Every listed crate has its own
`fuzz/fuzz_targets/parse_<suffix>.rs` (JS/TS uses `parse_js_ts.rs`, Markdown
`parse_md.rs`) and appears in the [CI fuzz-smoke matrix](../../../.github/workflows/ci.yml).
This verifies wiring, not a new fuzz campaign. Library folders below contain
**test-only regression modules** behind `#[cfg(test)]`; they are not production
framework adapters. See [library support](../../01-architecture/library-support.md).

| Language | Tests: parse / corpus / index / other | Ignored | Corpus: files / lines | Test-only libraries |
|---|---|--:|---|---|
| Rust | 23 / 7 / 9 / 26 (src/lib.rs: 3, tests/dead_code.rs: 4, tests/depgraph.rs: 3, tests/tokens.rs: 16) | 1 | 6 / 2,179 | — |
| Python | 9 / 8 / 8 / 28 (src/libraries/django/mod.rs: 1, src/libraries/flask/mod.rs: 1, tests/depgraph.rs: 2, tests/tokens.rs: 24) | 0 | 15 / 2,765 | flask, django |
| JS/TS | 10 / 12 / 8 / 5 (src/libraries/react/mod.rs: 3, tests/depgraph.rs: 2) | 0 | 7 / 2,193 | react |
| Java | 11 / 11 / 4 / 3 (tests/depgraph.rs: 3) | 1 | 6 / 1,989 | — |
| Kotlin | 13 / 13 / 9 / 4 (tests/depgraph.rs: 2, tests/scanner_eof.rs: 2) | 1 | 5 / 1,527 | — |
| C# | 7 / 10 / 4 / 4 (tests/depgraph.rs: 4) | 0 | 6 / 2,048 | — |
| C++ | 6 / 9 / 4 / 2 (tests/depgraph.rs: 2) | 0 | 5 / 1,568 | — |
| Go | 8 / 11 / 5 / 4 (tests/depgraph.rs: 4) | 0 | 7 / 3,002 | — |
| PHP | 13 / 10 / 5 / 3 (tests/depgraph.rs: 3) | 0 | 6 / 2,463 | — |
| Lua | 11 / 9 / 4 / 2 (tests/depgraph.rs: 2) | 0 | 5 / 1,525 | — |
| Bash | 10 / 9 / 5 / 3 (tests/depgraph.rs: 3) | 1 | 5 / 1,539 | — |
| PowerShell | 10 / 8 / 5 / 2 (tests/depgraph.rs: 2) | 0 | 5 / 1,515 | — |
| HTML | 14 / 9 / 7 / 2 (tests/depgraph.rs: 2) | 0 | 5 / 1,563 | — |
| CSS | 40 / 8 / 6 / 13 (src/lib.rs: 7, src/libraries/bootstrap/mod.rs: 1, src/libraries/tailwind/mod.rs: 3, tests/depgraph.rs: 2) | 0 | 12 / 3,240 | bootstrap, tailwind |
| XML | 9 / 10 / 6 / 0 (none) | 0 | 6 / 2,037 | — |
| XAML | 11 / 10 / 5 / 2 (tests/depgraph.rs: 2) | 0 | 6 / 1,860 | — |
| Markdown | 54 / 10 / 32 / 28 (tests/literals.rs: 4, tests/note_graph.rs: 24) | 0 | 6 / 1,825 | — |

### Area scores

Derived from the applicable checklist items with the same weights as the overall
score; unknown checks are excluded. Counts and unknowns remain in the checklists.

| Language | Functions | Types / OOP | Data | Relations | Search | Quality | Structure |
|---|--:|--:|--:|--:|--:|--:|--:|
| Rust | 79% | 75% | 25% | 50% | 70% | 75% | 100% |
| Python | 78% | 75% | 0% | 100% | 80% | 100% | 83% |
| JS/TS | 69% | 70% | 17% | 75% | 80% | 50% | 100% |
| Java | 80% | 85% | 38% | 63% | 80% | 25% | 67% |
| Kotlin | 72% | 79% | 33% | 83% | 80% | 50% | 50% |
| C# | 83% | 92% | 30% | 75% | 60% | 25% | 67% |
| C++ | 88% | 94% | 42% | 70% | 60% | 25% | 50% |
| Go | 67% | 90% | 0% | 40% | 60% | 25% | 83% |
| PHP | 93% | 83% | 50% | 100% | 80% | 50% | 50% |
| Lua | 88% | 75% | 0% | 67% | 60% | 25% | 50% |
| Bash | 100% | 100% | 50% | 100% | 80% | 50% | 33% |
| PowerShell | 42% | 25% | 25% | 67% | 80% | 50% | 33% |

### Rust

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `fn f()` at top level or in `mod` | `function_item` | `function`, no parent |
| `fn` inside `impl T` / `impl Tr for T` | `function_item` | `method`, parent `T` (generic arguments kept) |
| `fn` inside `trait`, with or without body | `function_item`, `function_signature_item` | `method`, parent the trait |
| `struct`, unit struct, `union` | `struct_item`, `union_item` | `struct`; fields as `field` (tuple fields `0`, `1`) |
| `enum` | `enum_item` | `enum`; variants as `constant` owned by the enum |
| `macro_rules!` | `macro_definition` | `function` |
| `let` and parameter bindings | `let_declaration`, `parameter` | `variable` owned by the function; `let name = |..| ..` is a `function` |
| `trait` | `trait_item` | `trait` |
| `type X = …`, associated `type` | `type_item`, `associated_type` | `type_alias`, parent the impl type |
| `const`, `static` | `const_item`, `static_item` | `constant`, `variable` |
| `mod m { … }`, `mod m;` | `mod_item` | `module` |
| `use a::{b, c as d}` | `use_declaration`, `use_list`, `scoped_use_list`, `use_as_clause` | `imports` per name |
| `f()`, `a::f()`, `x.m()`, `f::<T>()` | `call_expression`, `scoped_identifier`, `field_expression`, `generic_function` | `calls` |
| `impl Tr for T` | `impl_item` | `implements Tr`, owned by the file module; its associated types count as used |
| `trait B: A` | `trait_item` | `extends A` |
| `#[derive(Clone)]`, other non-compiler attributes | `attribute_item` | `references` to the item or derive named; a bare built-in attribute of the Rust reference (`#[ignore]`, `#[should_panic]`, `#[track_caller]`, `#[non_exhaustive]`…) is none, since an imported macro can't share its name (E0659) |
| a same-file free function used as a value, or inside a macro | `identifier`, `token_tree` | `references` |
| a `let` closure used as a value later in the same function (`.filter(eligible)`) | `identifier` | `references` |
| a type in a parameter, return type, field, `let`/`const`/`static` annotation, type alias, generic argument, cast or struct literal | `type_identifier`, `scoped_type_identifier` | `references`; the item's own type parameters and `Self` are skipped |
| the type a path goes through (`Dimension::Kind`, `Assessor::new()`) | `scoped_identifier` | `references` to the capitalized qualifier, besides the last segment |
| a same-file unit struct used as a value (`&Broken`) | `identifier` | `references` |
| `m::f(..)` and `x.f(..)` inside a macro's arguments (`vec![format::hits(..)]`) | `token_tree` | `references`; a `m::` path keeps its module/qualifier evidence, and `x.f(..)` the receiver evidence of a method call outside a macro: `self.f(..)` is qualified by the impl type, any other receiver is a member call (never resolved by a unique name) |
| a macro invoked through a path (`mct_corpus::standard_tests!`), or a same-file macro invoked inside another macro's arguments | `macro_invocation`, `token_tree` | `calls` |
| harness attributes such as `#[test]` / `#[tokio::test]` | `attribute_item` | a self-`references` relation to the following function |
| a bare call to an imported function inside a macro's arguments (`println!("{}", helper(x))`) | `token_tree` | `calls` |
| string literals | `string_literal`, `raw_string_literal` | literals for exact-text search |
| a prelude name (`Ok`, `Some`, `String`, `Box`, `Debug`, `Default`…) or a name imported from `std`/`core`/`alloc` | any of the above | marked `external` only when the file declares no item of that name, no non-`std` `use` of it reaches the site, no lowercase local shadows it, and no glob that could bring it in reaches the site. An enum variant is not a bare name (`Level::Debug` doesn't shadow the `Debug` derive) unless a glob over its enum reaches the site. A glob over an enum declared in the file and a `use super::*` in a top-level inline `mod` are otherwise transparent |
| a bare name a `use` of another crate brings in (`use serde::Serialize` then `#[derive(Serialize)]`, `Serialize` in a type, `helper()`) | any of the above | the crate as module evidence (`serde`), so a same-named item of this repository outside `serde/` is never the target; a crate of this repository is found by its directory (`mct_core` in `crates/mct-core/`). Only an import whose block reaches the site counts, and all of them must name one crate. None for a `crate::`/`self::`/`super::` path, an alias, a name the file declares or binds, a path from a module of the file (`mod support; use support::X`), or any import in a test, example, bench, binary or build script (they may import their own package by a name no path spells) |
| the trait of `impl Tr<..> for T` / `impl m::Tr for T` / a supertrait bound | `impl_item`, `trait_bounds` | `implements`/`extends` the trait's last segment without generic arguments (`From`, `Display`), with `kind = trait` unless external |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| `implements` owned by the file module | Bug | the type that implements the trait is not the owner |
| A call to a function of another file inside a macro that is not imported or is reached through a field or path (`x.f()`) | By design | the traversal comment limits macro identifiers to imported names and same-file free functions; other forms are `references`, to avoid mistaking fields and paths for calls |
| Visibility, generics, signatures | Model limit | see [What the index can hold](#what-the-index-can-hold) |
| A bare prelude name under a glob whose source is another file (`use super::*` in a file module, `use other::*`) is not marked `external`, and resolves to a same-named item elsewhere when there is only one | By design (parser) + model limit (index) | the parser can't see what the glob brings in, so it proves nothing; the index then reports a unique name match as `resolved`. On this repository: 14 `String` references in `mct-index/src/reviews/tests.rs` and `mct-cli/src/web/tests/group_invariants.rs` resolve to `LiteralCategory::String`. Fixing it needs "resolved by name" to be told apart from "resolved by evidence" in the index |
| A name imported from a package whose directory has another name (`use grep_searcher::X` for `crates/searcher/`) is `unresolved` in a library file | Model limit | the index matches module evidence against path segments and doesn't map package names to directories ([#374](https://github.com/Zubiarka8/mini-consumes-tokens/issues/374)). A missed edge, never a wrong one. No package of this repository that is imported has a renamed directory |
| `m::f()` where `m` is an inline `mod` of the same file is `unresolved` | Model limit | a function in an inline `mod` is indexed with no parent, so the `module = m` evidence finds no candidate (a missed edge, never a wrong one; `suite-resolution.json`, `rust: same name in different modules`) |
| The prelude list is the 2021 edition's | By design | `TryFrom`, `TryInto` and `FromIterator` are not in the 2015/2018 prelude; a crate on those editions that defines one of them is still read correctly, because a declared or imported name is never external |

### Python

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `def` / `async def` at top level | `function_definition` | `function` |
| `def` in a class | `function_definition` | `method`, parent the class |
| `def` inside a function | `function_definition` | `function`, no parent |
| `class C(A, B)` | `class_definition` | `class`; `extends` per base |
| decorated `def`/`class` | `decorated_definition` | the symbol, plus `references` to each decorator |
| `type X = …` (PEP 695) | `type_alias_statement` | `type_alias` |
| `import a.b`, `from .x import y as z` | `import_statement`, `import_from_statement`, `aliased_import`, `relative_import` | `imports` for the module and each name |
| `f()`, `obj.m()` | `call`, `attribute` | `calls` |
| a bare builtin (`open()`, `len()`, `class E(Exception)`, `@property`) | `call`, `identifier` | marked `external` only when nothing in the file binds the name — no `def`, `class`, import, parameter, assignment, loop, `with`/`except`/walrus/`match` target, `global` or `type` of it — and the file has no `from m import *`, `globals()`, `exec`, `vars()` or `sys.modules`. The list is CPython 3.14's `builtins` without `site`'s names |
| a call to an imported name (`from app.util import parse; parse()`, `import app.legacy as lg; lg.parse()`, `import app.util; app.util.parse()`) | `import_from_statement`, `import_statement`, `call` | carries the module as evidence (`util`, `legacy`): only a definition under a path component of that name can be the target, so a third-party module never resolves to a repo function. Not applied when anything else in the file binds the name, or the file is opaque (see the builtin row) |
| a bare call to a `def`/`class` of the same file (`count()` → `def count`) | `call`, `identifier` | `calls`, kept to this file when only a `def`/`class` outside a class body binds the name |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Module variables and constants, class attributes, `self.x`, dataclass fields | Not implemented | no arm for `assignment` or `expression_statement`; recorded as a known limit in `corpus-progress.md` |
| Lambdas | Not implemented | no arm for `lambda` |
| Enum subclasses | Not implemented | ordinary class traversal emits `class` plus `extends Enum`; assigned enum values are not symbols |
| Nested functions have no parent | Not implemented | the parent is set only for class members |
| Getter and setter as two rows | Not implemented | both are emitted as methods; the shared `field` kind could model a combined property, as in C# |
| Exact-text search in strings | Not implemented | the parser does not fill `literals` |
| A builtin is not `external` anywhere in a file that binds its name once, even in another function | By design | bindings are read file-wide, not per scope: it loses some `external` answers but never claims a wrong one |
| Two nested `def`s of one name in the same file are `ambiguous` | Model limit | the call is kept to the file, but `RelationTarget` can't name the enclosing function (`suite-resolution.json`, `report.py` `fmt`) |

### JS/TS

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `function f`, generators | `function_declaration`, `generator_function_declaration` | `function` |
| `const f = () => …`, `= function …` | `variable_declarator` + `arrow_function`/`function_expression` | `function` |
| `class` / `abstract class` | `class_declaration`, `abstract_class_declaration` | `class` |
| class methods, `constructor`, `get`/`set` | `method_definition` | `method` |
| class fields (`#private`, `static`) | `field_definition`, `public_field_definition` | `field`; an arrow-function value becomes `method` |
| `interface` | `interface_declaration` | `interface` (members skipped) |
| `enum` | `enum_declaration` | `enum` (members skipped) |
| `type X = …` | `type_alias_declaration` | `type_alias` |
| `namespace N {}` | `internal_module` | `module`; its functions keep no parent |
| `extends`, `implements` | `extends_clause`, `implements_clause` | `extends`, `implements` |
| ES `import`, CommonJS `require` | `import_statement`, `named_imports`, `namespace_import`, `call_expression` | `imports` |
| `export`, `module.exports.x = …` | `export_statement`, `assignment_expression` | `references`; a function value becomes `function` |
| `f()`, `new X()` | `call_expression`, `new_expression`, `member_expression` | `calls` |
| the receiver of `x.m()` | `call_expression` + `member_expression` `object` | qualified by `x`'s declared type: a TS annotation (`x: T`, `const x: T`), `= new T()`, `new T().m()`, a class field or parameter property (`this.f.m()`). `this.m()` is qualified by the enclosing class when its body declares `m`; otherwise it is left as before, since `m` may be inherited. A local without a proven type, a call result or any other expression is a member call: ambiguous even with one candidate, and only a `method` can be its target, so a function reached through a module object (`core.f()` on a loaded module) is unresolved. A name that is no local (a top-level object, an import) is left as before. Inherited and interface methods are not found through the qualifier (interface members are not indexed), so such a call is unresolved |
| a same-file function used as a value (`onclick: toggle`, `.map(card)`, `return wrapped`, `{ toggle }`) | `identifier`, `shorthand_property_identifier` | `references`; a parameter, pattern or non-function `let`/`const` of the same name in an enclosing function hides it, and a function's use of its own name is skipped |
| JSX `<Button />` | `jsx_opening_element`, `jsx_self_closing_element` | `references` (DOM tags excluded) |
| a TS type in an annotation, return type, alias value, generic argument or cast (`props: Props`, `useState<Problem>()`) | `type_identifier` | `references`; the enclosing declarations' type parameters are skipped (their constraints are not walked) |
| `export default f({ … })` | `export_statement` value | the call and its arguments are walked like any expression |
| object literal methods | `method_definition` under `object` | `function` |
| an ECMAScript global (`Math.max()`, `JSON.parse()`, `parseInt()`, `new Map()`) | `call_expression`, `new_expression` | marked `external` only when nothing in the file binds the name — no declaration, import, parameter, pattern or assignment of it — and the file has no direct `eval` or `with`. Host globals (`window`, `document`, `process`, `console`) are not on the list |
| a call to an imported name (`import { f } from "./fmt"; f()`, `import f from "m"`, `import * as m from "./m"; m.f()`, `new X()`) | `import_statement`, `call_expression`, `new_expression` | carries the module as evidence: the specifier's last segment without extension, or the directory of an `index` file (`fmt`, `legacy` for `./legacy/index.js`, `left-pad` for a package). Only a definition under a path component of that name can be the target, so a package never resolves to a repo function, and a re-export through a barrel file is unresolved rather than wrong. Not applied when anything else in the file binds the name, or under `eval`/`with` |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Interface members, enum members | Not implemented | `src/lib.rs:29` calls interface members a known limit, not a decision; `walker.rs` skips both bodies |
| `abstract m(): T;` | Not implemented | `abstract_method_signature` has no arm |
| Top-level `const`/`let`/`var` with a non-function value | Not implemented | `variable_declarator` is used only when the value is a function |
| Constructor parameter properties (`constructor(public x)`) | Not implemented | no arm for `required_parameter` with an accessibility modifier |
| Decorators, `super()` calls | Not implemented | no arm for `decorator`; `super` is not a call target |
| Overload signatures | Not implemented | only the implementation is a `method_definition` |
| Repeated code, exact-text search | Not implemented | no token source and no literals for JS/TS |
| A global is not `external` anywhere in a file that binds its name once, even in another function | By design | bindings are read file-wide, not per scope: it loses some `external` answers but never claims a wrong one |
| `console.log()`, `document.querySelector()` and other host calls are not `external` | By design | the language does not guarantee host globals, and a project may define them |

### Java

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `class`, `record` | `class_declaration`, `record_declaration` | `class` |
| record components (`record P(T a)`) | `formal_parameter` of `record_declaration` | `method` `a`, the implicit accessor, unless the body declares `a()` |
| `interface`, `@interface` | `interface_declaration`, `annotation_type_declaration` | `interface` |
| `enum` and its constants | `enum_declaration`, `enum_constant` | `enum`; constants as `field` |
| methods and constructors | `method_declaration`, `constructor_declaration` | `method`, one row per overload |
| fields | `field_declaration` | `field` |
| `extends`, `implements` | superclass / interfaces of `class_declaration` | `extends`, `implements` |
| `import a.b.C`, `import static`, `.*` | `import_declaration`, `scoped_identifier`, `asterisk` | `imports` (each emitted twice) |
| `m()`, `obj.m()`, `new X()` | `method_invocation`, `object_creation_expression` | `calls` |
| the target of `m()`, `obj.m()` | `method_invocation` | only a `method` can be its target: Java cannot invoke a field, so a same-named field is never a candidate |
| the receiver of `x.m()`, `T.m()` | `method_invocation` `object` | qualified by `x`'s declared type (parameter, local, field, `for`, resource, `catch`, pattern; `this.f`), or by `T` for a capitalized name that is no variable in scope (`Outer.Inner` too). Any other receiver — a call result, `var`, an untyped lambda parameter, a type variable — is a member call: ambiguous even with one candidate. A method inherited from a supertype is not found through the qualifier, so such a call is unresolved |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| `interface B extends A` | Not implemented | `extends_interfaces` is not read, while class `extends`/`implements` are |
| Anonymous class in a field initializer | Not implemented | the initializer is not traversed, so its members are skipped |
| Anonymous-class methods inside a method | Bug | generic traversal assigns them to the enclosing class instead of an anonymous owner |
| Annotations as relations, lambdas, `super()`/`this()` | Not implemented | no arm for `annotation`, `lambda_expression` or `explicit_constructor_invocation` |
| Separate file/type import evidence | By design | [push_type_import](../../../crates/mct-lang-java/src/lib.rs#L325) explicitly targets the declaring file module, alongside the imported symbol; repeated names alone are not a defect |
| Dead code only checks types | By design | methods are explicitly excluded in [dead_code.rs](../../../crates/mct-index/src/dead_code.rs#L7) |
| Maven/Gradle dependencies | By design | explicitly excluded in [Dependency detection](../../../internal/checklist.md#dependency-manifest-detection); the scope decision mentions Gradle's DSL |

### Kotlin

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| top-level `fun`, extension `fun T.f()` | `function_declaration` | `function`; an extension is `method` with parent `T` |
| `fun` in a class, object or interface | `function_declaration` | `method` |
| `class`, `interface`, `object` | `class_declaration`, `object_declaration` | `class` / `interface`; `object` as `class` |
| `val`/`var` property in a class | `property_declaration` | `field` |
| `class C(val x: Int)` | `class_parameter` | `field` |
| `enum class` entries | `enum_entry` | `field` |
| `: Base()`, `: Iface` | `constructor_invocation`, `user_type` | `extends` (with `()`), `implements` (without) |
| `import a.b.c`, `import … as x` | `import` | `imports` (alias when present) |
| `f()`, `obj.m()` | `call_expression`, `navigation_expression` | `calls` |
| the receiver of `x.m()` | `call_expression` + `navigation_expression` receiver | qualified by `x`'s declared type: a parameter or `val x: T` (`T?` and `List<T>` name `T` and `List`), `= T(…)` (a capitalized callee), `T(…).m()`, a class property or a `val`/`var` constructor property (`this.f.m()`). A capitalized name (`Money.of()`, an object or companion) is its own qualifier. `this.m()` is qualified by the enclosing class when its body declares `m`; otherwise it is left as before, since `m` may be inherited; inside a lambda `this` is not the class. A local without a proven type (`it`, a lambda or `for` variable, `val x = f()`), a call result, `this@Label` or any other expression is a member call: ambiguous even with one candidate. A lowercase name that is no local (a package, a top-level property) is left as before. Inherited and extension methods are not found through the qualifier, so such a call is unresolved |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Primary and secondary constructors | Not implemented | no arm for `primary_constructor` or `secondary_constructor` |
| `typealias`, top-level `val`/`var`/`const val` | Not implemented | no arm for `type_alias`; a top-level `property_declaration` gives no symbol |
| `object` as `class` | By design | explicit mapping in [crate docs](../../../crates/mct-lang-kotlin/src/lib.rs#L8) |
| `enum class` as `class`, `companion object` without a symbol | Not implemented | not documented as a decision; a companion's members get the outer class as parent |
| Interface inherits interface using `implements` | Bug | `interface B : A` has output of the wrong relation kind: `implements`, not `extends` |
| Anonymous object in a function | Not implemented | no class symbol; its named functions become parentless `function` rows |
| Infix and operator calls (`a + b`) | Not implemented | [crate docs](../../../crates/mct-lang-kotlin/src/lib.rs#L16) describe a known limit, not an explicit decision to omit them |
| One-line member without separator (`class A { fun f() {} }`) | Worked around | tree-sitter-kotlin-ng reports a missing separator; the parser reparses with a `;` in the blank before each such `}` and keeps the result only when it is clean ([`reparse_with_terminated_lines`](../../../crates/mct-lang-kotlin/src/lib.rs)) |
| Duplicate import names | Bug | path/stem or symbol import evidence can leave an extra external entry; not every import produces two rows |

### C#

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `namespace N {}` / `namespace N;` | `namespace_declaration`, `file_scoped_namespace_declaration` | `module`; types get `N::` in their parent |
| `class`, `record` | `class_declaration`, `record_declaration` | `class`; `record struct` is `struct` |
| `struct`, `interface`, `enum` | `struct_declaration`, `interface_declaration`, `enum_declaration` | `struct`, `interface`, `enum`; members as `constant` |
| methods, constructors, destructors, operators | `method_declaration`, `constructor_declaration`, `destructor_declaration`, `operator_declaration`, `conversion_operator_declaration` | `method` |
| fields, events | `field_declaration`, `event_field_declaration` | `field` |
| properties, indexers, event properties | `property_declaration`, `indexer_declaration`, `event_declaration` | `field` |
| local functions | `local_function_statement` | `function`, no parent |
| `delegate` | `delegate_declaration` | `type_alias` |
| `: Base, IFoo` | base list | `extends` / `implements` by the `IFoo` heuristic |
| `using N;`, `using static` | `using_directive` | `imports` |
| `f()`, `a?.F()`, `new T()` | `invocation_expression`, `conditional_access_expression`, `object_creation_expression` | `calls` |
| the receiver of `x.M()`, `x?.M()` | `member_access_expression` `expression`, `conditional_access_expression` `condition` | qualified by `x`'s declared type: a parameter, a typed local (`T?` and `List<T>` name `T` and `List`), `var x = new T()`, a `foreach`, pattern (`is T x`), `out` or `catch` variable, `new T().M()`, a field, a property or a primary constructor parameter (`this.f.M()` too). A predefined type (`string`) is its own qualifier, so no repository method is its target. A capitalized name that is no local or field (`Money.Of()`, a static class) is its own qualifier. `this.M()` is qualified by the enclosing type when its body declares `M`; otherwise it is left as before, since `M` may be inherited. A lambda parameter, `var x = F()`, a call result or any other expression is a member call: ambiguous even with one candidate. `base.M()` and a lowercase name that is no local or field are left as before. Inherited methods and extension methods (their parent is the static class) are not found through the qualifier, so such a call is unresolved |
| `[Attribute]` | `attribute` | `references XAttribute` |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Base list split by naming | By design | C# syntax does not separate base class from interfaces; the [documented heuristic](../../../crates/mct-lang-csharp/src/lib.rs#L420) uses the `IFoo` convention |
| Record positional parameters | Not implemented | no arm for the record's `parameter_list` |
| Static class used as a qualifier (`Helpers.X()`) | Not implemented | the qualifier of a member call is not a reference |
| Lambdas, locals | Not implemented | no arm for `lambda_expression` or `local_declaration_statement` |
| Base-constructor call, `using Alias = Type` symbol | Not implemented | base initializer arguments are traversed without a call to the base; alias directives emit imports only |
| `.csproj`/NuGet dependencies | By design | explicitly excluded in [Dependency detection](../../../internal/checklist.md#dependency-manifest-detection) |

### C++

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `namespace n {}` | `namespace_definition` | `module` |
| `class`, `struct`, `union` | `class_specifier`, `struct_specifier`, `union_specifier` | `class` / `struct` (`union` as `struct`) |
| free functions, templates | `function_definition`; recursion through `template_declaration` | `function` |
| member functions, out-of-line `A::m` | `function_definition`, `qualified_identifier` | `method`, parent `A` |
| constructors, destructors, `operator+` | `function_declarator` with `destructor_name`, `operator_name` | `method` |
| data members | `field_declaration` | `field` |
| global and namespace variables | `declaration` | `variable` |
| `enum`, `enum class` and their values | `enum_specifier` | `enum`; values as `constant` |
| `using X = …`, `typedef` | `alias_declaration`, `type_definition` | `type_alias` |
| `#include "x.h"`, `<vector>` | `preproc_include` | `imports` (path and stem) |
| base classes | `base_class_clause` | `extends` |
| `f()`, `a.m()`, `p->m()`, `ns::f()`, `T()` | `call_expression`, `field_expression`, `qualified_identifier` | `calls`; `new T()` is not handled |
| `std::f()`, `::std::f()`, `std::f<T>()`, `std::chrono::f()` | `call_expression`, `qualified_identifier`, `template_function` | `calls` named `f` (template arguments dropped), external: a program may not add declarations to namespace `std` ([namespace.std]). Not proven in a file that opens `namespace std` itself (a `std::hash` specialization), nor for a bare `f()` after `using namespace std;` |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| `#define`, lambdas, locals | Not implemented | no arm for `preproc_def` or `lambda_expression`; locals are skipped |
| `new T()` and base-constructor calls | Not implemented | no call relation for `new_expression` or the base initializer; ordinary `T()` expressions are calls |
| `const int X` is a `variable` | Not implemented | a global `const` gets the same kind as a mutable variable; only enum values are `constant` |
| A type used only in a declaration (`Widget w;`) | Not implemented | a declaration's type is not a reference |
| Header and source files are two rows | By design | [crate docs](../../../crates/mct-lang-cpp/src/lib.rs#L3) explicitly choose independent header/source rows and correlate matching names and parents |
| Some valid C++ is rejected | Grammar | tree-sitter-cpp 0.23.4, issue #82 |

### Go

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `package p` | `package_clause` | a `module` named `p` |
| `func F()` | `function_declaration` | `function`, parent the package |
| `func (r *T) M()` | `method_declaration` | `method`, parent `T` (pointer and generic receivers unwrapped) |
| `type T struct` | `type_spec` | `struct` |
| `type I interface { M() }` | `type_spec` | `interface`; each method as `method` |
| `type A = B`, `type A int` | `type_alias`, `type_spec` | `type_alias` |
| `import "x"`, grouped imports | `import_declaration`, `import_spec_list`, `import_spec` | `imports` (path and stem) |
| `f()`, `pkg.F()`, `x.M()`, `F[T]()` | `call_expression`, `selector_expression`, `type_instantiation_expression` | `calls` |
| the receiver of `x.M()` | `call_expression` + `selector_expression` `operand` | qualified by `x`'s declared named type: the method receiver, a parameter, `var x T`, `x := T{…}`, `x := &T{…}`, `x := new(T)`, a package-level `var` (`*T`, `Stack[T]` and `pkg.T` name `T`, `Stack` and `T`), `T{…}.M()`, `x.(T).M()`, or a field of a struct this file declares (`s.repo.M()`). A declared type without a name (`interface{ … }`, a slice, a variadic parameter) makes a member call: ambiguous even with one candidate. So does a struct this file declares with an embedded field and without a method `M` in this file, since `M` may be promoted. Where the syntax does not state the type (`x := f()`, a `range` variable, a multi-value assignment, a call result, an index expression, a field of a struct declared in another file, an imported package) the call is left as before, to name resolution, and a unique name is reported as `name` evidence. A promoted method on a struct declared in another file is not found through the qualifier, so such a call is unresolved |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Struct fields, `const`, `var` | Not implemented | no arm for `field_declaration`, `const_declaration` or `var_declaration`; kept as known-limit tests |
| Implicit interface satisfaction | By design | explicitly deferred pending type resolution in [Go crate docs](../../../crates/mct-lang-go/src/lib.rs#L3) and [open decision 3](../../../internal/checklist.md#open-decisions) |
| Struct and interface embedding | Not implemented | the decision above concerns conformance inference, not explicit embedded declarations; no embedding relation is emitted |
| Composite literals (`T{}`) and conversions as uses | Not implemented | `composite_literal` gives no relation; conversions appear as calls |
| Closures, locals | Not implemented | no arm for `func_literal` or short variable declarations |

### PHP

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `function f()` | `function_definition` | `function` |
| `class`, `interface`, `trait`, `enum` | `class_declaration`, `interface_declaration`, `trait_declaration`, `enum_declaration` | `class`, `interface`, `trait`, `enum` |
| methods, `__construct` | `method_declaration` | `method` |
| properties, promoted constructor parameters | `property_declaration`, `property_promotion_parameter` | `field` |
| `const` top-level and in types | `const_declaration` | `constant` |
| `case` in an enum | `enum_case` | `field` |
| `$f = function () {}`, `fn() =>` | `assignment_expression` | `function` |
| `extends`, `implements`, `use Trait;` | `base_clause`, `class_interface_clause`, `use_declaration` | `extends`, `implements` |
| `use A\B;` | `namespace_use_declaration` | one `imports` per clause, named after the imported class (an alias is not used), with its namespace as evidence |
| `require`/`include` with a literal path | `binary_expression`, function call | `imports` (path and stem) |
| `f()`, `$o->m()`, `A::m()`, `new X()` | `function_call_expression`, `member_call_expression`, `scoped_call_expression`, `object_creation_expression` | `calls` (static calls qualified by their class and namespace) |
| a call that can only reach one of PHP's core functions: `\strlen()`, or a bare `strlen()` in a file with no namespace and no `use function` of it | `function_call_expression` | `calls`, external: a global core function can't be declared again ("Cannot redeclare"). The list (`src/builtins.rs`) holds only functions every PHP 7.0 build has (`Core`, `standard`, `date`, `pcre`, `spl`, `json`) and the call-like keywords (`isset`, `empty`, `exit`…), so no polyfill of a newer function or of an optional extension (`str_contains`, `mb_*`, `ctype_*`) is taken for one. Not proven: a bare call inside a namespace (it may reach a namespaced function of that name) and a name the file declares itself (an `if (!function_exists(…))` polyfill) |
| type hints, `catch`, `instanceof`, `X::class`, `X::CONST` | `named_type`, `binary_expression`, `class_constant_access_expression`, `scoped_property_access_expression` | `references` |
| Laravel routes, `view()`, `withRouting()` | `scoped_call_expression`, `function_call_expression` | `calls` to controller methods, `imports` of `*.blade.php` and route files (see [library support](../../01-architecture/library-support.md#laravel)) |
| `*.blade.php` | Blade directive and `<x-…>` scan | `imports` / `references` of templates and classes |
| `#[Attribute]` | `attribute` | `references` |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| `define()` constants | By design | `define()` stays a call by the explicit [PHP decision](../../../crates/mct-lang-php/src/lib.rs#L68) |
| Global `$variables` | Not implemented | a top-level `assignment_expression` gives a symbol only for a closure value |
| Anonymous classes (`new class {}`) | Partly | `anonymous_class` gives only the `extends`/`implements` relations, no symbols; the one exception is an anonymous Laravel `Migration`, whose `up`/`down` methods are indexed under the migration file's module |
| `namespace` as a symbol | By design | explicit file-module-only [PHP decision](../../../crates/mct-lang-php/src/lib.rs#L28) |
| Trait as `trait`, `use Trait` as `implements` | By design | explicit [PHP mappings](../../../crates/mct-lang-php/src/lib.rs#L9), using the closest existing kinds |
| Class imports target the declaring file | By design | a class `use` names the file `Module` that declares the class in the imported namespace; `use function` / `use const` target that function or constant instead |
| Composer autoload map, `composer.lock` | Not needed / not read | `composer.json` `require`/`require-dev` are listed as dependencies; classes resolve through their declared namespace, so `autoload.psr-4` is not consulted and `composer.lock` is not read |
| PHP secret files | Not implemented | `wp-config.php` and `config/database.php` are not excluded (`internal/checklist.md` open decision 2) |

### Lua

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `function f()`, `local function f()` | `function_declaration` | `function` |
| `function M.f()`, `function M:f()` | `function_declaration` with `dot_index_expression`/`method_index_expression` | `method`, parent `M` |
| `local f = function() end` | `assignment_statement` + `function_definition` | `function` |
| `local M = {}` | `assignment_statement` + `table_constructor` | `module` |
| `require("x")` | `function_call` | `imports` |
| `f()`, `M.f()`, `o:m()` | `function_call` | `calls` |
| `self:m()` in `function T:x()`, `o:m()` | `function_call` + `method_index_expression` | `calls` qualified by `T`; any other receiver is a member call (type unknown) |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Classes built with `setmetatable` | Not implemented | only a table literal assigned to a name becomes a `module` |
| Fields (`self.x`), globals, top-level `local` values | Not implemented | `assignment_statement` is used only for functions and tables |
| Inheritance through `__index` | Not implemented | no relation for metatables |
| Dead code skips table/module methods | By design | methods are excluded by [the shared allowlist decision](../../../crates/mct-index/src/dead_code.rs#L7); free global/local functions remain candidates |

### Bash

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `f() {}`, `function f {}` | `function_definition` | `function`, parent the file |
| top-level `X=1`, `export`, `readonly`, `declare` | `variable_assignment`, `declaration_command` | `variable` |
| `source x`, `. x` | `command` | `imports` (path and stem) |
| any literal command | `command`, `command_name` | `calls` |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Variables inside functions (`local`) | By design | explicit noise/scope decision in [crate docs](../../../crates/mct-lang-bash/src/lib.rs#L20) |
| Commands through a variable (`$CMD arg`) | By design | explicit static-name decision in [crate docs](../../../crates/mct-lang-bash/src/lib.rs#L24) |
| `coproc NAME { … }` | Grammar | truncates the enclosing function, issue #79 |
| Some quoted `${a:+…}` semicolon forms | Grammar | `"${name:+before;after}"` still fails. The ERR-001 form `"${stale:+$stale; }"` is accepted by the parser's [targeted recovery](../../../crates/mct-lang-bash/src/lib.rs#L111), preserving original text and locations |

### PowerShell

#### Syntax it recognizes

| Source | Grammar node | Indexed as |
|---|---|---|
| `function Verb-Noun {}` | `function_statement` | `function`, parent the file |
| top-level `$x = …`, `$script:x = …` | `assignment_expression` | `variable` (scope prefix stripped) |
| `. .\lib.ps1`, `Import-Module X` | `command` | `imports` (path and stem) |
| any literal command or cmdlet | `command`, `command_name`, `command_name_expr` | `calls` |
| simple bare identifier comma lists (`Write-Output alpha,beta`) | `command_elements`, `generic_token` | accepted through [targeted recovery](../../../crates/mct-lang-powershell/src/lib.rs#L116); extraction retains original text and locations |

#### Why the gaps exist

| Gap | Cause | Detail |
|---|---|---|
| Classes, methods, properties, `enum` | By design | explicit exclusion in [crate docs](../../../crates/mct-lang-powershell/src/lib.rs#L30) |
| `$this.X = …` in a class becomes a variable `this` | Bug | seen in the probe: each such assignment adds a top-level `variable` named `this` |
| `: base(…)` constructors, `??`, ternary | Grammar | tree-sitter-powershell 0.26.4 rejects these forms; simple bare identifier comma lists are recovered, not an unconditional gap (`errors/REGISTRY.md` ERR-002) |
| Variables inside functions, computed import paths | By design | explicit static/scope limits in [crate docs](../../../crates/mct-lang-powershell/src/lib.rs#L16) |

## Markup and style languages

These parsers do not model functions, classes or variables. Here is what
they index instead:

| Language | Symbols | Relations | Not indexed |
|---|---|---|---|
| HTML | file `module`; `element` for each element with an `id`, nested under the nearest identified ancestor | `imports` for `<link rel="stylesheet">` and `<script src>` (path and stem); `references` to `#id` and each `.class` of an identified element | elements with only a `class`; inline `<script>`/`<style>` content; event attributes (`onclick`); template syntax such as `{% %}`, which gives wrong names (see `internal/checklist.md`, open decision 4) |
| CSS | file `module`; `rule` for each full selector and for each atom in it (`#header > .main` → `#header > .main`, `#header`, `.main`); nested rules resolved against their parent (`.parent .child`); rules inside `@media` | `imports` for `@import` (path and stem) | declarations and values; custom properties (`--brand`); `@keyframes`; pseudo-classes as atoms (`div.card:hover` → `div`, `.card`) |
| XML | file `module`; `element` for each element with `id`, `name` or `Name` (in that order, exact case) | none, [by design](../../../crates/mct-lang-xml/src/lib.rs#L6) | unnamed elements, attributes, text |
| XAML | file `module`; `element` for `x:Name` or `Name` | `references` from an event attribute (`Click="SaveBtn_Click"`) to the C# code-behind handler; `imports` for a relative `ResourceDictionary Source="Styles/Colors.xaml"`, pinned to the [target file](../../../crates/mct-lang-xaml/src/lib.rs#L330) | `{Binding …}` values; absolute, `pack:` and `;component` dictionary URIs; other attributes |
| Markdown | note `module`; `element` for each heading, nested by level, with its section range and `level` | `references` for `[[WikiLink]]`, its `#anchor`, `#tags`, frontmatter `title`/`aliases`/`tags`; `imports` for `![[embed]]` | code spans and fenced code (deliberately); links inside tables (table text is still collected as literals) |

## Features by language

The tools share one implementation for every language and work on what
the parser indexed. A tool cannot find what the tables above mark ❌. For
example, `find_references` on a Python module variable returns nothing,
because the variable is not a symbol.

Four features have their own per-language code, so they cover fewer
languages:

| Feature | Languages | Where it is decided |
|---|---|---|
| Repeated code (`mct-cli duplicates`) | Rust, Python | `build_token_sources` in `crates/mct-languages/src/lib.rs`; every other language is reported under `unsupported_languages` |
| String literals for `hybrid_search` `"quoted"` queries | Rust, Markdown | Only `mct-lang-rust` and `mct-lang-md` fill `ParsedFile::literals` |
| Brace-based body elision in `get_file_skeleton` | Rust, Go, Java, C++, C#, JS/TS, PHP, Kotlin | `BRACE_LANGUAGES` in `crates/mct-mcp-server/src/format.rs`; other languages show declaration lines |
| Declared dependencies in `get_indexing_status` | Rust, JS/TS, Python, Go | `manifest_language` in `crates/mct-index/src/manifests.rs` |

Dead code (`find_dead_code`, `mct-cli dead-code`) uses the same rules for
every language (`crates/mct-index/src/dead_code.rs`). The rules decide most
of the per-language results below:

- **What can be reported:** only `function`, `class`, `struct`, `interface`,
  `enum`, `trait` and `type_alias`. Methods are never reported, because
  interface implementations are called through dynamic dispatch.
- **Never reported:** fields, constants, variables, HTML elements, CSS
  rules and Markdown headings.
- **"Used" means** at least one indexed relation names the symbol,
  anywhere in the project, matched by name only.
- **Always kept:** a symbol named `main`, and anything that looks like a
  test, judged by its path (`tests/`, `*_test.*`, `*Test.*`, `*.spec.*`…)
  or its name (`test`, `test_*`).

Features that do not depend on the language, so they are not in the
table:

- `get_file_tree`, `get_project_overview`, `get_indexing_status`;
- `batch`, `discover_tool_categories`, `get_tool_schema`, `select_tools`;
- the semantic half of `hybrid_search` and `semantic_search`. These embed
  each indexed symbol (its signature, doc comment and callees, see
  `crates/mct-index/src/semantic.rs`) in every language. They need a build
  with the `semantic` feature, which was not tested here. Without it,
  `hybrid_search` falls back to lexical search and says so.

### Markup and style language features

| Feature | HTML | CSS | XML | XAML | Markdown |
|---|:-:|:-:|:-:|:-:|:-:|
| Find a definition | ✅ | ✅ | ✅ | ✅ | ✅ |
| `find_references` | ✅ | ✅ | — | ✅ | ✅ |
| Dead code | ❌ | ❌ | ❌ | ❌ | ❌ |
| Repeated code | ❌ | ❌ | ❌ | ❌ | ❌ |
| Import graph | ⚠️ | ⚠️ | — | ✅ | ⚠️ |
| File outline | ⚠️ | ⚠️ | ⚠️ | ⚠️ | ⚠️ |
| Exact text in strings | ❌ | ❌ | ❌ | ❌ | ✅ |

- **HTML, find a definition:** by `id`.
- **CSS, find a definition:** by selector (`.nav`, `#header`).
- **Markdown, find a definition:** notes and headings.
- **HTML, `find_references`:** query `#header`/`.nav`; the bare id `header` finds nothing.
- **CSS, `find_references`:** references come from HTML.
- **XML, `find_references`:** no relations.
- **XAML, `find_references`:** event handler names.
- **Markdown, `find_references`:** wiki links and anchors.
- **HTML, dead code:** elements are not candidates.
- **CSS, dead code:** an unused rule (`.orphan`) is never reported.
- **Markdown, dead code:** an unlinked note or heading is never reported.
- **HTML, import graph:** `<link>`/`<script src>` edges; the resolved path is also listed as external.
- **CSS, import graph:** the checked local `@import` resolves an edge, but its path also appears as an external import.
- **XML, import graph:** no imports.
- **XAML, import graph:** relative `ResourceDictionary Source` paths resolve to dictionary-file edges; absolute, assembly (`pack:`/`;component`) and computed URIs are skipped.
- **Markdown, import graph:** only `![[embeds]]` are edges; `[[links]]` are references, so linked notes are not in the graph.
- **HTML/XML/XAML/CSS, file outline:** declaration-line fallback with `# ...`; the placeholder is not syntax for these languages and one-line bodies remain visible.
- **Markdown, file outline:** top-level headings only.
- **Markdown, exact text in strings:** paragraph and pipe-table text are collected (subject to the shared prose filter).

## Checklist by language

`[x]` indexed or working, `[~]` partial, `[ ]` not indexed or not working,
`[?]` not tested. Each item says what kind it gets, or what is missing. Constructs the language
does not have are left out. The score summary above is the single source for
percentages and denominators; these checklists provide the scored evidence.

### Rust checklist (`rust`)

**Functions and methods**

- [x] Top-level function: `function`
- [x] Nested function (named, inside a function): `function` owned by the enclosing function
- [x] Lambda / closure assigned to a name: `let f = |x| …` is a `function` owned by the enclosing function; other assignment forms are not indexed
- [x] Method: `method`, parent is the impl type (generic arguments kept: `Circle<T>`)
- [x] Destructor / finalizer: `Drop::drop` as a `method`
- [x] Operator overload: `impl Add` → `add` method
- [x] Abstract / pure virtual method: trait method without body

**Types and object-oriented programming**

- [x] Struct: `struct`, unit structs too
- [x] Union: `union` is a `struct`, with its fields
- [x] Interface members: trait methods as `method`, default bodies too
- [x] Trait / mixin: `trait`
- [x] Enum: `enum`
- [x] Enum members: variants are `constant` symbols owned by the enum
- [x] Type alias: `type_alias`; associated types too, parented to the impl type
- [x] Module / namespace / package: file `module` plus `mod` blocks

**Fields, constants and variables**

- [x] Field / instance attribute: struct fields are `field` symbols; tuple fields are `0`, `1`
- [x] Constant (module or global): `const` → `constant`
- [x] Global / module-level variable: `static` and `static mut` → `variable`
- [x] Local variable: `variable` owned by its function (`let`, `for`, `if let`, `while let`, `match` arms)
- [x] Parameter: `variable` owned by its function (`let`, `for`, `if let`, `while let`, `match` arms)
- [x] Macro: `macro_rules!` is a `function`

**Relations**

- [x] Calls
- [x] Imports
- [~] Implements interface / trait: `impl Trait for T` → `implements`, but owned by the file module, not by `T`
- [x] Interface extends interface / supertrait: `trait B: A` → `extends`
- [x] Decorator / annotation / attribute: `#[derive(Clone)]` → `references` to `Clone`

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`, including macro invocations and imported functions called inside a macro; a same-file `f` inside a macro is a `references`
- [~] Every use: `find_references`: imported/qualified macro calls can have no use relation; same-file free functions get `references`
- [~] Blast radius and context: `impact_analysis`, `build_context_pack`: same gap as callers (macros)
- [x] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals collected and listed as `path:line`

**Features: Code quality**

- [x] Dead code: `find_dead_code`, `mct-cli dead-code`: functions and types; fields, macros and associated types of trait impls are not false positives
- [x] Repeated code: `mct-cli duplicates`: functions and methods

**Features: Structure**

- [x] Import graph: `get_dependency_graph`
- [x] File outline: `get_file_skeleton`
- [x] Declared dependencies in `get_indexing_status`: `Cargo.toml`

### Python checklist (`python`)

**Functions and methods**

- [x] Top-level function: `function`, `async def` too
- [~] Nested function (named, inside a function): `function` with no parent
- [ ] Lambda / closure assigned to a name: `f = lambda …` has no symbol
- [x] Method: `method`, parent is the class
- [x] Constructor: `__init__` as `method`
- [x] Destructor / finalizer: `__del__` as `method`
- [~] Property accessors (get/set): `@property` getter and `@x.setter` are two `method` rows with the same name
- [x] Operator overload: `__add__` as `method`
- [x] Abstract / pure virtual method: `@abstractmethod` method

**Types and object-oriented programming**

- [x] Class: `class`
- [x] Abstract class: `class` (abstract flag not stored)
- [~] Enum: an `Enum` subclass is a `class`, not an `enum`; assigned members have no symbol
- [ ] Enum members: assignments in an `Enum` subclass have no symbol
- [~] Record / data class: `@dataclass` is a `class`; its fields have no symbol
- [x] Nested / inner class: parent is the outer class
- [x] Type alias: PEP 695 `type X = …`
- [x] Module / namespace / package: file `module`

**Fields, constants and variables**

- [ ] Field / instance attribute: `self.x`, class attributes, `__slots__` and annotations have no symbol
- [ ] Constant (module or global): `UPPER = 1` is a plain assignment, no symbol
- [ ] Global / module-level variable: module assignments have no symbol
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls
- [x] Builtins: an unbound `open()`/`len()`/`Exception` is `external`
- [x] Constructor call (`new X()` / `X()`): `calls X`
- [x] Super / base constructor call: `super().__init__()` → `calls super` and `calls __init__`
- [x] Imports: module and each imported name
- [x] Class extends class: one `extends` per base
- [x] Decorator / annotation / attribute: `references` to the decorator

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [x] Every use: `find_references`
- [x] Blast radius and context: `impact_analysis`, `build_context_pack`
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [x] Dead code: `find_dead_code`, `mct-cli dead-code`: functions and classes
- [x] Repeated code: `mct-cli duplicates`: functions and methods

**Features: Structure**

- [x] Import graph: `get_dependency_graph`
- [~] File outline: `get_file_skeleton`: declaration-line fallback, as for Lua/Bash/PowerShell; a one-line body remains visible
- [x] Declared dependencies in `get_indexing_status`: `requirements.txt` only; not `pyproject.toml`

### JS/TS checklist (`javascript_typescript`)

**Functions and methods**

- [x] Top-level function: `function`, also `module.exports.x = function`
- [~] Nested function (named, inside a function): `function` with no parent
- [x] Lambda / closure assigned to a name: arrow and function expressions become `function`; a class-field arrow becomes `method`
- [x] Method: `method`
- [x] Constructor: `constructor` as `method`
- [~] Property accessors (get/set): `get x()` and `set x()` are two `method` rows with the same name
- [ ] Abstract / pure virtual method: `abstract m(): T;` has no symbol
- [~] Overloads kept apart: only the implementation signature is a row; overload signatures are dropped

**Types and object-oriented programming**

- [x] Class: `class`
- [x] Abstract class: `class`
- [x] Interface / protocol: `interface`
- [ ] Interface members: neither method signatures nor properties inside an `interface` have a symbol
- [~] Trait / mixin: a mixin factory becomes `function`; the returned class's methods have no parent
- [x] Enum: `enum`
- [ ] Enum members: members have no symbol
- [?] Nested / inner class: No nested-class form valid in JS/TS was independently verified
- [~] Anonymous class: class expression: methods have no parent
- [x] Type alias: `type_alias`
- [x] Module / namespace / package: file `module` plus `namespace`

**Fields, constants and variables**

- [x] Field / instance attribute: `field`, `#private` and `static` too
- [ ] Constructor-promoted field: `constructor(public x)` has no `field`
- [ ] Constant (module or global): `const X = 1` has no symbol (only function values do)
- [ ] Global / module-level variable: `let`/`var` with a non-function value has no symbol
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls
- [x] Standard globals: an unbound `Math.max()`/`JSON.parse()`/`parseInt()`/`new Map()` is `external`
- [x] Constructor call (`new X()` / `X()`): `calls X`
- [ ] Super / base constructor call: `super()` gives no relation
- [x] Imports
- [x] Class extends class
- [x] Implements interface / trait
- [x] Interface extends interface / supertrait: `extends`
- [ ] Decorator / annotation / attribute: `@Component` gives no relation

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [x] Every use: `find_references`
- [x] Blast radius and context: `impact_analysis`, `build_context_pack`
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [x] Dead code: `find_dead_code`, `mct-cli dead-code`: functions and classes
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [x] Import graph: `get_dependency_graph`
- [x] File outline: `get_file_skeleton`
- [x] Declared dependencies in `get_indexing_status`: `package.json`

### Java checklist (`java`)

**Functions and methods**

- [ ] Lambda / closure assigned to a name: no symbol
- [x] Method: `method`
- [x] Constructor: `method` named after the class; overloads are separate rows
- [x] Abstract / pure virtual method: `method`
- [x] Overloads kept apart: one row per overload, same name and parent

**Types and object-oriented programming**

- [x] Class: `class`
- [x] Abstract class: `class`
- [x] Interface / protocol: `interface`; `@interface` annotations too
- [x] Interface members: `method`, default and static methods too
- [x] Enum: `enum`
- [x] Enum members: `field`
- [x] Record / data class: `record` is a `class`; each component is its implicit accessor `method`
- [x] Nested / inner class: static and inner
- [~] Anonymous class: inside a method, its methods attach to the enclosing class; in a field initializer they have no symbol
- [~] Module / namespace / package: file `module`; `package` has no symbol

**Fields, constants and variables**

- [x] Field / instance attribute: `field`
- [~] Constant (module or global): `static final` is a `field`
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls: a method invocation only resolves to a `method`, never to a same-named field
- [x] Constructor call (`new X()` / `X()`): `calls X`
- [ ] Super / base constructor call: `super()` and `this()` give no relation
- [x] Imports: repeated target names can carry distinct file/type resolution evidence; repetition alone is not a parser defect
- [x] Class extends class
- [x] Implements interface / trait
- [ ] Interface extends interface / supertrait: `interface B extends A` gives no relation
- [ ] Decorator / annotation / attribute: `@Entity`, `@Override` give no relation

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [x] Every use: `find_references`
- [x] Blast radius and context: `impact_analysis`, `build_context_pack`
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [~] Dead code: `find_dead_code`, `mct-cli dead-code`: only classes, interfaces and enums: every method is skipped; the class holding `main` is reported
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [x] Import graph: `get_dependency_graph`
- [x] File outline: `get_file_skeleton`
- [ ] Declared dependencies in `get_indexing_status`

### Kotlin checklist (`kotlin`)

**Functions and methods**

- [x] Top-level function: `function`
- [~] Nested function (named, inside a function): `function` with no parent
- [ ] Lambda / closure assigned to a name: no symbol
- [x] Method: `method`; extension functions are also `method` with receiver type as parent
- [ ] Constructor: primary and secondary constructors have no symbol
- [x] Property accessors (get/set): one `field`, custom `get()`/`set()` included in its range
- [x] Operator overload: `operator fun plus` as `method`
- [x] Abstract / pure virtual method: `method`
- [x] Overloads kept apart: one row per overload

**Types and object-oriented programming**

- [x] Class: `class`
- [x] Abstract class: `class`
- [x] Interface / protocol: `interface`
- [x] Interface members: `method`, default bodies too
- [~] Enum: `enum class` is indexed as `class`
- [x] Enum members: `field`
- [x] Record / data class: `data class` is a `class`; `val`/`var` parameters are `field`
- [~] Singleton / companion object: `object` is a `class`; `companion object` has no symbol and its members get the outer class as parent
- [x] Nested / inner class: `inner`, sealed subclasses
- [~] Anonymous class: `object : A { … }` inside a function has no class symbol; its named functions become parentless `function` rows
- [ ] Type alias: `typealias` has no symbol
- [x] Module / namespace / package: file `module` plus one for the `package`

**Fields, constants and variables**

- [x] Field / instance attribute: `field`, `lateinit` too
- [x] Constructor-promoted field: `val`/`var` parameter → `field`
- [ ] Constant (module or global): top-level `const val` has no symbol; inside a companion it is a `field`
- [ ] Global / module-level variable: top-level `val`/`var` has no symbol
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls
- [x] Constructor call (`new X()` / `X()`): `calls X`
- [?] Super / base constructor call: Kotlin secondary-constructor delegation was not independently checked
- [~] Imports: aliases use the alias name; some imports emit duplicate target names and a false external graph entry
- [x] Class extends class: when the supertype has `()`
- [x] Implements interface / trait: supertype without `()`
- [~] Interface extends interface / supertrait: an `implements` relation is emitted, rather than `extends`
- [?] Decorator / annotation / attribute: Kotlin annotation extraction was not independently checked

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [x] Every use: `find_references`
- [x] Blast radius and context: `impact_analysis`, `build_context_pack`
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [x] Dead code: `find_dead_code`, `mct-cli dead-code`: top-level functions and classes; methods skipped
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [~] Import graph: `get_dependency_graph`: edges correct; an import emitted twice adds a false `external` entry
- [x] File outline: `get_file_skeleton`
- [ ] Declared dependencies in `get_indexing_status`

### C# checklist (`csharp`)

**Functions and methods**

- [~] Nested function (named, inside a function): local function as `function` with no parent
- [ ] Lambda / closure assigned to a name: no symbol
- [x] Method: `method`
- [x] Constructor: `method` named after the class, static constructor too
- [x] Destructor / finalizer: `~Name` as `method`
- [x] Property accessors (get/set): one `field` (auto and custom accessors, `init`, expression-bodied)
- [x] Operator overload: `method` named `operator +`
- [x] Abstract / pure virtual method: `method`
- [x] Overloads kept apart: one row per overload

**Types and object-oriented programming**

- [x] Class: `class`
- [x] Abstract class: `class`
- [x] Struct: `struct`
- [x] Interface / protocol: `interface`
- [x] Interface members: methods as `method`, properties as `field`
- [x] Enum: `enum`
- [x] Enum members: `constant`
- [~] Record / data class: `record` is a `class`; positional parameters have no symbol, body properties do
- [x] Nested / inner class: parent is the outer class
- [x] Partial class (several declarations): one `class` row per declaration, members keep the class as parent
- [~] Type alias: `delegate` becomes `type_alias`; `using Alias = System.String` emits imports, not an alias symbol
- [x] Module / namespace / package: file `module` plus block and file-scoped `namespace`; types get `Ns::` in their parent

**Fields, constants and variables**

- [x] Field / instance attribute: fields, properties, events and indexers all become `field`
- [ ] Constructor-promoted field: record positional parameters have no symbol
- [~] Constant (module or global): `const` is a `field`
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls
- [x] Constructor call (`new X()` / `X()`): `calls X`
- [ ] Super / base constructor call: `: base()` gives no call to the base constructor; calls inside its arguments can still be traversed
- [x] Imports: a plain `using System;` emits repeated target names; alias imports can emit distinct path and alias names
- [~] Class extends class: heuristic: the first base is `extends` unless it is named like `IFoo`; the rest are `implements`
- [~] Implements interface / trait: heuristic, as above
- [x] Interface extends interface / supertrait: `extends`
- [x] Decorator / annotation / attribute: `references XAttribute`

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [~] Every use: `find_references`: a static class used only as a qualifier (`Helpers.HelperFn()`) gets no reference
- [~] Blast radius and context: `impact_analysis`, `build_context_pack`: same gaps as references
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [~] Dead code: `find_dead_code`, `mct-cli dead-code`: types and local `function` rows are candidates; methods are skipped. `Program` and a static class used only as `Helpers.X()` can be reported
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [x] Import graph: `get_dependency_graph`
- [x] File outline: `get_file_skeleton`
- [ ] Declared dependencies in `get_indexing_status`

### C++ checklist (`cpp`)

**Functions and methods**

- [x] Top-level function: `function`, templates as plain functions
- [ ] Lambda / closure assigned to a name: no symbol
- [x] Method: `method`, out-of-line `A::m` gets parent `A`
- [x] Constructor: `method` named after the class
- [x] Destructor / finalizer: `~Name` as `method`, virtual and defaulted too
- [x] Operator overload: `method` named `operator+`, `friend` operators too
- [x] Abstract / pure virtual method: `= 0` method
- [x] Overloads kept apart: one row per overload

**Types and object-oriented programming**

- [x] Class: `class`
- [x] Abstract class: `class`
- [x] Struct: `struct`
- [~] Union: indexed as `struct`
- [x] Enum: `enum` and `enum class`
- [x] Enum members: `constant`
- [x] Nested / inner class: nested `struct`/`class`
- [x] Type alias: `using` and `typedef`
- [x] Module / namespace / package: file `module` plus `namespace`

**Fields, constants and variables**

- [x] Field / instance attribute: `field`, union members too
- [~] Constant (module or global): `const int X` → `variable`; `static const` member → `field`
- [x] Global / module-level variable: `variable`, in namespaces too
- [ ] Local variable
- [ ] Parameter
- [ ] Macro: `#define` has no symbol

**Relations**

- [x] Calls
- [~] Constructor call (`new X()` / `X()`): `X()` is a call; `new X()` gives no call relation
- [ ] Super / base constructor call: `B() : A(1) {}` gives no constructor-call relation to `A`
- [x] Imports: quoted and angle-bracket `#include`
- [x] Class extends class: every base, any access, as `extends`

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [~] Every use: `find_references`: a class used only in a declaration (`Widget w;`) gets no reference
- [~] Blast radius and context: `impact_analysis`, `build_context_pack`: same gaps as references
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [~] Dead code: `find_dead_code`, `mct-cli dead-code`: a class used only in a declaration (`Widget w;`) is reported
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [~] Import graph: `get_dependency_graph`: edges correct; the resolved `#include "util.h"` is also listed as external
- [x] File outline: `get_file_skeleton`
- [ ] Declared dependencies in `get_indexing_status`

### Go checklist (`go`)

**Functions and methods**

- [x] Top-level function: `function`, parent is the package
- [ ] Lambda / closure assigned to a name: `f := func() {}` has no symbol
- [x] Method: receiver methods, pointer and value receivers share the parent

**Types and object-oriented programming**

- [x] Struct: `struct`
- [x] Interface / protocol: `interface`
- [x] Interface members: `method` with the interface as parent
- [x] Type alias: both `type A = B` and `type A int`
- [~] Module / namespace / package: file `module` plus a package `module`; both rows share the name when the file is named after the package

**Fields, constants and variables**

- [ ] Field / instance attribute: struct fields have no symbol
- [ ] Constant (module or global): `const` has no symbol
- [ ] Global / module-level variable: `var` has no symbol
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls
- [x] Imports
- [ ] Implements interface / trait: structural, deliberately not inferred
- [ ] Interface extends interface / supertrait: embedded interfaces give no relation
- [ ] Struct embedding: embedded fields give no relation

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [~] Every use: `find_references`: a struct used only in a composite literal (`util.Widget{}`) gets no reference
- [~] Blast radius and context: `impact_analysis`, `build_context_pack`: same gaps as references
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [~] Dead code: `find_dead_code`, `mct-cli dead-code`: a struct used only in a composite literal is reported
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [~] Import graph: `get_dependency_graph`: edges correct; the module import path is also listed as external
- [x] File outline: `get_file_skeleton`
- [x] Declared dependencies in `get_indexing_status`: `go.mod`

### PHP checklist (`php`)

**Functions and methods**

- [x] Top-level function: `function`
- [~] Nested function (named, inside a function): emitted as `function`, with no parent
- [x] Lambda / closure assigned to a name: `$f = function …` / `fn` becomes `function` (no parent inside a method)
- [x] Method: `method`
- [x] Constructor: `__construct` as `method`
- [x] Destructor / finalizer: `__destruct` as `method`
- [x] Abstract / pure virtual method: `method`

**Types and object-oriented programming**

- [x] Class: `class`
- [x] Abstract class: `class`
- [x] Interface / protocol: `interface`
- [x] Interface members: `method`
- [x] Trait / mixin: `trait`, its properties and methods too
- [x] Enum: `enum`
- [x] Enum members: `case` as `field`
- [~] Anonymous class: `new class {}` gives no class or member symbols, except an anonymous Laravel `Migration` (`up`/`down` as methods of the file module)
- [~] Module / namespace / package: file `module`; `namespace` is deliberately not a symbol

**Fields, constants and variables**

- [x] Field / instance attribute: `field`, `readonly` and trait properties too
- [x] Constructor-promoted field: promoted parameter → `field`
- [x] Constant (module or global): `const` top-level and in classes → `constant`; `define()` is only a `calls` relation
- [ ] Global / module-level variable: `$x = …` has no symbol
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls
- [x] Constructor call (`new X()` / `X()`): `calls Foo`; `new static` gives nothing
- [x] Super / base constructor call: `parent::__construct()` → `calls __construct`
- [x] Imports: checked type `use` declarations emit repeated target names with distinct file/type evidence; `use function` / `use const` do not add a type-file import
- [x] Class extends class
- [x] Implements interface / trait: `use Trait;` also → `implements`
- [x] Interface extends interface / supertrait: `extends`
- [x] Decorator / annotation / attribute: `#[Entity]` → `references`

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [x] Every use: `find_references`
- [x] Blast radius and context: `impact_analysis`, `build_context_pack`
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [x] Dead code: `find_dead_code`, `mct-cli dead-code`: functions and classes
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [~] Import graph: `get_dependency_graph`: edges correct; the resolved `require_once` path is also listed as external
- [x] File outline: `get_file_skeleton`
- [ ] Declared dependencies in `get_indexing_status`

### Lua checklist (`lua`)

**Functions and methods**

- [x] Top-level function: `function` and `local function`
- [~] Nested function (named, inside a function): `function` with no parent
- [x] Lambda / closure assigned to a name: `local f = function() end` becomes `function`
- [x] Method: `M.f` and `M:f` become `method` of `M`
- [?] Operator overload: No Lua metamethod/operator fixture was checked

**Types and object-oriented programming**

- [~] Class: only a table literal `local M = {}` becomes `module`; `setmetatable(...)` classes have no symbol
- [x] Module / namespace / package: file `module`; table literals too

**Fields, constants and variables**

- [ ] Field / instance attribute: `self.x = …` has no symbol
- [ ] Global / module-level variable: globals and top-level `local` have no symbol
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls
- [x] Imports: `require`
- [ ] Class extends class: `setmetatable`/`__index` gives no relation

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [~] Every use: `find_references`: a table-based class (`M.Widget`) is not a symbol, so it has no references
- [~] Blast radius and context: `impact_analysis`, `build_context_pack`: same gaps as references
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [~] Dead code: `find_dead_code`, `mct-cli dead-code`: free global/local functions are candidates; table/module methods (`function M.f`) are skipped
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [x] Import graph: `get_dependency_graph`
- [~] File outline: `get_file_skeleton`: declaration line only; a one-line function is shown whole, then a placeholder
- [ ] Declared dependencies in `get_indexing_status`

### Bash checklist (`bash`)

**Functions and methods**

- [x] Top-level function: both `f() {}` and `function f {}`
- [x] Nested function (named, inside a function): `function` with the outer function as parent

**Types and object-oriented programming**

- [x] Module / namespace / package: file `module`

**Fields, constants and variables**

- [~] Constant (module or global): `readonly X=…` → `variable`
- [x] Global / module-level variable: `variable`, `export`/`readonly`/`declare` too
- [ ] Local variable: `local x` is deliberately not indexed

**Relations**

- [x] Calls: every literal command, local or external
- [x] Imports: `source`/`.`; path and stem

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [x] Every use: `find_references`
- [x] Blast radius and context: `impact_analysis`, `build_context_pack`
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [x] Dead code: `find_dead_code`, `mct-cli dead-code`: functions
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [~] Import graph: `get_dependency_graph`: edges correct; the resolved `source` path is also listed as external
- [~] File outline: `get_file_skeleton`: declaration line only; a one-line function is shown whole, then a placeholder
- [ ] Declared dependencies in `get_indexing_status`

### PowerShell checklist (`powershell`)

**Functions and methods**

- [x] Top-level function: `function`
- [x] Nested function (named, inside a function): `function` with the outer function as parent
- [~] Lambda / closure assigned to a name: a top-level `$f = { … }` is indexed as `variable`, not `function`
- [ ] Method: classes are not supported
- [ ] Constructor: classes are not supported
- [ ] Property accessors (get/set): classes are not supported

**Types and object-oriented programming**

- [ ] Class: `class` has no symbol
- [ ] Enum: `enum` has no symbol
- [ ] Enum members: no symbol
- [x] Module / namespace / package: file `module`

**Fields, constants and variables**

- [ ] Field / instance attribute: class properties have no symbol
- [x] Global / module-level variable: `variable`; scope prefixes (`$script:`) stripped
- [ ] Local variable
- [ ] Parameter

**Relations**

- [x] Calls: every literal command or cmdlet
- [x] Imports: dot-sourcing and `Import-Module`
- [ ] Class extends class: classes are not supported

**Features: Search and lookup**

- [x] Find a definition: `find_symbol`, `list_symbols`, `search_symbols`
- [x] Who calls / what is called: `find_callers`, `find_calls`
- [x] Every use: `find_references`
- [x] Blast radius and context: `impact_analysis`, `build_context_pack`
- [ ] Exact text in strings: `hybrid_search` with a `"quoted"` query: string literals are not collected; only symbol names match

**Features: Code quality**

- [x] Dead code: `find_dead_code`, `mct-cli dead-code`: functions
- [ ] Repeated code: `mct-cli duplicates`: no token source: listed under `unsupported_languages`

**Features: Structure**

- [~] Import graph: `get_dependency_graph`: edges correct; the resolved dot-sourced path is also listed as external
- [~] File outline: `get_file_skeleton`: declaration line only; a one-line function is shown whole, then a placeholder
- [ ] Declared dependencies in `get_indexing_status`

## Findings from this check

Each item is something the probes showed that no other record documented,
or that another record now gets wrong. Nothing was changed in code or in
the other documents.

**Observed parser and tool behavior (omissions, decisions and bugs):**

1. **PowerShell:** inside a `class` method or constructor, `$this.Radius = $r`
   is indexed as a top-level `variable` named `this`, once per assignment.
   The class has no symbol; these spurious variables are incorrect output.
   Other recursively visited expressions can still emit relations.
2. **Java:** `interface B extends A` gives no relation, while
   `class X implements A` and `class X extends Y` do. Kotlin, C#, PHP and
   TypeScript all record interface inheritance.
3. **TypeScript:** an `abstract m(): T;` method and every member of an
   `interface` have no symbol. Java, Kotlin, C#, Go and PHP index interface
   members.
4. **Rust:** `impl Trait for T` records `implements Trait` from the file
   module, not from `T`. Asking what implements the trait gives the file,
   not the type.
5. **Java and PHP:** the checked `import`/`use` lines emit duplicate target names (PHP no longer does: one `imports` per `use` clause).
   They are not necessarily identical relations: their `RelationTarget` evidence
   can distinguish file imports from symbol imports. In the Kotlin fixture
   an additional import stays unresolved and
   `get_dependency_graph` lists it as a false external import.
6. **Rust:** a call inside a macro's arguments, such as `println!("{}", f(x))`
   or `vec![f()]`, gives no relation when `f` is imported. When `f` is in the
   same file, it gives `references`, not `calls`. `find_callers` misses
   these calls. The imported helper is **not** reported as dead code, because
   the import counts as a use. A qualified `util::helper_fn()` inside a macro,
   with only `mod util;` and no helper import, **is** a false positive.
7. **C++ and Go:** using a type only to declare a variable (`Widget w;`) or
   in a composite literal (`util.Widget{}`) gives no relation. The type is
   reported as dead code even though it is used.
8. **C#:** a static class used only as a qualifier (`Helpers.HelperFn()`)
   gives no relation to the class. The class is reported as dead code.
9. **Dead code in Java, C# and Lua:** methods are never candidates. Java has no free functions, so its candidates are types.
   C# also has local functions indexed as `function`, which are candidates.
   Lua table/module functions (`function M.f`) are methods and are skipped;
   unused free global/local functions are reported. The class holding `main` (`Main`, `Program`) is reported,
   because only the name `main` is an entry point.
10. **`get_dependency_graph`:** in C++, Go, PHP, Bash, PowerShell and HTML,
    the checked local file imports can produce a path and a stem relation. One
    resolves, and the other is listed as an external import, so the
    resolved file also appears under "external imports". The edges are
    correct.
11. **Markdown:** `[[links]]` are `references`, not `imports`, so the
    dependency graph shows only `![[embeds]]`. Linked notes are not edges.

**Valid code the grammar rejects:**

1. **Kotlin** (`tree-sitter-kotlin-ng`): these one-line member forms
   without an explicit separator are syntax errors in the grammar:
   `class A { fun f() {} }`, `class A { val x = 1 }`, `object O { fun f() {} }`.
   The [official Kotlin grammar](https://kotlinlang.org/grammar/#classMemberDeclarations) permits an optional separator after a class member.
   The parser works around it. When a parse has an error, it puts a `;` in the space or tab before each closing `}` token that ends a member on its line, and reparses incrementally. The offsets stay the same, and a template's `${ … }` is never touched. It keeps the result only if it is clean. Error recovery can hide the rest of the file after the first missing separator, so it repeats this up to 16 times. A file with a real syntax error is still rejected. Before the workaround such a file contributed no symbols.
   Tests: `crates/mct-lang-kotlin/tests/one_line_bodies.rs`.
   A body with no blank before `}` (`class A {fun f()=1}`) is still rejected, because the patch has no byte to use.
2. **PowerShell** (`tree-sitter-powershell`): a constructor that chains to
   the base class, `B([int]$x) : base($x) {}`, is a syntax error in the
   tree-sitter grammar. This is valid [PowerShell constructor syntax](https://learn.microsoft.com/en-us/powershell/module/microsoft.powershell.core/about/about_classes_constructors#constructors-for-derived-classes).

**Records that are now out of date** (reported, not edited):

1. `internal/corpus-progress.md`, C/C++ row: says namespace functions are
   indexed as methods and locals (`auto x = f()`, `T x(args)`) as symbols.
   Neither happens now: namespace functions are `function` with the
   namespace as parent, and locals have no symbol.
2. `internal/corpus-progress.md`, C# row: says file-scoped namespaces,
   `record`, `enum`, `delegate`, events, operators, indexers, destructors
   and local functions have no symbol, and attributes no relation. All of
   them are indexed now.
3. `internal/corpus-progress.md`, PHP row: said `new Foo()` and `#[…]`
   attributes have no relation. They give `calls Foo` and `references`; the row
   was corrected with the namespace-aware PHP extraction.
4. `internal/checklist.md`, Kotlin row: says no integration test or fuzz
   harness was added. [`lang-crate-base.md`](../../06-templates/lang-crate-base.md)
   says every crate has a fuzz harness now. The status note at the top of
   that file already warns that rows are historical; this is history, not a
   claim that the current harness is absent.

Additional checks found that C++ `new X()` and base-constructor initializers
emit no constructor-call relation; C# base initializers also emit no call to
the base constructor (calls in their arguments are still visited). These are
missing extraction, not incorrect output. Kotlin extension functions are
`method`, not `function`. Python `Enum` subclasses are `class` rows with no
member symbols. None of these observations changes parser code.

## Re-checking a cell

`mct-cli probe <file>` reports only counts. To see what a parser emits for a
construct:

1. Write a small file with the construct in an empty folder outside the
   repository, for example `/tmp/probe/sample.kt`.
2. Confirm it parses: `mct-cli probe /tmp/probe/sample.kt`. A failure
   alone does not establish a grammar bug: first establish that the input
   is valid and inspect the diagnostic (syntax, budget, grammar loading, etc.).
   Record a confirmed grammar limitation separately from missing extraction.
3. Index the folder: `mct-cli --root /tmp/probe init`.
4. List the symbols, with kind and lines:
   `mct-cli call --root /tmp/probe --tool list_symbols --args '{"path":"sample.kt","format":"toon"}'`
5. Check the parent of one symbol (duplicate names can yield several hits):
   `mct-cli call --root /tmp/probe --tool find_symbol --args '{"name":"area","format":"toon"}'`
6. Check relations: `find_calls`/`find_callers` take `function`, while
   `find_references`/`impact_analysis` take `symbol`. A graph query alone
   cannot distinguish a missing parser relation from unresolved evidence;
   inspect the walker or a disposable registry-level parser probe as needed.

To check a feature, use a folder with at least two files: one that defines
the symbols and one that imports and uses them. Run the tool on it:

- `mct-cli call --root /tmp/probe --tool find_dead_code --args '{"format":"toon"}'`
- `mct-cli call --root /tmp/probe --tool get_dependency_graph --args '{"module":"main.kt","format":"toon"}'`
- `mct-cli call --root /tmp/probe --tool hybrid_search --args '{"query":"\"some exact text\"","format":"toon"}'`
- `mct-cli --root /tmp/probe duplicates --no-excerpts`. The default
  thresholds are 6 lines and 40 tokens; a short copy is not a group. Lower
  them with `--min-lines` and `--min-tokens` to test small examples.

For a lasting check, add an assertion to the crate's `tests/parse.rs` or
`tests/corpus.rs` instead. A ❌ that is a known, accepted limit should be
an assertion labeled `Known limit:`, as the
[library module template](../../06-templates/library-module-template.md) asks.

The [PHP parser note](mct-lang-php.md) documents its security exception.

## Keeping this page current

- **New language:** add implementation/verification inventory rows, syntax
  and gap tables, and a checklist for a programming language; extend the markup
  tables for a markup/style language. Omit inapplicable checklist constructs.
- **Parser change:** update affected syntax/gap tables and checklist items,
  then recalculate overall and area scores in the same PR.
- **Implementation change:** update the inventory rows and affected syntax
  and gap tables. A gap that is fixed leaves
  the table; a new decision moves a gap to "By design" with its source.
- **Tool change:** a tool that gains or loses a language (a new token
  source for `duplicates`, literals collected by another parser, a language
  added to `BRACE_LANGUAGES`, a new manifest format) updates the feature
  support table, affected checklists or markup rows, and feature/area scores.
- **Model change** (a new kind, relation or column): update
  [What the index can hold](#what-the-index-can-hold), then the cells that
  now become possible.
