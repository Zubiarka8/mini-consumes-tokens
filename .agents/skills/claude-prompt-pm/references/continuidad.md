# Cuotas y continuidad de sesión

Hechos publicados sobre cuotas y mecanismos para retomar el trabajo, separados por superficie. La sección 5 contiene el procedimiento para cuota agotada o cambio de agente (C2). Las citas literales van en inglés. Todo se leyó de primera mano el 2026-10-05; las cifras cambian, así que vuelve a comprobarlas antes de usarlas y no las copies a `SKILL.md`.

## 1. Planes de Claude (claude.ai y Claude Code)

- Ventana de sesión y límite semanal: «Your session-based usage limit will reset every five hours. Pro plans also have a weekly usage limit that applies across all models.» No se publica la cifra semanal ni el número de mensajes. Fuente: https://support.claude.com/en/articles/8325606
- Cuota compartida: «Both Pro and Max plans offer usage limits that are shared across Claude and Claude Code, meaning all activity in both tools counts against the same usage limits.» Fuente: https://support.claude.com/en/articles/11145838
- Escala: «there's no fixed message count», «Pro gives you at least 5x more usage per 5-hour session than Free». Fuente: https://claude.com/pricing
- Créditos de uso al agotar la cuota: «switch to consumption-based pricing at standard API rates». Tienen «a daily redemption limit of $2000» y admiten un tope mensual. Fuente: https://support.claude.com/en/articles/12429409
- Modelos en Pro, según la tabla de https://claude.com/pricing: Opus, Sonnet y Haiku están incluidos; Fable, «Usage credits». Según https://support.claude.com/en/articles/15424964 (fechado el 2026-09-02), en Pro Fable 5 y 5.1 «aren't included in your plan's usage limits» y se usan con créditos. En Max consumen hasta el 50 % del límite semanal.

## 2. Claude Code

- Espera automática: «When a claude.ai usage limit stops Claude mid-task, Claude Code waits in the open session and continues the task on its own after the limit resets.» Está activa por defecto con suscripción de claude.ai y requiere la versión 2.1.234 o posterior. No se activa sola si el reinicio está a más de 24 horas («a weekly limit can reset days out»). Fuente: https://code.claude.com/docs/en/interactive-mode (sección «Wait for a usage limit to reset»)
- Retomar sesiones: `claude --continue` «Reopens the most recent conversation in the current directory»; `claude --resume` abre el selector de sesiones, o retoma una por nombre o ruta de transcripción. Fuente: https://code.claude.com/docs/en/sessions
- Checkpoints: «Checkpoints are designed for quick, session-level recovery. For permanent version history and collaboration, continue using version control, such as Git». Fuente: https://code.claude.com/docs/en/checkpointing

## 3. API

Fable se factura en la API «at standard API rates». Los límites de la API no se han consultado. Fuente: https://support.claude.com/en/articles/15424964

## 4. Codex (ChatGPT)

- ChatGPT Work y Codex comparten uso.
- Para Plus solo hay estimaciones de mensajes locales por cada 5 horas, según el modelo: GPT-6 Astra 5-45, GPT-6.1 Sol 15-160, GPT-6 Sol 15-150 y GPT-6 Luna 350-3.000. La propia página dice que «These estimates are not fixed message limits».
- «Weekly limits may also apply», pero no se publica ninguna cifra semanal.
- Fuente: https://learn.chatgpt.com/docs/pricing

## Sin verificar

- Límites de ChatGPT Plus fuera de Codex: help.openai.com devuelve 403.
- Precios de los planes.
- Cómo trata Codex un `AGENTS.md` que es un symlink. La página de `AGENTS.md` de Codex no documenta imports `@` (leída el 2026-10-05; ver `openai.md`, sección 3).

## Síntesis propia

Ningún mecanismo anterior conserva el estado del proyecto entre herramientas: la espera y `--continue` solo sirven dentro de Claude Code, y los checkpoints remiten a git. Por eso un relevo debería apoyarse en `STATE.md` y en commits por subtarea (`review-checklist.md`, sección 1), no en la sesión. El procedimiento está en la sección 5.

## 5. Procedimiento: cuota agotada o cambio de agente

