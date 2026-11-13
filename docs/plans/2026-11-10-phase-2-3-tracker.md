# Phases 2 and 3: requested changes and progress

One file for everything requested in the "Plan de ejecución intensiva"
(2026-11-09) and the authorization message that followed it (2026-11-10), and
how far each item has got. Update a box only once its tests have passed. Every
number below was measured; none is an estimate.

Legend: `[x]` done (PR open or merged), `[~]` partly done, `[ ]` not started,
`[!]` blocked until the maintainer decides.

## Rules in force

From `rules.md` and the two messages:

- TDD for every defect fix and parser change. Small, coherent commits and PRs.
- `scripts/unix/check.sh` green before every commit.
- Open an issue before changing `LanguageParser`, the SQLite schema or MCP
  signatures. Repository content is in English.
- Constraints quoted verbatim:
  - "No marcar un símbolo como `external` únicamente por su nombre o porque no se encuentre un candidato."
  - "No modifiques una prueba únicamente para adaptar el resultado esperado a un comportamiento incorrecto"
  - "No modificar las referencias de benchmarks para ocultar regresiones"
  - "No descartes ni sobrescribas cambios existentes."
  - "No inventes resultados ni porcentajes de mejora."
  - "No presentes una estimación de rendimiento como resultado medido."
  - "No implementes una prohibición global de relaciones entre producción y fixtures: resuelve las relaciones utilizando evidencia real del código."
  - "No conviertas una relación ambigua en confirmada sin evidencia."
  - "No hagas merges automáticos."
  - "Solo detente cuando exista un bloqueo real, un fallo de seguridad o una decisión arquitectónica fuera del alcance autorizado."
- A report contains only: completed blocks, issue and PR links, test
  results, metrics, blockers and remaining work.

## Issues

- #320: resolution precision (D2–D6, evidence on `RelationHit`).
- #321: enriched symbol model (ADR-034).
- #322: reference counts (D7).
- #330: store each relation's resolution at index time. Blocked, see below.

## Block 0: close the current PR

