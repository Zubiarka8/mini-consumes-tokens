# Issue #74 — Long-fixture corpus progress by language

Tracking for [#74](https://github.com/Zubiarka8/mini-consumes-tokens/issues/74):
each `mct-lang-*` crate needs a `tests/corpus/` containing at least five
interconnected files of 300–600 lines, an `expected.snap`, one large file in
`malformed/`, and `tests/corpus.rs` (the shared `mct-corpus` harness plus
language-specific tests). See `crates/mct-corpus/src/lib.rs`.

**Keep this file current until the issue is closed.** Every corpus PR updates
its language row in the same PR (status, PR number, counts, and bugs); after
merge, set its status to **Done**. When no rows remain pending, close #74 and
remove the corresponding rule from `rules.md`.

The file/line and symbol/relation columns, along with the summary, are filled
by `scripts/unix/corpus-report.sh <language> --update-progress` (use
`scripts\\windows\\corpus-report.ps1` on Windows). Set the status, PR number,
and bugs manually.

Statuses: **Pending** → **In PR** → **Done** (merged into `main`).

| Language | Crate | Status | PR | Files / lines | Symbols / relations (cross-file) | Parser bugs found |
|---|---|---|---|---|---|---|
| Rust | `mct-lang-rust` | **Done** | #77 | 6 / 2,179 | 239 / 817 (218) | Associated type/const owner; nested `fn` inside a method treated as a method; module end +1 |
| Python | `mct-lang-python` | **Done** | #78 | 6 / 2,006 | 302 / 918 (223) | Module end +1; nested class missing parent; calls in decorator arguments missed; `type X = …` (PEP 695) has no symbol |
| Bash | `mct-lang-bash` | **Done** | #80 | 5 / 1,539 | 176 / 585 (162) | Module end +1; assignment prefix (`LC_ALL=C cmd`) treated as a variable; `coproc NAME { … }` parsed incorrectly by the grammar → #79 (test is `#[ignore]`) |
| C/C++ | `mct-lang-cpp` | **Done** | #83 | 5 / 1,568 | 423 / 493 (150) | Module end +1; namespace functions/variables treated as methods/fields; locals (`auto x = f()`, `T x(args)`) treated as symbols and their calls missed; out-of-line `ns::Class::m`/`Tmpl<T>::m` gets a different parent from its declaration; qualified name in `class Outer::Inner {`; anonymous namespace treated as an unnamed module; `enum`/`union`/`using`/`typedef` have no symbol; valid constructs rejected by the grammar → #82 |
| C# | `mct-lang-csharp` | **Done** | #85 | 6 / 2,048 | 454 / 588 (143) | Module end +1; no symbols for file-scoped `namespace X;`, `record`, `enum`, `delegate`, events, operators, indexers, destructors, and local functions; generic bases with `<…>` in the target and first `IFoo` treated as `extends`; calls in `?.`, `F<T>()`, `new T()`, `: base(…)`, expression-bodied properties, and initializers missed; `nameof` treated as a call; attributes have no relation |
| CSS | `mct-lang-css` | **Done** | #86 | 6 / 2,194 | 617 / 12 (0) | Module end +1; a rule ended on its selector line instead of its `}`; `@namespace` prefix (`svg\\|text`) treated as an `svg` element; atoms missed in `.a .b[attr]` (the grammar applies `[attr]` to the whole chain). Rejected by tree-sitter-css 0.25 (outside the corpus, #87): `@page :first`, `@import … layer()`/`supports()`, media-query ranges (`400px <= width`) |
| Go | `mct-lang-go` | In PR | #103 | | | |
| HTML | `mct-lang-html` | In PR | #105 | 5 / 1.563 | 302 / 675 (0) | Module end +1 (fixed). Open: an element spans its start tag only, so a `<script src>` inside an id element (`<body id>`) is an import from a symbol whose range does not contain it. Known limits, kept as tests: `class` tokens are referenced only on elements with an `id`; `rel="alternate stylesheet"`, preload/icon links, anchors and images are not imports |
| Java | `mct-lang-java` | In PR | #106 | 6 / 1.989 | 344 / 705 (142) | Module end +1 (fixed). Open, kept as tests: methods (and their calls) in enum constant bodies are lost; type arguments in `extends`/`implements` become supertypes (`Comparable<Money>` → implements `Money`); `record` and `@interface` have no symbol, so record methods attach to the enclosing class; anonymous-class methods attach to the enclosing type; `new T(…)`, `this(…)`/`super(…)` and method references leave no relation; a record's `implements` and `permits` leave no relation |
| JavaScript/TypeScript | `mct-lang-js-ts` | In PR | #107 | 6 / 1.854 | 229 / 552 (87) | Module end +1 (fixed). Open, kept as tests: object-literal methods/getters are methods without a parent; nested functions are parentless functions; TS `enum`/`namespace` and interface members have no symbol; JSX elements are not calls of their component |
| Kotlin | `mct-lang-kotlin` | In PR | #108 | 5 / 1.527 | 514 / 702 (92) | Module end +1 (fixed). Open, kept as tests: `enum class` bodies are skipped (entries, methods, properties and companion lost); local `val`s inside a class's methods become fields of the class; top-level and local `fun`s are methods (top-level ones without a parent); an extension's parent keeps its type arguments (`Collection<Order>`); the `package` module spans the header line only; companion objects and `typealias` have no symbol; infix/operator calls leave no relation |
| Lua | `mct-lang-lua` | In PR | #109 | 5 / 1.525 | 185 / 330 (34) | Module end +1 (fixed). Open, kept as tests: functions inside a table constructor (`{ f = function() … end }`) and their calls are lost; calls inside the called expression (`m.f(x):g()` → only `g`) are lost; every `x = { … }` is a module, locals included, and a non-name target keeps its full text (`self.routes[#self.routes + 1]`); `x = function … end` spans its target line only, so its body's calls fall outside its range |
| Markdown | `mct-lang-md` | In PR | #110 | 6 / 1.825 | 260 / 338 (229) | None found (every heuristic clean). Known limits, kept as tests: links in table cells and code are not scanned; all-digit tags and URL fragments are not tags |
| PHP | `mct-lang-php` | **Done** | #88 | 6 / 2,463 | 418 / 803 (339) | Module end +1; `?->` (null-safe) calls missed; `new Foo()` has no relation; `#[…]` attributes have no relation; anonymous-class members (`new class { … }`) indexed as methods/fields without a parent; closure assigned to a variable inside a method treated as a class member; `new` in the default value of a promoted parameter missed |
| PowerShell | `mct-lang-powershell` | In PR | #111 | 5 / 1.514 | 87 / 252 (57) | Module end +1 (fixed). Open, kept as tests: an advanced function's `begin`/`process`/`end` blocks are not visited (their calls are lost); `Import-Module (Join-Path …)` is imported as its source text, and dot-sourcing a computed path records nothing; `class`/`enum` and class methods have no symbol. Rejected by the grammar (outside the corpus, not yet filed): `??`/`??=`, the `? :` ternary, bare-word comma lists in command arguments (`-X a, b`), native arguments with `=` (`--timeout=300s`) and `--` |
| XAML | `mct-lang-xaml` | In PR | #112 | 6 / 1.860 | 157 / 125 (0) | Module end +1 (fixed). Open: an element spans its start tag only, so a handler on an unnamed descendant is attributed to a named ancestor whose range does not contain it (29 relations in the corpus). Known limits, kept as tests: events outside the parser's list (`Executed`, `Sorting`, `MouseDoubleClick`, `SelectedDateChanged`…) and `<EventSetter Handler>` leave no relation; `x:Key` names no symbol |
| XML | `mct-lang-xml` | In PR | #104 | 6 / 2.037 | 547 / 0 (0) | Module end +1 (fixed). Known limits, kept as tests: an element spans its start tag only; cross-file `ref="…"`/`<import>`/`sourceRef` leave no relation (XML emits none) |

**Summary: 7 done, 10 in PR, 0 pending (17 total).**

## Related issues

- #79 — `coproc NAME { … }` truncates the enclosing function (tree-sitter-bash 0.25). Open.
- #82 — valid C++ rejected as a syntax error by tree-sitter-cpp 0.23.4 (explicit instantiation, `using Ts::operator()...`, `using typename B<K>::V`). Open.
- Not yet filed — valid PowerShell 7 rejected by the PowerShell grammar: `$a ?? $b`, `$a ??= $b`, `$c ? 1 : 2`, `Cmd -X a, b`, `kubectl --timeout=300s`, `cmd -- arg` (each reproduced with `scripts/unix/parse-probe.sh` on a one-line file).
