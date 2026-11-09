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
  nav: "Main navigation",
  language: "Language",
  theme: "Color theme",
  themeSystem: "System",
  themeLight: "Light",
  themeDark: "Dark",
  how: "How it works",
  navWhy: "Why",
  navStart: "Get started",
  navFaq: "FAQ",
  navWeb: "Web",
  navObsidian: "Obsidian",
  capabilities: "Capabilities",
  docs: "Documentation",
  github: "View on GitHub",
  eyebrow: "MCP SERVER · A CODE MAP FOR YOUR AI ASSISTANT",
  headline: "Your AI reads less.",
  headlineAccent: "Your tokens go further.",
  intro: "A free MCP server that maps your code on your own machine. Claude Code, Cursor, Codex and other assistants ask the map and get the exact function, not the whole file.",
  readDocs: "Install it free",
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
    "A reader built on tree-sitter goes through each supported file and notes its functions, types and how they connect. That map is stored in a small database inside your project, and everything runs on your computer. The server answers your assistant from that database. While it runs, it updates changed files in the background once you stop editing for about 15 seconds.",
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
  mapNote: "One question in. Only the relevant code out.",
  problemLabel: "THE PROBLEM",
  problemTitle: "Your assistant pays tokens for every file it opens.",
  problemText: "Whole files bury the answer in text. Repeated searches lose the thread of a change. A map says what is defined where, who uses it and what it needs, so one precise question replaces a pile of files.",
  before: "Without a map",
  beforeText: "Open files. Search names. Guess the links.",
  after: "With a map",
  afterText: "Find the function. Follow its links. Get a focused summary.",
  comparisonNote:
    "A conceptual comparison, not a measurement of savings. Results depend on the project and the question.",
  howLabel: "THE MECHANISM",
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
  capabilitiesLabel: "WHAT YOU CAN DO",
  capabilitiesTitle: "Follow the code.\nSee the impact.",
  discoverTitle: "Get your bearings fast",
  discoverText: "Project overview, file tree, file outline. Find functions and types by name before you open any code.",
  changeTitle: "See what a change touches",
  changeText: "Follow who uses what across the map. Check the affected code and tests before you edit.",
  contextTitle: "Give your assistant just enough",
  contextText: "One request returns a function or type with its definition, dependencies and related code.",
  extras:
    "Also available: finding code that seems unused or duplicated (rough guesses), a dependency graph between files, and optional local search by meaning, which needs a special build and a small language model on your computer.",
  audience: "Built for developers who use MCP coding assistants on projects in several languages. The command-line tool also works alone.",
  webLabel: "LOCAL WEB DASHBOARD",
  webTitle: "Your whole project,\nin the browser.",
  webText: "Run mct-cli web. A dashboard opens from your own computer, listening on 127.0.0.1 only. It has dark and light themes and stops by itself after 30 minutes idle. The interface is in Spanish.",
  webCmdLabel: "Open the dashboard",
  web1Title: "Health at a glance",
  web1Text:
    "Counts, language bars, project health and recent activity. Metrics the index does not record are shown as unavailable, never invented.",
  web2Title: "Browse code and dependencies",
  web2Text:
    "Files and symbols with filters, a source preview and the imports of each file. The dependency graph comes as a tree or a table.",
  web3Title: "Review duplicate and unused code",
  web3Text:
    "Compare duplicates side by side with an aligned diff. Mark a finding as a false positive or not relevant, add a note and restore it later. Decisions stay on your computer.",
  web4Title: "Search once, bookmark any view",
  web4Text:
    "Keyword search over code, symbols, files, duplicates, dependencies and Obsidian notes. Filters live in the URL, so every view can be bookmarked.",
  obsLabel: "OBSIDIAN CODE ATLAS",
  obsTitle: "Browse your code map\nin Obsidian.",
  obsText: "Every scan also writes a vault of linked notes: one map per folder, one note per file with its imports and symbols. Open the folder in Obsidian and follow your project link by link. Optional and local; skip it with --no-obsidian.",
  obsCmdLabel: "Rebuild the vault by hand",
  obs1Title: "Linked notes, no code copied",
  obs1Text:
    "Each file note lists its imports and symbols with line ranges. Source text stays out of the vault, so it stays small and your code is not duplicated.",
  obs2Title: "Built for big projects",
  obs2Text:
    "A file note shows at most 100 symbols and 50 imports, with a count of what was left out. Folder maps page after 100 entries.",
  obs3Title: "Always up to date",
  obs3Text:
    "init and reindex refresh the vault, and so does the server's background refresh. Only changed notes are rewritten.",
  obs4Title: "Safe for your own notes",
  obs4Text:
    "Your notes and the .obsidian folder survive every regeneration. It never overwrites a generated note you edited, and it follows the same .mctignore rules as the map.",
  mockTry: "Try it: click around.",
  mockNote: "Illustrative example. Names and numbers are made up.",
  startLabel: "YOUR FIRST MAP",
  startTitle: "Three commands to your first map.",
  startText: "Install the command-line tool and the server from the docs. Then run these inside the project you want mapped.",
  initLabel: "Build the map of your project",
  registerLabel: "Connect your AI assistant",
  statusLabel: "Check the map is healthy",
  copy: "Copy command",
  copied: "Command copied",
  copyError: "Copy unavailable. Select the command and copy it manually.",
  reconnect:
    "Restart or reconnect your assistant after connecting it. Some assistants, like Codex, use their own settings format; follow their guide.",
  promptingLabel: "PROMPTING FOR AI",
  promptingTitle: "Let your assistant do the setup.",
  promptingText: "Paste a prompt into your assistant. It adds the tool, indexes your code, then uses the map instead of reading every file.",
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
  docsTitle: "From install to answers.",
  docsText:
    "Installation, connecting assistants, commands, tools, supported languages and how it works inside. The manual is in English.",
  guide1: "Install & connect",
  guide2: "Query & explore",
  guide3: "Understand & extend",
  faqLabel: "A FEW GOOD QUESTIONS",
  faqTitle: "Questions before you install.",
  faqMore: "Read more in the docs",
  faq1: "Where is the map stored?",
  answer1:
    "In your project, in a small local database (SQLite) inside the .mct-index folder. Everything is scanned on your computer. Your AI assistant may send tool answers to its provider, depending on that assistant's settings.",
  faq2: "Does every assistant work with it?",
  answer2:
    "The assistant needs MCP support to use the tools. The command-line tool also works on its own: scan, check health, find unused or duplicate code, open the local dashboard and write the Obsidian notes.",
  faq3: "Does the map show everything my code does?",
  answer3:
    "No. It only understands the supported languages. Things decided only while the program runs may be missed, and “unused” code is a guess, not a certainty. The docs explain these limits.",
  faq4: "What happens when I edit a file?",
  answer4:
    "While the server is connected, it notices changed files and updates the map once you stop editing, and the Obsidian notes follow. You can also ask for a rescan or check health from the command line.",
  faq5: "How is this different from a normal text search?",
  answer5: "A text search returns matching lines. The map returns where things are defined and how they connect: what uses a function, what it needs and where it is mentioned. Your assistant reads less and understands more.",
  faq6: "Which languages are supported?",
  answer6:
    "Each language has its own reader, built on tree-sitter (a popular code-parsing library). The documentation lists the supported languages, and adding one is a self-contained change.",
  faq7: "Does it work on Windows, macOS and Linux?",
  answer7:
    "Yes. The same CLI and MCP server run on all three, and the project’s tests and checks run on each of them.",
  faq8: "Can I exclude files or folders?",
  answer8:
    "Yes. Add paths to a .mctignore file to keep them out of the map. The same file keeps them out of the Obsidian notes. Build and dependency folders such as target and node_modules are already skipped.",
  faq9: "Can it search by meaning, not just by name?",
  answer9:
    "Yes, with the optional build that adds search by meaning. It combines name matching with a small language model that runs on your computer, so you can ask what code does. Without that build, search uses names only.",
  faq10: "Is it free to use?",
  answer10: "Yes. It is open source under the Apache 2.0 license. Read, run and extend the code on GitHub.",
  faq11: "Can I browse the map in Obsidian?",
  answer11:
    "Yes. Every scan writes linked notes to .mct-index/obsidian/. Open that folder as a vault in Obsidian: one map per folder and one note per file, listing its imports and symbols. Your code is not copied into the notes, and your own notes in the vault survive regeneration. Skip it with --no-obsidian.",
  faq12: "Is there a visual dashboard?",
  answer12:
    "Yes. mct-cli web serves a dashboard on localhost with search, a dependency graph, a notes reader and review of duplicate and unused code. It stops by itself after 30 minutes without use. The interface is in Spanish.",
  footer: "Better context. Fewer wasted tokens.",
  license: "Apache 2.0 license",
  top: "Back to top",
  principle1: "Your code",
  principle2: "A small map",
  principle3: "Your AI assistant",
};
export default en;
