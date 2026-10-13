# Claude prompting best practices (notas para una skill que reescribe peticiones como prompts para Claude)

Fuente única: [Prompting best practices](https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices) (versión /docs/en/; leída completa). La página se organiza en: Model-specific guidance, General principles, Output and formatting, Tool use, Thinking and reasoning, Agentic systems, Capability-specific tips, Migration considerations. Los nombres de sección abajo están en inglés tal como aparecen.

Regla de la página: donde una técnica nombra un modelo, es una medición en ese modelo; reevaluarla con evals propios antes de aplicarla a otro. Cada modelo (Fable 5.1/Mythos 5.1, Fable 5/Mythos 5, Sonnet 5.5, Sonnet 5, Opus 5.5, Opus 5, Opus 4.8) tiene su propia página de prompting ("Model-specific guidance"); leer la del modelo destino antes que las técnicas generales.

Qué leer para qué (índice propio, D4): redactar el prompt (fase 3), secciones 1, 2 y 9; tareas largas o agénticas en Claude Code, sección 5; thinking, sección 4; actualizar un prompt escrito para modelos anteriores de Claude, sección 7; qué evitar, sección 8.

## 1. Reglas generales (General principles)

### Be clear and direct
- Claude responde bien a instrucciones claras y explícitas. Si se quiere comportamiento "above and beyond", hay que pedirlo explícitamente; no confiar en que lo infiera de un prompt vago.
- Tratar a Claude como un empleado brillante pero nuevo, sin contexto sobre tus normas y flujos: cuanto más preciso el encargo, mejor el resultado.
- Regla de oro: mostrar el prompt a un colega con contexto mínimo; si se confundiría, Claude también.
- Ser específico en formato de salida y restricciones.
- Dar las instrucciones como pasos secuenciales (lista numerada o viñetas) cuando importa el orden o que no falte ningún paso.
- Antes/después (Example: Creating an analytics dashboard):
  - Menos efectivo: `Create an analytics dashboard`
  - Más efectivo: `Create an analytics dashboard. Include as many relevant features and interactions as possible. Go beyond the basics to create a fully-featured implementation.`

### Add context to improve performance
- Dar el contexto o la motivación detrás de una instrucción (por qué importa) ayuda a Claude a apuntar mejor; Claude generaliza a partir de la explicación.
- Antes/después (Example: Formatting preferences):
  - Menos efectivo: `NEVER use ellipses`
  - Más efectivo: `Your response will be read aloud by a text-to-speech engine, so never use ellipses since the text-to-speech engine will not know how to pronounce them.`

### Use examples effectively
- Los ejemplos son de las formas más fiables de dirigir formato, tono y estructura. Unos pocos ejemplos bien hechos (few-shot / multishot) mejoran precisión y consistencia.
- Deben ser: Relevant (reflejan el caso real), Diverse (cubren casos límite y varían lo suficiente para que Claude no capte patrones no deseados), Structured (envueltos en `<example>`; varios dentro de `<examples>`).
- Tip: incluir 3-5 ejemplos. Se puede pedir a Claude que evalúe la relevancia/diversidad de los ejemplos o que genere más a partir de los iniciales.

### Structure prompts with XML tags
- Las etiquetas XML ayudan a parsear sin ambigüedad prompts que mezclan instrucciones, contexto, ejemplos e inputs variables. Envolver cada tipo de contenido en su propia etiqueta (p. ej. `<instructions>`, `<context>`, `<input>`) reduce malinterpretaciones.
- Usar nombres de etiqueta consistentes y descriptivos; anidar cuando haya jerarquía natural (`<documents>` con `<document index="n">`).

### Give Claude a role
- Fijar un rol en el system prompt enfoca comportamiento y tono; basta una frase. Ejemplo de la página: `You are a helpful coding assistant specializing in Python.`

### Long context prompting (inputs de 20k+ tokens)
- Poner los datos largos arriba, antes de la consulta, instrucciones y ejemplos. Nota de la página: poner la consulta al final puede mejorar la calidad hasta un 30 % en tests, sobre todo con inputs complejos multidocumento.
- Estructurar múltiples documentos con `<documents>` > `<document index="n">` > `<source>` y `<document_content>` (más metadatos).
- Ground responses in quotes: pedir a Claude que primero cite las partes relevantes (p. ej. en `<quotes>`) y luego, basándose en ellas, haga la tarea (p. ej. en `<info>`).

