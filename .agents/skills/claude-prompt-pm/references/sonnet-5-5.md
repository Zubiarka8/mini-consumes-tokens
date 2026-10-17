# Prompting Claude Sonnet 5.5 (guía oficial de Anthropic)

Fuente: https://platform.claude.com/docs/es/build-with-claude/prompt-engineering/prompting-claude-sonnet-5-5 (cargó correctamente en ES; contrastada con la versión EN de la misma ruta). Solo se incluye contenido de esa página. Los prompts de ejemplo van en inglés porque así aparecen en la página (no traducir al insertarlos).

Qué leer para qué (índice propio, D4): redactar el prompt, «Qué recomienda hacer o evitar al promptearlo»; diferencias con la guía general, la primera sección; elegir modelo, «Cuándo conviene este modelo según la página».

## Qué es específico de Sonnet 5.5 frente a las buenas prácticas generales

### Takeaway
La página cubre solo patrones específicos de Sonnet 5.5 (esfuerzo recalibrado, iniciativa/alcance, pensamiento `between_tools`, JSON con razonamiento, actualizaciones de progreso, herramientas, verificación, rechazos). Remite a "Mejores prácticas de prompting" para técnicas comunes a todos los modelos. Los prompts de Sonnet 5 "deberían funcionar bien sin cambios".

### Cited Findings
- La guía cubre "patrones de prompting específicos de Claude Sonnet 5.5"; para técnicas de todos los modelos actuales remite a "Mejores prácticas de prompting" (intro).
- Los prompts existentes de Sonnet 5 deberían funcionar sin cambios; los patrones de "Prompting Claude Sonnet 5" siguen siendo un punto de partida razonable (intro).
- Para el trabajo de largo horizonte más difícil, un modelo Opus es la mejor opción (intro).
- Los niveles de `effort` están recalibrados: un nivel no produce el mismo pensamiento que en Sonnet 5; hay que rehacer el barrido con evals propias en vez de copiar el ajuste anterior (sección "Calibra el esfuerzo").
- Hay cinco cambios incompatibles de API respecto a Sonnet 5 (detalle en la guía de migración, no en esta página) (nota de la intro).
- Cinco rechazos de clasificadores de seguridad posibles (`cyber`, `bio`, `frontier_llm`, `reasoning_extraction`, `general_harms`) (sección "Rechazos de las salvaguardas").

### Inferences
- Para el reescritor de prompts: el grueso de lo específico del modelo vive en el system prompt/arnés/parámetros (effort, thinking, max_tokens), no en la redacción de la petición del usuario. La redacción general (claridad, contexto) la delega la página a la guía general.

### Gaps
- La página no da reglas de redacción genéricas (XML, ejemplos, roles, etc.); no se incluyen.

## Qué recomienda hacer o evitar al promptearlo (reglas accionables)

### Takeaway
Calibrar esfuerzo, usar los bloques de system prompt que la página ofrece para controlar iniciativa/alcance/verificación/búsqueda, no pedirle que "piense menos" ni que muestre su razonamiento, y cuidar dónde se inserta el texto del usuario a mitad de turno.

### Cited Findings

**Esfuerzo (sección "Calibra el esfuerzo")**
- `effort` es el control principal de cuánto piensa el modelo, calidad, latencia y costo.
- Empezar en `high` (predeterminado en la Claude API) salvo carga agéntica o sensible a latencia.
- Programación agéntica y herramientas multi-paso: empezar en `medium` para tareas bien especificadas; `high` para las más difíciles o largas.
- Chat y trabajo sensible a latencia: empezar en `medium` o `low` (más esfuerzo = más espera antes de que empiece la respuesta). Subir si la calidad lo exige.
- En `low`: pensamiento breve y puede omitir verificar un cambio. En `low` y `medium`, en tareas agénticas largas es más probable que se detenga a consultar al usuario antes de terminar.
- `max_tokens`: dejar espacio para pensamiento + respuesta (el pensamiento cuenta aunque no se devuelva); un límite pensado para "sin pensamiento" puede cortar la respuesta. Para programación agéntica: `max_tokens` = 128,000 (máximo del modelo) y usar streaming.
- Reservar `xhigh` y `max` para trabajos donde se haya medido mejora; pensamiento y respuestas se alargan mucho. En esos niveles no se acepta `between_tools`.
- Menos pensamiento = bajar el nivel. Desde `medium` piensa brevemente antes de casi toda respuesta (incluso un saludo). ANTIPATRÓN: pedir en el system prompt que piense menos no lo reduce de forma fiable. En `low` omite pensamiento en la mayoría de solicitudes simples.
- Cambiar `effort` de nivel superior entre solicitudes invalida la caché de prompts; para variar por turno usar el cambio de esfuerzo por mensaje (beta), que conserva la caché y requiere pensamiento adaptativo (con `between_tools` da error 400). Ejemplo de la página: sesión interactiva en `low`, subir a `high` ante un problema difícil.

