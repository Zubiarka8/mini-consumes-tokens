export const shellEn = {
  coordinate: "SOURCE → SYMBOLS → CONTEXT",
  localFirst: "LOCAL FIRST. RELATIONSHIP AWARE.",
  fieldGuide: "FIELD GUIDE",
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
  mapNote: "One question. Its relevant neighborhood.",
  problemLabel: "01 / THE PROBLEM",
  problemTitle: "Your repository is connected. Your context should be, too.",
  problemText:
    "Reading whole files gives an assistant a lot of text, but not always the relationships it needs. Repeated searches make it harder to follow a change across the codebase.",
  before: "A pile of files",
  beforeText: "Open files. Search names. Piece together relationships.",
  after: "A map of the code",
  afterText: "Locate a symbol. Follow its edges. Ask for focused context.",
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
    "Installation, client setup, CLI commands, MCP tools, supported languages, architecture and troubleshooting.",
  docsLanguageNote: "The existing reference manual is in English.",
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
