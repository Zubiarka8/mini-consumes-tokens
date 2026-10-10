// A deterministic illustrative repository, used by the spatial atlas and code inspector.
export const examples = [
  {
    name: "parse_request",
    file: "src/parser.rs:18",
    filename: "parser.rs",
    callers: "handle_request",
    calls: "validate_input",
    line: 18,
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
    source: [
      "pub fn validate_input(request: &Request) -> Result<(), Error> {",
      "    let input = normalize(&request.input);",
      "    if input.is_empty() { return Err(Error::EmptyInput); }",
      "    Ok(())",
      "}",
    ],
  },
  {
    name: "route",
    file: "src/main.rs:12",
    filename: "main.rs",
    callers: "main",
    calls: "handle_request",
    line: 12,
    source: [
      "pub fn route(input: &str) -> Result<Response, Error> {",
      "    handle_request(input)",
      "}",
    ],
  },
  {
    name: "normalize",
    file: "src/normalize.rs:6",
    filename: "normalize.rs",
    callers: "validate_input",
    calls: "trim, to_lowercase",
    line: 6,
    source: [
      "pub fn normalize(input: &str) -> String {",
      "    input.trim().to_lowercase()",
      "}",
    ],
  },
  {
    name: "build_response",
    file: "src/response.rs:8",
    filename: "response.rs",
    callers: "handle_request",
    calls: "Response::ok",
    line: 8,
    source: [
      "pub fn build_response(request: Request) -> Response {",
      "    Response::ok(request)",
      "}",
    ],
  },
];
export const symbolPositions: [number, number][] = [
  [-0.1, 0.85],
  [-0.1, -0.85],
  [1.75, 0.85],
  [-1.95, -0.85],
  [1.75, -0.85],
  [-1.95, 0.85],
];
// Request routing, parsing and validation, with normalization and response branches.
export const connections = [
  [3, 1],
  [1, 0],
  [0, 2],
  [2, 4],
  [1, 5],
];
export const layerKeys = ["files", "symbols", "context"] as const;
export type AtlasLayers = [boolean, boolean, boolean];
export function layerHeight(layer: number, separation: number) {
  return (1 - layer) * separation;
}
