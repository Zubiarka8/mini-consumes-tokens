export const atlasEn = {
  visibleLayers: "Visible layers",
  inspectFile: "Inspect file",
  exampleProject: "THIS REPOSITORY",
  projectBrief:
    "How mini-consumes-tokens works, shown on its own code: it turns your files into a map of the pieces and how they connect, then gives your assistant only what it needs.",
  role0: "Answers",
  role1: "Settings",
  role2: "Map keeper",
  role3: "Full scan",
  role4: "Quick update",
  role5: "Health check",
  tourHowText:
    "Tree-sitter reads each supported file and extracts symbols and the relations between them. A SQLite database keeps that graph inside your project, and indexing runs on your machine. The MCP server answers your assistant's tool calls from that database. While it runs, it reindexes changed files in the background once edits have been quiet for 1.5 seconds.",
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
  allSymbols: "See all symbols",
  hideSymbols: "Hide all symbols",
  allSymbolsTitle: "Every symbol in the three files",
  symbolCount: "{{count}} symbols",
  select: "Choose a piece to highlight",
  loading3d: "Loading the spatial atlas…",
  fallback:
    "The 3D view is unavailable on this device. In short: your files become a map of the pieces, and your assistant gets only what it needs.",
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
  spatialHint: "Drag to orbit · Scroll to zoom · Select a piece",
  flowFiles: "Your files",
  flowMap: "A map of the pieces and how they connect",
  flowNeeded: "Only what the assistant needs",
  layerFiles: "Your files",
  layerMap: "Map of the pieces",
  layerNeeded: "What the assistant needs",
};
