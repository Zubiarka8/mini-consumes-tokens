# Parser features

> **Verification status:** checked on 2026-11-04 with `mct-cli` (debug build, commit `bb5eb03`) against
> probe fixtures. `[?]` means the feature was not exercised in that check; it does not mean the feature is
> missing. The earlier record is the 2026-10-19 probe at commit `71ec6d0`.

This is the shared list of what a parser or feature tool can provide. Every item is marked for every
language in two views:

- [checklists/by-feature.md](checklists/by-feature.md): primary view, one block per feature;
- [checklists/by-language.md](checklists/by-language.md): one checklist per language, generated from the same table.

Adding a feature means adding one row here and one status per language in the generator.

A feature is **done** for a language when the index holds the symbol or relation with the expected kind,
name and parent, and the tool can use it. Partial means it works only in some forms. Not verified means no
probe or test has confirmed it yet.

## Code languages

### Functions and methods

| ID | Feature | Done when |
|---|---|---|
| FN01 | Top-level function | Free function at file or module top level, indexed as `function`. |
| FN02 | Nested function | Named function inside another function, indexed as `function` with the outer function as parent. |
| FN03 | Lambda or closure assigned to a name | `f = <lambda>` or `f := func…` gives a symbol, not only a `calls` relation. |
| FN04 | Method | Function declared in a class, struct, impl or table, indexed as `method` with its owner as parent. |
| FN05 | Constructor | Constructor indexed as `method`, named after the class or as a special name. |
| FN06 | Destructor / finalizer | Destructor or finalizer indexed as `method`. |
| FN07 | Operator overload | Operator implementation indexed as `method`. |
| FN08 | Abstract / pure virtual method | Method without a body (abstract, pure virtual, interface) indexed as `method`. |
| FN09 | Property accessors (get/set) | A getter and setter give one symbol, or two rows that are documented as the same property. |
| FN10 | Overloads kept apart | Each overload is its own row with the same name and parent. |

### Types and object-oriented programming

| ID | Feature | Done when |
|---|---|---|
| TY01 | Class | Class indexed as `class`. |
| TY02 | Abstract class | Abstract class indexed as `class` (the abstract flag is not stored). |
| TY03 | Struct | Struct indexed as `struct`. |
| TY04 | Union | Union indexed as its own kind or as `struct`, with its members. |
| TY05 | Interface / protocol | Interface indexed as `interface`. |
| TY06 | Interface members | Methods and properties inside an interface indexed as `method` or `field`. |
| TY07 | Trait / mixin | Trait or mixin indexed as `trait`, or as the closest kind, with its members. |
| TY08 | Enum | Enum indexed as `enum`. |
| TY09 | Enum members | Enum values indexed as `field` or `constant`. |
| TY10 | Record / data class | Record or data class indexed as `class`, with its components as `field`. |
| TY11 | Nested / inner class | Class declared inside another, with the outer class as parent. |
| TY12 | Anonymous class | Anonymous class indexed with its members attached to it. |
| TY13 | Partial class | Each declaration of a partial class gives a row; members keep the class as parent. |
| TY14 | Singleton / companion object | Singleton or companion object indexed, with its members under it. |
| TY15 | Type alias | Alias indexed as `type_alias`. |
| TY16 | Module / namespace / package | File-level `module` row, plus namespace or package blocks where the language has them. |

### Fields, constants and variables

| ID | Feature | Done when |
|---|---|---|
| DA01 | Field / instance attribute | Instance or class field indexed as `field`. |
| DA02 | Constructor-promoted field | Constructor parameter that becomes a field (`val`/`var`, `public x`) indexed as `field`. |
| DA03 | Constant (module or global) | Constant indexed as `constant`. |
| DA04 | Global / module-level variable | Module-level or global variable indexed as `variable`. |
| DA05 | Local variable | Variable inside a function indexed as a symbol. |
| DA06 | Parameter | Function parameter indexed as a symbol. |
| DA07 | Macro / preprocessor define | `#define` or `macro_rules!` indexed as a symbol. |