**Iniciativa y alcance (sección "Orienta la iniciativa y el alcance")**
- Hasta dónde llega depende del esfuerzo y de la solicitud: menor esfuerzo -> a veces consulta antes de terminar; mayor esfuerzo o solicitud abierta -> puede hacer más de lo pedido.
- Llevar el trabajo hasta el final (a `low`/`medium` a veces confirma planes, pregunta lo que podría resolver solo o se para tras una parte): probar primero un esfuerzo mayor; si no, añadir al system prompt:
```
Keep working until everything the user asked for is done, and only stop to ask when you can't go on without the user or before a risky step.

When the work the user asked for is done and checked, stop and report. Don't add features, tests, files, docs or refactors that weren't asked for. If you think one would help, mention it at the end instead of doing it.
```
  Efecto: más trabajo completado a `low`/`medium`, sesiones más largas y más caras. No sustituye las reglas propias sobre acciones riesgosas/irreversibles (mantenerlas en el system prompt).
- Adiciones no solicitadas al programar: tiende a añadir tests, documentación y pequeños archivos de soporte acordes con las convenciones del repo, en todos los niveles y más con esfuerzo alto; el cambio pedido en sí se mantiene cercano a lo solicitado. "La mayoría de los equipos lo agradecerán". Si se quieren cambios limitados a lo pedido: añadir solo el segundo párrafo del prompt anterior ("When the work the user asked for is done..."). En `xhigh`/`max` ese párrafo reduce adiciones y cambios más pequeños.
- Exhaustividad en `xhigh`/`max`: puede lanzar sus propias rondas de revisión/verificación (a veces con subagentes si el arnés los da) y hacer correcciones relacionadas. Más tiempo y tokens; ejecutar trabajo rutinario en `high` o menos. Para dirigir esa exhaustividad a la tarea:
```
When the work the user asked for is done and its checks pass, stop and report. Don't start extra rounds of review or hardening on your own, and don't launch reviewer sub-agents unless the user asked for a review. If you think a deeper review is worth doing, say so at the end.
```
  En pruebas a `max` evitó subagentes revisores y redujo el costo de sesión aproximadamente un tercio sin cambio de calidad; no elimina del todo las revisiones autoiniciadas.
- Solicitudes abiertas (ej. "muéstrame lo que puedes hacer con esto"): puede ponerse a crear una presentación, informe o vídeo cuando solo querías ideas. Decirlo en la solicitud o añadir:
```
When the user asks for ideas, options or a plan, give them that and stop. Don't start building or changing anything until they say to go ahead.
```

**Pensamiento sin planificación previa (sección "Ejecución sin pensamiento previo")**
- Para ejecutar sin pensamiento previo: `thinking: {"type": "between_tools"}`; es el ajuste de pensamiento más bajo, aceptado con esfuerzo `high` o inferior. Si hoy se usa el pensamiento desactivado, cambiar a `between_tools`.
- Con `xhigh`/`max` + `between_tools` -> error 400. Con `between_tools` el esfuerzo no puede cambiar a mitad de conversación (un `output_config.effort` por mensaje distinto -> 400); para variar por turno usar pensamiento adaptativo.
- ANTIPATRÓN: con `between_tools`, eliminar instrucciones que le digan al modelo que no piense; aumentan la probabilidad de que escriba etiquetas XML internas en la salida visible.
- No asumir que el primer bloque de contenido es texto (puede ser un bloque `thinking`, vacío con `display: "omitted"` en adaptativo, o de actualización de progreso con `between_tools`).
- Devolver los bloques `thinking` sin cambios junto con el resto del turno del asistente (el modelo recibe la nota completa, no el resumen).
- Para tareas de razonamiento sin herramientas usar pensamiento adaptativo: en una solicitud sin herramientas, `between_tools` hace que responda sin pensar.

