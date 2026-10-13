import type { atlasEn } from "./atlas.en";

export const atlasEs: Record<keyof typeof atlasEn, string> = {
  visibleLayers: "Capas visibles",
  layerSpacing: "Separación de capas",
  sourcePreview: "Código fuente seleccionado",
  inspectFile: "Explorar archivo",
  exampleProject: "ESTE REPOSITORIO",
  projectBrief:
    "mini-consumes-tokens tal como lo ve su propio índice: el servidor MCP, el índice que guarda y el indexador que lo llena. Tres archivos, seis símbolos y solo las llamadas que el índice resuelve.",
  agentQuestion: "LA PREGUNTA DEL AGENTE",
  query0: "¿Qué responde a las llamadas a herramientas de mi asistente?",
  query1: "¿Dónde lee el servidor su configuración?",
  query2:
    "¿Quién es dueño de la base de datos que leen todas las herramientas?",
  query3: "¿Qué ocurre en una reindexación completa?",
  query4: "¿Qué se ejecuta cuando solo cambian unos archivos?",
  query5: "¿Está sano el índice?",
  role0: "Servir las herramientas MCP",
  role1: "Configurar el servidor",
  role2: "Guardar el índice",
  role3: "Indexar todo el proyecto",
  role4: "Indexar rutas cambiadas",
  role5: "Informar del estado del índice",
  purpose0:
    "El servidor MCP guarda el índice tras un mutex, junto al registro de lenguajes y una caché de consultas. main.rs lo crea con MctServer::new, que lee sus opciones del entorno.",
  purpose1:
    "Las opciones del servidor: la caché de consultas, un embedder opcional y una caché compartida. from_env lee la configuración de la caché de las variables MCT_CACHE*.",
  purpose2:
    "Index envuelve la conexión SQLite y la raíz del proyecto. Sus métodos son lo que llama el servidor; reindex, reindex_paths y status pasan el trabajo al indexador.",
  purpose3:
    "Recorre el proyecto, indexa cada archivo compatible, elimina los archivos y manifiestos que ya no existen y registra cuándo terminó.",
  purpose4:
    "Actualiza solo las rutas que recibe: creadas, cambiadas, borradas o renombradas. La raíz del proyecto o un archivo de exclusión cambiado vuelve a una reindexación completa.",
  purpose5:
    "Lee de la base los archivos y símbolos por lenguaje, los fallos y las dependencias de manifiestos, y los devuelve como un IndexStatus.",
  tourHowText:
    "Tree-sitter lee cada archivo compatible y extrae los símbolos y sus relaciones. Una base SQLite guarda ese grafo dentro de tu proyecto, y la indexación se ejecuta en tu equipo. El servidor MCP responde con esa base a las llamadas a herramientas del asistente. Mientras está activo, reindexa en segundo plano los archivos modificados cuando las ediciones llevan 1,5 segundos en pausa.",
  tourOrientText:
    "Empieza aquí cuando aún no conoces ningún nombre. Estas herramientas describen la estructura y el contenido de un proyecto sin abrir archivos completos.",
  tourLookupText:
    "Cada herramienta responde a una pregunta y devuelve ubicaciones como ruta:línea, no el contenido de los archivos.",
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
  toolsReference: "Consulta la referencia de herramientas MCP",
  atlas: "Atlas de código",
  illustration:
    "Símbolos y llamadas resueltas del propio índice de este repositorio, no una vista en vivo",
  allSymbols: "Ver todos los símbolos",
  hideSymbols: "Ocultar todos los símbolos",
  allSymbolsTitle: "Todos los símbolos de los tres archivos",
  symbolCount: "{{count}} símbolos",
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
  tourServerTitle: "Una llamada a herramienta llega al servidor.",
  tourServerText:
    "MctServer responde a todas las herramientas MCP. main.rs lo crea con MctServer::new, que llama a ServerOptions::from_env para configurar la caché. Guarda el Index tras un mutex.",
  tourIndexTitle: "El índice es dueño de la base de datos.",
  tourIndexText:
    "Index envuelve la conexión SQLite y la raíz del proyecto, y main.rs lo abre con Index::open. Tres de sus métodos pasan el trabajo al indexador: reindex, reindex_paths y status.",
  tourReindexTitle: "Una reindexación completa recorre el proyecto.",
  tourReindexText:
    "reindex avanza en cuatro pasos. Recorrer: walk_entry_allowed, record_walk_failure. Indexar: index_file. Limpiar: remove_missing_files, remove_missing_manifests. Registrar: touch_last_indexed_at.",
  tourReindexPathsTitle: "Un cambio solo reindexa sus rutas.",
  tourReindexPathsText:
    "reindex_paths agrupa igual sus diez llamadas. Recorrer: to_relative_slash_path, is_rules_file, walk_entry_allowed, record_walk_failure. Indexar: reindex, index_noting_created, record_read_failure. Limpiar: remove_under_prefix, remove_renamed_away. Registrar: touch_last_indexed_at.",
  tourStatusTitle: "El estado sale de la misma base.",
  tourStatusText:
    "status lee los archivos y símbolos por lenguaje, los fallos y las dependencias, y construye un IndexStatus. Solo lo llama Index::status.",
  tourToolsTitle: "Ocho herramientas que llegan al índice.",
  tourToolsText:
    "Cada una de estas herramientas de server.rs llama a un método de Index. El número entre paréntesis es la línea de esa llamada.",
  tourMethodsTitle: "Ocho métodos de Index detrás de ellas.",
  tourMethodsText:
    "El lado de lib.rs de esas mismas llamadas: quién llama a cada método o qué pasa al indexador.",
  spatialHint:
    "Arrastra para girar · Desplázate para acercar · Elige un símbolo",
};
