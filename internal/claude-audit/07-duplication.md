# Agent 07 — share genuinely duplicated infrastructure

Follow the shared rules. Branch: `codex/audit-shared-parsers`, from changes 01–05 after integration. Goal: F08 without changing grammar behavior.

Confirmed duplication: CLI/MCP registries and identical `first_error` implementations in Rust/Python/C++. Use MCP to find which utilities are truly equivalent. Reuse the parity check introduced by 02. Prefer a small first PR for one utility family; do not rewrite 17 parsers in one delivery.

Share language composition or verify/generate both lists from a maintainable catalog. For tree-sitter utilities, use a layer that knows tree-sitter, never `mct-core`. Consider an iterative `first_error` traversal; first demonstrate the deep-input case and preserve the first error's location. If a new crate is needed, explain how it reduces maintenance and avoids dependency cycles.

Acceptance: registries remain in parity; fixtures produce the same symbols/relations/ranges; no new coupling from core to grammars/SQLite; no unexplained snapshot changes. Do not change public APIs across the project without an issue. Report concrete duplication removed and behavior tests, not estimated percentages.
