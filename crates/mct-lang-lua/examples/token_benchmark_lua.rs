//! Standalone Lua token benchmark using the same shared measurement method
//! and output format as `mct-cli/examples/token_benchmark.rs`. Each example
//! builds its own language registry and measures its own fixtures.
//!
//! Usage: cargo run -p mct-lang-lua --example token_benchmark_lua
//!
//! Named `token_benchmark_lua` (not `token_benchmark`) to avoid an
//! example-binary filename collision with `mct-cli`'s own
//! `examples/token_benchmark.rs` when both crates are built together.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::Arc;

use mct_core::LanguageRegistry;
use mct_lang_lua::LuaParser;

#[path = "../../mct-corpus/support/token_benchmark.rs"]
mod benchmark;
use benchmark::{run_language, Query, QueryKind};

fn main() {
    let lua_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/inventory-app");
    let mut lua_registry = LanguageRegistry::new();
    lua_registry.register(Arc::new(LuaParser));
    run_language(
        "Lua (inventory-app fixture)",
        &lua_root,
        lua_registry,
        &[
            Query {
                label: "find the definition of",
                kind: QueryKind::Symbol,
                term: "Inventory",
            },
            Query {
                label: "what calls this function",
                kind: QueryKind::Callers,
                term: "log",
            },
            Query {
                label: "who uses this symbol",
                kind: QueryKind::References,
                term: "addItem",
            },
        ],
    );
}
