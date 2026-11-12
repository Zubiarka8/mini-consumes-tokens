//! A module-level binding with no function value is a symbol of the file:
//! `const` is a `constant`, `let`/`var` a `variable`, exported or not, with
//! its declaration as signature. A function value stays a `function`; a
//! destructuring pattern, a `require(…)` binding (an import, as an ES
//! `import` is) and a binding inside a function or block declare nothing.

// Test code: an unwrap()/expect() here means a broken test precondition, and
// panicking is the correct behavior — this is not production code parsing
// untrusted repo content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mct_core::{ParsedFile, SymbolKind};
use mct_lang_js_ts::JsTsParser;

const SOURCE: &str = r#"const MAX = 5;
export const API_URL: string = "https://example.test";
let count = 0, total;
var legacy;
export let mode = "dev";
const handler = () => MAX;
const { a, b } = load();
const fs = require("fs");
function run() {
  const local = 1;
}
for (let i = 0; i < MAX; i++) {}
{
  let inner = 2;
}
"#;

fn parsed() -> ParsedFile {
    mct_corpus::parse_source(&JsTsParser, "src/config.ts", "TypeScript", SOURCE)
}

#[test]
fn a_module_level_binding_is_a_constant_or_variable() {
    let parsed = parsed();
    let bindings: Vec<(String, SymbolKind, u32, Option<String>, Option<String>)> = parsed
        .symbols
        .iter()
        .filter(|s| matches!(s.kind, SymbolKind::Constant | SymbolKind::Variable))
        .map(|s| {
            let signature = parsed
                .symbol_details
                .iter()
                .find(|d| d.symbol == s.id)
                .and_then(|d| d.signature.clone());
            (
                s.name.clone(),
                s.kind,
                s.location.line,
                s.parent.clone(),
                signature,
            )
        })
        .collect();
    let row = |name: &str, kind, line, signature: &str| {
        (
            name.to_string(),
            kind,
            line,
            None,
            Some(signature.to_string()),
        )
    };
    assert_eq!(
        bindings,
        [
            row("MAX", SymbolKind::Constant, 1, "const MAX"),
            row("API_URL", SymbolKind::Constant, 2, "const API_URL: string"),
            row("count", SymbolKind::Variable, 3, "let count"),
            row("total", SymbolKind::Variable, 3, "let total"),
            row("legacy", SymbolKind::Variable, 4, "var legacy"),
            row("mode", SymbolKind::Variable, 5, "let mode"),
        ]
    );
}

#[test]
fn a_function_value_stays_a_function() {
    let parsed = parsed();
    let functions: Vec<&str> = parsed
        .symbols
        .iter()
        .filter(|s| s.kind == SymbolKind::Function)
        .map(|s| s.name.as_str())
        .collect();
    assert_eq!(functions, ["handler", "run"]);
}
