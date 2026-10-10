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
| Python | `mct-lang-python` | **Done** | #78 | 15 / 2,765 | 400 / 1,362 (331) | Module end +1; nested class missing parent; calls in decorator arguments missed; `type X = …` (PEP 695) has no symbol. #114 added a Flask app (`web/flask_app.py`) and a Django app split by responsibility (`web/django_site/lending/`): no new bugs. Known limit: module-level assignments (`urlpatterns`, model fields) have no symbol |
| Bash | `mct-lang-bash` | **Done** | #80 | 5 / 1,539 | 176 / 585 (162) | Module end +1; assignment prefix (`LC_ALL=C cmd`) treated as a variable; `coproc NAME { … }` parsed incorrectly by the grammar → #79 (test is `#[ignore]`). Rejected by tree-sitter-bash 0.25.1 (outside the corpus; found 2026-10-10 in this repo's `scripts/unix/pr-body.sh`), not yet filed: `;` inside a `${var:+…}` alternative (`x="${a:+$a; }b"`); `${a:-; }` parses |
| C/C++ | `mct-lang-cpp` | **Done** | #83 | 5 / 1,568 | 423 / 493 (150) | Module end +1; namespace functions/variables treated as methods/fields; locals (`auto x = f()`, `T x(args)`) treated as symbols and their calls missed; out-of-line `ns::Class::m`/`Tmpl<T>::m` gets a different parent from its declaration; qualified name in `class Outer::Inner {`; anonymous namespace treated as an unnamed module; `enum`/`union`/`using`/`typedef` have no symbol; valid constructs rejected by the grammar → #82 |
| C# | `mct-lang-csharp` | **Done** | #85 | 6 / 2,048 | 454 / 588 (143) | Module end +1; no symbols for file-scoped `namespace X;`, `record`, `enum`, `delegate`, events, operators, indexers, destructors, and local functions; generic bases with `<…>` in the target and first `IFoo` treated as `extends`; calls in `?.`, `F<T>()`, `new T()`, `: base(…)`, expression-bodied properties, and initializers missed; `nameof` treated as a call; attributes have no relation |
| CSS | `mct-lang-css` | **Done** | #86 | 12 / 3,240 | 931 / 17 (0) | Module end +1; a rule ended on its selector line instead of its `}`; `@namespace` prefix (`svg\\|text`) treated as an `svg` element; atoms missed in `.a .b[attr]` (the grammar applies `[attr]` to the whole chain). Rejected by tree-sitter-css 0.25 (outside the corpus, #87): `@page :first`, `@import … layer()`/`supports()`, media-query ranges (`400px <= width`). #114 added one scenario per framework, each the app's own CSS with the framework as a versioned external dependency: `bootstrap-shop/` (Bootstrap 5.3.8 from its CDN URL), `tailwind-v3-app/` (v3.4 input file), `tailwind-v4-app/` (v4.1). Bug, not filed yet (ignored test; fixed on `fix/css-hex-escapes`): hex escapes are not decoded (`.\\33xl\\:…` → `.33xl:…`). Known limits, kept as tests: nested `&` rules keep their literal selector → #115; `@utility` names have no symbol. Rejected by the grammar (outside the corpus) → #116: string-prelude at-rules (`@config`/`@source`/`@reference`/`@plugin "…";`), `@custom-variant x (…);`, `@import "…" prefix(…)`, `@utility x-* {`, `@container` nested in a rule, nested `.b &` (non-leading `&`); not yet filed: Tailwind v3's documented `@media screen(md) {`; `--x-*: initial;` is also rejected but is not valid CSS |
| Go | `mct-lang-go` | **Done** | #121 | 7 / 3,002 | 291 / 832 (254) | Found and fixed (none pending): module end +1; `type A = B` (alias) had no symbol; methods on a generic receiver (`func (s *Stack[T]) Push`) got the parent `Stack[T]`; calls with explicit type arguments (`Map[A, B](x)`, and `Fail[T](x)`, which the grammar reads as a conversion) were missed. Known limits, kept as tests: no symbols for package `const`/`var` or struct fields; no relation for embedding or structural interface satisfaction; builtins and conversions (`len`, `string(x)`) appear as calls |
| HTML | `mct-lang-html` | **Done** | #105 | 5 / 1,563 | 302 / 675 (0) | Module end +1 |
| Java | `mct-lang-java` | **Done** | #106 | 6 / 1,989 | 377 / 799 (155) | Module end +1 |
| JavaScript/TypeScript | `mct-lang-js-ts` | **Done** | #107 | 7 / 2,193 | 274 / 671 (125) | Module end +1 |
| Kotlin | `mct-lang-kotlin` | **Done** | #108 | 5 / 1,527 | 483 / 720 (89) | Module end +1 |
| Lua | `mct-lang-lua` | **Done** | #109 | 5 / 1,525 | 163 / 363 (43) | Module end +1 |
| Markdown | `mct-lang-md` | **Done** | #110 | 6 / 1,825 | 260 / 338 (229) | None recorded in #110 |
| PHP | `mct-lang-php` | **Done** | #88 | 6 / 2,463 | 418 / 803 (339) | Module end +1; `?->` (null-safe) calls missed; `new Foo()` has no relation; `#[…]` attributes have no relation; anonymous-class members (`new class { … }`) indexed as methods/fields without a parent; closure assigned to a variable inside a method treated as a class member; `new` in the default value of a promoted parameter missed |
| PowerShell | `mct-lang-powershell` | **Done** | #111 | 5 / 1,515 | 87 / 261 (62) | Module end +1. Rejected by tree-sitter-powershell 0.26.4 (outside the corpus; found 2026-10-10 in this repo's `scripts/windows/pr-status.ps1`), not yet filed: a comma-separated bareword argument (`Select-Object Name,Id`, `gh x --json a,b`); `Write-Output 1,2` and `"a,b"` parse |
| XAML | `mct-lang-xaml` | **Done** | #112 | 6 / 1,860 | 157 / 125 (0) | Module end +1 |
| XML | `mct-lang-xml` | **Done** | #104 | 6 / 2,037 | 547 / 0 (0) | Module end +1 |

**Summary: 17 done, 0 in PR, 0 pending (17 total).**

## Related issues

- #79 — `coproc NAME { … }` truncates the enclosing function (tree-sitter-bash 0.25). Open.
- #82 — valid C++ rejected as a syntax error by tree-sitter-cpp 0.23.4 (explicit instantiation, `using Ts::operator()...`, `using typename B<K>::V`). Open.
- #115 — CSS nested rules are indexed with their literal `&` selector, not resolved against the parent (found in #114). Open.
- #116 — Tailwind v4 at-rule preludes and some CSS Nesting forms rejected by tree-sitter-css 0.25.0 (found in #114; earlier set in #87). Open.
