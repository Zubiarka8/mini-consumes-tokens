# Salvaguardas, informe final, revisión y escalado

Procedimientos para la fase 2 (revisar el plan), la fase 3 (redactar) y la fase 4 (revisar). Origen: informe `reports/Skill project manager IA.md`. Las ideas de fondo vienen de la guía oficial de Anthropic «Best practices for Claude Code» (code.claude.com/docs/en/best-practices): verificación ejecutable, evidencia en vez de afirmaciones, causa raíz sin suprimir errores, parar tras dos correcciones fallidas, test que falla antes del arreglo, revisor en contexto limpio y aviso de sobreingeniería. De la guía «Prompting best practices» (platform.claude.com) salen la regla de acciones destructivas y la señal de lógica ajustada a los tests. El artículo de ingeniería sobre harnesses de larga duración solo respalda la verificación extremo a extremo antes de dar una función por hecha. Las plantillas y listas concretas son síntesis propia: no existe un checklist oficial para revisores que no pueden ejecutar código, y nadie ha medido cuánto detectan estas señales.

Destinatario (propio, 2026-10-05): las secciones 1 a 7 se escribieron para Claude Code; donde dicen «Claude», léase «el implementador». Valen tal cual para un agente con terminal y repositorio (Claude Code o Codex). Para ChatGPT sin acceso visible al repositorio, el informe y la evidencia cambian según `openai.md`, sección 4. La sección 8 es solo de Claude Code.

## 1. Bloque de salvaguardas (copiar en cada prompt de implementación)

```
Límites:
- No modifiques, borres, omitas (skip/xfail/.only) ni relajes tests, lint, typecheck ni umbrales de cobertura. Si un test parece incorrecto, para y dímelo.
- No suprimas errores (ts-ignore, eslint-disable, catch vacío). Arregla la causa raíz.
- No cambies nada fuera del alcance indicado. Si hace falta, propónlo aparte.
- No añadas dependencias sin proponerlas antes (nombre y versión).
- Si algo no se puede verificar, dilo; no lo des por hecho.
- Antes de una acción destructiva, difícil de revertir o visible para otros (borrar ficheros o ramas, `git push --force`, `git reset --hard`, publicar o comentar fuera del repositorio), pregunta. No uses atajos como `--no-verify` ni descartes ficheros que no reconozcas.
- Si el proyecto usa Git, haz un commit por subtarea terminada y verificada y sigue las reglas del proyecto para rama, PR, fusión y reversión (en `rules.md`; si no las define, pregunta antes de crear ramas o PR). Si tienes que parar antes de acabar, deja el repositorio en un estado consistente y di qué queda sin commitear y por qué. Si no hay repositorio Git, dilo y no afirmes haber hecho commits.
```

Estas prohibiciones son consultivas: el modelo puede ignorarlas. La protección real está en el entorno (CI obligatorio, permisos, CODEOWNERS sobre ficheros de test). Recomiéndaselo al usuario en lugar de prometer que el prompt basta.

## 2. Informe final obligatorio (pedirlo al final del prompt)

```
Al terminar, responde con:
1. Cada criterio de aceptación: cumplido / no cumplido / no verificado.
2. `git diff --stat`; sin Git, la lista de ficheros tocados y su diff contra copias guardadas antes de editar.
3. Cada comando que ejecutaste, con su salida literal (tests, build, linter, typecheck), incluyendo recuento de pasados, fallidos y omitidos.
4. Dependencias nuevas, con nombre y versión.
5. Qué supusiste o no pudiste verificar.
```

Para bugs: pedir primero un test que falle y reproduzca el problema, y luego el arreglo. Así el test demuestra que prueba algo.

## 3. Checklist de revisión (fase 4)

Acota la revisión: marca solo desvíos de corrección o de requisitos declarados, no preferencias de estilo. Un revisor al que se pide "buscar huecos" casi siempre los encuentra y empuja a sobreingeniar.

Evidencia mínima:
- Estado de cada criterio de aceptación.
- Diff completo y lista de ficheros tocados; alarma si hay ficheros que la petición no mencionaba.
- Salida literal de tests con el comando exacto, más build, linter y typecheck.
- Ningún test borrado, saltado o relajado.
- Bug: test que falla antes y pasa después.
- Dependencias nuevas con nombre y versión; verifica que existen (los modelos inventan paquetes).
- Lista explícita de lo no verificado.

