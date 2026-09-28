# Issue #74 — progreso del corpus largo por lenguaje

Seguimiento de [#74](https://github.com/Zubiarka8/mini-consumes-tokens/issues/74):
cada crate `mct-lang-*` necesita `tests/corpus/` con ≥ 5 ficheros de 300–600
líneas que se referencian entre sí, `expected.snap`, un fichero grande en
`malformed/` y `tests/corpus.rs` (harness compartido `mct-corpus` + tests
propios del lenguaje). Ver `crates/mct-corpus/src/lib.rs`.

**Mantener actualizado hasta que el issue se cierre**: cada PR de corpus
cambia la fila de su lenguaje en el mismo PR (estado, PR, cifras, bugs), y
al mergear se pasa a **Hecho**. Cuando no quede ninguna fila pendiente, se
cierra #74 y se borra la regla correspondiente de `CLAUDE.md`.

Las columnas de ficheros/líneas y símbolos/relaciones, y la línea de resumen,
las rellena `scripts/unix/corpus-report.sh <lenguaje> --update-progress`
(`scripts\windows\corpus-report.ps1` en Windows); estado, PR y bugs se
escriben a mano.

Estados: **Pendiente** → **En PR** → **Hecho** (mergeado en `main`).

| Lenguaje | Crate | Estado | PR | Ficheros / líneas | Símbolos / relaciones (cross-file) | Bugs del parser encontrados |
|---|---|---|---|---|---|---|
| Rust | `mct-lang-rust` | **Hecho** | #77 | 6 / 2.179 | 239 / 775 (218) | owner de tipos/consts asociados; `fn` anidada dentro de método tomada como método; fin del módulo +1 |
| Python | `mct-lang-python` | **Hecho** | #78 | 6 / 2.006 | 302 / 918 (223) | fin del módulo +1; clase anidada sin parent; llamadas en argumentos de decoradores perdidas; `type X = …` (PEP 695) sin símbolo |
| Bash | `mct-lang-bash` | **Hecho** | #80 | 5 / 1.539 | 176 / 585 (162) | fin del módulo +1; asignación prefijo (`LC_ALL=C cmd`) tomada como variable; `coproc NAME { … }` mal parseado por la gramática → #79 (test `#[ignore]`) |
| C/C++ | `mct-lang-cpp` | En PR | #83 | 5 / 1.568 | 423 / 493 (150) | fin del módulo +1; funciones/variables de namespace tomadas como métodos/campos; locales (`auto x = f()`, `T x(args)`) tomados como símbolos y sus llamadas perdidas; `ns::Class::m`/`Tmpl<T>::m` fuera de línea con parent distinto de la declaración; `class Outer::Inner {` con nombre cualificado; namespace anónimo como módulo sin nombre; `enum`/`union`/`using`/`typedef` sin símbolo; construcciones válidas rechazadas por la gramática → #82 |
| C# | `mct-lang-csharp` | **Hecho** | #85 | 6 / 2.048 | 454 / 588 (143) | fin del módulo +1; `namespace X;` (file-scoped), `record`, `enum`, `delegate`, eventos, operadores, indexers, destructores y funciones locales sin símbolo; bases genéricas con `<…>` en el target y `IFoo` primero tomado como extends; llamadas en `?.`, `F<T>()`, `new T()`, `: base(…)`, cuerpos `=>` de propiedades e inicializadores perdidas; `nameof` como llamada; atributos sin relación |
| CSS | `mct-lang-css` | **Hecho** | #86 | 6 / 2.194 | 617 / 12 (0) | fin del módulo +1; una regla terminaba en la línea de su selector, no en su `}`; prefijo de `@namespace` (`svg\|text`) tomado como elemento `svg`; átomos perdidos en `.a .b[attr]` (la gramática aplica `[attr]` a toda la cadena). Rechazados por tree-sitter-css 0.25 (fuera del corpus, #87): `@page :first`, `@import … layer()`/`supports()`, rangos de media query (`400px <= width`) |
| Go | `mct-lang-go` | Pendiente | | | | |
| HTML | `mct-lang-html` | Pendiente | | | | |
| Java | `mct-lang-java` | Pendiente | | | | |
| JavaScript/TypeScript | `mct-lang-js-ts` | Pendiente | | | | |
| Kotlin | `mct-lang-kotlin` | Pendiente | | | | |
| Lua | `mct-lang-lua` | Pendiente | | | | |
| Markdown | `mct-lang-md` | Pendiente | | | | |
| PHP | `mct-lang-php` | En PR | #88 | 6 / 2.463 | 418 / 803 (339) | fin del módulo +1; llamadas `?->` (nullsafe) perdidas; `new Foo()` sin relación; atributos `#[…]` sin relación; miembros de una clase anónima (`new class { … }`) indexados como métodos/campos sin parent; closure asignada a variable dentro de un método tomada como miembro de la clase; `new` en el valor por defecto de un parámetro promocionado perdido |
| PowerShell | `mct-lang-powershell` | Pendiente | | | | |
| XAML | `mct-lang-xaml` | Pendiente | | | | |
| XML | `mct-lang-xml` | Pendiente | | | | |

**Resumen: 5 hechos, 2 en PR, 10 pendientes (de 17).**

## Issues derivados

- #79 — `coproc NAME { … }` truncates the enclosing function (tree-sitter-bash 0.25). Abierto.
- #82 — valid C++ rejected as a syntax error by tree-sitter-cpp 0.23.4 (explicit instantiation, `using Ts::operator()...`, `using typename B<K>::V`). Abierto.
