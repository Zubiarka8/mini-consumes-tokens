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
- [x] **D4, variable scopes.** A function's variable or a Lua file-level
      `local` is a candidate only in its own file (migration 15): #324.
- [~] **D5, receiver types.** JS/TS #326, Kotlin #327, C# #328, Go #329.
      Rust, Java and Python have no PR yet.
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
- [~] **D8, `Overrides`** (issue #349). Built from inheritance,
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
    - Known limit: a method of an enum constant's body or of an anonymous
      class has the enclosing type as its parent, so when it overrides a
      method of that same type (`State.next`, `new Listener() { … }` inside
      `Listener`) every such body is a candidate and the relation stays
      ambiguous. All 14 ambiguous relations are this case.
  - [ ] Kotlin/C# `override`, C++ `override`, TS `override`, Python
        `@override`.
- [ ] **D9, external and stdlib symbols.** Rust prelude (#318) and Python/JS
      builtins (#319) are done. Host and stdlib globals for the other
      languages are not.
- [x] **Evidence on `RelationHit`.** Additive MCP field: #325.
- [~] **Block 1 validation.** `crates/mct-eval/suite-resolution.json` has
      labelled cases. Positive and negative cases for D8 and D9 are still
      missing.

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
- [ ] Members of an anonymous class or an enum constant's body are recorded
      under the enclosing type (Java, Kotlin), which leaves their overrides
      ambiguous (see D8).
- [ ] Java: a field initializer is not walked, so calls and anonymous
      classes in `Runnable r = new Runnable() { … };` are not indexed.
- [ ] Relations: inheritance, implementations and overrides (3.3, depends on
      D8).
- [ ] Language-specific features (3.4), each checked against the real
      grammar.
- [ ] Positive and negative fixtures for each language (3.5).

## Block 4: quality, fixtures and performance

- [ ] Index pollution from `duplicate-app`, `omni-app`, `portfolio-3d-app`
      and other fixture projects.
- [ ] Path filter with a trailing slash.
- [ ] Inline module resolution.
- [ ] Reading long functions through the context tools.
- [ ] False positives in impact analysis and dead code (D7 #331 covers part
      of this).
- [ ] Consistent CLI and MCP output.
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
