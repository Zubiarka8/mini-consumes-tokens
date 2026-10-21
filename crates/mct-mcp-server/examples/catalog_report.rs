// Example/bench code: a panic here means a broken benchmark precondition,
// not a path processing untrusted repo-input content.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Size of the MCP tool catalog, per tool, and what `detail` / `max_tokens`
//! added to it.
//!
//! ```sh
//! cargo run -p mct-mcp-server --example catalog_report --release
//! ```
//!
//! "Before" is rebuilt from the live catalog, not from a second checkout: the
//! `detail` / `max_tokens` properties are removed from the three tools'
//! input schemas and the three `PRE` sentences are put back to their text at
//! `HEAD`. That isolates this change from every other edit of the working
//! tree. The catalog size (everything a client that loads every schema pays
//! once per session) is kept apart from the cost of a flow (only the schemas
//! of the tools the flow calls, for a client that loads schemas on demand).
//!
//! Three heuristics, never mixed and none a tokenizer: `chars/4`,
//! `EstimatedCounter`, and the `mct-eval` word/punctuation count that its
//! `catalog_tokens` baseline uses.

use std::path::Path;

use mct_index::{EstimatedCounter, ExcludeSet, Index, TokenCounter};
use mct_mcp_server::server::MctServer;
use rmcp::model::Tool;
use serde_json::Value;

const TOUCHED: [&str; 3] = [
    "search_symbols_like",
    "hybrid_search_like",
    "build_context_pack",
];

/// (new `PRE` sentence, its text at `HEAD`).
const PRE: [(&str, &str); 3] = [
    (
        "`query` non-blank; optional `detail` overview|summary|detailed, `max_tokens` (estimated).",
        "`query` non-blank.",
    ),
    (
        "`query` non-blank; `alpha` in 0..1 if given; optional `detail`, `max_tokens` (estimated) as in search_symbols_like.",
        "`query` non-blank; `alpha` in 0..1 if given.",
    ),
    (
        "`symbol` non-blank; `path`/`language` disambiguate; optional `detail` overview|summary|detailed, `max_tokens` (estimated).",
        "`symbol` non-blank; `path`/`language` disambiguate.",
    ),
];

fn eval_tokens(s: &str) -> usize {
    let mut tokens = 0usize;
    let mut in_word = false;
    for c in s.chars() {
        if c.is_alphanumeric() || c == '_' {
            if !in_word {
                tokens += 1;
                in_word = true;
            }
        } else {
            in_word = false;
            if !c.is_whitespace() {
                tokens += 1;
            }
        }
    }
    tokens
}

#[derive(Clone, Copy, Default)]
struct Size {
    chars4: usize,
    estimated: usize,
    eval: usize,
}

impl Size {
    fn of(text: &str) -> Size {
        Size {
            chars4: text.chars().count().div_ceil(4),
            estimated: EstimatedCounter.count(text).value,
            eval: eval_tokens(text),
        }
    }
    fn add(self, o: Size) -> Size {
        Size {
            chars4: self.chars4 + o.chars4,
            estimated: self.estimated + o.estimated,
            eval: self.eval + o.eval,
        }
    }
    fn sub(self, o: Size) -> Size {
        Size {
            chars4: self.chars4.saturating_sub(o.chars4),
            estimated: self.estimated.saturating_sub(o.estimated),
            eval: self.eval.saturating_sub(o.eval),
        }
    }
}

fn json_of(tool: &Tool) -> String {
    serde_json::to_string(tool).unwrap()
}

/// The tool as it was before `detail` / `max_tokens`.
fn before(tool: &Tool) -> Tool {
    let mut old = tool.clone();
    if !TOUCHED.contains(&tool.name.as_ref()) {
        return old;
    }
    let mut schema = (*tool.input_schema).clone();
    if let Some(Value::Object(props)) = schema.get_mut("properties") {
        props.remove("detail");
        props.remove("max_tokens");
    }
    old.input_schema = std::sync::Arc::new(schema);
    if let Some(desc) = &tool.description {
        let mut text = desc.to_string();
        for (new, old_pre) in PRE {
            text = text.replace(new, old_pre);
        }
        old.description = Some(text.into());
    }
    old
}

fn row(name: &str, b: Size, a: Size) {
    println!(
        "| {name} | {} → {} (+{}) | {} → {} (+{}) | {} → {} (+{}) |",
        b.chars4,
        a.chars4,
        a.chars4 - b.chars4,
        b.estimated,
        a.estimated,
        a.estimated - b.estimated,
        b.eval,
        a.eval,
        a.eval - b.eval,
    );
}

