# TASK-DOCS-MCP-PROSE checkpoint

Task / previous agent / status: TASK-DOCS-MCP-PROSE / worker (Claude) / completed
Branch / worktree / base / latest: docs/semantic-search-prose-limits / mini-consumes-tokens-docs-semantic-search-prose-limits / a3e10b0f450a93107061152feb862a1d242820a1 / 2b3900c (plus this checkpoint commit)

Objective and acceptance criteria: verify docs changed in a3e10b0..9c2f2ba against code: 20 tools defined; 19 default; select_tools only with MCT_TOOL_ATTENTION=1; semantic_search registered without the semantic feature and reports unavailable; Markdown prose limits; 24,000-byte limit applies to the literal list, not the whole response.

Completed:
- Verified against source at the base commit (all met):
  - 20 TOOL entries: crates/mct-mcp-server/src/tools.ttc (20 lines starting TOOL); KNOWN_TOOL_NAMES in src/ttc.rs:215-232.
  - select_tools removed unless env is "1": src/server.rs:663 (is_ok_and v == "1"), :1152-1154 (remove_route); attend errors when off :1356-1360.
  - 19 default: tests/tools_list.rs:54-57 (KNOWN_TOOL_NAMES.len() - 1).
  - semantic_search always registered (#[tool], server.rs:1829-1830); no-feature load() error embedder.rs:44-50; handler returns text "semantic_search unavailable — {reason}; use search_symbols or hybrid_search" server.rs:1850-1857. Args: query, top_k (1..=100), path, language only (server.rs:192-206).
  - Prose limits: crates/mct-core/src/literal.rs: 256 chars (:11), 8 chars min (:14), 500 per file (:18), two words with a letter + secret rejection (:65-75), distinct literals and first occurrence keeps its line (:114-124). Markdown paragraphs and table rows only: crates/mct-lang-md/src/lib.rs:674-687.
  - 24,000 bytes: format.rs:18 DEFAULT_BYTE_BUDGET; literal_hits uses BudgetedList::new(DEFAULT_BYTE_BUDGET) for the literal body only (format.rs:342), header and notes added outside.
  - batch rejects reindex, select_tools, batch: server.rs:521.
- Fixed one inaccuracy: tools.mdx said all result-list renderers use the 24,000-byte budget, but toon renderers (e.g. format.rs:362 literal_hits_toon) have no byte budget. Reworded to text-format lists; toon limited by row count.

Changed paths: website/content/docs/tools.mdx (fix commit), this file. No uncommitted changes.

Checks executed:
- cargo test --manifest-path <wt>/Cargo.toml -p mct-mcp-server --test tools_list --test catalog: catalog 3 passed 0 failed 0 ignored; tools_list 3 passed 0 failed 0 ignored.
- npm --prefix website run types:check: Types generated successfully, tsc no errors.
- npm --prefix website run build: exit 0.
- npm --prefix website run test:export: PASS: 29 HTML exports, 1101 local asset/link targets.
- npm --prefix website run test:resources: PASS: 177 matching, nonempty translation keys.
Pending: none. Full cargo test of the workspace not run (docs-only change).

Known problems: docs/02-crates/core/mct-mcp-server.md:11 says "Implements the 10 tools" (stale internal vault note, out of this task's scope, not edited). CONTRIBUTING.md:96 says "11 tools" (historical figure "as of this writing").
Dependencies: none.
Files to inspect first: website/content/docs/tools.mdx, website/content/docs/limitations.mdx.
Next action: coordinator review and integrate; optionally open a follow-up for the stale counts above.
