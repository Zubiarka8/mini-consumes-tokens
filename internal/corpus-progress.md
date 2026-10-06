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
| Python | `mct-lang-python` | **Done** | #78 | 8 / 2.730 | 391 / 1321 (294) | Module end +1; nested class missing parent; calls in decorator arguments missed; `type X = …` (PEP 695) has no symbol. #114 added Flask/Django files under `web/`: no new bugs |
| Bash | `mct-lang-bash` | **Done** | #80 | 5 / 1,539 | 176 / 585 (162) | Module end +1; assignment prefix (`LC_ALL=C cmd`) treated as a variable; `coproc NAME { … }` parsed incorrectly by the grammar → #79 (test is `#[ignore]`) |
| C/C++ | `mct-lang-cpp` | **Done** | #83 | 5 / 1,568 | 423 / 493 (150) | Module end +1; namespace functions/variables treated as methods/fields; locals (`auto x = f()`, `T x(args)`) treated as symbols and their calls missed; out-of-line `ns::Class::m`/`Tmpl<T>::m` gets a different parent from its declaration; qualified name in `class Outer::Inner {`; anonymous namespace treated as an unnamed module; `enum`/`union`/`using`/`typedef` have no symbol; valid constructs rejected by the grammar → #82 |
| C# | `mct-lang-csharp` | **Done** | #85 | 6 / 2,048 | 454 / 588 (143) | Module end +1; no symbols for file-scoped `namespace X;`, `record`, `enum`, `delegate`, events, operators, indexers, destructors, and local functions; generic bases with `<…>` in the target and first `IFoo` treated as `extends`; calls in `?.`, `F<T>()`, `new T()`, `: base(…)`, expression-bodied properties, and initializers missed; `nameof` treated as a call; attributes have no relation |
| CSS | `mct-lang-css` | **Done** | #86 | 6 / 2,194 | 617 / 12 (0) | Module end +1; a rule ended on its selector line instead of its `}`; `@namespace` prefix (`svg\\|text`) treated as an `svg` element; atoms missed in `.a .b[attr]` (the grammar applies `[attr]` to the whole chain). Rejected by tree-sitter-css 0.25 (outside the corpus, #87): `@page :first`, `@import … layer()`/`supports()`, media-query ranges (`400px <= width`) |
| Go | `mct-lang-go` | In PR | #103 | 7 / 3.002 | 291 / 832 (254) | Found and fixed (none pending): module end +1; `type A = B` (alias) had no symbol; methods on a generic receiver (`func (s *Stack[T]) Push`) got the parent `Stack[T]`; calls with explicit type arguments (`Map[A, B](x)`, and `Fail[T](x)`, which the grammar reads as a conversion) were missed. Known limits, kept as tests: no symbols for package `const`/`var` or struct fields; no relation for embedding or structural interface satisfaction; builtins and conversions (`len`, `string(x)`) appear as calls |
| HTML | `mct-lang-html` | Pending | | | | |
| Java | `mct-lang-java` | Pending | | | | |
| JavaScript/TypeScript | `mct-lang-js-ts` | Pending | | | | |
| Kotlin | `mct-lang-kotlin` | Pending | | | | |
| Lua | `mct-lang-lua` | Pending | | | | |
| Markdown | `mct-lang-md` | Pending | | | | |
| PHP | `mct-lang-php` | **Done** | #88 | 6 / 2,463 | 418 / 803 (339) | Module end +1; `?->` (null-safe) calls missed; `new Foo()` has no relation; `#[…]` attributes have no relation; anonymous-class members (`new class { … }`) indexed as methods/fields without a parent; closure assigned to a variable inside a method treated as a class member; `new` in the default value of a promoted parameter missed |
| PowerShell | `mct-lang-powershell` | Pending | | | | |
| XAML | `mct-lang-xaml` | Pending | | | | |
| XML | `mct-lang-xml` | Pending | | | | |

**Summary: 7 done, 1 in PR, 9 pending (17 total).**

## Related issues

- #79 — `coproc NAME { … }` truncates the enclosing function (tree-sitter-bash 0.25). Open.
- #82 — valid C++ rejected as a syntax error by tree-sitter-cpp 0.23.4 (explicit instantiation, `using Ts::operator()...`, `using typename B<K>::V`). Open.
- #115 — CSS nested rules are indexed with their literal `&` selector, not resolved against the parent (found in #114). Open.
- #116 — Tailwind v4 at-rule preludes and some CSS Nesting forms rejected by tree-sitter-css 0.25.0 (found in #114; earlier set in #87). Open.
