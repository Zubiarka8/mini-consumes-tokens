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
  - [!] Whether Kotlin (#327) and C# (#328) adopt Go's rule for a receiver
        with no stated type. Today that loses 12 (Kotlin) and 25 (C#) right
        resolutions. Waiting for the maintainer.
- [~] **D6, imports and aliases.** The imported module is carried as call
      evidence for Python and JS/TS (#323). Aliases, globs, Rust inline
      modules and name collisions across the other languages are not
      checked yet.
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
- [x] **Evidence on `RelationHit`.** Additive MCP field: #325.
- [~] **Block 1 validation.** `crates/mct-eval/suite-resolution.json` has
      labelled cases. D8 has 4 (Java, Python, TypeScript; each against a
      same-named method in an unrelated class), all correct. D9 has 2 for
      Lua (`setmetatable`, `string.format` next to another file's local
      aliases), both correct; before the fix, 1 wrong and 1 ambiguous. D9
      has 2 for Java (`items.add`, `List.of` under `import java.util.List`
      next to a same-package `shop.List`, checked with `javac` and
      `javap`), both correct; before the fix, 1 wrong and 1 missing. D9
      cases for the other languages wait on their fixes. D5 has 2 for
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
- [~] **2.5 Benchmarks.** SQLite size and coverage measured for each parser
      PR (in each PR body). Init time is within noise on this machine and is
      not claimed. Query latency, peak memory and response tokens before and
      after are not measured yet.

## Block 3: phase 3, all languages

- [x] Signatures, parameters, types, visibility and modifiers for every
      language with declarations (see 2.3).
- [ ] Updated symbol-coverage matrix, then work through the gaps by language
      group (3.1).
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

- [ ] Index pollution from `duplicate-app`, `omni-app`, `portfolio-3d-app`
      and other fixture projects.
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
- [x] Consistent CLI and MCP output. The three CLI reports with an MCP
      twin read the same index and agree, now pinned by
      `crates/mct-cli/tests/cli_mcp_parity.rs`:
      - `dead-code` and `find_dead_code`: the same `(path, name, line)` set,
        whole and scoped (`src/util`, `src/util/`, `./src/util`);
      - `duplicates` and `find_duplicate_code`: the same group,
        candidate and below-threshold counts;
      - `status` and `get_indexing_status`: the same text, apart from the
        tool's `Last verified unchanged …` line for its own process.
      On this repository: 124 dead-code candidates from both (the tool pages
      them at its response budget, `truncated: true`, `next_offset: 62`), and
      127 duplicate groups (20 REAL, 107 SUSPICIOUS) from both. One
      difference is deliberate: the CLI applies review decisions only with
      `--reviewed`, and the tool always does (`crates/mct-cli/src/reviewed.rs`).
- [ ] Evaluation suite covering every language: precision, coverage,
      performance and tokens.

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
