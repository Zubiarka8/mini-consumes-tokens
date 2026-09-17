# ROADMAP — mini-consumes-tokens (post-0.1.0)

> **Historical / point-in-time document**, frozen at commit `9ca9ab5` (2026-09-13, pre-0.2.0). Several candidates proposed below have since shipped (e.g. Kotlin, Markdown, `list_symbols`) — `CHANGELOG.md` is the current source of truth for what's actually released. Kept for planning history, not as a live status page.

Documento de análisis y planificación, no de implementación. No se ha escrito
código de producto para generar esto — es una lectura completa de
`checklist.md`, `CHANGELOG.md`, `README.md`, `RELEASING.md`, `CONTRIBUTING.md`
y el código de `ccm-core`/`ccm-index` (schema, indexer, queries) tal como
están en 0.1.0 (commit `9ca9ab5`, 2026-09-13).

Unidad de esfuerzo usada en todo el documento: **una sesión de este proyecto**
tal como se han venido completando (ejemplos conocidos: "sesión de Java+C#",
"sesión de C++/Go", "sesión de clippy enforcement"). Pequeño = una fracción de
sesión o una sesión corta y acotada. Mediano = una sesión completa,
comparable a "agregar un lenguaje nuevo". Grande = probablemente más de una
sesión, o una sesión con alto riesgo de desbordarse si no se acota de
antemano.

---

## Candidatos evaluados

### 1. Resolución de tipos vía LSP real

- **Qué es**: sustituir (o complementar) la resolución actual, que es 100%
  AST/heurística de nombre (`find_symbol`/`find_references`/`find_calls`
  hacen `WHERE name = ?1`, sin distinguir dos símbolos con el mismo nombre en
  scopes distintos — ver `ccm-index/src/queries.rs`), por resolución real de
  tipos usando un servidor de lenguaje (o una librería de resolución
  equivalente) por lenguaje.
- **Por qué importa**: hoy, en un repo real con dos clases distintas que
  tienen un método `run()`, `find_callers("run")` devuelve la unión de
  ambas sin distinguirlas — falso positivo silencioso. Esto empeora, no
  mejora, a medida que el repo crece (más símbolos → más colisiones de
  nombre), que es justo el escenario que un usuario con un monorepo grande
  va a golpear primero. También es el bloqueador explícito de
  `find_implementations` en Go (candidato #3) y mejoraría la precisión de
  `extends`/`implements` en C# (heurística de posición documentada como
  limitación aceptada) y C++ (mismo problema con `base_class_clause`).
- **Esfuerzo estimado**: **Grande**. No es "una sesión más": requiere elegir
  una estrategia por lenguaje (no hay un único protocolo que cubra los 8 —
  `rust-analyzer`, `pyright`/`pylsp`, `gopls`, `clangd`, etc., cada uno con
  su propio protocolo de arranque/vida útil de proceso), decidir si se
  invoca como subproceso persistente por lenguaje (coste de arranque y de
  mantenimiento de proceso, contradice hoy el principio de "sin llamadas de
  red" solo en el sentido de que ahora habría procesos externos, no
  network) o si se linkea una librería de resolución en-proceso donde
  exista. Probablemente amerita su propia sub-secuencia de sesiones, una
  por lenguaje o por grupo de lenguajes con protocolo compartible.