### Model self-knowledge
- Si la app debe que Claude se identifique o use cadenas de modelo concretas, decirlo en el prompt (ej.: "The assistant is Claude, created by Anthropic. The current model is Claude Opus 5.5." / indicar el model string exacto por defecto).

## 2. Salida y formato (Output and formatting)

### Communication style and verbosity
- Los modelos recientes son más directos, más conversacionales y menos verbosos; pueden saltarse resúmenes tras llamadas a herramientas. Si se quiere visibilidad: "After completing a task that involves tool use, provide a quick summary of the work you've done."
- Excepciones por modelo: Claude Opus 5 tiende a respuestas más largas y el esfuerzo no cambia de forma fiable la longitud visible, así que pedir concisión explícitamente. Claude Fable 5.1 escribe menos actualizaciones durante trabajo agéntico: pedir texto de progreso explícitamente y quitar instrucciones de brevedad sobre ese texto.

### Control the format of responses
1. Decir qué hacer en vez de qué no hacer. En vez de "Do not use markdown in your response" -> "Your response should be composed of smoothly flowing prose paragraphs."
2. Usar indicadores de formato XML: "Write the prose sections of your response in <smoothly_flowing_prose_paragraphs> tags."
3. Igualar el estilo del prompt al de la salida deseada (quitar markdown del prompt reduce el markdown en la salida).
4. Para control fino, dar guía detallada; la página ofrece el bloque `<avoid_excessive_markdown_and_bullet_points>` (prosa fluida, markdown solo para código inline/bloques y encabezados simples, listas solo para ítems realmente discretos o si el usuario las pide, nunca series de viñetas muy cortas).
- Aviso: Claude Fable 5.1 ya formatea menos; en ese modelo un bloque así puede suprimir estructura necesaria.

### LaTeX output
- Los modelos recientes usan LaTeX por defecto para matemáticas. Si se quiere texto plano: "Format your response in plain text only. Do not use LaTeX, MathJax, or any markup notation such as \( \), $, or \frac{}{}. Write all math expressions using standard text characters (e.g., "/" for division, "*" for multiplication, and "^" for exponents)."

### Document creation
- Para presentaciones/documentos visuales: "Create a professional presentation on [topic]. Include thoughtful design elements, visual hierarchy, and engaging animations where appropriate."

### Migrating away from prefilled responses
- Desde los modelos Claude 4.6 (y Mythos Preview) el prefill en el último turno de assistant ya no se admite (error 400). Alternativas de la página:
  - Forzar formato (JSON/YAML, clasificación): Structured Outputs; o pedir al modelo que se ajuste a la estructura; para clasificación, tools con campo enum.
  - Eliminar preámbulos: instrucción directa: "Respond directly without preamble. Do not start with phrases like 'Here is...', 'Based on...', etc."; o salida en etiquetas XML, structured outputs o tool calling; o recortar en post-proceso.
  - Evitar rechazos innecesarios: prompting claro en el mensaje `user`.
  - Continuaciones: mover al mensaje de usuario: "Your previous response was interrupted and ended with `[previous_response]`. Continue from where you left off." (si es manejo de errores y no hay penalización de UX, reintentar la petición).
  - Hidratación de contexto: inyectar recordatorios en el turno de usuario, vía herramientas o en la compactación.

## 3. Uso de herramientas (Tool use)

### Tool usage
- Los modelos recientes siguen instrucciones con precisión y necesitan indicación explícita para actuar. Si dices "can you suggest some changes", a veces solo sugerirá.
  - Menos efectivo (solo sugiere): `Can you suggest some changes to improve this function?`
  - Más efectivo (hace los cambios): `Change this function to improve its performance.` o `Make these edits to the authentication flow.`
- Para sesgar a la acción (system prompt) la página da `<default_to_action>` (implementar en vez de sugerir; si la intención no está clara, inferir la acción más útil y usar herramientas para descubrir lo que falta en lugar de adivinar). Para lo contrario, `<do_not_act_before_instructions>` (por defecto informar/investigar/recomendar; editar solo si se pide).
- Claude Opus 4.5/4.6 son más sensibles al system prompt: lenguaje agresivo antiguo puede causar sobre-activación de herramientas. Cambiar "CRITICAL: You MUST use this tool when..." por "Use this tool when...".

### Optimize parallel tool calling
- Los modelos recientes ejecutan llamadas independientes en paralelo (búsquedas especulativas, leer varios archivos, comandos bash). Es orientable: el bloque `<use_parallel_tool_calls>` lleva el éxito a ~100 % (paralelizar las independientes; secuenciales las dependientes; nunca usar placeholders ni adivinar parámetros). Para reducir: "Execute operations sequentially with brief pauses between each step to ensure stability."

