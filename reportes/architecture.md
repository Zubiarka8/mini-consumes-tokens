# Report: docs-architecture

## Researched
- MCP index tools (list_symbols, find_symbol, get_project_overview) on `mct-core` (registry, trait), `mct-mcp-server/src/registry.rs`, `mct-cli/src/main.rs`, `ttc.rs`.
- Manifests (all `crates/*/Cargo.toml`), `.github/workflows/ci.yml`, `tools.ttc`, `schema.rs` migrations, `scripts/unix/new-language-check.sh`, `fuzz/` dirs, rules.md (Dogfooding, Architecture).
- No local `.claude/skills` dir exists in this worktree; documentation-and-adrs not readable there, so no skill used.

## Findings
| File | Claim | Evidence | Action |
|---|---|---|---|
| ARCHITECTURE.md | "9 lookup tools" | `tools.ttc` has 18 `TOOL` blocks (matches `KNOWN_TOOL_NAMES`) | Fixed to 18; added tool groups |
| ARCHITECTURE.md | Lua is a 17th, non-shipped proof plugin; 17 listed languages | `registry.rs` and cli `build_registry` register 17 parsers including `LuaParser`; the listed 17 already include Lua | Removed false claim |
| ARCHITECTURE.md | "261 automated tests" | Not verifiable cheaply, certainly stale | Replaced with non-numeric statement |
| ARCHITECTURE.md | Shared rulebook description | `LanguageRegistry` in mct-core; `mct-tree-sitter` helper crate not mentioned | Added both |
| ARCHITECTURE.md | (none about migrations) | 10 `M::up` in `mct-index/src/schema.rs` | Added "10 as of this writing" |
| CONTRIBUTING.md | Register in "three places" | Also need root workspace members/deps and Cargo.toml deps in mcp-server and cli (`new-language-check.sh` marks them required); fuzz harness+CI matrix is required, not "if" | Rewrote step 4 |
| CONTRIBUTING.md | Crate deps `mct-core` + grammar | lang crates also depend on `mct-tree-sitter` and `tree-sitter` | Fixed step 1 |
| CONTRIBUTING.md | "11 tools" | 18 | Fixed |
| CONTRIBUTING.md | CI has three jobs; clippy without deny flags; fuzz "8" crates | ci.yml jobs: fmt, build-test, cargo-audit, fuzz-smoke; clippy has `-D clippy::unwrap_used/expect_used/panic`; fuzz matrix has 17 lang crates + mct-index | Fixed |
| CONTRIBUTING.md | LanguageParser methods, register panics on duplicate extension, ParseError::Syntax | `mct-core/src/lib.rs`, `registry.rs` | Verified, no change |
| CONTRIBUTING.md | Step 6 files, fuzz harness layout, TTC tests | `internal/checklist.md`, `fuzz/fuzz_targets/parse_<name>.rs`, `ttc.rs` exist | Verified |

## Changes
- ARCHITECTURE.md: commit 11bf1ad.
- CONTRIBUTING.md: second commit (see git log).

## Unverified
- Actual test count (did not run `cargo test`).
- ARCHITECTURE.md claims about secret skipping, Git-style fingerprinting, and cross-OS CI were not re-checked in code (CI OS matrix confirmed).
- `tools.ttc` token-reduction figure (~68%) kept as historical.

## Follow-ups
- rules.md "Architecture" says register in "exactly two places" and tool list omits `discover_tool_categories`/`get_tool_schema`, and mct-cli lists only `init/reindex/status/mcp-register` (main.rs also has self_update, probe, ignore_init, etc.); out of scope for this task.
- docs/06-templates/lang-crate-base.md and a second eval suite (`suite-portfolio-3d.json`) not reviewed.