**Razonamiento con salida JSON (sección "Tareas de razonamiento con salida JSON")**
- Aplica a JSON sobre tareas con varios pasos (ej. sumar cifras de un documento, aplicar una regla, ordenar elementos). El modelo a menudo responde sin pensar primero, sobre todo a `low`/`medium`.
- Preferir "structured outputs" donde existan (el texto es JSON conforme al esquema). Ahí el modelo solo puede razonar en su pensamiento; si lo omite, pierde precisión.
- Con pensamiento adaptativo, añadir al FINAL del system prompt: `Think the problem through before you answer.` A `high` acerca la precisión a la de `xhigh` con aumento moderado de tokens; a `low`/`medium` sube la precisión, sin alcanzar `high`, con mayor aumento de tokens.
- Alternativa: esfuerzo `xhigh` con pensamiento adaptativo (máxima precisión incluso sin la línea; más tokens que `high`).
- Usar adaptativo, no `between_tools`, en solicitudes sin herramientas (la línea no tiene efecto allí). Dividir en dos solicitudes (respuesta y luego JSON) dio alta precisión y cumplimiento de formato pero con costo y latencia muy altos.
- Con structured outputs a `low`/`medium`, ocasionalmente piensa hasta `max_tokens`; a `high`+ casi nunca. Tratar como fallida cualquier respuesta con `stop_reason: "max_tokens"` aunque el JSON sea válido, y reintentar; `max_tokens` suficiente pero no más de lo que se esté dispuesto a gastar.
- Sin structured outputs (JSON pedido en el prompt): el modelo suele razonar en el texto y poner el JSON al final, lo que rompe un parser que espere solo JSON. Solución: parsear el ÚLTIMO valor JSON (leer solo bloques `text`; desde cada `{` o `[` intentar parsear, continuar desde el final del valor parseado, conservar el último; no tomar de la primera `{` a la última `}` porque puede haber un borrador previo; si son varios valores seguidos, conservar la última secuencia separada solo por espacios, comas o saltos de línea; validar campos esperados y reintentar una vez). También considerar `xhigh` + adaptativo (razona en el pensamiento y casi siempre devuelve solo JSON; tokens totales ~ los de `high`).

**Actualizaciones de progreso (sección "Actualizaciones de progreso para el usuario")**
- Entre llamadas a herramientas escribe notas al usuario. Las de más de una o dos oraciones llegan como bloques `thinking` de actualización de progreso (texto vacío con el `display` predeterminado; un cliente que solo muestra `text` parece silencioso); las breves quedan como `text`.
- Para mostrarlas: `display: "updates"` (beta, header `thinking-display-updates-2026-08-18`). Con `between_tools` llegan con texto de resumen y no se necesita `display`; `between_tools` no admite `display`, `budget_tokens` ni `block_binding` (error 400).
- Si necesita mostrar texto exacto a mitad de turno (fragmento de código, pregunta), darle una herramienta sencilla para enviar mensajes al usuario, indicarle que la use solo para eso y declararla en la primera solicitud de la sesión para que la lista `tools` no cambie.
- Eliminar instrucciones antiguas tipo "guarda todos los hallazgos para la respuesta final". Si se quieren actualizaciones en puntos predecibles (p. ej. una línea antes de la primera llamada a herramienta y un breve resumen al final), decirlo en el system prompt; las sigue. Útil sobre todo en trabajo con humano en el bucle.
- Si aún hay silencios: el arnés cuenta pasos consecutivos de herramientas sin texto (p. ej. 5) y añade un recordatorio de un turno como mensaje del sistema con alcance de turno (beta): `The user hasn't heard from you in a while — say in a few words what you're doing, then continue.` Parar de enviarlo tras el 2º o 3º si sigue callado; dejar cada recordatorio en `messages` en solicitudes posteriores (conserva caché y pensamiento preservado). Texto frecuente del arnés tras resultados de herramientas puede hacerle sospechar de prompt injection. Efecto medido en la página: a `high`, con una herramienta de mensajes al usuario disponible, el recordatorio aumenta la frecuencia de actualizaciones y acorta los silencios más largos, sin cambio medible de calidad.

**Herramientas en chat/trabajo de conocimiento (sección "Uso de herramientas en chat y trabajo de conocimiento")**
- A veces responde de su conocimiento de entrenamiento cuando una búsqueda web detectaría detalles cambiados (qué está permitido, exigido o cobrado).
- ANTIPATRÓN: eliminar del prompt frases que desalienten herramientas ("usa herramientas solo cuando sea estrictamente necesario", "minimiza las llamadas a herramientas").
- Si hay herramienta de búsqueda, añadir:
```
Use the search tool to check specifics that may have changed since your training, such as what is allowed, required or charged, even when you feel confident. For researched work such as a report or a comparison, gather current sources rather than writing from your training knowledge.
```
  Importa sobre todo en productos de investigación y soporte.

