const en = {
  demoBadge: "Illustrative example",
  demoTitle: "One query, less context.",
  demoQueryLabel: "Choose a query",
  demoQuery0: "Where is the function that calculates an invoice's total?",
  demoQuery1: "When is an invoice due, and where does that deadline come from?",
  demoQuery2:
    "How does the app decide if an invoice is paid, pending or overdue?",
  demoQuery3: "Where are the customer's details added to a new invoice?",
  demoQuery4: "Where is a price formatted as money, and in which currency?",
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
    "Made-up example. The line counts only illustrate the selection; they are not measured savings.",
  coordinate: "CODE → MAP → ANSWER",
  localFirst: "ON YOUR COMPUTER. CONNECTED. PRECISE.",
  fieldGuide: "FIELD GUIDE",
  skip: "Skip to content",
  nav: "Main navigation",
  language: "Language",
  how: "How it works",
  capabilities: "Capabilities",
  docs: "Documentation",
  github: "View on GitHub",
  eyebrow: "A MAP OF YOUR CODE FOR YOUR AI ASSISTANT",
  headline: "Less searching.",
  headlineAccent: "More understanding.",
  intro:
    "AI coding assistants often read whole files just to find one thing. mini-consumes-tokens builds a searchable map of your project on your own computer, so your assistant can ask for only the parts it needs.",
  readDocs: "Explore the documentation",
  heroNote: "Open source · Free · Runs on your computer",
  visibleLayers: "Visible layers",
  sourcePreview: "Chosen source code",
  inspectFile: "Explore file",
  exampleProject: "THIS REPOSITORY",
  projectBrief:
    "This project as its own map sees it: the part that answers your assistant, the stored map and the part that builds it. Three files, six pieces of code and only the connections the map found.",
  agentQuestion: "THE ASSISTANT’S QUESTION",
  query0: "What answers my assistant's requests?",
  query1: "Where does the program read its settings?",
  query2: "What holds the stored map that every tool reads?",
  query3: "What happens when everything is scanned again?",
  query4: "What runs when only a few files change?",
  query5: "Is the map up to date and healthy?",
  role0: "Answer the assistant's requests",
  role1: "Load the settings",
  role2: "Keep the stored map",
  role3: "Scan the whole project",
  role4: "Scan only changed files",
  role5: "Report map health",
  purpose0:
    "The part of the program that answers your assistant. It keeps the stored map and remembers recent answers so repeated questions are fast. main.rs starts it and gives it its settings.",
  purpose1:
    "The server's settings: how recent answers are remembered and whether to search by meaning. from_env reads them when the program starts.",
  purpose2:
    "Holds the connection to the stored map (a small database) and the project folder. The server asks it for everything; when work has to be done, it passes it on to the scanner.",
  purpose3:
    "Walks through the whole project, stores what it finds in each supported file, removes files that no longer exist and notes when it finished.",
  purpose4:
    "Updates only the files it is told about: created, changed, deleted or renamed. If the project folder itself or an ignore file changed, it scans everything again.",
  purpose5:
    "Counts the files and pieces of code per language, lists any failures and returns a short health report.",
  tourHowText:
    "A reader built on tree-sitter goes through each supported file and notes its functions, types and how they connect. That map is stored in a small database inside your project, and everything runs on your computer. The server answers your assistant from that database. While it runs, it updates changed files in the background once you stop editing for about 1.5 seconds.",
  tourOrientText:
    "Start here when you don't know any names yet. These tools describe what a project contains without opening whole files.",
  tourLookupText:
    "Each tool answers one question and tells you where to look (file and line) without sending the file's contents.",
  tourCompositeText:
    "These tools combine several lookups into one answer. The ones that follow connections can also go further, up to 32 steps, and show long results page by page.",
  tourAdvancedText:
    "Run several questions together, load tool details only when needed and keep the map up to date.",
  ask_get_file_tree: "Which folders and files does the project have?",
  ask_get_project_overview:
    "What are the most important parts of this file, folder or project?",
  ask_list_symbols:
    "What is defined in this file or folder, and on which lines?",
  ask_get_file_skeleton:
    "What does this file look like without the inside of its functions?",
  ask_find_symbol: "Where is this function, class or variable defined?",
  ask_find_callers: "What calls this function?",
  ask_find_calls: "What does this function use, and where is each one defined?",
  ask_find_references: "Where is this used anywhere in the project?",
  ask_search_symbols: "I only know part of the name. What matches these words?",
  ask_hybrid_search:
    "I know what the code does, not its name. What matches by meaning? Without a language model installed, it matches by words only.",
  ask_impact_analysis:
    "What could break if I change this? What uses it, where it is mentioned and which tests look affected.",
  ask_build_context_pack:
    "Everything you need before editing something: its definition, comments and code, plus what uses it, what it uses and its tests, each listed once. It saves reading the files.",
  ask_find_dead_code:
    "Which functions or classes does nothing else use? A starting point for cleanup, not proof.",
  ask_batch:
    "Several known read-only questions in one request, each answer under its own heading. It can't refresh the map or contain other batches.",
  ask_discover_tool_categories:
    "Which tools exist? Names and a one-line purpose, grouped by topic.",
  ask_get_tool_schema:
    "What exactly does one tool need in order to be used? Loaded only when it's needed.",
  ask_reindex:
    "Refresh the map right now. force re-reads every file, even unchanged ones.",
  ask_get_indexing_status:
    "Is the map healthy? Files and pieces of code per language, the last run and any files that couldn't be read.",
  replaces: "In one call, instead of",
  toolsReference: "Read the MCP tools reference",
  atlas: "3D map of the code",
  illustration:
    "Pieces of code and connections from this project's own map, not a live view",
  allSymbols: "See all pieces of code",
  hideSymbols: "Hide all pieces of code",
  allSymbolsTitle: "Every piece of code in the three files",
  symbolCount: "{{count}} pieces of code",
  graphAlt:
    "Source files become connected pieces of code. Choosing one shows where it is defined, what uses it and what it depends on, in a short focused summary.",
  files: "Your files",
  symbols: "Code map",
  context: "What the assistant gets",
  select: "Choose a piece of code to explore",
  selected: "Chosen piece of code",
  definition: "Where it is defined",
  callers: "Used by",
  dependencies: "Uses",
  pack: "Focused summary",
  loading3d: "Loading the 3D map…",
  fallback:
    "The 3D view is unavailable on this device. You can still explore the source and context below.",
  rotate: "Rotate the 3D map",
  resetView: "Reset view",
  fullscreen: "Full screen",
  exitFullscreen: "Exit full screen",
  tourWhatTitle: "Your assistant asks the map, not the files.",
  tourWhatText:
    "mini-consumes-tokens scans your project and builds a map of its functions, types and methods, and of how they use each other. An AI assistant connected through MCP (a standard way to link tools to assistants) can then ask precise questions instead of reading whole files.",
  tourHowTitle: "Read, stored and served on your computer.",
  tourOrientTitle: "Get your bearings.",
  tourLookupTitle: "Ask one precise question.",
  tourCompositeTitle: "Several questions, one request.",
  tourAdvancedTitle: "Work on big projects.",
  tourServerTitle: "A request reaches the server.",
  tourServerText:
    "The server answers every request from your assistant. main.rs starts it with MctServer::new, which loads the settings through ServerOptions::from_env. It keeps the Index and lets one request use it at a time.",
  tourIndexTitle: "The stored map lives in one place.",
  tourIndexText:
    "Index holds the connection to the database and the project folder, and main.rs opens it with Index::open. Three of its methods hand the work to the scanner: reindex, reindex_paths and status.",
  tourReindexTitle: "A full scan walks the whole project.",
  tourReindexText:
    "reindex works in four steps. Look: walk_entry_allowed, record_walk_failure. Read: index_file. Clean up: remove_missing_files, remove_missing_manifests. Note the time: touch_last_indexed_at.",
  tourReindexPathsTitle: "A change scans only its files.",
  tourReindexPathsText:
    "reindex_paths uses the same four steps with ten helpers. Look: to_relative_slash_path, is_rules_file, walk_entry_allowed, record_walk_failure. Read: reindex, index_noting_created, record_read_failure. Clean up: remove_under_prefix, remove_renamed_away. Note the time: touch_last_indexed_at.",
  tourStatusTitle: "Health comes from the same database.",
  tourStatusText:
    "status counts files and pieces of code per language, failures and dependencies, and builds an IndexStatus report. Only Index::status calls it.",
  tourToolsTitle: "Eight tools that reach the stored map.",
  tourToolsText:
    "Each of these tools in server.rs asks the stored map one thing. The number in brackets is the line where it does so.",
  tourMethodsTitle: "Eight Index methods behind them.",
  tourMethodsText:
    "The lib.rs side of the same requests: who asks for each one, or what it passes on to the scanner.",
  spatialHint: "Drag to rotate · Scroll to zoom · Choose a piece of code",
  mapNote: "One question. Its relevant neighborhood.",
  problemLabel: "01 / THE PROBLEM",
  problemTitle:
    "Your project is connected. Your assistant's context should be, too.",
  problemText:
    "Reading whole files gives an assistant a lot of text, but not always how the parts relate. Repeated searches make it harder to follow a change through the whole project.",
  before: "A pile of files",
  beforeText: "Open files. Search names. Piece together relationships.",
  after: "A map of the code",
  afterText:
    "Find a function. Follow its connections. Ask for a focused summary.",
  comparisonNote:
    "A conceptual comparison, not a measurement of savings. Results depend on the project and the question.",
  howLabel: "02 / THE MECHANISM",
  howTitle: "From source to understanding.",
  step1Title: "Scan locally.",
  step1Text:
    "A reader goes through your code and finds its functions, types and how they connect. The result is saved in a small database inside your project.",
  step2Title: "Ask the map.",
  step2Text:
    "An AI assistant that supports MCP (a standard way to connect tools to assistants) asks where something is defined, what uses it and what it depends on, instead of scanning files again and again.",
  step3Title: "Bring the right context.",
  step3Text:
    "A focused summary gathers one piece of code and the parts connected to it. When you edit files, the map updates itself shortly after.",
  capabilitiesLabel: "03 / WHAT YOU CAN DO",
  capabilitiesTitle: "Follow the code.\nUnderstand the change.",
  discoverTitle: "Find your bearings",
  discoverText:
    "Get a project overview, a file tree or a file's outline. Find functions and types by name before opening their code.",
  changeTitle: "Trace a change",
  changeText:
    "Follow what uses what across the map. Check which parts and tests a change could affect.",
  contextTitle: "Give your assistant a focused summary",
  contextText:
    "Get a summary around one function or type, with its definition, dependencies and related code, in a single request.",
  extras:
    "Also available: finding code that seems unused (a rough guess) and optional local search by meaning, which needs a special build and a small language model on your computer.",
  audience:
    "For developers who use AI coding assistants that support MCP, in projects written in several languages. The command-line tool also works on its own.",
  startLabel: "04 / YOUR FIRST MAP",
  startTitle: "Start with your own project.",
  startText:
    "Install the command-line tool and the server first, following the documentation. Then run these commands inside the project you want to scan.",
  initLabel: "Build the map of your project",
  registerLabel: "Connect your AI assistant",
  statusLabel: "Check the map is healthy",
  copy: "Copy command",
  copied: "Command copied",
  copyError: "Copy unavailable. Select the command and copy it manually.",
  reconnect:
    "Restart or reconnect your assistant after connecting it. Some assistants, like Codex, use their own settings format; follow their guide.",
  promptingLabel: "05 / PROMPTING FOR AI",
  promptingTitle: "Ask your assistant to do it for you.",
  promptingText:
    "Paste one of these prompts into your AI assistant. It will add the tool to your project, index the code and then use the map instead of reading every file.",
  promptLabel1: "Add it to this project and index it",
  prompt1:
    "Add the mini-consumes-tokens MCP server (https://github.com/Zubiarka8/mini-consumes-tokens) to this project. First read the install guide at https://zubiarka8.github.io/mini-consumes-tokens/docs/installation/ and follow it to install the CLI. Then run `mct-cli --root . init` to build the index, `mct-cli --root . mcp-register`, and confirm with `mct-cli --root . status`. Do not guess the steps; use only the guide. Tell me when the index is ready.",
  promptLabel2: "Find code without reading whole files",
  prompt2:
    "Before opening any file, use the mini-consumes-tokens tools (list_symbols, find_symbol, get_file_skeleton) to locate the code you need. Open a full file only when you must read its body.",
  promptLabel3: "Check the impact before a change",
  prompt3:
    "I want to change <symbol>. Use impact_analysis from mini-consumes-tokens to list its callers, references and tests before you edit anything, then summarize the risk.",
  promptLabel4: "Refresh the index",
  prompt4:
    "Reindex this project with mini-consumes-tokens and tell me whether the index is up to date.",
  promptingNote:
    "Replace <symbol> with the function or class you want to change. The assistant must already be connected to the MCP server.",
  docsLabel: "THE REFERENCE MANUAL",
  docsTitle: "A clear path from install to insight.",
  docsText:
    "Installation, connecting assistants, commands, tools, supported languages, how it works inside and troubleshooting. The manual is in English.",
  guide1: "Install & connect",
  guide2: "Query & explore",
  guide3: "Understand & extend",
  faqLabel: "A FEW GOOD QUESTIONS",
  faqTitle: "Before you start.",
  faq1: "Where is the map stored?",
  answer1:
    "In your project, in a small local database (SQLite). Everything is scanned on your computer. Your AI assistant may send tool answers to its provider, depending on that assistant's settings.",
  faq2: "Does every assistant work with it?",
  answer2:
    "The assistant needs MCP support to use the tools. You can also use the command-line tool on its own to scan, check health and find code that seems unused.",
  faq3: "Does the map show everything my code does?",
  answer3:
    "No. It only understands the supported languages. Things decided only while the program runs may be missed, and “unused” code is a guess, not a certainty. The docs explain these limits.",
  faq4: "What happens when I edit a file?",
  answer4:
    "While the server is connected, it notices changed files and updates the map once you stop editing. You can also ask for a rescan or check health from the command line.",
  faq5: "How is this different from a normal text search?",
  answer5:
    "A text search returns the lines that match. The map returns where things are defined and how they connect: what uses a function, what it depends on and where it is mentioned, so your assistant reads less and understands more.",
  faq6: "Which languages are supported?",
  answer6:
    "Each language has its own reader, built on tree-sitter (a popular code-parsing library). The documentation lists the supported languages, and adding one is a self-contained change.",
  faq7: "Does it work on Windows, macOS and Linux?",
  answer7:
    "Yes. The same CLI and MCP server run on all three, and the project’s tests and checks run on each of them.",
  faq8: "Can I exclude files or folders?",
  answer8:
    "Yes. Add paths to a .mctignore file to keep them out of the map. Build and dependency folders such as target and node_modules are already skipped.",
  faq9: "Can it search by meaning, not just by name?",
  answer9:
    "Yes, with the optional build that adds search by meaning. It combines name matching with a small language model that runs on your computer, so you can ask what code does. Without that build, search uses names only.",
  faq10: "Is it free to use?",
  answer10:
    "Yes. It is open source under the Apache 2.0 license, and you can read, run and extend the code on GitHub.",
  footer: "Precise context starts with a better map.",
  license: "Apache 2.0 license",
  top: "Back to top",
  principle1: "Your code",
  principle2: "A small map",
  principle3: "Your AI assistant",
};
export default en;