## 4. Razonamiento y thinking (Thinking and reasoning)

### Overthinking and excessive thoroughness
- Opus 4.6 explora más, sobre todo con effort alto. Recomendaciones: sustituir defaults generales por instrucciones dirigidas ("Use [tool] when it would enhance your understanding" en vez de "Default to using [tool]"); quitar el sobre-prompting ("If in doubt, use [tool]" causa sobre-activación); usar `effort` más bajo como último recurso.
- Para frenar el sobre-análisis: "When you're deciding how to approach a problem, choose an approach and commit to it. Avoid revisiting decisions unless you encounter new information that directly contradicts your reasoning..."
- `budget_tokens` está deprecado en Opus/Sonnet 4.6 y devuelve 400 en 4.7+; preferir `effort` o `max_tokens` con adaptive thinking.
- Parámetros en la API (ejemplo de la página): `thinking: {"type": "adaptive"}` y `output_config: {"effort": "high"}` (antes: `thinking: {"type": "enabled", "budget_tokens": 10000}`).

### Leverage thinking & interleaved thinking capabilities
- Modelos 4.6+ usan adaptive thinking (`thinking: {type: "adaptive"}`): Claude decide cuándo y cuánto pensar según `effort` y complejidad. Según la página, rinde mejor que extended thinking. En Fable 5.1, Mythos 5.1, Fable 5, Mythos 5 y Opus 5.5 el thinking siempre está activo.
- Valores por defecto al omitir `thinking`: apagado en Opus 4.6-4.8 y Sonnet 4.6; encendido en Opus 5, Sonnet 5.5 y Sonnet 5 (en Opus 5 solo se puede desactivar con effort `high` o menor; en Sonnet 5.5 el mínimo es `between_tools`, con effort `high` o menor). Si no se usa extended thinking, no hace falta cambiar nada.
- Guiar el thinking tras usar herramientas: "After receiving tool results, carefully reflect on their quality and determine optimal next steps before proceeding..."
- Si piensa demasiado (típico con system prompts grandes): "Thinking adds latency and should only be used when it will meaningfully improve answer quality - typically for problems that require multistep reasoning. When in doubt, respond directly."
- Reglas:
  - Preferir instrucciones generales a pasos prescriptivos: "think thoroughly" suele dar mejor razonamiento que un plan paso a paso escrito a mano.
  - Los ejemplos multishot funcionan con thinking: presentar cada ejemplo como problema, método a aplicar y respuesta esperada.
  - Chain-of-thought manual como alternativa cuando el thinking está apagado: pedir que piense antes de responder y ponga la respuesta final en `<answer>`. En Fable 5.1, Fable 5, Opus 5.5, Opus 5 y Sonnet 5.5 apoyarse en thinking (con effort bajo si importa coste/latencia): pedir que escriba el razonamiento, p. ej. en `<thinking>`, puede ser rechazado.
  - Self-check: añadir "Before you finish, verify your answer against [test criteria]" detecta errores, sobre todo en código y matemáticas. Excepción: Opus 5 verifica solo; esas instrucciones causan sobre-verificación (quitarlas).
- Nota: con extended thinking desactivado, Opus 4.5 es sensible a la palabra "think"; usar "consider", "evaluate" o "reason through".

## 5. Sistemas agénticos y contexto largo (Agentic systems)

### Long-horizon reasoning and state tracking / Context awareness and multiwindow workflows
- Context awareness (seguimiento del presupuesto de tokens restante) lo tienen Sonnet 5, Sonnet 4.6, Sonnet 4.5 y Haiku 4.5.
- Si el harness compacta contexto o permite guardarlo en archivos, decírselo a Claude para que no termine pronto por presupuesto de tokens (la página da un prompt: el contexto se compactará automáticamente; no pares antes; guarda progreso y estado en memoria; sé persistente y autónomo).
- Multi-ventana: (1) prompt distinto para la primera ventana (montar el marco: tests, scripts); (2) tests en formato estructurado (`tests.json`) y recordar "It is unacceptable to remove or edit tests..."; (3) scripts de arranque (`init.sh`); (4) empezar de cero en vez de compactar a veces, siendo prescriptivo (`pwd`, revisar `progress.txt`, `tests.json`, git logs, correr un test de integración básico); (5) dar herramientas de verificación (computer use, browser use, MCP de automatización); (6) fomentar el uso completo del contexto sin dejar trabajo grande sin commitear.
- State management: formatos estructurados (JSON) para datos de estado; texto libre para notas de progreso; git para estado; enfatizar progreso incremental.