**Mensajes del usuario a mitad de turno (sección "Mensajes del usuario a mitad de turno")**
- Está entrenado para resistir inyección indirecta de prompts y a veces trata un mensaje genuino del usuario como posible inyección (si llega como mensaje del sistema justo tras un resultado de herramienta o dentro de un `tool_result`): puede decir que el resultado contenía texto que se hacía pasar por el usuario e ignorarlo o pedir confirmación.
- Causas: cuenta regresiva de tokens tras cada resultado, usuarios enviando mensajes a mitad de turno multi-paso, instrucciones/contexto del arnés tras cada resultado.
- Reglas: nunca poner texto del usuario dentro de un `tool_result` (lo más malinterpretado); entregar la entrada a mitad de turno como turno de usuario (bloque de texto en el mensaje de usuario que lleva los `tool_result`, después del último `tool_result`); avisos del arnés en un mensaje del sistema aparte, después de las palabras del usuario, nunca en el mismo bloque; en sesiones interactivas no añadir tu propia cuenta regresiva de tokens/presupuesto tras resultados de herramientas. Los task budgets (beta) añaden una cuenta similar pero no se ha visto que causen el fallo; si ocurre con uno configurado, probar sin él. Si ocurre con un recordatorio propio, enviarlo con menos frecuencia.

**Verificación en programación (sección "Verificación en tareas de programación")**
- Generalmente verifica antes de dar por terminado, pero a `low` a veces no ejecuta una comprobación real (p. ej. omite tests porque faltan dependencias).
- Si se ven cambios "completos" sin salida de tests/build, añadir al system prompt (a `low` hace raras las comprobaciones omitidas, sin cambio medible de calidad y con costo ligeramente mayor):
```
When you change code that can be run, built, or type-checked, run a real check that exercises the change before reporting it done: the project's tests, type-checker, or build, or the changed command itself. A syntax-only check, or a check command that failed to start, does not count; if all that is missing is the project's declared dependencies, install them with its own package manager and lockfile (e.g. npm install, pip install -r requirements.txt), never via sudo or the system package manager, unless told not to. Only if no real check can run here, say which one you did not run and why instead of reporting the change as done.
```

**Llamadas a herramientas tolerantes (sección "Manejo tolerante de llamadas a herramientas")**
- Ocasionalmente llama a una herramienta con nombre que difiere solo en mayúsculas (`bash` vs `Bash`) o pasa un parámetro conocido con nombre ligeramente distinto. En el arnés: aceptar la llamada si la coincidencia es inequívoca, o devolver `tool_result` con `is_error: true` indicando el nombre exacto esperado (suele corregirse en el siguiente turno).

**Entradas visuales complejas (sección "Herramientas para entradas visuales complejas")**
- Para gráficos densos y dibujos técnicos, darle forma de recortar, ampliar o ejecutar código sobre la imagen. En gráficos ayuda a todos los niveles; en dibujos técnicos solo desde `high` (más en `xhigh`/`max`). En gráficos, añadir herramientas ayuda más que subir esfuerzo (herramientas + `high` superó a sin herramientas + `max`, a una fracción del costo). Existe una "receta de la herramienta de recorte" (cookbook multimodal-crop-tool).

**Rechazos (sección "Rechazos de las salvaguardas")**
- Clasificadores de seguridad: el rechazo llega como respuesta normal con `stop_reason: "refusal"` y `stop_details.category`: `cyber` (malware/exploits; encontrar vulnerabilidades en código fuente está permitido; ciberseguridad de doble uso de alto riesgo no), `bio` (métodos de laboratorio peligrosos; preguntas cotidianas de salud/educativas no afectadas), `frontier_llm` (ayudar a desarrollar modelos de IA competidores), `reasoning_extraction` (pide reproducir su razonamiento interno en el texto de la respuesta), `general_harms` (otra área de la política de uso; trabajo benigno también puede activarlo).
- Si `bio` bloquea trabajo de ciencias de la vida, existe el Programa de Verificación de Ciencias de la Vida.
- El respaldo del lado del servidor (beta) reintenta rechazos `cyber` y `frontier_llm` en Claude Sonnet 5; no reintenta `bio`, `reasoning_extraction` ni `general_harms`.
- ANTIPATRÓN (relevante para el reescritor): quitar de los prompts instrucciones que pidan al modelo incluir su razonamiento en la respuesta (provocan `reasoning_extraction`). Con pensamiento adaptativo, leer el razonamiento en bloques de pensamiento resumido (`display: "summarized"`). Según la versión EN de la página, aún se puede pedir una breve explicación de la respuesta o un resumen de las acciones realizadas (esa frase final no aparece en la versión ES obtenida).

