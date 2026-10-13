import type en from "./en";
const es: Record<keyof typeof en, string> = {
  demoBadge: "Ejemplo ilustrativo",
  demoTitle: "Una consulta, menos contexto.",
  demoQueryLabel: "Elige una consulta",
  demoQuery0: "¿Dónde está la función que calcula el total de una factura?",
  demoQuery1: "¿Cuándo vence una factura y de dónde sale ese plazo?",
  demoQuery2: "¿Cómo decide la aplicación si una factura está pagada, pendiente o vencida?",
  demoQuery3: "¿Dónde se añaden los datos del cliente a una factura nueva?",
  demoQuery4: "¿Dónde se da formato de dinero a un precio y en qué moneda?",
  demoView: "Vista del código",
  demoFull: "Archivo completo",
  demoRelevant: "Contexto relevante",
  demoCode: "Código fuente de la factura",
  demoLines: "líneas de código",
  demoFullNote:
    "Solo las líneas resaltadas responden a esta consulta. Desplázate para ver el archivo completo.",
  demoSelectionNote:
    "La función y su dependencia, con los números de línea originales. El resto queda fuera del contexto.",
  demoDisclaimer:
    "Ejemplo inventado. Las líneas solo ilustran la selección; no son un ahorro medido.",
  coordinate:
    "CÓDIGO → MAPA → RESPUESTA",
  localFirst: "LOCAL. CONECTADO. PRECISO.",
  fieldGuide: "GUÍA DE CAMPO",
  skip: "Saltar al contenido",
  nav: "Navegación principal",
  language: "Idioma",
  how: "Cómo funciona",
  capabilities: "Capacidades",
  docs: "Documentación",
  github: "Ver en GitHub",
  eyebrow: "UN MAPA DE TU CÓDIGO PARA TU ASISTENTE DE IA",
  headline: "Menos búsquedas.",
  headlineAccent: "Más comprensión.",
  intro:
    "Los asistentes de IA suelen leer archivos enteros solo para encontrar una cosa. mini-consumes-tokens crea en tu propio equipo un mapa consultable de tu proyecto, para que tu asistente pida solo las partes que necesita.",
  readDocs: "Explora la documentación",
  heroNote:
    "Código abierto · Gratis · Funciona en tu equipo",
  visibleLayers: "Capas visibles",
  sourcePreview:
    "Código fuente elegido",
  inspectFile:
    "Explorar archivo",
  exampleProject: "ESTE REPOSITORIO",
  projectBrief:
    "Este proyecto tal como lo ve su propio mapa: la parte que responde a tu asistente, el mapa guardado y la parte que lo construye. Tres archivos, seis piezas de código y solo las conexiones que el mapa encontró.",
  agentQuestion:
    "LA PREGUNTA DEL ASISTENTE",
  query0:
    "¿Qué responde a las peticiones de mi asistente?",
  query1:
    "¿Dónde lee el programa su configuración?",
  query2:
    "¿Qué guarda el mapa que leen todas las herramientas?",
  query3:
    "¿Qué ocurre cuando se analiza todo de nuevo?",
  query4:
    "¿Qué se ejecuta cuando solo cambian unos archivos?",
  query5:
    "¿Está el mapa al día y sano?",
  role0:
    "Responder a las peticiones del asistente",
  role1:
    "Cargar la configuración",
  role2:
    "Guardar el mapa",
  role3:
    "Analizar todo el proyecto",
  role4:
    "Analizar solo los archivos cambiados",
  role5:
    "Informar de la salud del mapa",
  purpose0:
    "La parte del programa que responde a tu asistente. Guarda el mapa y recuerda las respuestas recientes para que las preguntas repetidas sean rápidas. main.rs lo arranca y le da su configuración.",
  purpose1:
    "La configuración del servidor: cómo se recuerdan las respuestas recientes y si se busca por significado. from_env la lee al arrancar el programa.",
  purpose2:
    "Guarda la conexión con el mapa (una pequeña base de datos) y la carpeta del proyecto. El servidor le pide todo; cuando hay que trabajar, se lo pasa al analizador.",
  purpose3:
    "Recorre todo el proyecto, guarda lo que encuentra en cada archivo compatible, elimina los archivos que ya no existen y anota cuándo terminó.",
  purpose4:
    "Actualiza solo los archivos que le indican: creados, cambiados, borrados o renombrados. Si cambia la carpeta del proyecto o un archivo de exclusión, analiza todo de nuevo.",
  purpose5:
    "Cuenta los archivos y las piezas de código por lenguaje, lista los fallos y devuelve un breve informe de salud.",
  tourHowText:
    "Un lector basado en tree-sitter recorre cada archivo compatible y anota sus funciones, tipos y cómo se conectan. Ese mapa se guarda en una pequeña base de datos dentro de tu proyecto, y todo se ejecuta en tu equipo. El servidor responde a tu asistente con esa base. Mientras está activo, actualiza en segundo plano los archivos modificados cuando dejas de editar durante 1,5 segundos.",
  tourOrientText:
    "Empieza aquí cuando aún no conoces ningún nombre. Estas herramientas describen qué contiene un proyecto sin abrir archivos completos.",
  tourLookupText:
    "Cada herramienta responde a una pregunta e indica dónde mirar (archivo y línea) sin enviar el contenido del archivo.",
  tourCompositeText:
    "Estas herramientas combinan varias consultas en una sola respuesta. Las que siguen conexiones pueden además llegar más lejos, hasta 32 pasos, y mostrar los resultados largos por páginas.",
  tourAdvancedText:
    "Haz varias preguntas juntas, carga los detalles de las herramientas solo cuando los necesites y mantén el mapa al día.",
  ask_get_file_tree: "¿Qué carpetas y archivos tiene el proyecto?",
  ask_get_project_overview:
    "¿Cuáles son las partes más importantes de este archivo, carpeta o proyecto?",
  ask_list_symbols:
    "¿Qué se define en este archivo o carpeta, y en qué líneas?",
  ask_get_file_skeleton:
    "¿Qué aspecto tiene este archivo sin el interior de sus funciones?",
  ask_find_symbol:
    "¿Dónde se define esta función, clase o variable?",
  ask_find_callers:
    "¿Qué llama a esta función?",
  ask_find_calls:
    "¿Qué usa esta función y dónde se define cada cosa?",
  ask_find_references:
    "¿Dónde se usa esto en el proyecto?",
  ask_search_symbols:
    "Solo conozco parte del nombre. ¿Qué coincide con estas palabras?",
  ask_hybrid_search:
    "Sé qué hace el código, no cómo se llama. ¿Qué coincide por significado? Sin un modelo de lenguaje instalado, busca solo por palabras.",
  ask_impact_analysis:
    "¿Qué puede romperse si cambio esto? Qué lo usa, dónde se menciona y qué pruebas parecen afectadas.",
  ask_build_context_pack:
    "Todo lo necesario antes de editar algo: su definición, comentarios y código, más lo que lo usa, lo que usa y sus pruebas, cada cosa una sola vez. Ahorra leer los archivos.",
  ask_find_dead_code:
    "¿Qué funciones o clases no usa nada más? Un punto de partida para limpiar, no una prueba.",
  ask_batch:
    "Varias preguntas de solo lectura ya conocidas en una sola petición, cada respuesta bajo su propio título. No puede actualizar el mapa ni contener otros lotes.",
  ask_discover_tool_categories:
    "¿Qué herramientas hay? Nombres y un propósito de una línea, agrupados por tema.",
  ask_get_tool_schema:
    "¿Qué necesita exactamente una herramienta para usarse? Se carga solo cuando hace falta.",
  ask_reindex:
    "Actualiza el mapa ahora mismo. force vuelve a leer todos los archivos, aunque no hayan cambiado.",
  ask_get_indexing_status:
    "¿Está sano el mapa? Archivos y piezas de código por lenguaje, la última ejecución y los archivos que no se pudieron leer.",
  replaces: "En una llamada, en lugar de",
  toolsReference: "Consulta la referencia de herramientas MCP",
  atlas: "Mapa 3D del código",
  illustration:
    "Piezas de código y conexiones del propio mapa de este proyecto, no una vista en vivo",
  allSymbols:
    "Ver todas las piezas de código",
  hideSymbols:
    "Ocultar todas las piezas de código",
  allSymbolsTitle:
    "Todas las piezas de código de los tres archivos",
  symbolCount:
    "{{count}} piezas de código",
  graphAlt:
    "Los archivos se convierten en piezas de código conectadas. Al elegir una, se muestra dónde se define, qué la usa y de qué depende, en un resumen breve y preciso.",
  files:
    "Tus archivos",
  symbols:
    "Mapa del código",
  context:
    "Lo que recibe el asistente",
  select:
    "Elige una pieza de código para explorarla",
  selected:
    "Pieza de código elegida",
  definition:
    "Dónde se define",
  callers:
    "La usan",
  dependencies:
    "Usa",
  pack:
    "Resumen preciso",
  loading3d: "Cargando el mapa 3D…",
  fallback:
    "La vista 3D no está disponible en este dispositivo. Puedes explorar el código y su contexto a continuación.",
  rotate: "Girar el mapa 3D",
  resetView: "Restablecer vista",
  fullscreen: "Pantalla completa",
  exitFullscreen: "Salir de pantalla completa",
  tourWhatTitle:
    "Tu asistente consulta el mapa, no los archivos.",
  tourWhatText:
    "mini-consumes-tokens analiza tu proyecto y crea un mapa de sus funciones, tipos y métodos, y de cómo se usan entre sí. Un asistente de IA conectado por MCP (una forma estándar de enlazar herramientas con asistentes) puede entonces hacer preguntas precisas en vez de leer archivos completos.",
  tourHowTitle:
    "Leído, guardado y servido en tu equipo.",
  tourOrientTitle: "Oriéntate.",
  tourLookupTitle: "Haz una pregunta precisa.",
  tourCompositeTitle:
    "Varias preguntas, una petición.",
  tourAdvancedTitle:
    "Trabaja con proyectos grandes.",
  tourServerTitle:
    "Una petición llega al servidor.",
  tourServerText:
    "El servidor responde a cada petición de tu asistente. main.rs lo arranca con MctServer::new, que carga la configuración con ServerOptions::from_env. Guarda el Index y deja que una petición lo use cada vez.",
  tourIndexTitle:
    "El mapa guardado vive en un solo sitio.",
  tourIndexText:
    "Index guarda la conexión con la base de datos y la carpeta del proyecto, y main.rs lo abre con Index::open. Tres de sus métodos pasan el trabajo al analizador: reindex, reindex_paths y status.",
  tourReindexTitle:
    "Un análisis completo recorre todo el proyecto.",
  tourReindexText:
    "reindex avanza en cuatro pasos. Recorrer: walk_entry_allowed, record_walk_failure. Leer: index_file. Limpiar: remove_missing_files, remove_missing_manifests. Anotar la hora: touch_last_indexed_at.",
  tourReindexPathsTitle:
    "Un cambio solo analiza sus archivos.",
  tourReindexPathsText:
    "reindex_paths sigue los mismos cuatro pasos con diez ayudantes. Recorrer: to_relative_slash_path, is_rules_file, walk_entry_allowed, record_walk_failure. Leer: reindex, index_noting_created, record_read_failure. Limpiar: remove_under_prefix, remove_renamed_away. Anotar la hora: touch_last_indexed_at.",
  tourStatusTitle: "El estado sale de la misma base.",
  tourStatusText:
    "status cuenta los archivos y las piezas de código por lenguaje, los fallos y las dependencias, y construye un informe IndexStatus. Solo lo llama Index::status.",
  tourToolsTitle:
    "Ocho herramientas que llegan al mapa guardado.",
  tourToolsText:
    "Cada una de estas herramientas de server.rs le pregunta una cosa al mapa guardado. El número entre paréntesis es la línea donde lo hace.",
  tourMethodsTitle: "Ocho métodos de Index detrás de ellas.",
  tourMethodsText:
    "El lado de lib.rs de esas mismas peticiones: quién pide cada una o qué pasa al analizador.",
  spatialHint:
    "Arrastra para girar · Desplázate para acercar · Elige una pieza de código",
  mapNote: "Una pregunta. Las conexiones que importan.",
  problemLabel: "01 / EL PROBLEMA",
  problemTitle:
    "Tu proyecto está conectado. El contexto de tu asistente también debería estarlo.",
  problemText:
    "Leer archivos completos da mucho texto al asistente, pero no siempre cómo se relacionan las partes. Las búsquedas repetidas dificultan seguir un cambio por todo el proyecto.",
  before: "Una pila de archivos",
  beforeText: "Abrir archivos. Buscar nombres. Reconstruir relaciones.",
  after: "Un mapa del código",
  afterText:
    "Encontrar una función. Seguir sus conexiones. Pedir un resumen preciso.",
  comparisonNote:
    "Comparación conceptual, no una medición del ahorro. Los resultados dependen del proyecto y de la pregunta.",
  howLabel: "02 / EL MECANISMO",
  howTitle: "Del código a la comprensión.",
  step1Title:
    "Analiza en local.",
  step1Text:
    "Un lector recorre tu código y encuentra sus funciones, tipos y cómo se conectan. El resultado se guarda en una pequeña base de datos dentro de tu proyecto.",
  step2Title:
    "Consulta el mapa.",
  step2Text:
    "Un asistente de IA compatible con MCP (una forma estándar de conectar herramientas a asistentes) pregunta dónde se define algo, qué lo usa y de qué depende, en vez de recorrer los archivos una y otra vez.",
  step3Title: "Recibe el contexto adecuado.",
  step3Text:
    "Un resumen preciso reúne un fragmento de código y las partes conectadas con él. Cuando editas archivos, el mapa se actualiza solo poco después.",
  capabilitiesLabel: "03 / QUÉ PUEDES HACER",
  capabilitiesTitle: "Sigue el código.\nComprende el cambio.",
  discoverTitle: "Oriéntate en el proyecto",
  discoverText:
    "Obtén una visión general, el árbol de archivos o el esquema de un archivo. Encuentra funciones y tipos por su nombre antes de abrir su código.",
  changeTitle: "Sigue el alcance de un cambio",
  changeText:
    "Sigue qué usa a qué por todo el mapa. Comprueba qué partes y pruebas puede afectar un cambio.",
  contextTitle:
    "Dale a tu asistente un resumen preciso",
  contextText:
    "Obtén un resumen alrededor de una función o tipo, con su definición, dependencias y código relacionado, en una sola petición.",
  extras:
    "También disponible: encontrar código que parece sin uso (una estimación) y búsqueda local opcional por significado, que requiere una compilación especial y un pequeño modelo de lenguaje en tu equipo.",
  audience:
    "Para desarrolladores que usan asistentes de código de IA compatibles con MCP, en proyectos escritos en varios lenguajes. La herramienta de línea de comandos también funciona por separado.",
  startLabel: "04 / TU PRIMER MAPA",
  startTitle:
    "Empieza con tu propio proyecto.",
  startText:
    "Instala primero la herramienta de línea de comandos y el servidor siguiendo la documentación. Después ejecuta estos comandos dentro del proyecto que quieras analizar.",
  initLabel:
    "Crea el mapa de tu proyecto",
  registerLabel:
    "Conecta tu asistente de IA",
  statusLabel:
    "Comprueba que el mapa está sano",
  copy: "Copiar comando",
  copied: "Comando copiado",
  copyError: "No se puede copiar. Selecciona el comando y cópialo manualmente.",
  reconnect:
    "Reinicia o reconecta el asistente tras conectarlo. Algunos asistentes, como Codex, usan su propio formato de configuración; sigue su guía.",
  promptingLabel: "05 / PROMPTING PARA IA",
  promptingTitle: "Pídele a tu asistente que lo haga por ti.",
  promptingText:
    "Pega uno de estos prompts en tu asistente de IA. Añadirá la herramienta a tu proyecto, indexará el código y después usará el mapa en lugar de leer todos los archivos.",
  promptLabel1: "Añádelo a este proyecto e indéxalo",
  prompt1:
    "Añade el servidor MCP mini-consumes-tokens (https://github.com/Zubiarka8/mini-consumes-tokens) a este proyecto. Primero lee la guía de instalación en https://zubiarka8.github.io/mini-consumes-tokens/docs/installation/ y síguela para instalar la CLI. Después ejecuta `mct-cli --root . init` para crear el índice, `mct-cli --root . mcp-register`, y comprueba con `mct-cli --root . status`. No adivines los pasos; usa solo la guía. Avísame cuando el índice esté listo.",
  promptLabel2: "Encuentra código sin leer archivos enteros",
  prompt2:
    "Antes de abrir ningún archivo, usa las herramientas de mini-consumes-tokens (list_symbols, find_symbol, get_file_skeleton) para localizar el código que necesitas. Abre un archivo completo solo cuando tengas que leer su contenido.",
  promptLabel3: "Comprueba el impacto antes de un cambio",
  prompt3:
    "Quiero cambiar <symbol>. Usa impact_analysis de mini-consumes-tokens para listar sus llamadores, referencias y tests antes de editar nada, y resume el riesgo.",
  promptLabel4: "Actualiza el índice",
  prompt4:
    "Reindexa este proyecto con mini-consumes-tokens y dime si el índice está al día.",
  promptingNote:
    "Sustituye <symbol> por la función o clase que quieras cambiar. El asistente debe estar ya conectado al servidor MCP.",
  docsLabel: "EL MANUAL DE REFERENCIA",
  docsTitle: "De la instalación a entender tu código.",
  docsText:
    "Instalación, conexión de asistentes, comandos, herramientas, lenguajes compatibles, cómo funciona por dentro y resolución de problemas. El manual está en inglés.",
  guide1: "Instala y conecta",
  guide2: "Consulta y explora",
  guide3: "Comprende y amplía",
  faqLabel: "ALGUNAS BUENAS PREGUNTAS",
  faqTitle: "Antes de empezar.",
  faq1:
    "¿Dónde se guarda el mapa?",
  answer1:
    "En tu proyecto, en una pequeña base de datos local (SQLite). Todo se analiza en tu equipo. Tu asistente de IA puede enviar las respuestas de las herramientas a su proveedor, según la configuración de ese asistente.",
  faq2: "¿Funciona con cualquier asistente?",
  answer2:
    "El asistente necesita soporte MCP para usar las herramientas. También puedes usar la herramienta de línea de comandos por separado para analizar, comprobar la salud y encontrar código que parece sin uso.",
  faq3:
    "¿El mapa muestra todo lo que hace mi código?",
  answer3:
    "No. Solo entiende los lenguajes compatibles. Lo que solo se decide mientras el programa se ejecuta puede escaparse, y el código «sin uso» es una estimación, no una certeza. La documentación explica estos límites.",
  faq4: "¿Qué ocurre cuando edito un archivo?",
  answer4:
    "Mientras el servidor está conectado, detecta los archivos modificados y actualiza el mapa cuando dejas de editar. También puedes pedir un nuevo análisis o comprobar la salud desde la línea de comandos.",
  faq5:
    "¿En qué se diferencia de una búsqueda de texto normal?",
  answer5:
    "Una búsqueda de texto devuelve las líneas que coinciden. El mapa devuelve dónde se define cada cosa y cómo se conectan: qué usa una función, de qué depende y dónde se menciona, para que tu asistente lea menos y entienda más.",
  faq6: "¿Qué lenguajes admite?",
  answer6:
    "Cada lenguaje tiene su propio lector, basado en tree-sitter (una conocida biblioteca para analizar código). La documentación enumera los lenguajes compatibles y añadir uno nuevo es un cambio autocontenido.",
  faq7: "¿Funciona en Windows, macOS y Linux?",
  answer7:
    "Sí. La misma CLI y el mismo servidor MCP funcionan en los tres, y las pruebas del proyecto se ejecutan en cada uno.",
  faq8: "¿Puedo excluir archivos o carpetas?",
  answer8:
    "Sí. Añade rutas a un archivo .mctignore para dejarlas fuera del mapa. Las carpetas de compilación y dependencias, como target y node_modules, ya se omiten.",
  faq9: "¿Puede buscar por significado, no solo por nombre?",
  answer9:
    "Sí, con la compilación opcional que añade la búsqueda por significado. Combina la coincidencia de nombres con un pequeño modelo de lenguaje que se ejecuta en tu equipo, así que puedes preguntar qué hace el código. Sin esa compilación, la búsqueda usa solo nombres.",
  faq10: "¿Es gratis?",
  answer10:
    "Sí. Es de código abierto con licencia Apache 2.0 y puedes leer, ejecutar y ampliar el código en GitHub.",
  footer: "Un mejor mapa es el inicio de un contexto preciso.",
  license: "Licencia Apache 2.0",
  top: "Volver al inicio",
  principle1:
    "Tu código",
  principle2:
    "Un mapa pequeño",
  principle3:
    "Tu asistente de IA",
};
export default es;