Síntesis operativa propia. Los hechos con fuente se marcan «(§N)» y remiten a las secciones anteriores de este fichero, donde están la cita, la URL y la fecha. Las cuotas y mecanismos de 5.2 y 5.3 son de Claude Code. Si el implementador es Codex o ChatGPT, 5.1 y la idea de 5.4 sirven igual (registrar el estado, pegar `STATE.md` completo), pero el encargo se redacta con `openai.md` (síntesis propia).

### 5.1 Registrar el estado antes de cambiar nada

Pídeselo a Claude si aún responde; si no, al usuario desde el terminal:
- Si usa Git, trabajo sin commit y último commit: `git status --short` y `git log --oneline -1`; si no, registra los ficheros modificados que puedas comprobar y «ninguno» como último commit.
- Último comando ejecutado y su resultado literal.
- Tests o checks que fallan, con el comando exacto.
- Siguiente paso concreto.
- Referencia al último prompt enviado a Claude (fecha y primera línea).

Si hay Git y trabajo sin commit, el usuario decide si lo deja así o lo guarda en un commit marcado como incompleto. En ningún caso cuenta como verificado.

Anótalo en la plantilla de `STATE.md` (fase 5 de `SKILL.md`, igual que `templates/STATE.template.md`): commit, trabajo sin commitear, último comando, checks fallidos y último prompt van en **Punto de reanudación**; el siguiente paso, con su check, en **Siguiente paso**; los intentos abandonados, en **Descartado**; la cuota agotada, en **Riesgos y bloqueos**. Lo que no se haya comprobado se escribe «no verificado».

### 5.2 Decidir cómo seguir (lo decide el usuario)

Presenta las tres opciones sin cifras: los límites cambian y el usuario ve los suyos en su cuenta.
- **Esperar al reinicio en Claude Code.** Hay una ventana de sesión que se reinicia y un límite semanal (§1). La cuota se comparte con claude.ai (§1), así que usar el chat mientras tanto también la consume. Claude Code puede esperar solo en la sesión abierta, pero no lo hace si el reinicio queda lejos, como puede pasar con el límite semanal (§2, umbral exacto allí).
- **Usar créditos de uso**, solo si el usuario ya los tiene habilitados. Se cobran a precio de API (§1). Tiene coste: no lo propongas como opción por defecto ni lo des por activado.
- **Preparar un relevo** a otra sesión o agente (5.4). Ningún mecanismo de Claude Code conserva el contexto fuera de él (síntesis propia, ver arriba).

### 5.3 Esperar o retomar en Claude Code

Antes de continuar, comprueba:
- Si usa Git, que `git status --short` y `git log --oneline -1` coinciden con lo anotado en 5.1; si no, compara los ficheros con las copias disponibles y marca lo que no puedas comprobar. Si hay diferencias, actualiza el estado antes de seguir.
- Que los tests que fallaban siguen fallando igual, con el mismo comando.
- Si la espera automática continuó la tarea por su cuenta (§2), lee lo que hizo antes de enviar un prompt nuevo y revísalo con `review-checklist.md` (sección 3).

Mecanismos documentados en §2: espera automática en la sesión abierta; `claude --continue` o `claude --resume` para reabrir una sesión; checkpoints para recuperar dentro de la sesión, pero el historial permanente es git. Si la sesión se perdió, o tras dos correcciones fallidas (`review-checklist.md`, sección 4), abre una sesión nueva con el prompt de 5.4.

### 5.4 Prompt de relevo

Plantilla para una sesión nueva de Claude Code. Pega `STATE.md` completo, sin resumirlo ni rellenar huecos: lo no comprobado se deja «no verificado» y lo que no existe, «ninguno». Para otro destinatario, adapta el bloque final con `openai.md`, sección 4.

```
Estado: <STATE.md completo, tal cual, incluidos Punto de reanudación y Descartado>.
Reglas: lee `rules.md` antes de empezar.
Tarea: <una sola, la de Siguiente paso>. Fuera de alcance: <...>. No repitas lo que figura en Descartado.
Verificaciones pendientes: <check de Siguiente paso, Checks fallidos y campos «no verificado» de Punto de reanudación, con el resultado esperado>.
Antes de cambiar nada, comprueba que los ficheros coinciden con Punto de reanudación: con `git status --short` y `git log --oneline -1` si hay Git, o con las copias disponibles si no lo hay. Si no coincide o no puedes comprobarlo, para y dímelo.
<bloque de salvaguardas e informe final de review-checklist.md, secciones 1 y 2>
```