### Inferences
- Al reescribir peticiones para Sonnet 5.5: no incluir "piensa menos/no pienses", "minimiza llamadas a herramientas" ni "muestra tu razonamiento paso a paso en la respuesta"; sí delimitar alcance explícitamente ("si pido ideas/plan, no construyas"; "no añadas extras no pedidos") si el usuario quiere cambios mínimos, y pedir verificación real en tareas de código.
- Si la petición es de razonamiento con salida JSON, conviene incluir "Think the problem through before you answer." al final del system prompt (con pensamiento adaptativo).

### Gaps
- La página no da ejemplos de peticiones de usuario reescritas; sus únicos ejemplos son los bloques de system prompt citados arriba.
- No da valores numéricos de temperatura u otros parámetros de muestreo; solo `effort`, `thinking` (pensamiento adaptativo, `between_tools`, `display`; con `between_tools` tampoco `budget_tokens` ni `block_binding`), `max_tokens`.

## Cuándo conviene este modelo según la página

### Takeaway
La página es una guía de prompting, no de selección de modelo; solo dice que para el trabajo de largo horizonte más difícil un modelo Opus es mejor opción. Hay indicaciones implícitas de uso (programación agéntica, herramientas multi-paso, chat/latencia, investigación y soporte, lectura de gráficos).

### Cited Findings
- "Para el trabajo de largo horizonte más difícil, un modelo Opus es la mejor opción" (intro).
- Contextos que la página menciona como destino: programación agéntica y herramientas multi-paso (`medium`/`high`), chat y trabajo sensible a latencia (`low`/`medium`), productos de investigación y soporte (búsqueda de datos vigentes), lectura de gráficos y dibujos técnicos con herramientas, trabajo human-in-the-loop (actualizaciones de progreso) (secciones "Calibra el esfuerzo", "Uso de herramientas en chat...", "Herramientas para entradas visuales complejas", "Actualizaciones de progreso...").
- Cubre cambios de API en otra página ("Novedades de Claude Sonnet 5.5") y migración desde Sonnet 5 en la guía de migración; no leídas (fuera de alcance).

### Inferences
- No hay en la página criterios comparativos de coste/velocidad frente a otros modelos; no se deben inventar.

### Gaps
- Sin tabla de "cuándo elegir Sonnet 5.5 vs otros modelos" en la página, salvo la mención a Opus.

## Verificación

Fecha: 2026-10-04.

- Comprobado: todas las reglas, citas literales de prompts (los 5 bloques de system prompt, el recordatorio de silencio y la línea "Think the problem through..."), nombres de sección, parámetros (`effort` con `low`/`medium`/`high`/`xhigh`/`max`, `thinking` `between_tools`, `display` `updates`/`omitted`/`summarized`, `max_tokens` 128,000, header `thinking-display-updates-2026-08-18`, `output_config.effort`, `stop_reason`, categorías de rechazo) y cifras (cinco reglas de envío, ~un tercio de ahorro, recordatorio tras ~5 pasos y parar tras 2º-3º) contra las versiones ES y EN de la página. No se encontraron citas ni parámetros incorrectos ni afirmaciones sin respaldo en las secciones "Cited Findings".
- Corregido/añadido: se añadió el efecto medido del recordatorio de silencio a `high` (sección de actualizaciones de progreso) y se precisó el Gap de parámetros (`budget_tokens`, `block_binding` no admitidos con `between_tools`).
- Notas: la frase final sobre poder pedir una breve explicación o resumen de acciones aparece solo en la versión EN (ya señalado en el fichero). Las marcas "Inferences" siguen siendo inferencias del autor, no de la fuente.
- Limitación: la herramienta de lectura (WebFetch) devuelve el texto procesado por un modelo pequeño, no HTML bruto; el contenido recibido parecía íntegro y literal, pero no se puede garantizar al 100 % que no haya omisiones menores. No se leyeron las páginas enlazadas (migración, novedades, effort, etc.).
