# Issue #74 — Long-fixture corpus progress by language

Tracking for [#74](https://github.com/Zubiarka8/mini-consumes-tokens/issues/74):
each `mct-lang-*` crate needs a `tests/corpus/` containing interconnected
files (at least five of 300–700 lines, none longer), an `expected.snap`, one
large file in `malformed/` that the parser rejects, and `tests/corpus.rs` (the
shared `mct-corpus` harness plus language-specific tests). Conventions and the
review checklist: `CONTRIBUTING.md`, "Language corpus".

**Keep this file current until the issue is closed.** Every corpus PR updates
its language row in the same PR (status, PR number, counts, and bugs); after
merge, set its status to **Done**. When no rows remain pending, close #74 and
remove the corresponding rule from `rules.md`.

The file/line and symbol/relation columns, along with the summary, are filled
by `scripts/unix/corpus-report.sh <language> --update-progress` (use
`scripts\\windows\\corpus-report.ps1` on Windows). Set the status, PR number,
and bugs manually.

Statuses: **Pending** → **In PR** → **Done** (merged into `main`). The number in
parentheses counts relations whose target *name* is defined in another corpus
file; it is a name match, not a resolved cross-file reference.

| Language | Crate | Status | PR | Files / lines | Symbols / relations (name-matched across files) | Parser bugs found |
|---|---|---|---|---|---|---|
| Rust | `mct-lang-rust` | **Done** | #77 | 6 / 2,179 | 239 / 817 (218) | Associated type/const owner; nested `fn` inside a method treated as a method; module end +1 |
| Python | `mct-lang-python` | **Done** | #78 | 6 / 2,006 | 302 / 918 (223) | Module end +1; nested class missing parent; calls in decorator arguments missed; `type X = …` (PEP 695) has no symbol |
| Bash | `mct-lang-bash` | **Done** | #80 | 5 / 1,539 | 176 / 585 (162) | Module end +1; assignment prefix (`LC_ALL=C cmd`) treated as a variable; `coproc NAME { … }` parsed incorrectly by the grammar → #79 (test is `#[ignore]`) |
| C/C++ | `mct-lang-cpp` | **Done** | #83 | 5 / 1,568 | 423 / 493 (150) | Module end +1; namespace functions/variables treated as methods/fields; locals (`auto x = f()`, `T x(args)`) treated as symbols and their calls missed; out-of-line `ns::Class::m`/`Tmpl<T>::m` gets a different parent from its declaration; qualified name in `class Outer::Inner {`; anonymous namespace treated as an unnamed module; `enum`/`union`/`using`/`typedef` have no symbol; valid constructs rejected by the grammar → #82 |
| C# | `mct-lang-csharp` | **Done** | #85 | 6 / 2,048 | 454 / 588 (143) | Module end +1; no symbols for file-scoped `namespace X;`, `record`, `enum`, `delegate`, events, operators, indexers, destructors, and local functions; generic bases with `<…>` in the target and first `IFoo` treated as `extends`; calls in `?.`, `F<T>()`, `new T()`, `: base(…)`, expression-bodied properties, and initializers missed; `nameof` treated as a call; attributes have no relation |
| CSS | `mct-lang-css` | **Done** | #86 | 6 / 2,194 | 617 / 12 (0) | Module end +1; a rule ended on its selector line instead of its `}`; `@namespace` prefix (`svg\\|text`) treated as an `svg` element; atoms missed in `.a .b[attr]` (the grammar applies `[attr]` to the whole chain). Rejected by tree-sitter-css 0.25 (outside the corpus, #87): `@page :first`, `@import … layer()`/`supports()`, media-query ranges (`400px <= width`) |
| Go | `mct-lang-go` | In PR | #103 | 7 / 3.002 | 291 / 832 (254) | Found and fixed (none pending): module end +1; `type A = B` (alias) had no symbol; methods on a generic receiver (`func (s *Stack[T]) Push`) got the parent `Stack[T]`; calls with explicit type arguments (`Map[A, B](x)`, and `Fail[T](x)`, which the grammar reads as a conversion) were missed. Known limits, kept as tests: no symbols for package `const`/`var` or struct fields; no relation for embedding or structural interface satisfaction; builtins and conversions (`len`, `string(x)`) appear as calls |
| HTML | `mct-lang-html` | In PR | #105 | 5 / 1,563 | 302 / 675 (0) | Fixed in the PR: module end +1; an element spanned its start tag only, so a `<script src>` inside `<body id>` was an import from a symbol whose range did not contain it. Known limits, kept as tests: `class` tokens are referenced only on elements with an `id`; `rel="alternate stylesheet"`, preload/icon links, anchors and images are not imports |
| Java | `mct-lang-java` | In PR | #106 | 6 / 1,989 | 377 / 799 (155) | Fixed in the PR: module end +1; enum constant bodies skipped; type arguments in `extends`/`implements` became supertypes (`Comparable<Money>` → implements `Money`); `record` and `@interface` had no symbol; `new T(…)` left no relation. Open, not filed (`#[ignore]`): `this(…)`/`super(…)` and method references (`Money::plus`) leave no relation. Known limits: anonymous-class methods and enum constant bodies attach to the enclosing type; `permits` is no relation |
| JavaScript/TypeScript | `mct-lang-js-ts` | In PR | #107 | 7 / 2,193 | 274 / 671 (125) | Fixed in the PR: module end +1; object-literal methods/getters were methods without a parent (now functions); a function nested in a method inherited the class as parent; TS `enum`/`namespace` had no symbol. Adds a plain-JavaScript `.jsx` file. Known limits: interface members are not symbols; a namespace's functions stay top-level; JSX elements are not calls of their component |
| Kotlin | `mct-lang-kotlin` | In PR | #108 | 5 / 1,527 | 483 / 720 (89) | Fixed in the PR: module end +1; `enum class` bodies skipped; local `val`s in methods became fields; top-level and local `fun`s were methods (top-level ones without a parent); an extension's parent kept its type arguments; the `package` module spanned the header line only. Open, not filed (`#[ignore]`): companion objects and `typealias` have no symbol. Known limits: infix/operator calls leave no relation; the functions of a local class or `object : T { … }` expression are top-level functions |
| Lua | `mct-lang-lua` | In PR | #109 | 5 / 1,525 | 163 / 363 (43) | Fixed in the PR: module end +1; functions in a table constructor (`{ f = function() … end }`) and their calls lost; calls inside the called expression (`m.f(x):g()`) lost; every `x = { … }` was a module, locals included, and a non-name target kept its full text; `x = function … end` spanned its target line only |
| Markdown | `mct-lang-md` | In PR | #110 | 6 / 1,825 | 260 / 338 (229) | None found (every heuristic clean). Known limits, kept as tests: links in table cells and code are not scanned; all-digit tags and URL fragments are not tags. Markdown has no syntax errors: once the corpus guards land, its corpus needs the `malformed_may_parse` opt-out |
| PHP | `mct-lang-php` | **Done** | #88 | 6 / 2,463 | 418 / 803 (339) | Module end +1; `?->` (null-safe) calls missed; `new Foo()` has no relation; `#[…]` attributes have no relation; anonymous-class members (`new class { … }`) indexed as methods/fields without a parent; closure assigned to a variable inside a method treated as a class member; `new` in the default value of a promoted parameter missed |
| PowerShell | `mct-lang-powershell` | In PR | #111 | 5 / 1,515 | 87 / 261 (62) | Fixed in the PR: module end +1; an advanced function's `begin`/`process`/`end` blocks and `param()` were not visited (their calls lost); `Import-Module (Join-Path …)` was imported as its source text (a computed path is now no import). Known limits: `class`/`enum` and class methods have no symbol; computed import paths are not evaluated. Rejected by the grammar (outside the corpus, not yet filed): `??`/`??=`, the `? :` ternary, bare-word comma lists in command arguments (`-X a, b`), native arguments with `=` (`--timeout=300s`) and `--` |
| XAML | `mct-lang-xaml` | In PR | #112 | 6 / 1,860 | 157 / 125 (0) | Fixed in the PR: module end +1; an element spanned its start tag only, so a handler on an unnamed descendant was attributed to a named ancestor whose range did not contain it. Known limits, kept as tests: events outside the parser's list (`Executed`, `Sorting`, `MouseDoubleClick`, `SelectedDateChanged`…) and `<EventSetter Handler>` leave no relation; `x:Key` names no symbol |
| XML | `mct-lang-xml` | In PR | #104 | 6 / 2,037 | 547 / 0 (0) | Fixed in the PR: module end +1; an element spanned its start tag only. Known limit, kept as a test: cross-file `ref="…"`/`<import>`/`sourceRef` leave no relation (XML emits none) |

**Summary: 7 done, 10 in PR, 0 pending (17 total).**

## Related issues

- #79 — `coproc NAME { … }` truncates the enclosing function (tree-sitter-bash 0.25). Open.
- #82 — valid C++ rejected as a syntax error by tree-sitter-cpp 0.23.4 (explicit instantiation, `using Ts::operator()...`, `using typename B<K>::V`). Open.
- Not yet filed — valid PowerShell 7 rejected by the PowerShell grammar: `$a ?? $b`, `$a ??= $b`, `$c ? 1 : 2`, `Cmd -X a, b`, `kubectl --timeout=300s`, `cmd -- arg` (each reproduced with `scripts/unix/parse-probe.sh` on a one-line file).
- Not yet filed — valid Tailwind CSS v3 input rejected by tree-sitter-css 0.25: `@media screen(md) { … }` (the documented `screen()` function; reproduced with `scripts/unix/parse-probe.sh` on a one-line file). Kept out of the CSS corpus in #117.
