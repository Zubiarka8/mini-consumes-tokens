# Checkpoint: depgraph-languages

Task: issue #28 (`get_dependency_graph`), language side. Audit every supported
language crate for the `Imports` relations the dependency graph is built from,
and make each one carry enough `RelationTarget` evidence to resolve to the
target file's `Module` symbol. Branch `claude/depgraph-languages`, base
`feat/get-dependency-graph` @ 4e370de.

Constraints kept: `LanguageParser`, the SQLite schema and the MCP signatures are
untouched. Only what each `mct-lang-*` parser emits changed, within the existing
model (`RelationKind::Imports` plus `RelationTarget` `path` / `module` / `kind` /
`language` / `external`). A field is set only when the source proves it. The
original relation (name as written, e.g. `./util`, `net/http`, `a.b.c`) is kept
unchanged beside the new file-level relation, because existing tests and
snapshots assert it; a simple name that would only duplicate it is merged into
one relation (Lua, Python).

## How a file edge is formed

A file-level import is an `Imports` relation named after the target file's
`Module` symbol (the file stem, or the namespace/package name for C#/Go/Kotlin)
with `kind = Module`. The index resolves it to exactly one candidate when the
evidence narrows it enough; `path` pins one file, `module` demands the named
path segment, `external` marks a positive out-of-repo target.

## Audit table

| Language | Mechanism | Emitted before | Resolves to a file now | Change / limitation |
|---|---|---|---|---|
| Rust | `use` | yes, item names, no path evidence | item resolves to its defining file; `module` (path segment) or `qualifier` (type) now narrows it, so `use serde::Serialize` no longer matches a local `Serialize` | `module`/`qualifier` evidence on `use` items. Aliased `use a::b as c` stays unqualified (the real name is not `to_name`). `std/core/alloc` already external |
| Rust | `mod name;` | no | yes: `name.rs` (Module `name`) via `module = name`, `kind = Module` | new relation. `name/mod.rs` has Module `mod` and is not matched (limitation); `#[path]` ignored; inline `mod x {}` is not an import |
| Python | `import a.b.c`, `from a.b import x`, relative forms | item names only, `from` module skipped | yes: Module `c` under a path segment `b`; `from .x import y` scoped to the importing file's package dir | new relation per module, `kind = Module`. `import pkg` (package `__init__.py`, Module `__init__`) and `from . import x` are not resolved; stdlib is not marked external (no positive evidence list) |
| JS/TS | ES `import`, `export ... from`, `require`, `import()` | binding names (alias) and `require` spec string | yes for relative specs: exact `path` for `.ts/.tsx/.mts/.cts/.jsx`; stem + parent-dir `module` for extensionless and `.js/.mjs/.cjs`; `.css` imports link to the css Module (`language = css`); `node:` builtins external | extensionless `./dir` resolving to `dir/index.ts` is not resolved (Module is named `index`); bare specifiers and path aliases (`@/x`) record nothing (cannot prove local or external); `.json`/assets skipped |
| Go | `import "a.b/c/pkg"` | full import path as `to_name` | partly: Module named by the last element (version suffix `/vN` skipped), `module` = that dir. The target is a package, so a multi-file package is `ambiguous` over exactly its own files; a single-file package whose stem equals the package name also reports `ambiguous` (two Module symbols, same file) | only domain-rooted paths (first element has a dot) are recorded: a stdlib name (`errors`, `encoding/json`) is also a common file stem and would resolve to the wrong local file. Dotless local modules are not resolved. The graph builder should treat an ambiguous result whose candidates are all in one directory (or one file) as that package/file |
| Java | `import a.b.Type`, `import static a.b.Type.m` | last identifier only | yes: Module `Type` (public type lives in `Type.java`) under package segment `b`; `java.*`/`javax.*` external | package wildcard `a.b.*` names a directory, records nothing; non-public/secondary top-level types in another file are reached only through the existing item relation; nested `Outer.Inner` resolves to the `Outer` file |
| Kotlin | `import a.b.Type [as T]` | last identifier / alias | yes when the file is `Type.kt` (convention): Module `Type` under segment `b`; `kotlin.*`/`java.*`/`javax.*` external | a type in a differently named file is reached through the existing item relation; wildcard and top-level function imports record nothing new |
| C# | `using A.B;` | last identifier only (never matched anything) | namespace, not a file: Module named `A.B` held by every file declaring it. Resolves only when a single file declares the namespace, else `ambiguous` over those files; `System*`/`Microsoft*` external | `using static`, generic aliases skipped; alias `using X = A.B;` targets `A.B`. Namespaces nested via braces keep the inner name only (limitation) |
| C/C++ | `#include "x.h"` | path string as name | yes when the file is next to the includer: exact `path` (header never confused with `x.cpp`) | includes found only through `-I` paths (`#include "inventory/model.hpp"` from `src/`) do not resolve; `<angle>` includes record nothing new (no external claim, `<project/x.h>` can be local) |
| PHP | `use A\B\Type`, group `use A\{B, C}`, `use function/const` | last segment / alias | `use` of a type: Module `Type` under namespace-directory segment `B` (PSR-4); `require/include` of a literal or `__DIR__ . '/x.php'`: exact `path` | files that hold several classes (non-PSR-4) rely on the existing item relation; `use function/const` and computed includes record nothing new; a bare relative include is read against the includer's directory (PHP also searches `include_path`/cwd) |
| Lua | `require("a.b.c")` | spec string | yes: Module `c` under segment `b`; single-segment `require("x")` carries `kind = Module` on the existing relation | `a/b/init.lua` is Module `b`, so `require("a.b")` matches it; literal `dofile`/`loadfile` `.lua` paths resolve by exact `path` (assumed project-root relative) |
| Bash | `source` / `.` | path text as written | `.sh/.bash` files: exact `path` for literal relative paths and script-dir idioms (`$(dirname "$0")/x.sh`, `${BASH_SOURCE%/*}`); `$VAR/x.sh` proves only the stem (+ literal parent dir) | a plain relative `source` is really cwd/`PATH` relative (read against the script's dir: a mismatch stays unresolved); absolute and `~` paths and non-`.sh` names record nothing |
| PowerShell | dot-sourcing, `Import-Module` | spec string | literal relative `.ps1`/`.psm1` path: exact `path` | `. "$PSScriptRoot/x.ps1"` and other computed paths are still not recorded (kept as the crate documents; it is the dominant idiom, so real repos will mostly be unresolved); bare module names (`Import-Module Pester`) stay name-only |
| HTML | `<link rel=stylesheet href>`, `<script src>` | raw URL/path as name | local `.css`/`.js` etc: exact `path` + target `language` (`css`, `javascript_typescript`) with `kind = Module`; absolute URLs external | other asset types and `<a href>`/`<img src>` are not imports |
| CSS | `@import` | path as written | local `.css`: exact `path` (relative to the importing file); absolute URL external | bare package names (`@import "tailwindcss"`) record no file evidence |
| XAML | `<ResourceDictionary Source>` | not emitted (only event-handler References to code-behind) | relative `.xaml` source: exact `path` | `pack:`/`;component` URIs skipped; the `Foo.xaml` <-> `Foo.xaml.cs` pairing stays a `References` link, not an import |
| XML | none | none | no | cannot express file-level imports: the crate is structural only (dialects such as MSBuild `<Import Project>` / POM `<module>` would each need their own convention). Recorded as a limitation, not faked |
| Markdown | `![[Note]]` embed | yes, `Imports` with `kind = Module` + `path` | yes (already) | unchanged, confirmed by the existing parse/index tests. Plain links stay `References` |

## Known cross-cutting limitations for the graph builder

- Duplicated rows for the same import: the original relation (name as written)
  and the new file-level relation both exist and both appear in
  `find_references` by name. Java/Kotlin/C#/PHP simple imports repeat the same
  `to_name` (one resolved to the file Module, the other unqualified and
  usually `ambiguous` between a class and its file Module, both in one file).
  Filter on resolution / `kind` when building edges. `find_references("x")` can
  now also list the import beside a call (the Bash `deploy` integration test
  was narrowed to `calls`).
- Ambiguous but single-file: Go and Java-style "class Money + module Money in
  the same file" produce candidates all in one file. The edge is unambiguous at
  file level even though the index says `ambiguous`; the builder should
  collapse candidates by file.
- Stdlib is only marked `external` where the language gives positive evidence
  (Rust `std/core/alloc`, Java `java/javax`, Kotlin `kotlin/java/javax`, C#
  `System/Microsoft`, JS `node:`, absolute URLs in HTML/CSS). Python, Go, PHP,
  Lua, Bash, C++ stdlib/third-party imports simply stay unresolved.
- Resolution only ever picks a file that is indexed, so imports of unparsed
  files (json, svg, generated code) stay unresolved.

## Tests

Each changed crate has a new `tests/depgraph.rs` (cross-file fixture indexed
through `mct-index`, asserting resolution and candidate file, plus the exact
`RelationTarget` evidence where the source proves less). Corpus snapshots that
legitimately grew (extra `Imports` rows) were blessed; two existing assertions
that encoded the old behaviour were updated (HTML "imports carry no evidence",
CSS relation count, XAML "references only", Bash `find_references("deploy")`).

## Verification

`scripts/unix/check.sh -p <crate>` (fmt, test, clippy) passes for: mct-lang-js-ts,
python, rust, go, java, kotlin, csharp, cpp, php, lua, bash, powershell, html,
css, xaml. `cargo run -p mct-eval`: 31 cases, accuracy 1.000, "No regressions";
the baseline was not refreshed.