### Balancing autonomy and safety
- Sin guía, Opus 4.6 puede hacer acciones difíciles de revertir (borrar archivos, force-push, publicar). Prompt de la página: valorar reversibilidad e impacto; acciones locales y reversibles libremente; pedir confirmación para destructivas (rm -rf, borrar ramas/tablas), difíciles de revertir (`git push --force`, `reset --hard`) o visibles para otros (push, comentar PRs, enviar mensajes); no usar atajos destructivos (`--no-verify`).

### Research and information gathering
- Dar criterios de éxito claros; pedir verificación entre múltiples fuentes; en investigación compleja: desarrollar hipótesis competidoras, registrar niveles de confianza, autocriticar el plan y mantener un archivo de notas/árbol de hipótesis.

### Subagent orchestration
- Los modelos orquestan subagentes de forma nativa; basta tener las herramientas bien definidas. Vigilar el uso excesivo (Opus 4.6 y Opus 5): añadir guía de cuándo sí/no ("Use subagents when tasks can run in parallel, require isolated context, or involve independent workstreams... For simple tasks, sequential operations, single-file edits... work directly").

### Chain complex prompts
- Con adaptive thinking y subagentes, la mayoría del razonamiento multipaso se resuelve internamente. El encadenado explícito (varias llamadas API) sigue siendo útil para inspeccionar salidas intermedias o forzar una estructura de pipeline. Patrón más común: self-correction = borrador -> revisión contra criterios -> refinado, cada paso una llamada separada.

### Reduce file creation in agentic coding
- Claude puede crear archivos temporales como bloc de notas. Para evitarlo: "If you create any temporary new files, scripts, or helper files for iteration, clean up these files by removing them at the end of the task."

### Overeagerness
- Opus 4.5/4.6 tienden a sobre-ingeniería (archivos extra, abstracciones, flexibilidad no pedida). Contramedida: prompt "Avoid over-engineering. Only make changes that are directly requested or clearly necessary" con puntos Scope, Documentation, Defensive coding (validar solo en fronteras del sistema) y Abstractions (sin helpers para operaciones de una sola vez).

### Avoid focusing on passing tests and hardcoding
- Claude puede centrarse en que pasen los tests o usar scripts de apaño. Pedir: solución general y de calidad con herramientas estándar, correcta para todas las entradas válidas, sin valores hardcodeados; los tests verifican, no definen la solución; si la tarea es inviable o un test es incorrecto, avisar en lugar de rodearlo.

### Minimizing hallucinations in agentic coding
- Bloque `<investigate_before_answering>`: nunca especular sobre código no abierto; si el usuario menciona un archivo, LEERLO antes de responder; no afirmar nada sobre el código sin investigar.

## 6. Capacidades específicas (Capability-specific tips)
- Improved vision capabilities: Opus 4.5/4.6 mejoran en imágenes y extracción de datos; dar una herramienta de recorte/zoom (crop tool) o skill aporta mejora consistente. Se puede analizar vídeo troceándolo en frames.
- Frontend design: sin guía, el modelo converge a patrones genéricos ("AI slop"). Se da el bloque `<frontend_aesthetics>`: tipografía distintiva (evitar Arial/Inter), color y tema cohesivos con variables CSS, movimiento (CSS-only; una carga de página bien orquestada), fondos con atmósfera; evitar fuentes sobreusadas, gradientes morados sobre blanco, layouts predecibles; variar entre temas claro/oscuro y no converger en elecciones comunes (p. ej. Space Grotesk).

## 7. Migración (Migration considerations)
1. Ser específico sobre el comportamiento deseado.
2. Enmarcar con modificadores que suban calidad y detalle (ejemplo del dashboard arriba).
3. Pedir explícitamente funciones concretas (animaciones, elementos interactivos).
4. Actualizar la configuración de thinking (adaptive + `effort`).
5. Abandonar los prefills.
6. Ajustar el prompting anti-pereza: los modelos 4.6 son más proactivos y pueden sobre-activarse con instrucciones antes necesarias.
7. Devolver los thinking blocks sin modificar y mantener el historial append-only (en Fable 5.1, Opus 5.5, Sonnet 5.5 modificar antes de un bloque de thinking da error o descarta el bloque).

