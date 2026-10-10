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
// `symbol` selects an example while the stage is in the middle of the dialog.
export const tourStages: {
  key: string;
  icon: LucideIcon;
  layer?: 0 | 1 | 2;
  symbol?: (typeof examples)[number]["name"];
}[] = [
  { key: "tourWhat", icon: Network, layer: 1 },
  { key: "tourHow", icon: Layers, layer: 0 },
  { key: "tourOrient", icon: Compass, layer: 0 },
  { key: "tourLookup", icon: Search, layer: 1, symbol: "parse_request" },
  { key: "tourComposite", icon: Boxes, layer: 2, symbol: "validate_input" },
  { key: "tourAdvanced", icon: Zap, layer: 1, symbol: "handle_request" },
  { key: "tourStart", icon: Terminal },
];
export const layerKeys = ["files", "symbols", "context"] as const;
export type AtlasLayers = [boolean, boolean, boolean];
export function layerHeight(layer: number, separation: number) {
  return (1 - layer) * separation;
}
