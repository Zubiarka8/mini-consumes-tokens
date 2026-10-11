import {
  Boxes,
  Compass,
  Layers,
  Network,
  Search,
  Terminal,
  Zap,
  type LucideIcon,
} from "lucide-react";

// A deterministic illustrative repository, used by the spatial atlas and code inspector.
// Each position is an [x, z] point on the boards; together they are centred on the origin.
export const examples = [
  {
    name: "parse_request",
    file: "src/parser.rs:18",
    filename: "parser.rs",
    callers: "handle_request",
    calls: "validate_input",
    line: 18,
    position: [0, 0.85],
    source: [
      "pub fn parse_request(input: &str) -> Result<Request, Error> {",
      "    let request = Request::from_str(input)?;",
      "    validate_input(&request)?;",
      "    Ok(request)",
      "}",
    ],
  },
  {
    name: "handle_request",
    file: "src/server.rs:42",
    filename: "server.rs",
    callers: "route",
    calls: "parse_request, build_response",
    line: 42,
    position: [-1.75, -0.85],
    source: [
      "pub fn handle_request(input: &str) -> Result<Response, Error> {",
      "    let request = parse_request(input)?;",
      "    Ok(build_response(request))",
      "}",
    ],
  },
  {
    name: "validate_input",
    file: "src/validation.rs:9",
    filename: "validation.rs",
    callers: "parse_request",
    calls: "normalize",
    line: 9,
    position: [1.75, 0],
    source: [
      "pub fn validate_input(request: &Request) -> Result<(), Error> {",
      "    let input = normalize(&request.input);",
      "    if input.is_empty() { return Err(Error::EmptyInput); }",
      "    Ok(())",
      "}",
    ],
  },
] as const;
// Destructuring the tuple makes a missing example a type error.
const [parseRequest, handleRequest, validateInput] = examples;
// The server calls the parser, which calls validation.
export const connections = [
  [handleRequest, parseRequest],
  [parseRequest, validateInput],
] as const;
// The full-screen tour, from first contact to advanced use. `layer` lights a board and
// `symbol` selects an example while the stage is in the middle of the dialog. Tool
// outputs follow the real server's output format, applied to request-service.
type TourTool = {
  name: string;
  params: string;
  replaces?: string;
  output?: string;
};
export const tourStages: {
  key: string;
  icon: LucideIcon;
  layer?: 0 | 1 | 2;
  symbol?: (typeof examples)[number]["name"];
  tools?: TourTool[];
  docs?: string;
}[] = [
  { key: "tourWhat", icon: Network, layer: 1 },
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
    symbol: "parse_request",
    tools: [
      {
        name: "find_symbol",
        params: 'name: "parse_request"',
        output:
          "1 definition(s) of `parse_request`:\nsrc/parser.rs:18:1 [rust] function parse_request",
      },
      {
        name: "find_callers",
        params: 'function: "parse_request"',
        output:
          "1 caller(s) of this function:\nsrc/server.rs:43:19 [rust] handle_request --calls--> parse_request -> src/parser.rs:18",
      },
      {
        name: "find_calls",
        params: 'function: "parse_request"',
        output:
          "2 call(s) made by this function:\nsrc/parser.rs:19:28 [rust] parse_request --calls--> from_str (unresolved)\nsrc/parser.rs:20:5 [rust] parse_request --calls--> validate_input -> src/validation.rs:9",
      },
      {
        name: "find_references",
        params: 'symbol: "validate_input"',
        output:
          "1 reference(s):\nsrc/parser.rs:20:5 [rust] parse_request --calls--> validate_input -> src/validation.rs:9",
      },
      {
        name: "search_symbols",
        params: 'query: "parse req"',
        output:
          "1 match(es) for `parse req`, best first:\nsrc/parser.rs:L18-L22 [rust] function parse_request",
      },
      {
        name: "hybrid_search",
        params: 'query: "reject empty input"',
        output:
          "1 match(es) for `reject empty input`, best first:\nsrc/validation.rs:L9-L13 [rust] function validate_input",
      },
    ],
  },
  {
    key: "tourComposite",
    icon: Boxes,
    layer: 2,
    symbol: "validate_input",
    tools: [
      {
        name: "impact_analysis",
        params: 'symbol: "validate_input", depth',
        replaces: "find_callers + find_references",
        output:
          "Impact analysis for `validate_input`:\n  1 direct caller(s)\n  1 reference(s) total (calls/imports/extends/implements/plain)\n  0 likely affected test(s)",
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
    symbol: "handle_request",
    tools: [
      {
        name: "batch",
        params: "queries: [{ tool, args }] ≤ 25",
        output:
          "batch: 2 queries\n\n[1] find_symbol\n1 definition(s) of `handle_request`:\nsrc/server.rs:42:1 [rust] function handle_request\n\n[2] find_callers\n1 caller(s) of this function:\nsrc/server.rs:43:19 [rust] handle_request --calls--> parse_request -> src/parser.rs:18",
      },
      { name: "discover_tool_categories", params: "—" },
      { name: "get_tool_schema", params: "name" },
      { name: "reindex", params: "force = false" },
      { name: "get_indexing_status", params: "verbose_dependencies = false" },
    ],
  },
  { key: "start", icon: Terminal, docs: "/docs/tools/" },
];
export const layerKeys = ["files", "symbols", "context"] as const;
export type AtlasLayers = [boolean, boolean, boolean];
export function layerHeight(layer: number, separation: number) {
  return (1 - layer) * separation;
}
