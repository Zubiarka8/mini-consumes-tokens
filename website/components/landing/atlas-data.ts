import {
  Boxes,
  Compass,
  Database,
  FolderSync,
  Gauge,
  Layers,
  ListTree,
  Network,
  RefreshCw,
  Search,
  Server,
  Settings,
  Terminal,
  Zap,
  type LucideIcon,
} from "lucide-react";

// Three files of this repository, in the order a request crosses them. `x` is the file's
// column on the boards.
export const files = [
  { name: "server.rs", path: "crates/mct-mcp-server/src/server.rs", x: -1.75 },
  { name: "lib.rs", path: "crates/mct-index/src/lib.rs", x: 0 },
  { name: "indexer.rs", path: "crates/mct-index/src/indexer.rs", x: 1.75 },
] as const;
// Their top-level symbols with at least one resolved call, as the index reports them.
// Each position is an [x, z] point on the boards; together they are centred on the origin.
export const examples = [
  {
    name: "MctServer",
    file: 0,
    line: 673,
    callers: "main.rs:61 (MctServer::new)",
    calls: "ServerOptions::from_env (server.rs:1069)",
    position: [-1.75, -0.6],
    source: [
      "pub struct MctServer {",
      "    index: Arc<Mutex<Index>>,",
      "    /// The index's project root, kept here so the embedding model can be",
      "    /// loaded without taking the index mutex (`Index::root` is immutable).",
      "    root: std::path::PathBuf,",
    ],
  },
  {
    name: "ServerOptions",
    file: 0,
    line: 602,
    callers: "MctServer::new (server.rs:1069)",
    calls: "CacheConfig::from_env (server.rs:616)",
    position: [-1.75, 0.6],
    source: [
      "pub struct ServerOptions {",
      "    pub cache: CacheConfig,",
      "    /// Embedder for `hybrid_search`'s semantic side; `None` loads the build's",
      "    /// own model lazily (see `crate::embedder`).",
      "    pub embedder: Option<Box<dyn mct_index::Embedder>>,",
    ],
  },
  {
    name: "Index",
    file: 1,
    line: 83,
    callers: "main.rs:47 (Index::open)",
    calls: "indexer::reindex, reindex_paths, status (lib.rs:186, 199, 203)",
    position: [0, 0],
    source: [
      "pub struct Index {",
      "    conn: Connection,",
      "    /// Canonicalized project root — every relative path stored in the",
      "    /// database is validated against this to reject path traversal.",
      "    root: PathBuf,",
    ],
  },
  {
    name: "reindex",
    file: 2,
    line: 91,
    callers: "Index::reindex (lib.rs:186), reindex_paths (indexer.rs:577, 586)",
    calls:
      "walk_entry_allowed, record_walk_failure, index_file, remove_missing_files, remove_missing_manifests, touch_last_indexed_at",
    position: [1.35, -0.85],
    source: [
      "pub fn reindex(",
      "    index: &mut Index,",
      "    registry: &LanguageRegistry,",
      "    force: bool,",
      ") -> Result<ReindexReport> {",
    ],
  },
  {
    name: "reindex_paths",
    file: 2,
    line: 547,
    callers: "Index::reindex_paths (lib.rs:199)",
    calls:
      "reindex, index_noting_created, remove_under_prefix, remove_renamed_away, record_read_failure and 5 more",
    position: [2.2, 0.05],
    source: [
      "pub fn reindex_paths(",
      "    index: &mut Index,",
      "    registry: &LanguageRegistry,",
      "    paths: &[PathBuf],",
      ") -> Result<ReindexReport> {",
    ],
  },
  {
    name: "status",
    file: 2,
    line: 1200,
    callers: "Index::status (lib.rs:203)",
    calls: "—",
    position: [1.35, 0.95],
    source: [
      "pub fn status(index: &Index) -> Result<IndexStatus> {",
      "    let mut stmt = index.conn.prepare(",
      '        "SELECT f.language, COUNT(DISTINCT f.id), COUNT(s.id)',
      "         FROM files f LEFT JOIN symbols s ON s.file_id = f.id",
      '         GROUP BY f.language ORDER BY f.language",',
    ],
  },
] as const;
// Destructuring the tuple makes a missing example a type error.
const [mctServer, serverOptions, index, reindex, reindexPaths, status] =
  examples;
