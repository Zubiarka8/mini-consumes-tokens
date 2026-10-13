const en = {
  demoBadge: "Illustrative example",
  demoTitle: "One query, less context.",
  demoQuery: "Locate calculateTotal and its taxRate dependency.",
  demoView: "Code view",
  demoFull: "Full file",
  demoRelevant: "Relevant context",
  demoCode: "Invoice source code",
  demoLines: "lines of code",
  demoFullNote:
    "Only the highlighted lines answer this query. Scroll to explore the full file.",
  demoSelectionNote:
    "The function and its dependency, with original line numbers. The rest stays out of the context.",
  demoDisclaimer:
    "Synthetic example. Line counts illustrate the selection; they are not measured token savings.",
  coordinate: "SOURCE → SYMBOLS → CONTEXT",
  localFirst: "LOCAL FIRST. RELATIONSHIP AWARE.",
  fieldGuide: "FIELD GUIDE",
  skip: "Skip to content",
  nav: "Main navigation",
  language: "Language",
  how: "How it works",
  capabilities: "Capabilities",
  docs: "Documentation",
  github: "View on GitHub",
  eyebrow: "A CODE ATLAS FOR YOUR AI AGENT",
  headline: "Less searching.",
  headlineAccent: "More understanding.",
  intro:
    "Give your coding agent a map of your repository. mini-consumes-tokens turns source code into a local symbol graph, so your assistant can ask for the context that matters.",
  readDocs: "Explore the documentation",
  heroNote: "Open source · Rust CLI + MCP server",
  visibleLayers: "Visible layers",
  layerSpacing: "Layer spacing",
  sourcePreview: "Selected source code",
  inspectFile: "Inspect file",
  exampleProject: "THIS REPOSITORY",
  projectBrief:
    "mini-consumes-tokens as its own index sees it: the MCP server, the index it holds and the indexer that fills it. Three files, six symbols and only the calls the index resolves.",
  agentQuestion: "THE AGENT’S QUESTION",
  query0: "What answers my assistant's tool calls?",
  query1: "Where does the server read its settings?",
  query2: "What owns the database every tool reads?",
  query3: "What happens on a full reindex?",
  query4: "What runs when only a few files change?",
  query5: "Is the index healthy?",
  role0: "Serve the MCP tools",
  role1: "Configure the server",
  role2: "Hold the index",
  role3: "Index the whole project",
  role4: "Index changed paths",
  role5: "Report index health",
  purpose0:
    "The MCP server keeps the index behind a mutex, with the language registry and a query cache. main.rs builds it with MctServer::new, which reads its options from the environment.",
  purpose1:
    "The server's options: the query cache, an optional embedder and a shared cache. from_env reads the cache settings from the MCT_CACHE* variables.",
  purpose2:
    "Index wraps the SQLite connection and the project root. Its methods are what the server calls; reindex, reindex_paths and status hand the work to the indexer.",
  purpose3:
    "Walks the project, indexes each supported file, removes the files and manifests that are gone and records when the run finished.",
  purpose4:
    "Updates only the paths it is given: created, changed, deleted or renamed. The project root or a changed ignore file falls back to a full reindex.",
  purpose5:
    "Reads files and symbols per language, failures and manifest dependencies from the database and returns them as an IndexStatus.",
  tourHowText:
    "Tree-sitter reads each supported file and extracts symbols and the relations between them. A SQLite database keeps that graph inside your project, and indexing runs on your machine. The MCP server answers your assistant's tool calls from that database. While it runs, it reindexes changed files in the background once edits have been quiet for 15 seconds.",
  tourOrientText:
    "Start here when you don't know any names yet. These tools describe the layout and contents of a project without opening whole files.",
  tourLookupText:
    "Each tool answers one question and returns locations as path:line, not file contents.",
  tourCompositeText:
    "Composite tools combine several lookups into one answer. Relation tools also accept depth, to follow callers of callers up to 32 hops, and offset, to page through long results.",
  tourAdvancedText:
    "Group queries, load tool schemas only when needed and keep the index up to date.",
  ask_get_file_tree: "Which folders and files does the project have?",
  ask_get_project_overview:
    "What are the most important declarations in this file, folder or project?",
  ask_list_symbols:
    "What is defined in this file or folder, with kind and line range?",
  ask_get_file_skeleton:
    "What does this file look like without its function bodies?",
  ask_find_symbol: "Where is this symbol defined?",
  ask_find_callers: "Who calls this function?",
  ask_find_calls:
    "What does this function call, and where is each callee defined?",
  ask_find_references:
    "Where is this symbol used at all: calls, imports, inheritance?",
  ask_search_symbols:
    "I only know part of the name. Which symbols match these words?",
  ask_hybrid_search:
    "I know what the code does, not its name. What matches by meaning? Without an embedding model it ranks by words only.",
  ask_impact_analysis:
    "What could break if I change this? Its callers, every reference and the tests that look affected.",
  ask_build_context_pack:
    "Everything needed before editing this symbol: its definition, doc comment and source, plus each caller, callee, dependency and related test, listed once. It also saves reading the files.",
  ask_find_dead_code:
    "Which top-level symbols does nothing in the index reference? A heuristic starting point, not proof.",
  ask_batch:
    "Several known read-only queries in one call, each answer under its own header. reindex and nested batches are not allowed.",
  ask_discover_tool_categories:
    "Which tools exist? Names and one-line purposes, grouped, without input schemas.",
  ask_get_tool_schema:
    "What is one tool's full input schema? Loaded only when the client needs it.",
  ask_reindex:
    "Refresh the index right now. force parses every file again, even unchanged ones.",
  ask_get_indexing_status:
    "Is the index healthy? Files and symbols per language, last run and parse failures.",
  replaces: "In one call, instead of",
  toolsReference: "Read the MCP tools reference",
  atlas: "Code atlas",
  illustration:
    "Symbols and resolved calls from this repository's own index, not a live view",
  allSymbols: "See all symbols",
  hideSymbols: "Hide all symbols",
  allSymbolsTitle: "Every symbol in the three files",
  symbolCount: "{{count}} symbols",
  graphAlt:
    "Source files become connected symbols. Selecting a symbol reveals its definition, callers and dependencies in a focused context pack.",
  files: "Source files",
  symbols: "Symbol graph",
  context: "Agent context",
  select: "Choose a symbol to inspect",
  selected: "Selected symbol",
  definition: "Definition",
  callers: "Called by",
  dependencies: "Calls",
  pack: "Focused context pack",
  loading3d: "Loading the spatial atlas…",
  fallback:
    "The 3D view is unavailable on this device. You can still explore the source and context below.",
  rotate: "Rotate the atlas",
  resetView: "Reset view",
  fullscreen: "Full screen",
  exitFullscreen: "Exit full screen",
  tourWhatTitle: "Your agent asks the map, not the files.",
  tourWhatText:
    "mini-consumes-tokens indexes your repository into a graph of symbols (functions, types, methods) and the calls and imports between them. A coding assistant connected over MCP can then ask precise questions instead of reading whole files.",
  tourHowTitle: "Parsed, stored and served on your machine.",
  tourOrientTitle: "Get your bearings.",
  tourLookupTitle: "Ask one precise question.",
  tourCompositeTitle: "Several questions, one call.",
  tourAdvancedTitle: "Work at scale.",
  tourServerTitle: "A tool call reaches the server.",
  tourServerText:
    "MctServer answers every MCP tool. main.rs builds it with MctServer::new, which calls ServerOptions::from_env for the cache settings. It holds the Index behind a mutex.",
  tourIndexTitle: "The index owns the database.",
  tourIndexText:
    "Index wraps the SQLite connection and the project root, and main.rs opens it with Index::open. Three of its methods hand the work to the indexer: reindex, reindex_paths and status.",
  tourReindexTitle: "A full reindex walks the project.",
  tourReindexText:
    "reindex runs in four steps. Walk: walk_entry_allowed, record_walk_failure. Index: index_file. Clean up: remove_missing_files, remove_missing_manifests. Record: touch_last_indexed_at.",
  tourReindexPathsTitle: "A change reindexes only its paths.",
  tourReindexPathsText:
    "reindex_paths groups its ten callees the same way. Walk: to_relative_slash_path, is_rules_file, walk_entry_allowed, record_walk_failure. Index: reindex, index_noting_created, record_read_failure. Clean up: remove_under_prefix, remove_renamed_away. Record: touch_last_indexed_at.",
  tourStatusTitle: "Health comes from the same database.",
  tourStatusText:
    "status reads files and symbols per language, failures and dependencies, and builds an IndexStatus. Only Index::status calls it.",
  tourToolsTitle: "Eight tools that reach the index.",
  tourToolsText:
    "Each of these server.rs tools calls one Index method. The number in brackets is the line of that call.",
  tourMethodsTitle: "Eight Index methods behind them.",
  tourMethodsText:
    "The lib.rs side of the same calls: who calls each method, or what it hands to the indexer.",
  spatialHint: "Drag to orbit · Scroll to zoom · Select a symbol",
  mapNote: "One question. Its relevant neighborhood.",
  problemLabel: "01 / THE PROBLEM",
  problemTitle: "Your repository is connected. Your context should be, too.",
  problemText:
    "Reading whole files gives an assistant a lot of text, but not always the relationships it needs. Repeated searches make it harder to follow a change across the codebase.",
  before: "A pile of files",
  beforeText: "Open files. Search names. Piece together relationships.",
  after: "A map of the code",
  afterText: "Locate a symbol. Follow its edges. Ask for focused context.",
  comparisonNote:
    "A conceptual comparison, not a token benchmark. Results depend on the repository and the query.",
  howLabel: "02 / THE MECHANISM",
  howTitle: "From source to understanding.",
  step1Title: "Index locally.",
  step1Text:
    "Tree-sitter parses supported source into symbols and relationships. A SQLite database keeps the graph in your project.",
  step2Title: "Ask the graph.",
  step2Text:
    "An MCP-capable assistant queries definitions, references, callers and dependencies instead of repeatedly scanning files.",
  step3Title: "Bring the right context.",
  step3Text:
    "Context packs assemble a definition and its relevant neighborhood. Incremental indexing refreshes changed files as edits settle.",
  capabilitiesLabel: "03 / WHAT YOU CAN DO",
  capabilitiesTitle: "Follow the code.\nUnderstand the change.",
  discoverTitle: "Find your bearings",
  discoverText:
    "Explore a project overview, file tree or file skeleton. Find symbols by name before diving into their source.",
  changeTitle: "Trace a change",
  changeText:
    "Follow callers, callees and references across the graph. Use impact analysis to inspect affected symbols and tests.",
  contextTitle: "Give an agent a focused brief",
  contextText:
    "Build a context pack around a symbol, with its definition, dependencies and related code in one query.",
  extras:
    "Also available: candidate dead-code detection (a heuristic) and optional local semantic search, which requires a semantic build and an embedding model.",
  audience:
    "For developers using MCP-capable coding assistants in repositories with multiple supported languages. The CLI also works on its own.",
  startLabel: "04 / YOUR FIRST MAP",
  startTitle: "Start with your own repository.",
  startText:
    "Install the CLI and server first using the documentation. Then run these commands inside the project you want to index.",
  initLabel: "Build the local index",
  registerLabel: "Register an MCP client",
  statusLabel: "Check indexing health",
  copy: "Copy command",
  copied: "Command copied",
  copyError: "Copy unavailable. Select the command and copy it manually.",
  reconnect:
    "Reconnect your assistant after registration. Some clients, including Codex, need their own configuration format; follow the client guide.",
  docsLabel: "THE REFERENCE MANUAL",
  docsTitle: "A clear path from install to insight.",
  docsText:
    "Installation, client setup, CLI commands, MCP tools, supported languages, architecture and internals. The existing reference manual is in English.",
  guide1: "Install & connect",
  guide2: "Query & explore",
  guide3: "Understand & extend",
  faqLabel: "A FEW GOOD QUESTIONS",
  faqTitle: "Before you start.",
  faq1: "Where does the index live?",
  answer1:
    "In your project, in a local SQLite database. Indexing runs on your machine. Your connected AI client may send tool responses to its provider according to that client’s settings.",
  faq2: "Does every assistant work with it?",
  answer2:
    "The assistant needs MCP support to call the tools. You can use the CLI independently for indexing, health checks and candidate dead-code detection.",
  faq3: "Is the graph a complete picture of my code?",
  answer3:
    "No. Extraction depends on supported languages and syntax. Dynamic behavior may not be resolved, and unused-code results are candidates, not proof. The docs explain these limits.",
  faq4: "What happens when I edit a file?",
  answer4:
    "While the MCP server is connected, a background watcher updates changed files after edits settle. You can also request a reindex or check index health from the CLI.",
  footer: "Precise context starts with a better map.",
  license: "Apache 2.0 license",
  top: "Back to top",
};
export default en;