### Relations

| ID | Feature | Done when |
|---|---|---|
| RE01 | Calls | Call from one symbol to another, as a `calls` relation. |
| RE02 | Constructor call | `new X()` or `X()` gives `calls X`. |
| RE03 | Super / base constructor call | `super()`, `parent::__construct()` or a base initializer gives a call to the base constructor. |
| RE04 | Imports | Import, include or require gives an `imports` relation for the module and each name. |
| RE05 | Class extends class | Base class gives `extends`. |
| RE06 | Implements interface / trait | Interface or trait implementation gives `implements`, owned by the implementing type. |
| RE07 | Interface extends interface / supertrait | Interface inheritance gives `extends`. |
| RE08 | Decorator / annotation / attribute | Decorator or attribute gives a `references` relation. |
| RE09 | Struct / type embedding | Embedded field or type gives a relation. |

### Search and lookup

| ID | Feature | Done when |
|---|---|---|
| SE01 | Find a definition | `find_symbol`, `list_symbols` and `search_symbols` return the symbol. |
| SE02 | Callers and callees | `find_callers` and `find_calls` return the relation, including inside macros. |
| SE03 | Every use | `find_references` returns each use of the symbol. |
| SE04 | Blast radius and context | `impact_analysis` and `build_context_pack` include callers, references and tests. |
| SE05 | Exact text in strings | `hybrid_search` with a `"quoted"` query finds literal text. |

### Code quality

| ID | Feature | Done when |
|---|---|---|
| QU01 | Dead code | `find_dead_code` reports unused symbols of the kinds it checks, without false positives. |
| QU02 | Repeated code | `mct-cli duplicates` covers the language (it has a token source). |

### Structure

| ID | Feature | Done when |
|---|---|---|
| ST01 | Import graph | `get_dependency_graph` shows local import edges without false external entries. |
| ST02 | File outline | `get_file_skeleton` shows the file's declarations with bodies collapsed. |
| ST03 | Declared dependencies | `get_indexing_status` reads the language's manifest. |

## Markup and style languages

HTML, CSS, XML, XAML and Markdown do not model functions, classes or variables, so they use this set.

| ID | Feature | Done when |
|---|---|---|
| MK01 | Find a definition | Element, selector, heading or note is found by its name. |
| MK02 | Find references | `find_references` returns the links to the element. |
| MK03 | Dead code | `find_dead_code` reports unused elements. Markup is never reported. |
| MK04 | Repeated code | `mct-cli duplicates` covers the language. |
| MK05 | Import graph | `get_dependency_graph` shows local file edges. |
| MK06 | File outline | `get_file_skeleton` shows the structure of the file. |
| MK07 | Exact text in strings | `hybrid_search` with a `"quoted"` query finds literal text. |

## Libraries and packages

Checked per framework and per manifest in [checklists/by-library.md](checklists/by-library.md).
The library features are:

- LB01 Declared dependency: The manifest is read and the package is stored in `dependencies` with its version as written.
- LB02 Framework structure: Components, classes, decorators, routes or view functions are indexed as symbols.
- LB03 Framework calls and hooks: Hook calls, framework calls and framework imports become relations.
- LB04 Template and markup links: Template names, JSX tags or class names are connected to the symbols or files they name.
- LB05 Stylesheet selectors: Selectors and their class, id and tag components are indexed.

## Shared tool features

These do not depend on the language, so they are not in the checklists:

- `get_file_tree`, `get_project_overview`, `get_indexing_status`;
- `batch`, `discover_tool_categories`, `get_tool_schema`, `select_tools`;
- the semantic half of `hybrid_search` and `semantic_search`, which needs the `semantic` build feature.

## Not in this structure

- Index model limits (visibility, modifiers, signatures, variable types) are in
  [symbol-coverage.md](symbol-coverage.md#what-the-index-can-hold). They are not features.
