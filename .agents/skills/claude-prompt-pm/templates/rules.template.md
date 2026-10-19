# Reglas del proyecto

Única fuente de reglas. `CLAUDE.md` y `AGENTS.md` solo apuntan aquí.
Escribe solo lo que el agente que programa (Claude, Codex u otro) no puede deducir del código (menos de 200 líneas). Si quitar una línea no haría que se equivoque, bórrala.

## Qué es
<una o dos frases: qué hace el proyecto y para quién>

## Comandos
- Instalar: `<comando>`
- Ejecutar: `<comando>`
- Tests (todos): `<comando>`
- Un solo test: `<comando>`
- Lint / typecheck / build: `<comando>`
- Entorno: <variables obligatorias (solo nombres, nunca valores) y particularidades de la máquina>

## Convenciones no obvias
- <estilo, nombres o patrones que no se deducen leyendo el código>

## Arquitectura (solo lo que exige leer varios ficheros)
- <flujo o decisión de diseño que no es evidente>
- Documentación: <fichero o URL> para <cuándo consultarlo>

## Git
- <nombre de ramas, formato de commit, PR y fusión, cómo revertir; o «sin Git»>

## Límites
- No modificar: <ficheros o carpetas>
- Antes de añadir una dependencia, proponerla.
- Los tests no se borran, saltan ni relajan para pasar.

## Verificación
Una tarea está terminada cuando: <comandos que deben pasar y cómo se comprueba>