Señales de evidencia floja:
- "Todos los tests pasan" sin log.
- Lenguaje modal ("debería funcionar").
- Menos tests que en la rama base.
- Cambios en configuración de tests, CI, snapshots o cobertura.
- Tests que solo afirman contra mocks.
- Lógica que solo cubre las entradas de los tests (valores fijos o casos especiales para que pasen).

Control independiente: pide el log de CI del commit, no el que pega Claude. Si no coincide con lo afirmado, escala. Opcional: una sesión nueva de Claude que revise el diff contra los criterios, solo corrección y alcance.

## 4. Cuándo escalar a un humano técnico

- El cambio toca autenticación, pagos, permisos, datos personales, migraciones de base de datos, infraestructura, secretos o producción.
- Falta CI, o contradice la evidencia.
- El diff modifica tests, lint o umbrales, o añade supresiones.
- Aparecen dependencias desconocidas o ficheros fuera de alcance.
- Hace falta una tercera ronda de corrección sobre lo mismo. Tras dos fallos, lo oficial es reiniciar la sesión con un prompt mejor en vez de seguir iterando.
- La acción es irreversible.

## 5. Tamaño de tarea

Una tarea es válida si su diff se describe en una frase y tiene un check ejecutable. Ningún estudio fija un tamaño óptimo en líneas o minutos. Reevalúa la granularidad con cada modelo nuevo.

## 6. Revisar el plan del implementador (fase 2)

Origen del paso «plan antes de implementar», según el implementador:
- Claude Code: respaldado por la guía de Claude Code («Explore first, then plan, then code»; modo plan en la sección 8).
- Codex y ChatGPT: lo que respalda `openai.md` y nada más: en Codex, `/plan` para investigar y proponer antes de editar (sección 3); en ChatGPT Work, pedir un plan cuando el enfoque importa (sección 2). Aplicarles los criterios de abajo es adaptación propia.
- El asistente activo para sí mismo: adaptación propia; ninguna fuente lo trata. Si el usuario ya pidió la implementación, aplica estos criterios a tu propio plan y sigue sin pedir aprobación. Para y consulta solo si el usuario pidió revisar el plan, o si hay una decisión bloqueante, un cambio de alcance o una acción irreversible que requiera su intervención (sección 4).

Los criterios son síntesis propia y valen para cualquier implementador. Antes de aprobar el plan, comprueba:
- Alcance: cubre el objetivo y nada de lo que quedó fuera.
- Ficheros: nombra los que tocará; alarma si aparecen ficheros que la tarea no explica.
- Orden: pasos pequeños, cada uno con su check ejecutable.
- Riesgos: señala lo irreversible o lo que toca la sección 4; si aparece, escala antes de implementar.
- Supuestos: lo que el implementador dio por hecho sin verlo en el repositorio; pregúntalo o márcalo como pendiente.

Si el plan falla en algo, devuélvelo con el punto concreto; no lo corrijas tú en el prompt de implementación.

## 7. Ejemplo de prompt de implementación

Ejemplo inventado para fijar la forma; el proyecto, los ficheros y los comandos no existen. El patrón es el mismo para cualquier tipo de tarea; solo los bugs tienen regla propia con fuente (sección 2). Para un refactor, pedir que los tests existentes pasen sin modificarlos es inferencia, no regla de la guía.

````
Objetivo: añade la opción `--format csv` al comando `report export`, que hoy solo exporta JSON.

Contexto: lee `rules.md`. El exportador JSON está en `src/export/json.ts`; imita su estructura en `src/export/csv.ts`.

Fuera de alcance: otros formatos, cambios en el JSON actual y la interfaz web.

Modo: explora y propón un plan antes de implementar. Espera mi aprobación.

Criterios de aceptación:
- `npm test -- tests/export` pasa, con al menos un test nuevo para CSV (cabecera y una fila con comas escapadas).
- `report export --format csv` sobre `fixtures/sample.json` produce un CSV con una línea de cabecera y una por registro.
- `npm run lint` y `npm run typecheck` terminan sin errores.

<bloque de salvaguardas de la sección 1>

<informe final de la sección 2>
````

## 8. Funciones de Claude Code en este flujo

Fuente de todos los hechos: https://code.claude.com/docs/en/best-practices (sección entre paréntesis). La columna «Uso en la skill» es decisión propia: ChatGPT no ejecuta comandos, así que lo que es acción del usuario se le recomienda, no se mete en el prompt. Ninguna fila es obligatoria en todos los prompts.

