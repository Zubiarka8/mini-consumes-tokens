# Agent 03 — reduce false dead-code candidates

Follow the shared rules. Branch: `codex/audit-dead-code`. Goal: implement the compatible part of F07 without deleting functions or indiscriminately hiding candidates.

Cases: `symbol_kind_str` is used inside `params![...]` in `write_parsed_file` but has no indexed references; `pub fn actual_use() {} pub fn entry() { let _f = actual_use; }` marks `actual_use` as a candidate; a function named `verifies_behavior` with `#[test]` inside `#[cfg(test)] mod tests` is also reported as a candidate.

Scope: `crates/mct-lang-rust/src` and tests, `crates/mct-index/src/dead_code.rs` and focused tests, plus MCP dead-code tests if needed. Do not modify `indexer.rs`, the index `lib.rs`, registries, `server.rs`, semantic search, or `mct-core` models. Another agent owns reindexing.

First determine what information the model already retains for identifying tests and value uses. Implement compatible improvements: extraction of verifiable static references and test exclusion based on a reliable signal. Do not assume every identifier in a macro is a call, and do not exclude every function whose name contains `test`. If identifying attributes requires a `LanguageParser` contract/schema change, prepare a design and proposed issue, and complete the parts that do not require that change.

Acceptance: active tests are not recommended for removal when sufficient evidence exists; value references and the `params!` case are covered or their exact remaining limitation is explained; a genuinely unreferenced function remains a candidate. Keep the heuristic warning. Add negative tests for homonyms/unrelated identifiers. Deliver a local commit and proposed PR, with remaining work explicit.
