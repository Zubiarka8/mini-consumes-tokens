# Encargos para ChatGPT y Codex

Reglas para redactar y revisar un encargo cuyo destinatario es ChatGPT o Codex. Las de Claude están en `best-practices.md`, `opus-5-5.md` y `sonnet-5-5.md`; no se aplican aquí sin evidencia (sección 5).

Fuentes, leídas íntegras en Markdown con curl el 2026-10-05:
- P = «Prompting», https://learn.chatgpt.com/docs/prompting (cubre ChatGPT Chat, ChatGPT Work y Codex).
- A = «Custom instructions with AGENTS.md», https://learn.chatgpt.com/docs/agent-configuration/agents-md (Codex).

Marcas: sin marca, resumen fiel de la fuente indicada; «(propio)», adaptación o decisión de esta skill.

Qué leer para qué: redactar para cualquiera de los dos, sección 1; para ChatGPT, 2; para Codex, 3; informe y revisión, 4; qué no trasladar desde las guías de Claude, 5.

## 1. Receta común (P, «Prompting overview» y siguientes)

- Un prompt corto suele bastar. Para tareas grandes o importantes, incluir lo que importe: **Goal**, **Context**, **Output**, **Boundaries**. Solo las partes útiles; no hay formato obligatorio.
- Empezar por el resultado, no por una lista de pasos. Describir el proceso solo cuando el proceso importa.
- Contexto: solo lo que pueda cambiar el resultado, y qué tomar de cada fuente.
- Límites: los «one or two boundaries that matter most», p. ej. «Flag missing information instead of guessing» o preparar un mensaje como borrador sin enviarlo. No controlar cada paso.
- Decir cómo se usará el resultado. En trabajo importante, pedir una comprobación final («flagging information it couldn't verify») y revisarlo después.
- Corregir con mensajes de seguimiento concretos en vez de empezar de cero.

(propio) La receta Objetivo / Contexto / Salida / Límites de la fase 1 de `SKILL.md` coincide con esta, así que el encargo conserva su forma; cambian los detalles de las secciones 2 a 4.

## 2. ChatGPT (P, «Prompting for ChatGPT Work»)

- P distingue Chat (preguntas, borradores, reescrituras cortas) de ChatGPT Work (varias fuentes o herramientas, secuencia de pasos, cambios o entregable grande).
- En Work: describir el resultado, dar el material, nombrar la audiencia y explicar cómo se revisará; pedir que planifique, reúna información, cree ficheros y los compruebe antes de terminar.
- Separar lo obligatorio de las mejoras opcionales. Pedir plan cuando el enfoque importa. «Require your approval before ChatGPT sends, publishes, or changes information other people rely on».
- Las fuentes conectadas requieren el plugin correspondiente y dependen del plan y de la configuración del espacio de trabajo. Un proyecto permite que varios chats compartan ficheros, fuentes o una carpeta local («Add useful context»).

(propio) P no detalla qué puede hacer ChatGPT sobre esa carpeta ni que ejecute comandos en el repositorio del usuario. Si el entorno no muestra esa capacidad, el encargo pide ficheros completos o un parche, y el usuario ejecuta los checks (sección 4).

## 3. Codex (P, «Prompting Codex»; A)

- Un buen prompt «names the behavior you want, points to the relevant code or reproduction steps, preserves important constraints, and says how to verify the change».
- Tarea de varios pasos: `/plan` para investigar y proponer antes de editar; `/goal` después, cuando Goal mode esté disponible.
- Contexto: la extensión de IDE incluye los ficheros abiertos; en la CLI hay que mencionar rutas (`@`, `/mention`).
- Los comandos locales corren en un sandbox que limita ficheros y red; para salir de él, Codex sigue la política de aprobación del usuario. En la nube, internet está desactivado durante la fase de agente salvo que se habilite.
- Bugs: pasos de reproducción y restricciones «matter more than a high-level description». Ejemplo oficial: reproducir primero, proponer el parche, ejecutar checks y «Report the commands and results».
- Revisión: `/review`, con foco opcional; en GitHub, `@codex review` si Code review está activado.
- `AGENTS.md` (A): Codex lo lee antes de trabajar; concatena el global y los de cada directorio desde la raíz del proyecto hasta el actual, con un tope por defecto de 32 KiB. A no documenta imports del tipo `@fichero`.

(propio) Si el `AGENTS.md` del proyecto solo remite a `rules.md`, el encargo dice explícitamente «lee `rules.md`»: que Codex abra el fichero por esa frase es inferencia, no hecho documentado. No pongas `@rules.md` en un prompt para Codex.

## 4. Informe y revisión según el destinatario (propio)

| Destinatario | Evidencia que se puede pedir | Informe |
|---|---|---|
| Codex con el repositorio | Diff, comandos y salidas, como Claude Code | Secciones 1 y 2 de `review-checklist.md`, más la dirección de la sección 3 de este fichero |
| ChatGPT sin acceso visible al repositorio | Ficheros completos o parche; nada ejecutado en el repositorio | Estado de cada criterio; ficheros entregados; comandos que debe ejecutar el usuario y resultado esperado; supuestos y lo no verificado. Nada de «tests pasan» |
| ChatGPT con acceso confirmado (conector, fichero de proyecto) | Solo lo que esa herramienta muestre | Pide que cite el fichero o la fuente usada |

La revisión sigue la sección 3 de `review-checklist.md`. Sin ejecución en el repositorio, los checks los ejecuta el usuario y su salida es la única evidencia válida.

## 5. Qué no trasladar desde las guías de Claude (propio)

No uses en encargos para ChatGPT o Codex, sin fuente de OpenAI que lo respalde: etiquetas XML como recomendación, parámetros de la API de Claude (`effort` y similares), comandos o funciones de Claude Code (`/clear`, modo plan con `Shift+Tab`, subagentes, `CLAUDE.md`, imports `@`) ni las reglas específicas de Opus o Sonnet. Tampoco hay en estas fuentes una guía por modelo de OpenAI: no inventes parámetros ni nombres de modelo.

## Verificación

Fecha: 2026-10-05. Método: `curl` de la versión `.md` de cada página (la propia página indica que existe), lectura íntegra.

Comprobado: todas las afirmaciones sin «(propio)» de las secciones 1 a 3, en P y A, con las secciones citadas.

Consultada y no usada: https://developers.openai.com/api/docs/guides/reasoning-best-practices.md (leída íntegra el mismo día). Trata modelos de razonamiento de la API (o3, o4-mini) frente a GPT-4.1; no consta que se aplique a los modelos actuales de ChatGPT o Codex, así que sus reglas («Avoid chain-of-thought prompts», delimitadores, «Formatting re-enabled») no se convierten en reglas aquí.

No comprobado: páginas enlazadas (sandbox, Goal mode, plugins, Code review, proyectos); capacidades concretas de cada plan o superficie; si ChatGPT Work ejecuta comandos sobre un repositorio local. Las notas de partida están en `research_notes/Buenas prácticas OpenAI/`; las afirmaciones de esas notas que no aparecen en P o A no se usaron.