// Only resolved calls: MctServer reaches Index through a mutex, which the index can't type.
export const connections = [
  [mctServer, serverOptions],
  [index, reindex],
  [index, reindexPaths],
  [index, status],
  [reindexPaths, reindex],
] as const;
// A cue lights the connections of its tour or story stage step by step; any other key
// is "idle", which lights them all.
export const CUE_STEP = 300;
export type CueKey = string;
export type Cue = { key: CueKey; id: number };
// The full-screen tour: the six symbols, the methods the server and the index expose,
// then what each tool asks. `layer` lights a board, `symbol` selects an example and
// `steps` lists the connections each cue step lights while the stage is in the middle
// of the dialog.
type TourTool = {
  name: string;
  params: string;
  replaces?: string;
};
type TourMethod = { name: string; at: string; link: string };
export const tourStages: {
  key: string;
  icon: LucideIcon;
  layer?: 0 | 1 | 2;
  symbol?: (typeof examples)[number]["name"];
  steps?: number[][];
  methods?: TourMethod[];
  tools?: TourTool[];
  docs?: string;
}[] = [
  { key: "tourWhat", icon: Network, layer: 1, steps: [[0], [1, 2, 3], [4]] },
  {
    key: "tourServer",
    icon: Server,
    layer: 1,
    symbol: "MctServer",
    steps: [[0]],
  },
  {
    key: "tourIndex",
    icon: Database,
    layer: 1,
    symbol: "Index",
    steps: [[1], [2], [3]],
  },
  {
    key: "tourReindex",
    icon: RefreshCw,
    layer: 1,
    symbol: "reindex",
    steps: [[1], [4]],
  },
  {
    key: "tourReindexPaths",
    icon: FolderSync,
    layer: 1,
    symbol: "reindex_paths",
    steps: [[2], [4]],
  },
  { key: "tourStatus", icon: Gauge, layer: 1, symbol: "status", steps: [[3]] },
  {
    key: "tourTools",
    icon: Settings,
    layer: 2,
    symbol: "MctServer",
    methods: [
      {
        name: "list_symbols",
        at: "server.rs:1324",
        link: "→ Index::list_symbols (1347)",
      },
      {
        name: "find_symbol",
        at: "server.rs:1359",
        link: "→ Index::find_symbol_matching_scoped (1376)",
      },
      {
        name: "find_references",
        at: "server.rs:1678",
        link: "→ Index::relation_page (1697)",
      },
      {
        name: "find_callers",
        at: "server.rs:1754",
        link: "→ Index::relation_page (1773)",
      },
      {
        name: "reindex",
        at: "server.rs:2076",
        link: "→ Index::reindex (2081)",
      },
      {
        name: "get_indexing_status",
        at: "server.rs:2089",
        link: "→ Index::status (2097)",
      },
      {
        name: "get_file_skeleton",
        at: "server.rs:2122",
        link: "→ Index::list_symbols (2135)",
      },
      {
        name: "get_project_overview",
        at: "server.rs:2153",
        link: "→ Index::find_callers (2234)",
      },
    ],
  },
  {
    key: "tourMethods",
    icon: ListTree,
    layer: 1,
    symbol: "Index",
    steps: [[1, 2, 3]],
    methods: [
      { name: "Index::open", at: "lib.rs:94", link: "← main.rs:47" },
      {
        name: "Index::reindex",
        at: "lib.rs:185",
        link: "→ indexer::reindex (186)",
      },
      {
        name: "Index::reindex_paths",
        at: "lib.rs:194",
        link: "→ indexer::reindex_paths (199)",
      },
      {
        name: "Index::status",
        at: "lib.rs:202",
        link: "→ indexer::status (203)",
      },
      {
        name: "Index::find_symbol_matching_scoped",
        at: "lib.rs:222",
        link: "← find_symbol (server.rs:1376)",
      },
      {
        name: "Index::find_callers",
        at: "lib.rs:385",
        link: "← get_project_overview (server.rs:2234)",
      },
      {
        name: "Index::relation_page",
        at: "lib.rs:623",
        link: "← find_references, find_callers (server.rs:1697, 1773)",
      },
      {
        name: "Index::list_symbols",
        at: "lib.rs:728",
        link: "← list_symbols, get_file_skeleton (server.rs:1347, 2135)",
      },
    ],
  },
  { key: "tourHow", icon: Layers, layer: 0 },
  {
    key: "tourOrient",
    icon: Compass,
    layer: 0,
    tools: [
      { name: "get_file_tree", params: "path, depth = 3" },
      {
        name: "get_project_overview",
        params: "path, max_symbols_per_module = 8",
      },
      { name: "list_symbols", params: "path, kind" },
      { name: "get_file_skeleton", params: "path" },
    ],
  },
  {
    key: "tourLookup",
    icon: Search,
    layer: 1,
    tools: [
      { name: "find_symbol", params: "name" },
      { name: "find_callers", params: "function" },
      { name: "find_calls", params: "function" },
      { name: "find_references", params: "symbol" },
      { name: "search_symbols", params: "query" },
      { name: "hybrid_search", params: "query" },
    ],
  },
  {
    key: "tourComposite",
    icon: Boxes,
    layer: 2,
    tools: [
      {
        name: "impact_analysis",
        params: "symbol, depth",
        replaces: "find_callers + find_references",
      },
      {
        name: "build_context_pack",
        params: "symbol, depth, source_lines = 40",
        replaces: "find_symbol + find_calls + find_callers + impact_analysis",
      },
      { name: "find_dead_code", params: "path" },
    ],
  },
  {
    key: "tourAdvanced",
    icon: Zap,
    layer: 1,
    tools: [
      { name: "batch", params: "queries: [{ tool, args }] ≤ 25" },
      { name: "discover_tool_categories", params: "—" },
      { name: "get_tool_schema", params: "name" },
      { name: "reindex", params: "force = false" },
      { name: "get_indexing_status", params: "verbose_dependencies = false" },
    ],
  },
  { key: "start", icon: Terminal, docs: "/docs/tools/" },
];
// The landing story, outside full screen: what the atlas shows, then the mechanism's
// three steps. Same shape as a tour stage; `key` names its title and text, and empty
// `steps` dims every connection. `view` orbits the camera around the focused board;
// without one, the camera keeps its starting pose.
export type View = {
  distance: number;
  polar: number;
  azimuth: number;
  // Moves the orbit centre up or down from the focused board.
  lift?: number;
};
export const storyStages: {
  key: string;
  layer: 0 | 1 | 2;
  symbol?: (typeof examples)[number]["name"];
  steps: number[][];
  view?: View;
}[] = [
  { key: "tourWhat", layer: 1, steps: [[0], [1, 2, 3], [4]] },
  {
    key: "step1",
    layer: 0,
    symbol: "Index",
    steps: [],
    view: { distance: 7.4, polar: 1.05, azimuth: 0.3, lift: -0.8 },
  },
  {
    key: "step2",
    layer: 1,
    symbol: "Index",
    steps: [[1], [2], [3]],
    view: { distance: 8.4, polar: 1, azimuth: -0.15 },
  },
  {
    key: "step3",
    layer: 2,
    symbol: "reindex",
    steps: [[1], [4]],
    view: { distance: 10, polar: 1.22, azimuth: 0.75, lift: 1 },
  },
];
export const layerKeys = ["files", "symbols", "context"] as const;
export type AtlasLayers = [boolean, boolean, boolean];
export function layerHeight(layer: number, separation: number) {
  return (1 - layer) * separation;
}