| Tema | Hecho documentado | Uso en la skill (propio) |
|---|---|---|
| Modo plan | Separa exploración de ejecución: Claude lee y responde sin cambiar nada. Útil si el enfoque es incierto, el cambio toca varios ficheros o el código es desconocido; «If you could describe the diff in one sentence, skip the plan» («Explore first, then plan, then code»). | Aplica. El prompt pide plan solo en tareas largas o arriesgadas (fase 2, sección 6); las tareas de una frase van directas. Activar el modo (`Shift+Tab`) lo decide el usuario. |
| `/clear` | Reinicia el contexto entre tareas no relacionadas; tras corregir más de dos veces lo mismo, `/clear` y prompt nuevo más específico («Course-correct early and often», «Avoid common failure patterns»). | Aplica. Recomendar al usuario `/clear` antes de una tarea no relacionada con la anterior y tras dos correcciones fallidas (sección 4). Si la tarea nueva continúa la anterior (siguiente paso del mismo plan), conservar el contexto o limpiarlo es criterio del usuario; la guía no lo decide. |
| Subagentes | Contexto propio; sirven para investigar sin llenar la conversación principal y para revisar el diff en contexto limpio, viendo solo el diff y los criterios («Use subagents for investigation», «Add an adversarial review step»). | Opcional. Pedirlo en el prompt para explorar un repositorio grande o como revisión independiente (sección 3); no en cada prompt. |
| `CLAUDE.md` | Se lee al inicio de cada conversación; corto, solo lo que aplica siempre; excluye lo deducible del código y lo que cambia a menudo; admite imports `@path/to/import` («Write an effective CLAUDE.md»). | Aplica. `templates/CLAUDE.template.md` importa `rules.md` con `@rules.md`; el prompt remite a `rules.md` sin copiarlo (fase 3, paso 5); el estado cambiante va a `STATE.md`, no a `CLAUDE.md`. |

## 9. Alcance de modelos e idioma (v1)

Reglas propias de la versión 1, no de las guías.

- Modelos: la skill solo tiene guía para Opus 5.5 y Sonnet 5.5 (`opus-5-5.md`, `sonnet-5-5.md`). Si el destinatario es ChatGPT o Codex, usa `openai.md`, que no tiene guía por modelo: no fijes modelo ni parámetros salvo que el usuario los dé. Si el usuario pide otro modelo de Claude, dile que no hay guía específica para él y pide confirmación antes de adaptar el prompt. Si confirma, usa solo `best-practices.md`, sin parámetros ni reglas propias de ese modelo.
- Idioma: vale para cualquier destinatario. El prompt, el veredicto y las explicaciones propias van en español. Los bloques literales y los ejemplos citados de las fuentes se dejan en su idioma; `sonnet-5-5.md` pide no traducir sus prompts de ejemplo.

## Verificación

Fecha: 2026-10-04.

Comprobado en https://code.claude.com/docs/en/best-practices (todas las atribuciones son exactas):
- Verificación ejecutable como palanca principal: sección «Give Claude a way to verify its work».
- Evidencia en vez de afirmaciones: «Have Claude show evidence rather than asserting success».
- «address the root cause, don't suppress the error»: cita literal en la tabla de esa sección.
- Parar tras dos correcciones: «corrected Claude more than twice… /clear and start fresh with a more specific prompt» y «After two failed corrections, /clear…».
- Test que falla antes del arreglo: ejemplo «write a failing test that reproduces the issue, then fix it».
- Revisor que busca huecos y empuja a sobreingeniería: aviso en «Add an adversarial review step» («will usually report some… leads to over-engineering»; flagear solo corrección o requisitos).
- Sesión nueva o subagente revisor en contexto limpio: «A fresh context improves code review» y «Writer/Reviewer pattern».

Artículo de harnesses (anthropic.com/engineering/effective-harnesses-for-long-running-agents): respalda solo que Claude marca funciones como completas sin pruebas extremo a extremo y que hay que autoverificar. No trata causa raíz, test que falla primero ni revisores en contexto limpio (menciona agentes de QA solo como posibilidad futura).

Corregido: la cabecera citaba «guías oficiales» sin fuente; ahora nombra la guía concreta y delimita lo que aporta el artículo de harnesses. Plantillas sin cambios.