fn main() {
    let dir = std::env::temp_dir().join(format!("mct-catalog-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let registry = mct_mcp_server::registry::build_registry();
    let index = Index::open_in_memory(Path::new(&dir), ExcludeSet::default()).unwrap();
    let server = MctServer::new(index, registry);
    let tools = server.tool_catalog();

    println!(
        "# Tool catalog size (heuristic tokens: chars/4 | EstimatedCounter | mct-eval count)\n"
    );
    println!("## Whole catalog\n");
    println!("| | chars/4 | EstimatedCounter | mct-eval |");
    println!("|---|---|---|---|");
    let (mut total_before, mut total_after) = (Size::default(), Size::default());
    let mut rows = Vec::new();
    for tool in &tools {
        let (b, a) = (Size::of(&json_of(&before(tool))), Size::of(&json_of(tool)));
        total_before = total_before.add(b);
        total_after = total_after.add(a);
        rows.push((tool.name.to_string(), b, a));
    }
    row(
        &format!("all {} tools", tools.len()),
        total_before,
        total_after,
    );
    // `mct-eval` counts the serialized `Vec<Tool>`, with array punctuation.
    let all_after = serde_json::to_string(&tools).unwrap();
    let all_before = serde_json::to_string(&tools.iter().map(before).collect::<Vec<_>>()).unwrap();
    row(
        "serialized list (what mct-eval counts)",
        Size::of(&all_before),
        Size::of(&all_after),
    );

    println!("\n## Tools whose size changed\n");
    println!("| tool | chars/4 | EstimatedCounter | mct-eval |");
    println!("|---|---|---|---|");
    for (name, b, a) in &rows {
        if a.estimated != b.estimated || a.chars4 != b.chars4 {
            row(name, *b, *a);
        }
    }
    let untouched = rows
        .iter()
        .filter(|(n, b, a)| !TOUCHED.contains(&n.as_str()) && (a.estimated != b.estimated))
        .count();
    println!("\nTools outside the three above whose size changed: {untouched}");

    println!("\n## Where the growth of each touched tool sits\n");
    println!("| tool | part | EstimatedCounter | chars/4 |");
    println!("|---|---|---|---|");
    for tool in tools.iter().filter(|t| TOUCHED.contains(&t.name.as_ref())) {
        let old = before(tool);
        let desc = |t: &Tool| t.description.as_deref().unwrap_or("").to_string();
        let d = Size::of(&desc(tool)).sub(Size::of(&desc(&old)));
        println!(
            "| {} | description | +{} | +{} |",
            tool.name, d.estimated, d.chars4
        );
        let Some(Value::Object(props)) = tool.input_schema.get("properties") else {
            continue;
        };
        for field in ["detail", "max_tokens"] {
            if let Some(schema) = props.get(field) {
                let s = Size::of(&format!("\"{field}\":{schema}"));
                println!(
                    "| {} | property `{field}` | +{} | +{} |",
                    tool.name, s.estimated, s.chars4
                );
            }
        }
    }

    println!("\n## On demand: `get_tool_schema` contract block (PRE line)\n");
    println!(
        "`PRE`, `POST` and `EFFECTS` are not in the catalog descriptions (which are `WHEN | NOT | TAGS`); they are returned only by `get_tool_schema`. The three `PRE` sentences changed:\n"
    );
    println!("| tool | PRE before → after, EstimatedCounter | chars/4 |");
    println!("|---|---|---|");
    for (tool, (new, old)) in TOUCHED.iter().zip(PRE) {
        let (n, o) = (Size::of(new), Size::of(old));
        println!(
            "| {tool} | {} → {} (+{}) | +{} |",
            o.estimated,
            n.estimated,
            n.estimated - o.estimated,
            n.chars4 - o.chars4
        );
    }

    println!("\n## Cost of a flow\n");
    println!(
        "A client that loads every schema up front pays the whole catalog once per session, \
whatever the flow. A client that loads schemas on demand (`discover_tool_categories` + \
`get_tool_schema`, or a deferred-tool search) pays only the tools the flow calls.\n"
    );
    println!("| flow | tools called | schemas, EstimatedCounter (before → after) |");
    println!("|---|---|---|");
    let flows: [(&str, &[&str]); 6] = [
        ("locate a function (default args)", &["search_symbols_like"]),
        ("locate, overview + budget", &["search_symbols_like"]),
        ("context pack", &["build_context_pack"]),
        ("find a decision in ADRs", &["search_symbols_like"]),
        ("callers of a symbol", &["find_callers"]),
        ("exact phrase, hybrid", &["hybrid_search_like"]),
    ];
    for (name, used) in flows {
        let (mut b, mut a) = (Size::default(), Size::default());
        for (n, bb, aa) in &rows {
            if used.contains(&n.as_str()) {
                b = b.add(*bb);
                a = a.add(*aa);
            }
        }
        println!(
            "| {name} | {} | {} → {} (+{}) |",
            used.join(", "),
            b.estimated,
            a.estimated,
            a.estimated - b.estimated
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}
