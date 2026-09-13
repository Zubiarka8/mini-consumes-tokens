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

## JavaScript/TypeScript (`crates/ccm-lang-js-ts/tests/fixtures/webapp`)

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 54 | ~13 | 1637 (grep 800 + read 837 across 2 files) | ~409 | 96.7% |
| what calls this function (`log`) | 63 | ~15 | 998 (grep 380 + read 618 across 2 files) | ~249 | 93.7% |
| who uses this symbol (`addItem`) | 71 | ~17 | 1103 (grep 266 + read 837 across 2 files) | ~275 | 93.6% |

## C++ (`crates/ccm-lang-cpp/tests/fixtures/billing-app`)

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 106 | ~26 | 1350 (grep 740 + read 610 across 3 files) | ~337 | 92.1% |
| what calls this function (`log`) | 91 | ~22 | 1130 (grep 548 + read 582 across 3 files) | ~282 | 91.9% |
| who uses this symbol (`addItem`) | 86 | ~21 | 1408 (grep 798 + read 610 across 3 files) | ~352 | 93.9% |

5-file fixture (`Invoice.h`/`.cpp`, `Logger.h`/`.cpp`, `Main.cpp`) — one more file matches per grep query than Java/C#/JS-TS because the header/source split means both the declaration and the definition are real, separate matches, which is exactly the case this crate is built to handle (see `ccm-lang-cpp/src/lib.rs`'s module doc). The reduction holds despite that extra file.

## Go (`crates/ccm-lang-go/tests/fixtures/billing-app`)

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 35 | ~8 | 1580 (grep 711 + read 869 across 3 files) | ~395 | 97.8% |
| what calls this function (`Log`) | 94 | ~23 | 794 (grep 356 + read 438 across 2 files) | ~198 | 88.2% |
| who uses this symbol (`AddItem`) | 42 | ~10 | 1003 (grep 555 + read 448 across 2 files) | ~250 | 95.8% |

## Status toward the general "3+ languages" benchmark criterion

**Met** (an earlier session): Java, C#, and JavaScript/TypeScript have real benchmark numbers — 90–92%, 92%, and 94–97% character reduction respectively across the three canonical queries; no longer blocking anything. C++ (92–94%) and Go (88–98%) benchmarks were run this session alongside their crates, mostly as continued evidence per language rather than to satisfy the general criterion. Rust and Python were implemented earlier and still aren't benchmarked; the harness remains language-agnostic, so adding them later is just two more `run_language` calls with a Rust/Python fixture, not new tooling — tracked as a (non-blocking) remaining item in `checklist.md`.