- [x] `feat/rust-parser-next` published and merged (#318).

## Block 1: defects D2–D9

- [x] **D2, false targets in fixtures and builtins.** Python `open`/`len` and
      TS `Math.max` fixed through builtin and import evidence, with no
      production → fixture ban: #319, #323.
- [~] **D3, non-callable targets.** Java is done: a call never resolves to a
      field (#319). Not done for C# and Kotlin, where a delegate field or a
      function-typed property can be called.
  - [!] #365: the fix needs a predicate in the `relation_candidates` view
        (a migration). On this repository all 27 C#/Kotlin calls with a
        `Field` candidate are ambiguous, and every such field is
        non-callable (`new Price(…)` next to `Price Price { get; }`).
        Waiting for the maintainer.
- [x] **D4, variable scopes.** A function's variable or a Lua file-level
      `local` is a candidate only in its own file (migration 15): #324.
- [~] **D5, receiver types.** Java (#319), JS/TS #326, Kotlin #327, C#
      #328, Go #329.
  - [x] Rust, proven receivers only: a local whose only binding in the
        function states its type (a parameter `x: T`/`&T`/`&mut T`/`T<..>`,
        `let x: T`, `let x = T { .. }`) qualifies `x.m()` by `T`, as a
        method; a type the file imports from another crate also carries
        that crate as module evidence (`use serde_json::Value`). A type
        parameter, a `dyn`/`impl` type, a std type (`String`,
        `Box<T>`), an aliased import, or a name imported from two crates
        is not proof; `self.m()` is unchanged. This repository: 199 ambiguous
        calls became resolved, each to the stated type (112 in the same
        file, 87 through a `use`); 144 resolved calls became unresolved,
        all wrong before (`tree_sitter::Node` `walk()`/`parent()` →
        `Search.walk`/`Ctx.parent`); 43 ambiguous calls became unresolved
        (std containers behind a `type` alias, `serde_json::Value`,
        `tracing` types). No right resolution was lost; dead code
        unchanged.
  - [x] Python, proven receivers only: `self.m()`/`cls.m()` when the class
        body defines `m`, a parameter annotated with a class, a local bound
        once to `T(…)` or `x: T = …`, and `T.m()` on a class the file
        defines or imports are qualified by that class; a receiver typed by
        a builtin container or `str` (`opts: dict`, `rows = []`) is
        external. Any other receiver is left to name resolution, which
        keeps the `[!]` question below open for Python too. This
        repository: 30 ambiguous calls became resolved, each to the stated
        class; 20 resolved calls became external, all wrong before
        (`list.extend` → `Report.extend`, `dict.values` →
        `Repository.values`); `Path.home()` lost a wrong target; 644
        unresolved or ambiguous builtin calls became external. No right
        resolution was lost; dead code unchanged.
  - [x] Python `self.attr.m()`: qualified by the type the enclosing class
        annotates for `attr` (`attr: T` in its body, `self.attr: T = …` in
        a method), external for a builtin type, when the class defines no
        method or class `attr`, every type the file states for an attribute
        `attr` (annotations, `x.attr = U(…)`, literals) agrees, and the file
        sets no attribute dynamically (`setattr`, `__dict__`, `vars`,
        `__setattr__`, `__getattr__`). This repository: 3 unresolved calls
        became external (`str.casefold`/`str.strip` on `str` attributes of
        the Python corpus); no qualifier or other language changed; dead
        code unchanged (124).
  - [!] Whether Kotlin (#327) and C# (#328) adopt Go's rule for a receiver
        with no stated type. Today that loses 12 (Kotlin) and 25 (C#) right
        resolutions. Waiting for the maintainer.
- [~] **D6, imports and aliases.** The imported module is carried as call
      evidence for Python and JS/TS (#323). Aliases, globs, Rust inline
      modules and name collisions across the other languages are not
      checked yet.
  - [x] Rust: a bare name a `use` of another crate brings in gets the crate
        as module evidence, in calls, type references, derive and
        attribute arguments, path qualifiers and trait names. It carries
        none for a `crate::`/`self::`/`super::` path, an alias, a declared
        or locally bound name, a path from a module of the file, or a file
        that may import its own package (tests, examples, benches,
        binaries, build scripts). On this repository, Rust relations went
        441 ambiguous → resolved, 144 ambiguous → unresolved, and
        90 resolved → unresolved. Every one of the 90 was wrong:
        `rusqlite::Connection` → `web/http.rs`, `serde::Serialize` → a
        `depgraph` fixture, `tree_sitter::Tree` → `Action.Tree`. One
        `reindex` call in a test went resolved → ambiguous. Dead code is
        unchanged (124). Test: `mct-lang-rust/tests/import_evidence.rs`.
        A package whose directory has another name loses the edge: #374.
- [x] **D7, reference counts.** A relation resolved elsewhere, or external,
      no longer keeps a symbol alive in dead code: #331.
  - [!] #330 (overview ranking and dashboard counts) needs a migration and an
        ADR. Waiting for the maintainer.
- [x] **D8, `Overrides`** (issue #349). Built from inheritance,
      implementation and type evidence only, never from a shared name.
  - [x] Core `RelationKind::Overrides` and Rust (`impl Trait for T`): 258
        relations on this repository, 186 resolved (each to a same-named
        method of the named trait), 62 external (std traits), 10
        unresolved (third-party traits), 0 ambiguous. Dead code unchanged.
  - [x] Java `@Override`: one relation per direct supertype, plus the JDK
        root (`Object`, `Enum`, `Record`) for `equals`, `hashCode`,
        `toString`, `clone` and `finalize` when the type extends no class.
        On this repository: 49 relations from 43 methods, 15 resolved (each
        to the declaring interface or class), 5 external, 15 unresolved (JDK
        interfaces such as `Runnable`, or a supertype that doesn't declare
        the method), 14 ambiguous. Dead code unchanged.
    - Known limit: a method of an anonymous class has the enclosing type
      as its parent, so when it overrides a method of that same type
      (`new Listener() { … }` inside `Listener`) every such body is a
      candidate and the relation stays ambiguous. An enum constant's body
      no longer has this limit: its methods have the constant as their
      parent (see Block 3), and 12 of the 14 ambiguous relations are now
      resolved, each to the enum's own method. The 2 left are anonymous
      classes.
  - [x] Kotlin `override fun`: same rule, with the supertypes from the
        delegation specifiers (an enum entry's body: the enum; `object : T`:
        `T`) and the root `Any` (`Enum` for an enum class) for `equals`,
        `hashCode` and `toString`. On this repository: 39 relations from 30
        functions, 15 resolved (each to the declaring interface or class), 8
        external, 11 unresolved, 5 ambiguous, all 5 the limit above (members
        of `object : InventoryListener { … }` inside `InventoryListener`, and
        an enum entry's body). Dead code unchanged. `override val` properties
        get no relation yet.
  - [x] C# `override` methods: the one candidate is the base list's first
        entry, since only a base class's `virtual`/`abstract` method can be
        overridden. A class with no base list has the root `object` (a
        struct `ValueType`) for `Equals`, `GetHashCode` and `ToString`; a
        first entry named like an interface (`IShape`) keeps both it and
        `object`. On this repository: 20 relations from 17 methods, 9
        resolved (each to the base class declaring the method), 7 external,
        4 unresolved (`Exception` and other framework bases), 0 ambiguous.
        Dead code unchanged. `override` properties get no relation yet.
  - [x] TypeScript `override` methods: the one candidate is the class the
        class `extends` (TypeScript accepts `override` only there, with no
        implicit root). An `abstract` method signature is now recorded as a
        method of its class, so an override of it can resolve. This
        repository has no TypeScript `override`; the corpus gains one
        symbol (`Observable.get`). Dead code and index size unchanged. A
        class expression's methods get no relation.
  - [x] Python `@override` (PEP 698): one candidate per positional base
        written as a name (`Base[int]` is `Base`), else the root `object`,
        external, only for the methods `object` defines. The decorator
        counts only when every binding of a bare `override` is an import
        from `typing` or `typing_extensions` (the `try`/`except ImportError`
        pair included), or through a module alias of one of them; a
        user-defined `override` gives nothing. A base the syntax can't name
        (`make_base()`) gets no relation and rules out the root. This
        repository and the corpus have no Python `override`: corpus, dead
        code and index size unchanged.
  - [x] C++ `override` member functions: one candidate per base of the
        class (`Base<T>` and `ns::Base` are `Base`), no root. Only the class
        body can write `override`, so an out-of-line definition gets no
        relation; a destructor gets none either (it overrides `~Base`, a
        different name). This repository (the C++ corpus): 11 relations, 10
        resolved, 1 ambiguous (`Product::describe`: see Block 3). Dead code
        unchanged; index size 26,132,480 → 26,128,384 B (page rounding).
  - [x] Property overrides, with the shared `push_property_overrides`
        (same candidates as the language's methods, target kind `field`, no
        root: `Any` and `object` declare no property). Kotlin `override
        val`/`var`, in a body or as a constructor property (corpus +14,
        `OrderEvent`'s data classes); C# `override` properties, indexers
        (`this`) and events, field-like ones too (corpus +7); TypeScript
        `override` fields, where an arrow-valued field is a method and so
        targets a method (corpus unchanged). Measurements in the PR.
  - [x] PHP 8.3 `#[\Override]` methods: one candidate per parent class and
        interface (an interface's: those it extends), resolved through the
        namespace; no root. The attribute counts only when its name resolves
        to the global `Override` (a bare `#[Override]` in a namespace needs
        `use Override;`). A trait's method gets none (PHP checks it where
        the trait is used). This repository and the corpus have no
        `#[\Override]`: corpus, dead code and index size unchanged.
        PowerShell classes override by name only, with no syntax to show it,
        so they get no relation.
- [~] **D9, external and stdlib symbols.** Rust prelude (#318) and Python/JS
      builtins (#319) are done. Host and stdlib globals for the other
      languages are not, except:
  - [x] Lua 5.4 standard globals (basic functions and library tables) a
        file never binds are external, in a bare call (`setmetatable(…)`)
        or through the library table (`math.floor(…)`). Any binding of the
        name in the file (a local, a parameter, a global assignment,
        `function type()`) leaves it unproven, and a file touching `_ENV`,
        `_G` or `setfenv` marks nothing. This repository: 99 calls become
        external; the 19 that had a target were all wrong (another file's
        `local setmetatable`/`floor`/`format` alias, or `Money:max` for
        `math.max`). 5.1-only names (`unpack`, `loadstring`) are left out,
        because projects often define them as globals.
  - [x] Go: `pkg.F(…)` through an import of a standard package (Go 1.24's
        public `src/` packages; `json` for `encoding/json`, `rand` for
        `math/rand/v2`, an alias) is external. `cmd/go` resolves a standard
        path from GOROOT before any module, so no repository directory is
        that import. A name the file uses anywhere other than as a selector
        operand (a local `path := …`, a parameter) is not proven; dot and
        blank imports bind nothing. This repository: 136 calls become
        external; the 16 with a target were all wrong (`strings.Split` →
        `Money.Split`, `time.Now` and `errors.Is`/`Unwrap` ambiguous between
        repo methods). Universe builtins (`len`, `append`) stay unresolved:
        a package-level declaration in a sibling file can shadow them, which
        one file can't rule out. No eval case yet: there is no Go toolchain
        here to run its truth.
  - [x] Java: a call whose receiver type is bound by a single-type
        `java.`/`javax.` import (`import java.util.List;`: `List.of()`,
        `items.add()` on a `List items` field or local) is external. A
        single-type import shadows the package's own types (JLS §6.4.1),
        so a same-package `List` is never the target. Not proven: a name
        the file also declares as a type or type parameter, two imports of
        one name, static and on-demand imports, and `java.lang` names used
        without an import (`Math`, `String`). This repository: 180 calls
        become external; the only one with a target was wrong (`LOG.log` on
        a `java.util.logging.Logger` field → another fixture's `Logger.log`).
        A constructor call of such a type (`new HashMap<>()` under
        `import java.util.HashMap`) is external too; a qualified
        `new java.util.ArrayList<>()` is left to resolution. This
        repository: 28 more calls become external, none had a target.
        JDK supertypes (`implements Comparator<T>`) are not marked yet,
        which leaves their `Overrides` to name resolution.
  - [x] PHP: a call that can only reach a global core function
        (`\strlen()`, or a bare `strlen()` in a file with no namespace and
        no `use function` of it) is external, since a global core function
        is enabled. The list has only PHP 7.0 `Core`,
        `standard`, `date`, `pcre` and `spl` functions and the
        call-like keywords (`isset`, `empty`). Not proven: a bare call in a
        namespace, and a name the file declares itself (a polyfill). PHP 8
        `disable_functions` can permit a core function to be redefined in
        another file; runtime configuration is not visible to the parser. This
        repository: 2 calls become external, almost all of its PHP is
        namespaced. The one with a target was wrong (`count()` in
        `omni-app/web/api.php`, ambiguous between two `count` methods).
        Dead code unchanged. Test: `mct-lang-php/tests/builtins.rs`.
  - [x] C++: a call through `std::` (`std::sort(…)`, `::std::move(…)`,
        `std::make_unique<T>(…)`) is external, since a program may not add
        declarations to namespace `std`. A file that opens `namespace std`
        (a `std::hash` specialization) and a bare call after
        `using namespace std;` prove nothing. A qualified template call is
        now named without its arguments (`make_unique`, not
        `make_unique<Writer>`), as an unqualified one already was. This
        repository: 62 C++ calls become external. The 10 that had a target
        were all wrong (`std::to_string(…)` → the `Quantity.to_string`
        method). Dead code unchanged. Test: `mct-lang-cpp/tests/std_calls.rs`.
  - [x] Rust: `u64::from(x)`, `try_from`, `into`, `try_into` on a primitive
        type no item, `use` or glob of the file shadows is a member call,
        ambiguous even with one candidate. Core's blanket impls
        (`impl<T> From<T> for T`, `TryFrom<U> for T where U: Into<T>`) always
        implement the trait for the primitive too, and a primitive has no
        inherent method, so a repository `impl From<Id> for u64` is one
        candidate among several. Inside a macro's arguments the same
        evidence applies. This repository: 95 calls get the flag; the 11 that
        were resolved, all to the corpus's `impl From<ProductId> for u64`,
        were all wrong (`u64::from(l.on_hand)`, `u64::from(*byte)`) and are
        now ambiguous; the other 84 had no candidate. Dead code unchanged
        (124). Test: `mct-lang-rust/tests/primitive_conversions.rs`.
- [x] **Evidence on `RelationHit`.** Additive MCP field: #325.
- [~] **Block 1 validation.** `crates/mct-eval/suite-resolution.json` has
      labelled cases. D8 has 4 (Java, Python, TypeScript; each against a
      same-named method in an unrelated class), all correct. D9 has 2 for
      Lua (`setmetatable`, `string.format` next to another file's local
      aliases), both correct; before the fix, 1 wrong and 1 ambiguous. D9
      has 2 for Java (`items.add`, `List.of` under `import java.util.List`
      next to a same-package `shop.List`, checked with `javac` and
      `javap`), both correct; before the fix, 1 wrong and 1 missing. D9
      has 3 for C++ (`std::sort` and `std::to_string` next to `util::sort`
      and `Quantity::to_string`, plus the qualified `util::sort` that must
      stay resolved, checked with `clang++`), all correct; before the fix,
      2 wrong (`std::sort` → `util::sort`, `std::to_string` →
      `Quantity::to_string`). No PHP or Go case: there is no `php` or Go
      toolchain here to run their truth. D9 cases for the other languages
      wait on their fixes. D5 has 2 for
      Python (`cat.find` on `cat: Catalogue` next to `Shelf.find`,
      `opts.get` on a `dict` next to `View.get`, run with CPython), both
      correct; before the fix, 1 wrong and 1 ambiguous. Rust's D5 label
      (`stack.len()` on `stack: &Stack`, next to a free `fn len`) is now
      correct; before, ambiguous.

## Block 2: phase 2, SQLite and enriched model

- [x] **2.1 Design.** ADR-034
      (`docs/01-architecture/adrs/034-symbol-details.md`).
- [x] **2.2 Migration.** Migration 16 adds `symbol_details` and
      `symbol_parameters`, with migration tests: #332.
- [x] **2.3 Parsers emit details** (table in
      `docs/02-crates/parsers/symbol-coverage.md`):

  | Language | PR | Notes |
  |---|---|---|
  | Rust | #333 | |
  | Go | #334 | |
  | JS/TS | #335 | |
  | Python | #336 | |
  | Java | #337 | |
  | Kotlin | #338 | |
  | C# | #339 | |
  | PHP | #340 | |
  | C++ | #341 | |
  | Lua | #342 | |
  | Bash | #343 | |
  | PowerShell | #345 | |
  | CSS, HTML, Markdown, XML, XAML | — | Out of scope: they have no declarations with signatures. |

- [x] **2.4 MCP uses the metadata.**
  - [x] `build_context_pack`: uses the stored signature, with the same
        120-character cap as before and only while the file still holds the
        indexed revision. Measured on 48 symbols in 12 languages: full packs
        173,215 → 173,027 approximate tokens, `summary` 13,190 → 12,849; 6 of
        48 packs grow, the ones whose header spans several lines.
        #346.
  - [x] `find_symbol`: a `signature` column (TOON) and ` | header` (text),
        same cap and stale-file rule (issue #347). Measured on 72 names in
        18 languages: TOON 16,336 → 18,061 approximate tokens (+10.6%, about
        24 per call), text 14,575 → 15,694 (+7.7%); median CLI call latency
        within noise.
  - [x] `find_calls`, `find_callers`, `find_references`, `impact_analysis`:
        decided not to add signatures. Their rows are relation sites, not
        definitions, often hundreds per call, and `build_context_pack`
        already gives every related symbol's signature once.
- [x] **2.5 Benchmarks.** SQLite size and coverage measured for each parser
      PR (in each PR body). Before and after the whole stack (`main` at
      e000eb65 against #383), on one machine with other work running, so
      only differences larger than the run-to-run spread are claimed:
  - [x] `mct-eval` main suite (33 cases, 20 timed calls each, two runs per
        version): accuracy 1.000 and MRR 0.917 for both. Response tokens
        7,012 → 7,134 (+1.7%), all from the signatures of 2.4; 16 cases are
        byte-identical. `find_symbol` cases go from 0.003–0.026 ms to
        0.035–0.127 ms p50 (the signature lookup), every other case moves
        less than 0.2 ms.
  - [x] This repository's snapshot, fresh `init`, three interleaved runs:
        SQLite 24.4 → 26.2 MB (+7.5%). Peak RSS (87–100 MB against
        100–101 MB) and time (5.9–7.9 s against 6.6–7.3 s) overlap.
  - [x] `mct call`, seven interleaved runs (the median includes the startup
        incremental check): `find_symbol`, `find_callers`,
        `impact_analysis`, `build_context_pack` and `get_project_overview`
        within 0.01 s of `main` (0.16–0.30 s), peak RSS +0.2–0.6 MB. Response
        bytes grow with signatures: `find_symbol` 156 → 179,
        `find_callers` 448 → 484, `impact_analysis` 8,314 → 8,840.
  - [x] `find_dead_code` was the one regression: 0.33 → 0.79 s median,
        bisected to the per-symbol counting of #322 (0.29–0.49 s before
        that commit, 0.68–0.95 s at it). Both halves of its `UNION` read
        `resolved_names`, which ran its candidate lookups twice;
        materializing it gives 0.63 s with an identical report. Detected
        117 → 122: −2 from #382, and +7 whose only same-named relations
        resolve elsewhere (a Python `renderer`, an HTML `style.css`, a Rust
        `escape_field` in `toon.rs`), which #322 no longer counts.

## Block 3: phase 3, all languages

- [x] Signatures, parameters, types, visibility and modifiers for every
      language with declarations (see 2.3).
- [~] Updated symbol-coverage matrix, then work through the gaps by language
      group (3.1).
  - [x] The score tables had drifted from the checklists that parser PRs
        update (Rust constructs 60% → 98% after the checklist gaps were
        closed, Python, JS/TS, Java and PHP by 1–6 points). They are
        recomputed, and `crates/mct-languages/tests/coverage_scores.rs`
        recomputes them from the checklists on every test run.
  - [x] Re-probe the cells still dated 2026-10-19 (`71ec6d0`): every item
        not marked `[x]`, in all 12 languages, with a release `mct-cli`.
        Stale cells corrected: TypeScript abstract methods are indexed;
        Kotlin `: Base(4)`/`: this(0)` and `@Marker` were `[?]` and give no
        relation (66% constructs, down from 70% because two unverified items
        are now scored); Lua classes, `extends` through `setmetatable`,
        metamethods, variables and duplicates were added by `dc683c35`
        (constructs 54% → 82%). The other cells were confirmed as written.
  - [ ] Work through the gaps by language group.
    - [x] Go fields, constants and variables: each named struct field is a
          `field` of the struct (an embedded one is none), each package-level
          `const` / `var` name, grouped or not, a `constant` / `variable` of
          the package, with signature and exported-name visibility. So that
          these names don't widen call matches, a call now carries its
          syntax evidence: a bare `f()` is qualified by this file's package
          unless `f` is a local; `v.f()` on a value whose type isn't proven
          targets a method; `pkg.F()` through an unaliased repository import
          ending in `pkg` is qualified by `pkg`; an import path names a
          module only. Tests: `tests/fields_and_values.rs`,
          `tests/call_evidence.rs`. Corpus: 291 -> 463 symbols. Repository
          snapshot (Go, 861 relations; 138 fields, 24 constants, 14
          variables added): 7 ambiguous -> resolved (`Describe(&b, err)`,
          `store.Open()`, `billing.Log(…)`); 3 unresolved -> resolved (calls
          of func-typed fields through a proven receiver type, as
          `p.Jitter(d)`); 1 resolved -> unresolved (`cmd.run(…)`, a
          func-typed field of an unproven receiver that matched the function
          `main.run`); none newly ambiguous. Dead code 122 -> 122.
    - [x] Python module values and class attributes: each name a
          module-level assignment binds first is a `variable` (`constant`
          when annotated `Final`); a class-body assignment or annotation is a
          `field` of the class (dataclass and `Enum` members too). `_`,
          attribute and subscript targets declare nothing, and `self.x` is
          still no symbol. So that these names don't widen call matches: a
          call, decorator or base naming something the file neither imports
          nor binds targets this file; a class attribute hides an import only
          in its class body; `from pkg.util import x` carries `util` as
          module evidence; `v.m()` on a local or a non-name value of unstated
          type must be a method. Tests: `tests/module_values.rs`,
          `tests/name_evidence.rs`. Corpus: 400 -> 570 symbols, relations
          unchanged. Repository snapshot (Python, 7323 relations; 140 fields
          and 204 variables added): 46 unresolved -> resolved and 10
          ambiguous -> resolved, all correct but one (`DEFAULT_LOAN_DAYS`,
          `CSV_CONFIG` through imports, `toml_reader` in its own file; the
          wrong one is `library/cli.py` `int(…)`, in a `from … import *` file,
          now matching a fixture variable `int`); 17 resolved or ambiguous ->
          unresolved, every one a wrong match before (`path.stat()` to
          `agent_bench.stat`, Django's `render` to `Report.render`); 121
          resolved -> ambiguous, all `v.m()` method calls, 62 of them to a
          method in another directory (`dict.values()` to
          `Repository.values`, `open(…).close()`), 59 in the same directory
          (`ctx.echo(…)` to `Context.echo`). Dead code 122 -> 122.
    - [x] Python module builtin literals: `X.m()` on a name the file binds
          once, at module level, to a builtin literal (`TABLE = {}`), that no
          import, `import *` or `def`/`class` can bind, is `external`, as on
          a local `rows = []`. Test: `tests/name_evidence.rs`. Repository
          snapshot (Python, 7323 relations): 8 unresolved -> external and 9
          ambiguous -> external (`STACK_CONFIG.keys()`, `SCENARIOS.get(…)`;
          each receiver checked as a module dict literal), nothing else
          changed. Dead code 122 -> 122.
    - [x] Python `self.x` fields (#412, ADR-036): the first `self.x = …` in
          a method is a `field` of the class. `RelationTarget::not_field`
          (migration 17) lets a parser rule out a field as the target;
          Python sets it on `v.m()` and `@v.m` whose receiver class is not
          proven, unless `v` is the method's own `self`. Tests:
          `tests/self_fields.rs`, `tests/name_evidence.rs`,
          `schema::tests::not_field_migration_rules_out_only_fields`.
          Repository snapshot (Python, 7323 relations): 117 new fields, 3
          unresolved -> resolved, each a `self.f()` call on a field holding a
          callable (`self._key(item)`, `self._build()`); the 6 wrong matches
          of the draft (`CSV_CONFIG.items()` to `Page.items`,
          `@admin.action` to `AuditRecord.action`) stay unresolved. Other
          languages unchanged. Dead code 122 -> 122.
    - [x] Go embedding: an embedded struct field (`Base`, `*Base`,
          `pkg.Base`, `List[T]`) and an interface element naming one type
          (`Reader`, `io.Writer`, which must be an interface) are an
          `extends` of the embedding type; a union or `~T` element gives
          none. A bare name is qualified by the package, `pkg.T` is external
          through a standard package and qualified by `pkg` through an
          unaliased repository import. Test: `tests/embedding.rs`. Corpus:
          835 -> 838 relations. Repository snapshot (Go, 861 -> 864
          relations): 3 new `extends`, all resolved to the embedded type
          (`Service` -> `Store`, `Transaction` -> `Meta`, `Meta` -> `Tags`).
          Dead code 122 -> 122.
    - [x] Go composite literals: `T{…}`, `&T{…}`, `pkg.T{…}` and
          `T[int]{…}` reference `T`, with an embedded type's evidence; a
          slice, map or elided-type literal references nothing. Test:
          `tests/composite_literals.rs`. Corpus: 838 -> 917 relations.
          Repository snapshot (Go, 864 -> 946 relations): 77 new
          `references`, each resolved to a struct of the package its
          qualifier names, and 5 external (`time.Time{}`). Dead code
          122 -> 122.
    - [x] C++ constant globals: at namespace scope, `const`/`constexpr`
          on a name or an array (`const int A`, `const int C[]`), a pointer
          that is itself `const` (`char* const q`) and a `constexpr` pointer
          are a `constant`; a pointer to const, a reference and `constinit`
          stay a `variable`, a class member a `field`. Test:
          `tests/const_globals.rs`. Corpus: 4 `variable` -> `constant`
          (`kMaxNameLength`, `kSkuPrefix`, `kSchemaVersion`, `kSep`).
          Repository snapshot (C/C++, 6059 symbols, 542 relations): the same
          4 kinds change, nothing else. Dead code 122 -> 122.
    - [x] Lua instance fields: the first `self.x = …` in a colon method
          `function T:m()` (or a closure inside it) is a `field` of `T`,
          unless `T` already declares `x`. An explicit `self` parameter, a
          `local self = setmetatable({}, T)` constructor or a plain function
          declares none: `self`'s table is not known there. Test:
          `tests/self_fields.rs`. Corpus: 410 -> 417 symbols, relations
          unchanged. Repository snapshot: Lua fields 28 -> 35 (the corpus is
          the only Lua), relations unchanged, dead code 122 -> 122.
    - [x] PHP global variables: the first `$x = …` at file scope (in a
          namespace or a top-level block too) is a `variable` named without
          `$`, with no parent, as PowerShell's are; a function, method,
          closure or class body has its own scope, and a superglobal, `$$x`,
          a property, an element or a destructuring pattern declares none. So
          that these names don't widen call matches, a bare `f()` targets a
          function only (a variable is called as `$f()`). Test:
          `tests/global_variables.rs`. Corpus: 418 -> 420 symbols (`$clock`,
          `$out` in `console.php`), relations unchanged. Repository snapshot
          (PHP, 1184 relations): 3 variables added; 7 bare `count(…)` calls
          inside a namespace, ambiguous between the methods
          `InMemoryRepository.count` and `Collection.count`, are now
          unresolved (each is PHP's `count`, which no function of the
          repository declares); nothing else changed. Dead code 123 -> 123.
    - [x] TS enum members: each member of `enum`, `const enum` or `declare
          enum` (`A`, `B = 2`, `'c-d' = 3`) is a `field` of the enum, the
          property `E.A` the enum object has at run time (as a Java, Kotlin
          or PHP enum constant is a field); a computed or numeric name
          declares none, and a call in a member's value is a call from the
          enum. A type annotation can't name a field or an enum member, so
          every TS type reference now sets `not_field`; without it, `: Paid`
          became ambiguous between `interface Paid` and the corpus member
          `State.Paid`. Test: `tests/enum_members.rs`. Corpus: 307 -> 318
          symbols, relations unchanged. Repository snapshot (JS/TS, 24397
          relations): 11 fields added, no relation changed. Dead code
          123 -> 123.
    - [x] TS interface members: a method signature (`total(): number`) is
          a `method` and a property signature (`readonly id: string`) a
          `field` of the interface, each with its signature; overloads give
          the first one only, and a call, construct or index signature, a
          computed or numeric name or a member of a nested object type
          declares none. A value typed by an interface (`cart: Cart`) now
          reaches the signature in `cart.total()`. So that the new names
          don't widen name matches: a bare `f()` and a call on a name that
          is no local (`performance.now()`) or on `this` outside a class set
          `not_field`, and a bare call to a local (`onSaved()` on a
          destructured parameter) can reach a function of the file only,
          never a method. Test: `tests/interface_members.rs`. Corpus:
          318 -> 402 symbols, relations unchanged in number. Repository
          snapshot (JS/TS, 24397 relations): 225 fields and 20 methods
          added; 32 ambiguous and 2 resolved calls, all `x.now()` or
          `Array.from` that had matched a corpus method, are now
          unresolved; 4 unresolved become resolved (`bridge.source`,
          `bridge.cancel`, `StockSource.levels`, `ExchangeRates.rate`, each
          on a value of the interface's type); 50 unresolved member calls
          on an untyped local (`bridge.save()` in the bundles and tests)
          become ambiguous with the interface method; a private `this.#e()`
          in `notes.js` resolves to its same-file method instead of being
          ambiguous with a field of `review.js`; 130 ambiguous calls keep
          their status with a changed candidate list. Dead code 123 -> 123.
    - [x] Python lambdas bound to a name: `f = lambda x: …` at module
          level is a `function`, in a class body a `method` (its first
          parameter the receiver), as a JS arrow bound to a `const` is; the
          signature is the binding up to the body (`f = lambda x`) and a
          call in the body is a call from it. A lambda among several targets,
          annotated, or inside a function declares nothing new. Test:
          `tests/lambda_functions.rs`. Corpus unchanged. Repository snapshot
          (Python, 1529 symbols, 7323 relations): no change, since no file
          binds a lambda to a name. Dead code 123 -> 123.
    - [x] Python enums: a class that subclasses a base of the standard
          `enum` module (`Enum`, `IntEnum`, `StrEnum`, `Flag`, `IntFlag`) is
          an `enum`, its assigned members staying `field`s of it. The base
          counts only when the file proves it is the standard one: a bare
          name only `from enum import …` binds, or the base on a module alias
          of `enum`; a base named `Enum` from a relative or other import, a
          local class or a later assignment leaves a `class`. Test:
          `tests/enum_classes.rs`. Corpus: `Genre` and `MembershipTier`
          become `enum`, nothing else changes. Repository snapshot (Python,
          7323 relations): the same 2 kinds change, no relation changes.
          Dead code 123 -> 123.
    - [x] Kotlin enum kind: an `enum class` is an `enum` (an `enum` class
          modifier or an enum body, so the annotated one-entry form the
          grammar misreads counts too); entries stay `field`s and members
          keep their kinds. Test: `tests/enum_classes.rs`. Corpus:
          `Currency`, `Scope`, `State`, `VatCategory` become `enum`, nothing
          else changes. Repository snapshot (Kotlin, 806 relations): the
          same 4 kinds change, no relation changes. Dead code 123 -> 123.
    - [~] Go func values: a package-level `var f = func(…) {…}` (one
          name, no type, one function literal) is a `function` with the
          signature `var f = func(…) result`, and the literal's body is
          owned by it; `f := func() {}` inside a function still has no
          symbol. Test: `tests/func_values.rs`. Repository snapshot (Go, 946
          relations): no symbol or relation changes. Dead code 123 -> 123.
    - [x] PowerShell classes and enums: a `class` is a `class`, its
          properties `field`s and its methods and constructors `method`s
          (each owns its body's commands); an `enum` is an `enum` with its
          members as `field`s. A command's target must be a `function`, so
          a bare `Reset` never reaches a method `Reset`. This reverses the
          old "By design" exclusion in the crate docs. Class bases and
          `[Type]` literal uses are still no relations. Test:
          `tests/classes.rs`. Corpus: 82 -> 103 symbols, 261 relations
          unchanged. Repository snapshot (PowerShell, 817 relations,
          re-measured with `init --force`): 499 -> 520 symbols (the
          corpus copy's 2 classes, 1 enum, 12 fields, 6 methods), no
          relation changes. Dead code 123 -> 123.
    - [x] PowerShell constructor calls: `[Location]::new(…)` and
          `New-Object Location` (`-TypeName` too) give `calls Location`,
          and only a `class` can be the target. A dotted or generic type
          (.NET's), another static member (`[math]::Abs`) and `New-Object
          -ComObject` give none. Test: `tests/constructor_calls.rs`.
          Corpus: 261 -> 266 relations, the 5 constructions of `Location`
          and `CycleCount`. Repository snapshot (PowerShell, re-measured
          with `init --force`): 817 -> 822 relations, the corpus copy's 5
          constructions, all resolved. Dead code 123 -> 123.
    - [x] PowerShell class bases: as in C#, a class's first base is
          `extends` (its target must be a `class`) unless it is named like
          `IFoo`, and the rest are `implements` (an `interface` target; a
          script declares none, so they stay unresolved). Test:
          `tests/class_bases.rs`. Corpus: no bases, no changes. Repository
          snapshot (PowerShell, 817 relations): no changes. Dead code
          123 -> 123.
    - [x] Kotlin class-body ownership: a primary constructor's parameter
          defaults, property initializers, `init` blocks and secondary
          constructor bodies run as the class is built, so their calls
          come from the class, no longer from the file module. Test:
          `tests/class_body_calls.rs`. Corpus: 34 calls change owner, from
          the file to their class, with the same targets and resolutions.
          Repository snapshot (Kotlin, 806 relations, re-measured with
          `init --force`): 53 calls change owner, no target or resolution
          changes. Dead code 123 -> 123.
    - [!] Kotlin constructors as symbols: shelved. A constructor `method`
          named after its class adds a second candidate to every bare
          `B(…)` call, so 53 corpus calls go from resolved to ambiguous.
          Java (31) and C# (26) corpus calls already have that ambiguity.
          A fix needs the index to tell a constructor from its class (a
          view change, which needs an issue first). Decision pending.
    - [x] C# record positional parameters: each `parameter` of a
          `record P(int X)` is a public `field` `X` of the record (the
          generated property), with `int X` as signature, unless the body
          declares a member `X`. A class or struct primary-constructor
          parameter stays no member. A field is never the target of
          `new T()`. Test: `tests/record_parameters.rs`. Corpus: 454 -> 476
          symbols; 5 `new Price(…)` calls ambiguous -> resolved, 3 ambiguous
          calls lose a field candidate. Repository snapshot (C#, 692
          relations, re-measured with `init --force`): 496 -> 518 symbols,
          the same 5 calls ambiguous -> resolved and 3 calls that lose a
          field candidate. Dead code 123 -> 123.
    - [x] Python nested functions: a `def` inside a function or method is a
          `function` whose parent is that function or method, as a Rust
          nested fn is. A `def` in a class body stays a `method` of the
          class. Test: `tests/nested_functions.rs`. Corpus: 1506 relations,
          no resolution changes (17 candidates now name their enclosing
          function). Repository snapshot (Python, 7323 relations, with
          `init --force`): no resolution or owner changes; 46 candidates
          now name their enclosing function. Dead code 123 -> 123.
    - [x] JS/TS nested functions: a `function` declaration or a function
          value (`const show = () => …`) inside a function or method body
          is a `function` whose parent is that function or method. A
          bare call to it keeps this file as evidence: a function's
          parent is no type. Test: `tests/nested_functions.rs`. Corpus:
          1005 relations, no resolution changes (20 candidates now name
          their enclosing function). Repository snapshot (JS/TS, 24397
          relations, with `init --force`): no resolution or owner changes;
          582 resolved and 2222 ambiguous relations now name an enclosing
          function among their candidates. Dead code 123 -> 123.
    - [x] Rust impl owner: `impl Tr for T` is now `T implements Tr` when
          the file declares one struct, union or enum `T` (before or after
          the `impl`, generic arguments and path ignored). A `T` from
          another file, or two of that name here, leaves the file module as
          owner. Test: `tests/impl_owner.rs`. Corpus: 19 `implements`
          change owner, from the module to their type. Repository snapshot
          (Rust, 96923 relations, with `init --force`): 136 `implements`
          change owner (126 to a struct, 10 to an enum), no target or
          resolution changes. Dead code 123 -> 123.
    - [x] PowerShell type literals: a `[T]` other than `[T]::new()`'s
          (`[StorageZone]::Frozen`, `[Location] $x`, `[OutputType([T])]`,
          a method's return and parameter types) is `references T` when
          this file declares one class or enum `T` (any case), pinned to
          this file and to that kind, so a method named like the type is
          never a candidate. A script class is visible in another file
          only through `using module`, so other names (`[string]`,
          `[IO.File]`) give no relation. Test: `tests/type_literals.rs`.
          Corpus: 266 -> 274 relations. Repository snapshot (PowerShell,
          with `init --force`): 822 -> 830 relations, all 8 resolved.
          Dead code 123 -> 123.
    - [x] C# static-class qualifier: `Helpers.Format(x)`, with a
          capitalized qualifier that is no local or field, also
          `references Helpers`, whose target must be a `class` (`static`
          is class-only), so the class is no longer dead code for that use
          and a method named like it is never the target. A struct,
          delegate or BCL qualifier stays unresolved; none is marked
          `external`. Test: `tests/static_qualifier.rs`. Corpus: 72 new
          references, 34 resolved, 38 unresolved (`Math`, `Console`,
          `Task`, the struct `Sku`, …). Repository snapshot (C#, with
          `init --force`): 692 -> 767 relations, 36 resolved, 39
          unresolved. Dead code 123 -> 123.
    - Measurement note: until 2026-11-13 a repository snapshot taken
          with `init` (no `--force`) kept the parse of unchanged files, so
          the C#, PowerShell and Kotlin entries above first reported no
          changes. They are re-measured; earlier entries were not.
    - [x] JS/TS module-level bindings: `const X = 1` is a `constant`,
          `let`/`var` a `variable` (exported or not, each name of
          `let a = 0, b`), with no parent and the declaration without its
          value as signature. A function value stays a `function`; a
          destructuring pattern and a `require(…)` binding (an import) declare
          nothing; nor does a binding inside a function or block. A bare call
          to such a binding targets this file, as one to a function does.
          Test: `tests/top_level_variables.rs`. Corpus: 295 -> 307 symbols.
          Repository snapshot (JS/TS, 24218 relations, 215 constants and 585
          variables added): 68 unresolved -> resolved (named imports such as
          `{ store } from "./core.js"` and same-file calls such as
          `badgeVariants(…)`); 1292 resolved -> ambiguous, of which 1255 are
          within one minified bundle (a top-level `var e` beside a nested
          `function e`), 22 bundle calls to a minifier-reused local that
          matched another bundle by name, and 15 named imports of a binding
          named like its file (`{ drafts } from "./drafts"`, now ambiguous
          with the module symbol it matched before); 57 unresolved ->
          ambiguous, 30 ambiguous -> resolved. Dead code 122 -> 122
          (constants and variables are not dead-code candidates).
    - [x] JS/TS call and import evidence: a bare call `f()` (and a
          same-file function used as a value) with no import or global
          evidence targets this file when nothing around it binds `f` and the
          file declares `f` outside any type, or when an enclosing local
          binds `f` and the file declares no `f` (the local has no symbol, so
          the call stays unresolved). Each ES import binding names the module
          its specifier ends in, as a call through it already did. Test:
          `tests/same_file_calls.rs`. Corpus unchanged. Repository snapshot
          (JS/TS, 24218 relations): 2219 ambiguous -> resolved, 329
          ambiguous -> unresolved, 73 resolved -> unresolved (calls to
          locals that matched another file's name, among them
          `const { duplicateBrief } = await ai()` whose name match was right
          by coincidence, with no evidence); none newly ambiguous or newly
          resolved from unresolved. Dead code 122 -> 122.
    - [x] TS constructor parameter properties: `constructor(private
          readonly store: Store, public size = 0, protected owner?: User)`
          gives a `field` of the class per parameter with an accessibility
          keyword or `readonly`, at the parameter, with its visibility and
          `readonly`; a plain parameter declares nothing. `this.f()` is now
          qualified by the class when its body declares `f` as a field or
          parameter property, as it already was for a method. Test:
          `tests/parameter_properties.rs`. Corpus: 275 -> 295 symbols.
          Repository snapshot (JS/TS, 24218 relations): 3 `this.now()` calls
          now resolve to their own class's field; 1 unresolved call
          (`Array.from` in a minified bundle that binds `Array`) and 2
          resolved bare calls (`to(e)` in `review-dist/review.js`) now also
          match a new corpus field by name, the existing name-only fallback
          for calls with no target (#395); 33 unresolved calls (mostly
          `performance.now()` and bare `now()`) become ambiguous the same
          way. Dead code 122 -> 122.
    - [x] JS/TS decorators: `@sealed`, `@ns.track` and `@Component({…})`
          give `calls` from the decorated class, field or method (a
          parameter's from its method), since the runtime calls the
          decorator with what it decorates. TypeScript puts a method's
          decorators before it in the class body and JavaScript inside it;
          both are the method's. The callee helpers are shared with
          `call_expression`. Test: `tests/decorators.rs`. Corpus and
          repository snapshot unchanged (no decorator): JS/TS relations
          24218, dead code 122.
    - [x] Kotlin file-level declarations: `typealias` is a `type_alias`,
          `const val` a `constant`, any other `val`/`var` a `variable` (each
          name of `val (a, b) = …` too), all with no parent; an extension
          property `val T.p` is a `field` of `T`, as an extension `fun` is a
          method of `T`. Test: `tests/top_level_declarations.rs`. Corpus:
          symbols 484 → 488 (`Handler`, `Rate`, `NO_DISCOUNT`,
          `Money::isPositive`). Repository snapshot: the same 4 Kotlin
          symbols, relations unchanged at 806, dead code at 122.
    - [x] C++ `#define`: an object-like macro is a `constant` and a
          function-like one a `function` (as Rust's `macro_rules!`), with no
          parent, since the preprocessor knows no namespace or class. An
          include guard (`#ifndef X` then `#define X` with no value) is no
          symbol. Test: `tests/macros.rs`. Corpus unchanged (no macro).
          Repository snapshot: C++ constants 16 → 17 (`CONFIG` in the
          depgraph fixture), relations unchanged at 542, dead code at 122.
    - [x] Kotlin qualified supertypes: `class C : store.Basket<T>()` and
          `: store.Sized` gave `extends`/`implements store` (the package),
          and an `override` looked for the member in `store`. Every
          supertype now takes the type's last segment, as the constructor
          call already did. The repository snapshot has no qualified
          supertype: Kotlin relations unchanged at 806, dead code at 122.
    - [x] C++ mem-initializers: one that names a base of the class
          (`B() : A(1)`, `ns::A<int>{2}`) gives `calls A` from the
          constructor, and one that names the class itself (a delegating
          constructor) gives `calls B`, both restricted to a constructor
          declared in that class; `std::` bases are external. The bases come
          from the class definition in the same file, so an out-of-line
          constructor of a class defined in another file still gives none
          (checklist line stays `[~]`). Corpus snapshot: 4 new relations.
          Repository snapshot: C++ relations 538 → 542 (2 resolved to
          `StorageError::StorageError`, 1 ambiguous between two `Product`
          constructors, 1 `std::runtime_error` unresolved because the file
          opens `namespace std`); dead code unchanged at 122.
    - [x] Kotlin `: Base(4)` in a class or object header and
          `constructor(…) : super(…)` give `calls Base` from the class (a
          Kotlin constructor is no symbol), restricted to a class; `super(…)`
          only when the header names one supertype without `()`, and to
          `Any`, external, when it names none. `this(…)` gives nothing (the
          class calling itself). On the snapshot: 14 new Kotlin relations
          (10 resolved to sealed-class parents, 4 unresolved stdlib
          exceptions); dead code unchanged at 122.
    - [x] C# `: this(…)` and `: base(…)` give `calls`, restricted to a
          constructor declared in the enclosing type or in the base list's
          first entry (an interface there has none, so nothing matches);
          `: base()` with no base list (`object`) is external. On the
          snapshot: 18 new C# relations (12 resolved, 5 ambiguous between
          overloaded constructors, `Exception` unresolved); dead code
          unchanged at 122.
    - [x] Java `this(…)` and `super(…)` give `calls`, restricted to a
          constructor (a `method`) declared in the enclosing class or in
          the class `extends` names; `super()` with no `extends`
          (`Object`) or on a JDK-imported superclass is external. On the
          snapshot: 9 new Java relations (`Orders` → `Jdbc.Jdbc`
          resolved; 8 `java.lang` exceptions unresolved, since an
          unimported `java.lang` name is not proven external); dead code
          unchanged at 122.
    - [x] JS/TS `super(…)` gives `calls Base` for the base a class
          declaration names, with the same global and import evidence as
          `new Base()`. On the snapshot: 7 new JS/TS relations (6 `Error`
          external, `Observable` resolved); dead code unchanged at 122.
    - [x] Java and Kotlin annotations: `@Audited` on a declaration gives
          `references Audited` from the annotated symbol, restricted to an
          annotation type (Java `interface`, Kotlin `class`), as C#
          attributes already did. On the snapshot: Java relations 891 →
          959 (14 resolved, 54 unresolved, 43 of them `@Override`, which
          nothing in the repository declares), Kotlin 789 → 792
          (3 unresolved). Dead code unchanged at 122.
    - [x] Kotlin: `@X annotation class D` and `@X enum class E { A }` at
          file level had no symbol, because tree-sitter-kotlin-ng 1.1.0
          parses them as an annotated infix expression without a syntax
          error. The parser reparses with those annotations blanked (same
          offsets), so the class is indexed; its own annotations give no
          relation. Corpus: `PricingDsl` indexed (symbols 483 → 484).
          Snapshot: both `@PricingDsl` references unresolved → resolved;
          dead code unchanged at 122.
    - [x] C++ `new T(…)`, `new T`, `new ns::T{}` and `new T<U>(…)` give
          `calls T`, as `T()` does; `new std::T` is external and
          `new int[4]` gives nothing. No change on the snapshot (538 C++
          relations before and after; dead code 122).
    - [x] Kotlin supertypes: `interface B : A` gives `extends` (it gave
          `implements`) restricted to an `interface`, and `: Base(4)`
          targets a `class`, so a factory function `fun Base()` is not a
          candidate. A class's bare supertype keeps `implements` with no
          kind, since it can be the superclass when only secondary
          constructors call `super(…)`. No change on the snapshot (27
          Kotlin supertype relations before and after; none from an
          interface).
    - [x] PowerShell: an assignment inside a `class` body (`$this.Radius = $r`,
          a method's `$diff = …`) and a member or element assignment
          (`$cfg.Name = …`, `$list[0] = …`) no longer add a script
          `variable`. Corpus: symbols 87 → 82, all five removed ones inside
          class methods; relations unchanged at 261.
    - [x] Java supertypes: `interface B extends A` gives an `extends`
          relation, and every supertype relation targets a type (`class` for
          a class's `extends`, `interface` otherwise), so a same-named
          constructor or method is no longer a candidate. On the snapshot:
          relations 890 → 891, `Repository.java:224 Jdbc` ambiguous →
          resolved to the class, dead code unchanged at 122. Python, Go,
          Java and Rust cells re-probed; the others are still to do.
- [ ] Inherited methods, using supertype data.
- [~] Members of an anonymous class or an enum constant's body are recorded
      under the enclosing type (Java, Kotlin), which leaves their overrides
      ambiguous (see D8).
  - [x] Enum constant bodies: a method of `NEW { … }` (Java) or of an enum
        entry's body (Kotlin) has the constant as its parent, the way javac
        compiles each body to its own subclass. The constant is a field of
        the enum, and the enum stays the body's supertype. On this
        repository: Java overrides 14 → 2 ambiguous and 14 → 26 resolved,
        Kotlin 5 → 4 ambiguous and 28 → 29 resolved, each new resolution to
        the enum's abstract or open method. Relation and symbol counts and
        dead code are unchanged.
  - [ ] Anonymous classes (`new Listener() { … }`, `object : T { … }`). They
        have no name in the source, so a parent for their members is a
        naming decision (javac's `Outer$1`, the enclosing method, or no
        parent). Waiting for that decision.
- [x] Kotlin: the grammar reports a syntax error for a one-line body whose
      last member has no terminator (`interface Shape { fun area(): Double }`,
      `ADD { override fun apply() = 1 };`). The parser now reparses
      incrementally with a `;` in the blank before each such `}`, at the same
      offsets, and keeps only a clean tree; a real syntax error is still
      rejected (`tests/one_line_bodies.rs`). This repository's 14 Kotlin files
      were already clean, so its index is unchanged. Each reparse pass costs
      one parse, and a 12 KB file of 400 one-line classes takes 32 ms against
      3 ms for its multi-line spelling (release build, 20 runs).
- [ ] C++: a member declared in the class and defined out of line
      (`Product::describe`) is two symbols, so a relation to it, an
      `overrides` or a qualified call, has two candidates and stays
      ambiguous. Merging the pair is a resolution change for every C++
      relation (see D8).
- [x] Java: a field initializer is walked like an initializer block, so
      the calls and anonymous classes in `Runnable r = new Runnable() { … };`
      are indexed. Corpus: +16 calls (`new HashMap<>()`,
      `Logger.getLogger(…)`), symbols unchanged; this repository: the same
      16 Java calls, dead code unchanged.
- [ ] Relations: inheritance, implementations and overrides (3.3, depends on
      D8).
- [ ] Language-specific features (3.4), each checked against the real
      grammar.
- [ ] Positive and negative fixtures for each language (3.5).

## Block 4: quality, fixtures and performance

- [~] Index pollution from `duplicate-app`, `omni-app`, `portfolio-3d-app`
      and other fixture projects.
  - [x] Rust: an imported crate's item is never resolved to a same-named
        fixture item (`crates/mct-eval` `#[derive(Serialize)]` →
        `mct-lang-rust/tests/fixtures/depgraph/src/other.rs`); see D6.
  - [!] Production relations whose only candidate is in a fixture, in the
        other languages. Most are a function local to another function
        (JS/TS `matchMedia` → a `const` inside `tests/dashboard/harness.mjs`'s
        setup, `vi` → a nested function of the minified `notes-dist/notes.js`):
        155 such resolutions on this repository (154 JS/TS, 1 Python). The
        fix is a predicate in the `relation_candidates` view (a migration):
        #376. Waiting for the maintainer.
- [x] Path filter with a trailing slash. A scope path is normalized once in
      `mct-index` (`queries::scope_path`): `src/`, `./src`, `./src/` and
      `src//` are `src`, and `.`/`./` is the whole project. Before, any of
      them matched nothing (`LIKE 'src//%'`) in `find_symbol`,
      `find_references`/`find_calls`/`find_callers`, `list_symbols`,
      `get_project_overview` and `find_dead_code`; `find_duplicate_code`
      only trimmed the trailing slash.
- [ ] Inline module resolution.
- [~] Reading long functions through the context tools.
  - [x] `build_context_pack` caps a body at 200 lines (`source_lines`), and
        its footer said "raise `source_lines`" even at that cap. For a body
        past the cap the footer now names the hidden range (`… 60 more
        line(s), L201-L260: … read them from the file`), and while
        `source_lines` is below 200 it says how far raising it can go. On
        this repository, `mct-lang-rust` `visit_node` (L291-L827) ends with
        `L491-L827`. Test: `context_pack.rs`
        `a_body_longer_than_the_source_lines_cap_names_the_lines_to_read`.
  - [ ] Paging past the cap through the tool itself (`source_offset`):
        an MCP signature change, so issue #371 first.
- [ ] False positives in impact analysis and dead code (D7 #331 covers part
      of this).
  - [x] Python: a function or class passed by name as an argument
        (`safely(score_verdicts, …)`, `sorted(rows, key=rank)`,
        `isinstance(exc, NotFoundError)`) is a `references` relation, recorded
        only with the evidence a bare call to the name gets (this file's
        `def`/`class`, an import, a builtin); a parameter or local records
        nothing. This repository: 165 new Python relations, 123 external
        (builtins such as `str`, `int`, `list`), 24 resolved, all to the
        right definition (`urls.py`'s `checkout` → `views.py`, not
        `LendingService.checkout`), and 18 unresolved (imported constants
        and stdlib classes, which have no indexed candidate). Dead code
        124 → 122 (`score_verdicts`, `score_subgroups`). Test:
        `mct-lang-python/tests/function_values.rs`.
  - [x] Python, the other value positions: an element of a tuple, list,
        set or dict value (`isinstance(e, (A, B))`, `{"k": rank}`), the right
        side of an assignment (`model = BookRecord`, `handler = rank`) and a
        returned name (`return wrapper`), with the same evidence rule. This
        repository: 44 new Python relations, 27 resolved (all to the right
        definition: Django `model = BookRecord` → `lending/models.py`,
        `return register` → its nested `def`), 11 external, 4 ambiguous (two
        decorators of `library/errors.py` each return a nested `wrapper`, and
        `path=` evidence can't tell the two apart), 2 unresolved. Dead code
        unchanged (122).
- [x] Consistent CLI and MCP output. The three CLI reports with an MCP
      twin read the same index and agree, now pinned by
      `crates/mct-cli/tests/cli_mcp_parity.rs`:
      - `dead-code` and `find_dead_code`: the same `(path, name, line)` set,
        whole and scoped (`src/util`, `src/util/`, `./src/util`);
      - `duplicates` and `find_duplicate_code`: the same members in the
        fixture's duplicate group, and matching group, candidate and
        below-threshold counts;
      - `status` and `get_indexing_status`: the same text, apart from the
        tool's `Last verified unchanged …` line for its own process.
      On this repository: 124 dead-code candidates from both (the tool pages
      them at its response budget, `truncated: true`, `next_offset: 62`), and
      127 duplicate groups (20 REAL, 107 SUSPICIOUS) from both. One
      difference is deliberate: the CLI applies review decisions only with
      `--reviewed`, and the tool always does (`crates/mct-cli/src/reviewed.rs`).
- [~] Evaluation suite covering every language: precision, coverage,
      performance and tokens.
  - [x] `suite-resolution.json` has labelled cases for C#, JavaScript and
        Bash, with separate cases for a repository target and an external one
        per language. Truth was run with `dotnet run`, Node and `bash`.
        JavaScript is 1.00. C# is
        0.50: `_notifier.Notify` reaches `INotifier.Notify`, and `id.Trim()`
        on a `string` is unresolved, not external. Bash is 0.00:
        `log_info` after `source "$(dirname "$0")/../lib/log.sh"` is
        ambiguous between `log.sh` and `other.sh`, and the builtin `printf`
        is unresolved. Nothing is wrong. These language-level scores summarize
        two separately baselined cases each, so an improvement in one cannot
        hide a regression in the other. Case-average accuracy changes when cases
        are split; compare label coverage across versions instead.
        None of the three misses is provable from one file, so none is
        forced:
        - `printf` can be redefined by a function in any script that
          sources this one or that it sources, so "builtin" rests on the
          name alone;
        - with `log.sh` and `other.sh` both defining `log_info`, only the
          `source` graph tells them apart. That is index-level resolution
          through `Imports`, a cross-cutting change that needs an issue
          first;
        - `id.Trim()` on a `string` reaches `System.String.Trim` only when an
          overload applies; otherwise C# falls back to an extension method,
          which a repository may declare.
  - [ ] Kotlin, Go, PHP and PowerShell: no `kotlinc`, `go`, `php` or `pwsh`
        here to run their truth.

## Block 5: deliveries

- [x] Changes split into stacked PRs (#324 → #345), each with tests,
      measurements and a merge-danger note.
- [ ] Rebase the stack as PRs merge. No PR is merged without authorization.

## Done when

The criteria from the plan:

- [ ] D2–D9 are fixed, or their unavoidable limits are justified.
- [ ] Resolution is backed by positive and negative tests.
- [x] The enriched model is implemented and verified.
- [x] The SQLite migrations work.
- [ ] Every supported parser has reviewed, documented coverage.
- [ ] Searches and the MCP context use the new metadata.
- [ ] The relevant tests pass.
- [ ] No unjustified performance or token regression.
- [x] The changes are organized in reviewable PRs.
- [ ] The documentation reflects the real state of the project.
