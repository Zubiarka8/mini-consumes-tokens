import type en from "./en";
const es: Record<keyof typeof en, string> = {
  demoBadge: "Ejemplo ilustrativo",
  demoTitle: "Una consulta, menos contexto.",
  demoQuery: "Localiza calculateTotal y su dependencia taxRate.",
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
    "Ejemplo sintético. Las líneas ilustran la selección; no son un ahorro de tokens medido.",
  coordinate: "CÓDIGO → SÍMBOLOS → CONTEXTO",
  localFirst: "LOCAL. CONECTADO. PRECISO.",
  fieldGuide: "GUÍA DE CAMPO",
  skip: "Saltar al contenido",
  nav: "Navegación principal",
  language: "Idioma",
  how: "Cómo funciona",
  capabilities: "Capacidades",
  docs: "Documentación",
  github: "Ver en GitHub",
  eyebrow: "UN ATLAS DE CÓDIGO PARA TU AGENTE DE IA",
  headline: "Menos búsquedas.",
  headlineAccent: "Más comprensión.",
  intro:
    "Dale a tu agente un mapa del repositorio. mini-consumes-tokens transforma el código fuente en un grafo local de símbolos para que tu asistente consulte el contexto que necesita.",
  readDocs: "Explora la documentación",
  heroNote: "Código abierto · CLI en Rust + servidor MCP",
  visibleLayers: "Capas visibles",
  layerSpacing: "Separación de capas",
  sourcePreview: "Código fuente seleccionado",
  inspectFile: "Explorar archivo",
  exampleProject: "PROYECTO DE EJEMPLO",
  projectBrief:
    "Un pequeño servicio en Rust: recibe una petición, la interpreta y valida sus datos. Sigue el mismo código a través de las tres capas.",
  agentQuestion: "LA PREGUNTA DEL AGENTE",
  query0: "¿Dónde se interpreta la petición y qué función la valida?",
  query1: "¿Qué ocurre cuando el servidor recibe una petición?",
  query2: "¿Dónde se rechazan los datos vacíos y quién usa esa comprobación?",
  role0: "Interpretar la petición",
  role1: "Procesar la petición",
  role2: "Validar los datos",
  purpose0:
    "El parser construye un Request y delega la validación. Su llamador y su dependencia explican dónde encaja en el recorrido de la petición.",
  purpose1:
    "El punto de entrada llama al parser y construye una respuesta. Seguir esa llamada lleva al código de interpretación y validación.",
  purpose2:
    "La validación normaliza los datos y rechaza un valor vacío. El llamador muestra cómo se llega a esta comprobación desde el parser.",
  tourHowText:
    "Tree-sitter lee cada archivo compatible y extrae los símbolos y sus relaciones. Una base SQLite guarda ese grafo dentro de tu proyecto, y la indexación se ejecuta en tu equipo. El servidor MCP responde con esa base a las llamadas a herramientas del asistente. Mientras está activo, reindexa en segundo plano los archivos modificados cuando las ediciones llevan 1,5 segundos en pausa.",
  tourOrientText:
    "Empieza aquí cuando aún no conoces ningún nombre. Estas herramientas describen la estructura y el contenido de un proyecto sin abrir archivos completos.",
  tourLookupText:
    "Cada herramienta responde a una pregunta y devuelve ubicaciones como ruta:línea, no el contenido de los archivos. Los ejemplos usan request-service.",
  tourCompositeText:
    "Las herramientas compuestas combinan varias consultas en una sola respuesta. Las herramientas de relaciones aceptan además depth, para seguir llamadores de llamadores hasta 32 saltos, y offset, para paginar resultados largos.",
  tourAdvancedText:
    "Agrupa consultas, carga los esquemas de las herramientas solo cuando los necesites y mantén el índice al día.",
  ask_get_file_tree: "¿Qué carpetas y archivos tiene el proyecto?",
  ask_get_project_overview:
    "¿Cuáles son las declaraciones más importantes de este archivo, carpeta o proyecto?",
  ask_list_symbols:
    "¿Qué se define en este archivo o carpeta, con su tipo y rango de líneas?",
  ask_get_file_skeleton:
    "¿Qué forma tiene este archivo sin el cuerpo de sus funciones?",
  ask_find_symbol: "¿Dónde se define este símbolo?",
  ask_find_callers: "¿Quién llama a esta función?",
  ask_find_calls: "¿A qué llama esta función y dónde se define cada una?",
  ask_find_references:
    "¿Dónde se usa este símbolo: llamadas, importaciones, herencia?",
  ask_search_symbols:
    "Solo conozco parte del nombre. ¿Qué símbolos coinciden con estas palabras?",
  ask_hybrid_search:
    "Sé qué hace el código, no cómo se llama. ¿Qué coincide por significado? Sin un modelo de embeddings ordena solo por palabras.",
  ask_impact_analysis:
    "¿Qué puede romperse si cambio esto? Sus llamadores, todas sus referencias y las pruebas que parecen afectadas.",
  ask_build_context_pack:
    "Todo lo necesario antes de editar este símbolo: su definición, su comentario de documentación y su código, más cada llamador, llamada, dependencia y prueba relacionada, una sola vez. Además ahorra leer los archivos.",
  ask_find_dead_code:
    "¿Qué símbolos de primer nivel no tienen ninguna referencia en el índice? Una heurística para empezar, no una prueba.",
  ask_batch:
    "Varias consultas de solo lectura ya conocidas en una sola llamada, cada respuesta bajo su propio encabezado. No admite reindex ni batch anidados.",
  ask_discover_tool_categories:
    "¿Qué herramientas hay? Nombres y propósito en una línea, por categorías, sin esquemas de entrada.",
  ask_get_tool_schema:
    "¿Cuál es el esquema de entrada completo de una herramienta? Se carga solo cuando el cliente lo necesita.",
  ask_reindex:
    "Actualiza el índice ahora mismo. force vuelve a analizar todos los archivos, aunque no hayan cambiado.",
  ask_get_indexing_status:
    "¿Está sano el índice? Archivos y símbolos por lenguaje, última ejecución y errores de análisis.",
  replaces: "En una llamada, en lugar de",
  exampleOutput: "Ejemplo de salida",
  toolsReference: "Consulta la referencia de herramientas MCP",
  atlas: "Atlas de código",
  illustration: "Repositorio ilustrativo, no un índice en vivo",
  graphAlt:
    "Los archivos se convierten en símbolos conectados. Al elegir un símbolo, se muestran su definición, sus llamadores y sus dependencias en un paquete de contexto preciso.",
  files: "Archivos fuente",
  symbols: "Grafo de símbolos",
  context: "Contexto del agente",
  select: "Elige un símbolo para explorarlo",
  selected: "Símbolo seleccionado",
  definition: "Definición",
  callers: "Lo llaman",
  dependencies: "Llama a",
  pack: "Paquete de contexto preciso",
  loading3d: "Cargando el atlas espacial…",
  fallback:
    "La vista 3D no está disponible en este dispositivo. Puedes explorar el código y su contexto a continuación.",
  rotate: "Girar el atlas",
  resetView: "Restablecer vista",
  fullscreen: "Pantalla completa",
  exitFullscreen: "Salir de pantalla completa",
  tourWhatTitle: "Tu agente consulta el mapa, no los archivos.",
  tourWhatText:
    "mini-consumes-tokens indexa tu repositorio en un grafo de símbolos (funciones, tipos, métodos) y de las llamadas e importaciones entre ellos. Un asistente de código conectado por MCP puede hacer preguntas precisas en vez de leer archivos completos.",
  tourHowTitle: "Analizado, guardado y servido en tu equipo.",
  tourOrientTitle: "Oriéntate.",
  tourLookupTitle: "Haz una pregunta precisa.",
  tourCompositeTitle: "Varias preguntas, una llamada.",
  tourAdvancedTitle: "Trabaja a escala.",
  spatialHint:
    "Arrastra para girar · Desplázate para acercar · Elige un símbolo",
  mapNote: "Una pregunta. Las conexiones que importan.",
  problemLabel: "01 / EL PROBLEMA",
  problemTitle:
    "Tu repositorio está conectado. Tu contexto también debería estarlo.",
  problemText:
    "Leer archivos completos aporta mucho texto, pero no siempre las relaciones que el asistente necesita. Las búsquedas repetidas dificultan seguir un cambio por todo el código.",
  before: "Una pila de archivos",
  beforeText: "Abrir archivos. Buscar nombres. Reconstruir relaciones.",
  after: "Un mapa del código",
  afterText:
    "Localizar un símbolo. Seguir sus conexiones. Pedir contexto preciso.",
  comparisonNote:
    "Comparación conceptual, no una medición de tokens. Los resultados dependen del repositorio y de la consulta.",
  howLabel: "02 / EL MECANISMO",
  howTitle: "Del código a la comprensión.",
  step1Title: "Indexa en local.",
  step1Text:
    "Tree-sitter analiza el código compatible y extrae símbolos y relaciones. Una base SQLite guarda el grafo dentro de tu proyecto.",
  step2Title: "Consulta el grafo.",
  step2Text:
    "Un asistente compatible con MCP consulta definiciones, referencias, llamadores y dependencias en vez de explorar archivos repetidamente.",
  step3Title: "Recibe el contexto adecuado.",
  step3Text:
    "Los paquetes de contexto reúnen una definición y sus conexiones relevantes. La indexación incremental actualiza los archivos cuando los cambios se estabilizan.",
  capabilitiesLabel: "03 / QUÉ PUEDES HACER",
  capabilitiesTitle: "Sigue el código.\nComprende el cambio.",
  discoverTitle: "Oriéntate en el proyecto",
  discoverText:
    "Explora una visión general, el árbol de archivos o el esquema de un archivo. Encuentra símbolos por su nombre antes de entrar en su código.",
  changeTitle: "Sigue el alcance de un cambio",
  changeText:
    "Recorre llamadores, llamadas y referencias en el grafo. Usa el análisis de impacto para examinar los símbolos y las pruebas afectados.",
  contextTitle: "Dale a tu agente un contexto preciso",
  contextText:
    "Crea un paquete alrededor de un símbolo con su definición, dependencias y código relacionado en una sola consulta.",
  extras:
    "También disponible: detección de posibles símbolos sin uso (una heurística) y búsqueda semántica local opcional, que requiere una compilación semántica y un modelo de embeddings.",
  audience:
    "Para desarrolladores que usan asistentes de código compatibles con MCP en repositorios con varios lenguajes admitidos. La CLI también funciona por separado.",
  startLabel: "04 / TU PRIMER MAPA",
  startTitle: "Empieza con tu propio repositorio.",
  startText:
    "Instala primero la CLI y el servidor siguiendo la documentación. Después ejecuta estos comandos dentro del proyecto que quieras indexar.",
  initLabel: "Crea el índice local",
  registerLabel: "Registra un cliente MCP",
  statusLabel: "Comprueba el estado del índice",
  copy: "Copiar comando",
  copied: "Comando copiado",
  copyError: "No se puede copiar. Selecciona el comando y cópialo manualmente.",
  reconnect:
    "Reconecta el asistente tras registrarlo. Algunos clientes, como Codex, necesitan su propio formato de configuración; consulta su guía.",
  docsLabel: "EL MANUAL DE REFERENCIA",
  docsTitle: "De la instalación a entender tu código.",
  docsText:
    "Instalación, configuración de clientes, comandos CLI, herramientas MCP, lenguajes compatibles, arquitectura y resolución de problemas. El manual existente está en inglés.",
  guide1: "Instala y conecta",
  guide2: "Consulta y explora",
  guide3: "Comprende y amplía",
  faqLabel: "ALGUNAS BUENAS PREGUNTAS",
  faqTitle: "Antes de empezar.",
  faq1: "¿Dónde se guarda el índice?",
  answer1:
    "En tu proyecto, en una base SQLite local. La indexación se ejecuta en tu equipo. Tu cliente de IA puede enviar respuestas de las herramientas a su proveedor según la configuración del cliente.",
  faq2: "¿Funciona con cualquier asistente?",
  answer2:
    "El asistente necesita soporte MCP para consultar las herramientas. Puedes usar la CLI por separado para indexar, comprobar el estado y detectar posibles símbolos sin uso.",
  faq3: "¿El grafo muestra todo lo que hace mi código?",
  answer3:
    "No. La extracción depende de los lenguajes y la sintaxis compatibles. El comportamiento dinámico puede no resolverse y los símbolos sin uso son candidatos, no certezas. La documentación explica estos límites.",
  faq4: "¿Qué ocurre cuando edito un archivo?",
  answer4:
    "Mientras el servidor MCP está conectado, un observador actualiza los archivos modificados cuando los cambios se estabilizan. También puedes solicitar una reindexación o comprobar el índice desde la CLI.",
  footer: "Un mejor mapa es el inicio de un contexto preciso.",
  license: "Licencia Apache 2.0",
  top: "Volver al inicio",
};
export default es;