- **Riesgo/dependencias**: toca el modelo de `core` (un símbolo ya no se
  identifica solo por `name`+`parent`; necesitaría una noción de tipo/scope
  resuelto) y probablemente el schema de `ccm-index` (nueva columna o tabla
  para el tipo resuelto). Un cambio de este tamaño en `core`/`index`
  **sí obligaría a revalidar los 8 lenguajes existentes** (mismo criterio de
  riesgo que HTML/CSS, candidato #4) — es el cambio de mayor blast radius de
  todo este roadmap.
- **Bloqueante**: No, para el uso actual. Ya es la decisión abierta #1 del
  proyecto; sigue sin bloquear ningún criterio de terminado.

### 2. Patrones de exclusión de secretos — Ruby/PHP/Swift

- **Corrección esta sesión**: PHP ya tiene crate propio (`ccm-lang-php`,
  ver candidato #5), pero eso no cambia nada aquí — los dos patrones de
  PHP de abajo (`wp-config.php`/`config/database.php`) siguen **sin
  implementar**, tal como ya decía esta entrada. Lo que sí estaba mal era
  `checklist.md`, que en algún punto afirmó no haber identificado ninguna
  convención de secretos para PHP — contradecía directamente lo que ya
  decía este párrafo; corregido en `checklist.md`, ver Decisión abierta #2
  ahí.
- **Qué es**: agregar a `ccm-index/src/exclude.rs` los patrones de
  convención de secretos específicos de Ruby (`config/master.key`,
  `config/credentials.yml.enc`), PHP (`.env` ya cubierto genéricamente, pero
  `wp-config.php`/`config/database.php` no) y Swift
  (`GoogleService-Info.plist`, `*.xcconfig` con credenciales).
- **Por qué importa**: protege a un usuario que indexa un repo Ruby/PHP/Swift
  hoy mismo, sin tener el crate de lenguaje correspondiente — la exclusión de
  paths no depende de tener un `LanguageParser` registrado, ya que
  `ccm-index` ni siquiera necesita parsear el archivo para excluirlo por
  patrón de ruta. Es decir: **este candidato no depende de tener primero un
  crate de Ruby/PHP/Swift** (candidato #5) — son ortogonales. (El caso de
  PHP ya lo demuestra en la práctica: el crate existe desde esta sesión y
  los dos patrones de secretos de PHP siguen sin agregarse.)
- **Esfuerzo estimado**: **Pequeño**. Mismo patrón que la resolución de Go
  esta sesión: añadir constantes a `DEFAULT_EXCLUDE_PATTERNS` + tests en
  `ccm-index/tests/exclude.rs`. Sub-sesión de menos de una sesión completa.
- **Riesgo/dependencias**: ninguno. No toca `core` ni ningún crate de
  lenguaje.
- **Bloqueante**: No. Es la decisión abierta #2, ya documentada como
  diferida sin ser pendiente activa.

### 3. `find_implementations` sobre Go

- **Qué es**: una tool (o parámetro de una tool existente) que, dado un
  nombre de interfaz Go, devuelva los structs que la implementan
  estructuralmente (mismo conjunto de métodos), no por palabra clave.
- **Por qué importa**: es el único lenguaje de los 8 donde "qué implementa
  esta interfaz" no se puede responder hoy — un usuario Go que pregunte
  "¿quién implementa `Logger`?" no tiene tool para eso, y adivinarlo a mano
  vía `Grep` es exactamente el trabajo de tokens que este proyecto existe
  para evitar.
- **Esfuerzo estimado**: sin la resolución de tipos (candidato #1), un
  heurístico estructural *dentro* del propio `ccm-lang-go` (comparar el
  conjunto de nombres de método de cada struct contra el conjunto de la
  interfaz, sin resolver tipos de parámetros) es factible como **Mediano**
  — similar en tamaño a la sesión de C++/Go, pero con riesgo real de falsos
  positivos/negativos si dos structs no relacionados comparten nombres de
  método por coincidencia (más probable cuanto más grande el repo, mismo
  problema de fondo que el candidato #1). Con resolución de tipos real, baja
  a **Pequeño** (una consulta más sobre información ya resuelta).
- **Riesgo/dependencias**: **depende explícitamente de la decisión #1**
  (documentado ya en el proyecto) — evaluar como una interfaz binaria: o se
  hace ahora con el heurístico de solo-nombres-de-método aceptando su
  margen de error, o se pospone completa hasta LSP. No es coherente hacer
  ambas.
- **Bloqueante**: No. Es la decisión abierta #3 del proyecto.

### 4. HTML/CSS/JSX estructural

- **Qué es**: extender el modelo de `core` con nuevos `SymbolKind` (p. ej.
  `Element`/`StyleRule`) y `RelationKind` (p. ej. `Renders`/`StylesTarget`)
  para indexar HTML/CSS/JSX como grafo real, en vez de solo la lógica
  embebida (que ya se indexa hoy vía la recursión genérica de
  `ccm-lang-js-ts` sobre nodos no reconocidos).
- **Por qué importa**: un usuario frontend preguntando "¿qué componente
  renderiza este botón?" o "¿qué reglas CSS afectan a esta clase?" no puede
  responderlo con las tools actuales — es contexto real que hoy solo se
  consigue con `Read`/`Grep` sobre archivos `.html`/`.css`/`.tsx`, exactamente
  el costo de tokens que el proyecto busca eliminar. Es además el gap más
  visible para cualquier usuario de un stack web moderno (React/Vue con CSS
  modules, Tailwind, etc.), que es un segmento de usuario muy grande.
- **Esfuerzo estimado**: **Grande**. No es solo un nuevo crate de lenguaje —
  es la única otra decisión de este roadmap (junto con LSP) que **modifica
  `ccm-core`** directamente. Un `SymbolKind`/`RelationKind` nuevo aparece en
  el `match` exhaustivo de `symbol_kind_str`/`relation_kind_str`
  (`ccm-index/src/indexer.rs`) y en cualquier lugar que enumere kinds — el
  compilador señala todos los sitios (match exhaustivo), pero **cada uno de
  los 8 crates de lenguaje existentes debe revisarse y sus tests
  re-verificarse** para confirmar que ninguno rompe con el nuevo kind
  disponible (aunque no lo use). Mismo patrón de "cambio en `core` obliga a
  revalidar todo" ya identificado en el proyecto para la decisión de LSP.
- **Riesgo/dependencias**: alto — es un cambio de `core`, no aditivo de
  forma tan limpia como agregar un lenguaje nuevo (que por diseño no toca
  `core`). Recomendado no combinar en la misma sesión que ningún otro
  cambio de `core` (p. ej. no intentar HTML/CSS y LSP en la misma sesión).
- **Bloqueante**: No. Ya estaba diferido explícitamente (ver "Decisión
  resuelta: alcance de 'web'" en `checklist.md`) y el JS/TS lógico ya
  funciona sin esto.

### 5. Lenguajes adicionales (Kotlin, Swift, Ruby)

- **PHP implementado — sacado de esta lista** (corregido al auditar
  `checklist.md`: esta sección seguía nombrando PHP como candidato
  pendiente, y la nota de abajo seguía diciendo "estos 4 ya están en
  `KNOWN_PENDING_LANGUAGES`" cuando la entrada `("php", "php")` de esa
  constante ya se había eliminado al implementar `ccm-lang-php` —
  desincronización real entre este documento y el estado del código, nunca
  cruzada hasta ahora). Ver fila de PHP en la tabla de cobertura de
  `checklist.md`. El candidato #2 (secretos `wp-config.php`/
  `config/database.php`) sigue abierto de forma independiente — no
  dependía de que el crate existiera primero.
- **Qué es**: nuevos crates `ccm-lang-<kotlin|swift|ruby>` siguiendo el
  patrón ya probado 11 veces (`LanguageParser` + registro en
  `ccm-mcp-server`/`ccm-cli` + fixtures + fuzzing + benchmark).
- **Por qué importa**: estos 3 siguen en `KNOWN_PENDING_LANGUAGES`
  (`ccm-index/src/indexer.rs`) — el propio código ya anticipa que un
  repo con estos lenguajes hoy se reporta como "lenguaje pendiente" en
  `get_indexing_status` en vez de fallar silenciosamente. Kotlin (Android +
  backend JVM en alza) y Swift (iOS) son los candidatos de mayor demanda
  real por volumen de proyectos; Ruby sigue teniendo bases de código
  grandes en producción (Rails) que se beneficiarían igual que Java/C# se
  beneficiaron.
- **Esfuerzo estimado**: **Mediano cada uno** — es la unidad de referencia
  literal del proyecto ("similar a agregar un lenguaje nuevo"). Ninguno
  tiene una complejidad estructural conocida comparable a la de C++
  (correlación declaración/definición) o Go (interfaces implícitas), así
  que no hay razón a priori para esperar que cueste más que Java/C#.
- **Riesgo/dependencias**: bajo, arquitectónicamente — el mismo argumento
  que ya se demostró con Lua: el trait `LanguageParser` generaliza sin tocar
  `core`/`index`. La única dependencia real es el candidato #2 (secretos)
  si se quiere resolver *antes* de indexar Ruby en un repo con
  credenciales sin excluir — no es un bloqueo duro, es un orden recomendado.
- **Bloqueante**: No.

### 6. `find_types(kind?)`

- **Qué es**: una tool (o parámetro `kind` opcional agregado a
  `find_symbol`) que liste símbolos filtrando por
  `Class`/`Struct`/`Interface`/`Enum`/`Trait`/`TypeAlias` — los 6 kinds de
  tipo que `SymbolKind` ya distingue (`ccm-core/src/symbol.rs:15-28`).
- **Por qué importa**: un usuario explorando un repo desconocido a menudo
  quiere "qué tipos existen" antes de saber qué nombre buscar con
  `find_symbol` — hoy no hay forma de listarlos sin ya saber el nombre
  exacto. Caso de uso real: "dame todas las interfaces de este módulo" antes
  de decidir cuál implementar.
- **Esfuerzo estimado**: **Pequeño**. El dato ya existe sin cambios de
  schema — `symbols.kind` ya se guarda como TEXT. Es una query nueva en
  `ccm-index/src/queries.rs` (filtrar por `kind IN (...)` en vez de por
  `name`) + 1 tool nueva o un parámetro opcional en `find_symbol` + tests.
  Coherente con la nota que ya deja `checklist.md`: *"revisar si caben como
  parámetro de una tool existente... en vez de una tool nueva"* — este es
  exactamente ese caso: se recomienda como parámetro, no tool nueva, para no
  romper el límite de 6-8 tools.
- **Riesgo/dependencias**: ninguno. No toca `core` ni ningún lenguaje.
- **Bloqueante**: No.

### 7. `find_tests(symbol?)`

- **Qué es**: exponer como tool de primera clase el heurístico de nombre
  `test`/`test_*` que hoy solo vive *dentro* de la composición de
  `impact_analysis` (`ccm-mcp-server/src/server.rs`), para poder listar
  tests relacionados con un símbolo sin pedir el análisis de impacto
  completo.
- **Por qué importa**: hoy, para saber "¿qué tests cubren esto?" sin querer
  también callers/references, hay que pagar el costo completo de
  `impact_analysis` — más caro de lo necesario para una pregunta más
  simple. Caso de uso: antes de tocar una función, un usuario quiere correr
  solo sus tests, no auditar todo el impacto.
- **Esfuerzo estimado**: **Pequeño**. La lógica de heurística ya existe y
  está probada (usada por `impact_analysis` hoy); es extraerla a su propia
  query/tool reutilizando el mismo código, no reescribirla.
- **Riesgo/dependencias**: bajo. Riesgo de producto, no técnico: agregarla
  como tool #8 deja el proyecto en el límite exacto del criterio de 6-8
  tools declarado — si se agrega esta, `find_types` (candidato #6) debería
  ir como parámetro de tool existente y no como tool nueva, para no romper
  el criterio con las dos juntas.
- **Bloqueante**: No.

### 8. `find_dead_code()`

- **Qué es**: una tool/query que liste símbolos definidos sin ninguna
  relación entrante (`NOT EXISTS (SELECT 1 FROM relations WHERE to_name =
  symbols.name)`), como candidatos a código muerto.
- **Por qué importa**: caso de uso real de limpieza de repo — "¿qué puedo
  borrar con confianza?" es una pregunta cara de responder manualmente
  (requiere `Grep` de cada símbolo uno por uno) y es justo el tipo de tarea
  agregada donde una tool sola vale más que la suma de `find_references`
  repetidas.
- **Esfuerzo estimado**: **Pequeño** para una primera versión (la query es
  directa sobre el schema actual, sin cambios de `core`/`index`), pero con
  una advertencia real: el heurístico de "sin relación entrante" tiene
  **falsos positivos estructurales conocidos** — puntos de entrada
  (`main`, handlers de framework invocados por nombre desde fuera del
  repo indexado, funciones exportadas como API pública de una librería,
  implementaciones de trait/interfaz invocadas por despacho dinámico en vez
  de por nombre literal) aparecerían como "muertos" sin estarlo. Esto no
  cambia el esfuerzo de construirla, pero sí el de que sea *útil* sin
  generar ruido — considerar si vale la pena una sub-iteración pequeña para
  excluir heurísticamente `main`/símbolos con kind `Trait`/`Interface`
  method antes de exponerla como tool "de confianza".
- **Riesgo/dependencias**: ninguno técnico. Riesgo de producto: si el ruido
  de falsos positivos es alto, la tool entrena al usuario a no confiar en
  ella — peor que no tenerla. Evaluar con un repo real antes de comprometerse
  a exponerla.
- **Bloqueante**: No.

### 9. `semantic_search(query)`

- **Qué es**: búsqueda semántica (no solo por nombre exacto/FTS5 léxico)
  sobre símbolos, vía embeddings.
- **Por qué importa**: `find_symbol` hoy requiere saber el nombre exacto
  (o vía FTS5, coincidencia léxica) — un usuario que pregunta "¿dónde se
  calcula el precio con impuestos?" sin saber que la función se llama
  `computeTotalWithVat` no tiene forma de encontrarla sin `Grep` de texto
  libre, que es exactamente el costo que el proyecto quiere evitar.
- **Esfuerzo estimado**: **Grande**, y distinto en naturaleza a todo lo
  demás en este roadmap: no es "agregar una tool sobre el schema
  existente", es agregar una pieza de infraestructura completamente nueva
  (generación de embeddings, almacenamiento vectorial, y una decisión de
  producto sobre si el modelo de embeddings corre localmente o por API).
- **Riesgo/dependencias**: **tensión de arquitectura, no solo de esfuerzo**:
  el README declara explícitamente "no network calls by default — zero
  telemetry" como propiedad central del proyecto. Un embedding por API
  rompe esa propiedad salvo que sea estrictamente opt-in; un embedding local
  (modelo pequeño empaquetado) evita la llamada de red pero agrega una
  dependencia de peso considerable (runtime de inferencia, tamaño de
  binario) a un proyecto que hoy es un binario Rust ligero sin dependencias
  de ML. Requiere una decisión de producto explícita del usuario antes de
  estimarse con más precisión — no es una sesión más, es una familia de
  decisiones nueva.
- **Bloqueante**: No, y candidato a quedar fuera de alcance indefinidamente
  salvo decisión explícita — ver "Descartado por ahora".

### 10. Rendimiento a escala (monorepos grandes) — investigación previa

- **Qué es**: no es una mejora en sí, es una tarea de investigación:
  generar (o conseguir) un repo sintético de cientos de miles de archivos y
  medir tiempo de indexado inicial, tamaño resultante del `.sqlite`, y
  latencia de las 7 queries actuales contra un grafo grande, para saber si
  hay un cuello de botella real antes de que un usuario lo reporte como bug.
- **Por qué importa esta investigación (no la mejora en sí todavía)**: la
  lectura del código actual ya deja ver **tres sospechas concretas, no
  confirmadas**, que justifican medir antes de optimizar a ciegas:
  1. `reindex()` (`ccm-index/src/indexer.rs:86-233`) hace `WalkDir` sobre
     **todo** el árbol en cada corrida, y para cada archivo no excluido
     **lee el archivo completo a memoria** (`std::fs::read`) solo para
     calcular el hash de contenido y decidir si cambió — incluso archivos
     sin cambios pagan una lectura completa de disco en cada reindex, no
     solo los cambiados. En un monorepo de cientos de miles de archivos,
     esto es I/O que crece con el tamaño total del repo, no con el tamaño
     del diff.
  2. El parseo es **secuencial, un archivo a la vez** — no hay paralelismo
     (`rayon` o hilos) sobre el `WalkDir`, así que el tiempo de indexado
     inicial escala linealmente con el número de archivos sin aprovechar
     múltiples cores.
  3. La resolución de cada `SymbolRelation` a un símbolo existente
     (`write_parsed_file`, `ccm-index/src/indexer.rs:341-347`) hace **una
     query SQL por relación** (`SELECT id FROM symbols WHERE name = ?1
     LIMIT 1`), dentro de la misma transacción — con índice sobre `name`
     esto es barato por query individual, pero con decenas de miles de
     relaciones por archivo grande, el número total de queries en un
     monorepo podría ser significativo. Además, con `LIMIT 1` sin
     desempate determinístico, cuantos más símbolos con el mismo nombre
     existan (más probable en un repo grande), más arbitraria es la
     resolución — esto conecta directamente con el candidato #1 (LSP), pero
     es un problema de *correctness* aparte del de *performance*.
- **Esfuerzo estimado**: **la investigación en sí, Mediano** (construir o
  adaptar un generador de repo sintético con distribución realista de
  archivos/símbolos por lenguaje, correr y medir las 3 sospechas de arriba
  con métricas reales, no estimadas). **No se estima la corrección de lo
  que se encuentre** — eso depende de qué aparezca; podría no encontrarse
  nada que amerite cambio, o podría requerir una sesión de trabajo (p. ej.
  paralelizar el `WalkDir` con `rayon`, o cachear metadata de archivo
  — tamaño+mtime — para evitar leer contenido completo cuando el archivo no
  cambió) proporcional a lo que se mida.
- **Riesgo/dependencias**: bajo para la investigación en sí (no toca
  producto); el riesgo real está en las mejoras que salgan de ella, a
  evaluar después con datos reales en vez de aquí a ciegas.
- **Bloqueante**: No hoy — no hay reporte real de un usuario con este
  problema, es prevención.

### 11. Observabilidad local — `get_token_savings_estimate()` o reporte post-sesión

- **Qué es**: exponer al usuario del plugin, en su propio uso diario, una
  estimación de cuántos tokens se está ahorrando al usar las tools MCP en
  vez de `Read`/`Grep`/`Glob` — hoy ese dato solo existe como benchmark
  interno de desarrollo (`benchmarks/token-benchmark.md`,
  `ccm-cli/examples/token_benchmark.rs`), no como algo que el usuario final
  vea.
- **Por qué importa**: el objetivo declarado del proyecto desde el inicio es
  "reducir el gasto de tokens" — hoy esa afirmación solo se demuestra en
  fixtures de desarrollo, nunca al usuario real en su propio repo con su
  propio patrón de uso. Un usuario que adopta el plugin no tiene forma de
  verificar que le está funcionando *a él*, solo puede confiar en el
  benchmark genérico del README.
- **Esfuerzo estimado**: **Pequeño a Mediano**, según alcance:
  - Versión pequeña: una tool `get_token_savings_estimate()` que reporte,
    para las queries ya ejecutadas en la sesión MCP activa, una estimación
    comparando el tamaño de la respuesta real de cada tool contra una
    estimación de lo que habría costado un `Grep`/`Read` equivalente
    (misma metodología que el benchmark existente, aplicada en vivo en vez
    de a un fixture fijo). Requiere que el servidor MCP lleve un contador
    interno de uso durante su vida de proceso — estado nuevo pero acotado,
    no toca `core`/`index`.
  - Versión mediana: un reporte post-sesión más completo (acumulado, quizás
    persistido entre sesiones) — esto sí empieza a acercarse a una feature
    de producto con su propio almacenamiento, más cercano en tamaño a medio
    lenguaje nuevo que a un ajuste pequeño.
- **Riesgo/dependencias**: riesgo de producto, no técnico — si se agrega
  como tool #8/#9, compite por el mismo presupuesto del criterio de 6-8
  tools que `find_types`/`find_tests`/`find_dead_code`. Sopesar si esta
  encaja mejor como *comando de `ccm-cli`* (`ccm-cli --root . stats`) en vez
  de tool MCP — evita el problema del límite de tools por completo, y tiene
  sentido porque es una pregunta que el usuario se hace fuera del flujo de
  un agente, no algo que Claude Code necesite invocar por sí mismo.
- **Bloqueante**: No.

### 12. Deuda técnica menor detectada al pasar (no documentada como decisión abierta)

Encontrada al revisar `checklist.md`/`README.md` completos, no solo la lista
de decisiones abiertas — ninguna bloquea nada, se agrupan aquí para no
perderlas:

- **Fixtures de integración asimétricas**: los 6 lenguajes más recientes
  (Java, C#, JS/TS, C++, Go, y el poliglota) tienen fixture de integración
  end-to-end vía `ccm-index` real; Rust y Python (los dos primeros,
  sesión 1) solo tienen tests a nivel de parser. Esfuerzo: **Pequeño** —
  añadir un fixture de integración a cada uno siguiendo el patrón ya
  establecido 6 veces.
- **Benchmark de tokens incompleto para Rust y Python**: los 5 lenguajes
  más recientes tienen benchmark documentado; Rust/Python no, aunque el
  criterio general de "3+ lenguajes benchmarkeados" ya está cumplido y no
  bloquea nada. Esfuerzo: **Pequeño**, mismo patrón que los otros 5.
- **`README.md`: sección "Benchmark of tokens saved" desactualizada** — dice
  literalmente "Planned, not yet implemented", pero el benchmark **ya está
  implementado y corrido para 5 lenguajes** (ver `checklist.md` y
  `benchmarks/token-benchmark.md`). Esto es una inconsistencia real entre
  archivos del proyecto, no solo una mejora — vale la pena señalarla ahora
  aunque no es parte del alcance de "mejoras candidatas": es una corrección
  de documentación de una sola sesión muy corta (probablemente minutos, no
  una unidad de sesión completa).
- **`README.md`: "Status" dice "7 languages implemented"** pero la tabla de
  abajo y el resto del proyecto ya cuentan 8 (con Go incluido) más Lua como
  prueba de arquitectura — mismo tipo de inconsistencia de documentación
  que el punto anterior, incluso más directa.
- **Ejecución real de publicación en crates.io/GitHub Release**: preparada
  (`RELEASING.md`, `.github/workflows/release.yml`) pero deliberadamente no
  ejecutada — es una decisión del usuario, no una tarea de Claude, pero se
  menciona aquí porque es la única pieza de "entregable general" que sigue
  sin cerrar y no está en ningún otro lugar del roadmap.

---

## Orden sugerido

Secuencia razonada, no fechas ni versiones. Principio: primero lo que no
toca `core` y es barato, dejar lo que toca `core` para sesiones dedicadas y
aisladas entre sí, e investigar antes de comprometerse en lo que no se puede
estimar con confianza todavía.

1. **Housekeeping de documentación** (parte de candidato #12: README
   desactualizado en "Status" y "Benchmark of tokens saved"). Trivial, cero
   riesgo, deja el proyecto coherente antes de seguir construyendo sobre él.
2. **Candidato #2 — secretos Ruby/PHP/Swift**. Pequeño, cero dependencias,
   cierra una de las 3 decisiones abiertas documentadas sin tocar `core`.
3. **Candidato #6 — `find_types` como parámetro de `find_symbol`**.
   Pequeño, cero riesgo, alto valor de exploración para cualquier usuario
   nuevo del repo indexado.
4. **Candidato #7 — `find_tests`**, inmediatamente después de #6 y no antes:
   evaluar ambos juntos para decidir cuál va como tool nueva y cuál como
   parámetro, respetando el límite de 6-8 tools de una sola vez en vez de
   decidirlo dos veces por separado.
5. **Candidato #10 — investigación de rendimiento a escala**, en paralelo
   con lo anterior si hay ancho de banda: es investigación, no cambia
   producto, y su resultado informa si alguna mejora de rendimiento debe
   colarse *antes* de que el proyecto tenga más usuarios con monorepos
   grandes. Cuanto antes se sepa si las 3 sospechas del indexer son reales,
   más barato es corregirlas (menos código nuevo construido encima del
   patrón actual de indexado secuencial + lectura completa por archivo).
6. **Candidato #5 — un lenguaje adicional (Kotlin o Swift primero, por
   demanda)**, usando el ancho de banda de "sesión mediana conocida" mientras
   las decisiones grandes (#1, #4) maduran. No depende de nada de lo
   anterior.
7. **Candidato #8 — `find_dead_code`**, después de #10: si la investigación
   de escala revela que el schema necesita un índice adicional o un ajuste
   para que la query de "sin relación entrante" sea barata en repos grandes,
   mejor saberlo antes de exponer la tool.
8. **Candidato #11 — observabilidad de tokens**, evaluando primero si va
   como subcomando de `ccm-cli` (recomendado) en vez de tool MCP — decisión
   de producto a tomar por el usuario antes de estimar la sesión con
   precisión.
9. **Decisión #1 (LSP) y candidato #4 (HTML/CSS/JSX) — sesiones grandes y
   aisladas, una por vez, nunca combinadas entre sí ni con ninguna otra
   mejora de `core` en la misma sesión.** Cuál va primero es una decisión
   del usuario, no algo que este roadmap deba resolver — ambas son cambios
   de `core` que obligan a revalidar los 8 lenguajes existentes, así que el
   orden entre ellas importa menos que garantizar que nunca se solapen.
10. **Candidato #3 — `find_implementations` sobre Go**, inmediatamente
    después de que se resuelva la decisión #1 (si se resuelve con LSP) o
    como sesión mediana aislada con el heurístico de solo-nombres-de-método
    (si el usuario decide no esperar a LSP). Esto es exactamente la
    dependencia que el propio proyecto ya documentó — este roadmap solo la
    hace explícita en la secuencia.
11. **Candidato #9 — `semantic_search`**: al final, y solo tras una decisión
    de producto explícita del usuario sobre la tensión con "zero network
    calls by default" (ver "Descartado por ahora" para la alternativa de no
    perseguirlo en absoluto).

---

## Descartado por ahora

- **`semantic_search` vía API externa de embeddings**: descartado como
  *default* del proyecto — rompe directamente la propiedad "no network
  calls by default / zero telemetry" que el README declara como
  característica central, no un detalle incidental. No se descarta la
  variante local (embeddings on-device), pero esa variante no se ha
  evaluado en profundidad aquí porque depende de una decisión de producto
  previa (¿vale la pena el peso de un runtime de inferencia en un binario
  que hoy es un CLI/servidor Rust ligero?) que le corresponde al usuario, no
  a este análisis. Revisar solo si aparece una necesidad concreta reportada
  por un usuario real, no de forma especulativa.
- **Tablas por lenguaje en el schema de SQLite**: nunca se consideró en
  serio (el proyecto fue diseñado desde la sesión 1 con schema único +
  columna `language`), pero se anota aquí explícitamente porque es la
  alternativa obvia que alguien podría proponer al ver el candidato de
  rendimiento a escala (#10) — particionar por lenguaje no resuelve ninguna
  de las 3 sospechas identificadas (lectura completa de archivos sin
  cambios, parseo secuencial, resolución de relaciones por nombre), así que
  no hay razón para reabrir esa decisión de arquitectura por este motivo.
- **Paralelizar el indexado como mejora ya decidida**: deliberadamente no se
  incluye como candidato de mejora en firme en esta versión del roadmap
  (solo como sospecha dentro de #10) — proponer "usar `rayon`" sin medir
  primero sería exactamente el tipo de estimación inventada que esta sesión
  tiene prohibido producir. Se revisita con datos después de #10, no antes.
- **Sustituir `LIMIT 1` en la resolución de relaciones por una heurística de
  scope sin LSP** (p. ej. preferir un símbolo en el mismo archivo o incluso
  en un archivo dentro del mismo directorio, antes de matchear por nombre
  global): se consideró como una mejora intermedia más barata que LSP
  completo, pero se descarta como *candidato separado* porque cualquier
  heurística de este tipo es exactamente el tipo de "resolución de tipos
  parcial" que la decisión abierta #1 ya engloba — tratarla aparte
  duplicaría la discusión en vez de resolverla. Si el usuario decide no
  esperar a LSP completo, esta heurística debería evaluarse como *alcance
  reducido* de la decisión #1, no como un candidato #13 nuevo.