## 8. Antipatrones señalados (resumen)
- Prompts vagos esperando comportamiento "extra" (Be clear and direct).
- Prohibiciones sin motivo ("NEVER use ellipses") en vez de explicar la razón (Add context).
- Decir qué NO hacer en vez de qué hacer (Control the format of responses).
- Ejemplos poco diversos o mezclados sin etiquetas con las instrucciones (Use examples effectively).
- Preguntar "can you suggest..." cuando se quiere que ejecute (Tool usage).
- Lenguaje en mayúsculas/agresivo ("CRITICAL: You MUST") y "If in doubt, use [tool]" -> sobre-activación en modelos recientes (Tool usage; Overthinking).
- Prescribir un plan de razonamiento paso a paso en vez de una instrucción general (Leverage thinking).
- Mantener instrucciones de verificación en Opus 5 (sobre-verificación) o de brevedad/estructura incompatibles con el modelo (Communication style; Control the format).
- Poner la consulta antes de documentos largos (Long context prompting).
- Prefill en el último turno en modelos 4.6+ (Migrating away from prefilled responses).
- Pedir "think" con thinking apagado en Opus 4.5 (Note en Leverage thinking).
- Sobre-ingeniería y hardcodeo para pasar tests si no se acota (Overeagerness; Avoid focusing on passing tests).

## 9. Ejemplos antes/después para la skill (de la página o adaptados)
| Petición original | Reescritura según la guía |
|---|---|
| `Create an analytics dashboard` | `Create an analytics dashboard. Include as many relevant features and interactions as possible. Go beyond the basics to create a fully-featured implementation.` (página) |
| `NEVER use ellipses` | `Your response will be read aloud by a text-to-speech engine, so never use ellipses since the text-to-speech engine will not know how to pronounce them.` (página) |
| `Can you suggest some changes to improve this function?` | `Change this function to improve its performance.` (página) |
| `Do not use markdown in your response` | `Your response should be composed of smoothly flowing prose paragraphs.` (página) |
| Resumir un informe largo (adaptado de Long context prompting) | Documentos en `<documents><document index="1"><source>..</source><document_content>..</document_content></document></documents>` arriba; después la tarea; pedir primero citas relevantes en `<quotes>` y luego el resumen. |

Esqueleto sugerido derivado de las secciones (adaptación, no cita textual): rol en una frase -> contexto/motivo -> datos largos arriba en XML -> tarea explícita en pasos si el orden importa -> formato de salida en positivo -> 3-5 ejemplos en `<examples>` si aplican -> criterio de verificación al final.

## Limitaciones
- Las secciones "Frontend design" y "Improved vision capabilities" se refieren solo a Opus 4.5/4.6 en la página.
- La página mezcla guía general con notas específicas por modelo (Opus 4.5/4.6/5, Fable 5.1, Sonnet 5.5...); la propia página dice que lo medido en un modelo debe re-evaluarse antes de aplicarlo a otro. La guía por modelo detallada vive en páginas separadas (no leídas aquí).
- La página no contiene reglas para ChatGPT ni para reescritura automática de prompts; esas aplicaciones son inferencia del consumidor de estas notas.

## Verificación
- Fecha: 2026-10-04.
- Cómo se leyó: WebFetch de la versión /docs/en/ devolvió el markdown de la página completa (~65 KB, con bloques de código, prompts y tablas literales), no un resumen; se contrastó línea a línea con estas notas.
- Comprobado: nombres de sección, citas literales (prompts de ejemplo, antes/después), nombres de parámetro (`budget_tokens`, `effort`, `thinking: {type: "adaptive"}`, `max_tokens`), etiquetas XML de los prompts, cifras (3-5 ejemplos, 20k+ tokens, hasta 30 %, ~100 %), y las notas por modelo (Opus 4.5/4.6/5, Fable 5.1, Sonnet 5.5). Todo coincidía; no se encontró ninguna afirmación sin respaldo ni cita deformada. Los elementos marcados como adaptación (esqueleto sugerido, fila de resumen en la tabla de la sección 9, apartado Limitaciones sobre ChatGPT) son inferencia declarada.
- Añadido: regla de re-evaluar por modelo y la tabla de guías por modelo; parámetro `output_config: {"effort"}` y valores por defecto de `thinking` por modelo; modelos con context awareness; reintento en continuaciones; alcance de Frontend/Vision.
- No se pudo comprobar: la versión /docs/es/ (no se intentó cargar); las páginas por modelo enlazadas (prompting-claude-*), la guía de effort y de thinking, que quedan fuera de la fuente única.
