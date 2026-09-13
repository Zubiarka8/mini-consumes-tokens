# Token benchmark: MCP tools vs. Read/Grep/Glob

Methodology (`crates/ccm-cli/examples/token_benchmark.rs`, run via `cargo run -p ccm-cli --example token_benchmark`):

- **MCP**: the exact formatted text an MCP tool call (`find_symbol`/`find_callers`/`find_references`) would return, measured in characters.
- **Read/Grep/Glob baseline**: the realistic alternative without semantic navigation — grep the exact term across every file in the fixture, then Read the full content of every file that matched (deduplicated). Grep alone rarely gives enough context to stop there, so the baseline includes both steps, matching how an agent actually investigates without an index.
- **~tokens**: `chars / 4`, a standard rough English/code heuristic — not a real tokenizer run, just enough to see the order of magnitude. Character counts (the exact numbers) are the primary, reproducible metric.
- Three canonical queries per language: "find the definition of X" (`find_symbol`), "what calls this function" (`find_callers`), "who uses this symbol" (`find_references`).

Caveat: fixtures are intentionally small (3 files, ~10–20 lines each) — see "Cobertura de lenguajes" in `checklist.md` for why (no versioned larger fixture yet). Absolute counts here are small; the reduction *ratio* is the meaningful signal, and it should hold or improve on larger real repos, since MCP output stays proportional to the number of actual matches while the grep+read baseline grows with the size of every file a plain-text match happens to touch, regardless of relevance.

## Java (`crates/ccm-lang-java/tests/fixtures/billing-app`)

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 77 | ~19 | 838 (grep 253 + read 585 across 2 files) | ~209 | 90.8% |
| what calls this function (`log`) | 95 | ~23 | 955 (grep 416 + read 539 across 2 files) | ~238 | 90.1% |
| who uses this symbol (`addItem`) | 90 | ~22 | 1131 (grep 546 + read 585 across 2 files) | ~282 | 92.0% |

## C# (`crates/ccm-lang-csharp/tests/fixtures/billing-app`)

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 77 | ~19 | 936 (grep 260 + read 676 across 2 files) | ~234 | 91.8% |
| what calls this function (`Log`) | 97 | ~24 | 1180 (grep 551 + read 629 across 2 files) | ~295 | 91.8% |
| who uses this symbol (`AddItem`) | 98 | ~24 | 1244 (grep 568 + read 676 across 2 files) | ~311 | 92.1% |

## Status toward the general "3+ languages" benchmark criterion

Java and C# done (this session). Rust and Python were implemented in an earlier session but not yet benchmarked — the harness above is language-agnostic (any registered `LanguageParser` works), so running it for them is now just a matter of adding two more `run_language` calls with a Rust/Python fixture, not new tooling. Not yet done — tracked in `checklist.md`.