No se pudo comprobar: la herramienta de lectura (WebFetch) devuelve extractos procesados por un modelo pequeño, no el texto íntegro; el de best-practices parecía casi completo, el del artículo de harnesses es un resumen, así que las ausencias indicadas ahí son menos firmes. La frase «lo oficial» sobre la tercera ronda (sección 4) se apoya en la regla de las dos correcciones, pero «tercera ronda» como umbral de escalado a un humano es síntesis propia. Tampoco se comprobaron otras páginas de Anthropic.

2026-10-05: añadidas la línea de commits en la sección 1 y las secciones 6 y 7. Comprobado con curl en la misma página de best-practices que existe la sección «Explore first, then plan, then code» y el uso de plan mode. Los criterios de la sección 6, la línea de commits y el ejemplo de la sección 7 son síntesis propia.

2026-10-05 (E4): añadida la sección 8. Leído con curl el Markdown íntegro de https://code.claude.com/docs/en/best-practices.md (misma página que la URL de Fuentes). Comprobado en el texto: plan mode, Shift+Tab, «without making changes» y «skip the plan» en «Explore first, then plan, then code»; `/clear` y la regla de más de dos correcciones en «Course-correct early and often» y «Avoid common failure patterns»; subagentes en «Use subagents for investigation» y «Add an adversarial review step»; contenido, exclusiones e imports de CLAUDE.md en «Write an effective CLAUDE.md». No se usaron las páginas de subagentes ni de memoria (se descargaron, pero la tabla no depende de ellas), así que límites como profundidad de imports o número de subagentes quedan fuera. La columna «Uso en la skill» es síntesis propia; que recomendar `/clear` antes de cada tarea nueva mejore el resultado en este flujo no está medido.

2026-10-05 (cierre v1): Git condicional en las secciones 1 y 2 (sin repositorio, se dice y se compara con copias); fila de `/clear` de la sección 8 limitada a lo que dice la guía, tareas no relacionadas, en vez de «cada tarea nueva»; sección 9 añadida. Todo es síntesis propia, sin fuente nueva.

2026-10-05 (auditoría de buenas prácticas): leído con curl el Markdown íntegro de https://platform.claude.com/docs/en/build-with-claude/prompt-engineering/claude-prompting-best-practices.md y de https://code.claude.com/docs/en/best-practices.md. Añadidas dos líneas:
- Sección 1, acciones destructivas: adaptación del prompt de ejemplo de «Balancing autonomy and safety» (pedir confirmación ante acciones destructivas, difíciles de revertir o visibles para otros; no usar `--no-verify` ni descartar ficheros desconocidos). La página lo presenta para Opus 4.6; `opus-5-5.md` y `sonnet-5-5.md` piden mantener reglas propias de confirmación para acciones arriesgadas, y el bloque no tenía ninguna.
- Sección 3, señal de lógica ajustada a los tests: sale de «Avoid focusing on passing tests and hardcoding» («Do not hard-code values or create solutions that only work for specific test inputs»). Usarla como señal de revisión es decisión propia.
En `templates/rules.template.md` se añadieron tres campos de la tabla «Include/Exclude» de «Write an effective CLAUDE.md»: Entorno («Developer environment quirks (required env vars)»), Git («Repository etiquette (branch naming, PR conventions)»; la sección 1 ya remitía a `rules.md` para rama y PR, pero la plantilla no tenía dónde escribirlo) y Documentación («link to docs instead»; el «cuándo consultarlo» es adaptación propia). Lo que ya cubrían este fichero, `best-practices.md` y `SKILL.md` (verificación, evidencia, causa raíz, test que falla primero, revisor en contexto limpio, contexto concreto, sobreingeniería) no se repitió. Sin probar: si estas líneas cambian el comportamiento de Claude en una sesión real. El contraste con dos artículos de OpenAI no aportó ninguna regla nueva; uno no se pudo leer (403).

2026-10-05 (reparto de papeles): añadidos el párrafo «Destinatario» de la cabecera y la línea de ChatGPT/Codex de la sección 9. Síntesis propia, sin fuente nueva; las reglas de OpenAI y su verificación están en `openai.md`.

2026-10-05 (corrección): la sección 6 pasa a «plan del implementador» y separa el origen por implementador. Lo atribuido a OpenAI se limita a lo que recoge `openai.md` (secciones 2 y 3); el resto se marca como adaptación propia. Sin fuente nueva.
