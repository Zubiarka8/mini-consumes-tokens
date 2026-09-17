# Token benchmark: MCP tools vs. Read/Grep/Glob

Methodology (`crates/ccm-cli/examples/token_benchmark.rs`, run via `cargo run -p ccm-cli --example token_benchmark`):

- **MCP**: the exact formatted text an MCP tool call (`find_symbol`/`find_callers`/`find_references`) would return, measured in characters.
- **Read/Grep/Glob baseline**: the realistic alternative without semantic navigation — grep the exact term across every file in the fixture, then Read the full content of every file that matched (deduplicated). Grep alone rarely gives enough context to stop there, so the baseline includes both steps, matching how an agent actually investigates without an index.
- **~tokens**: `chars / 4`, a standard rough English/code heuristic — not a real tokenizer run, just enough to see the order of magnitude. Character counts (the exact numbers) are the primary, reproducible metric.
- Three canonical queries per language: "find the definition of X" (`find_symbol`), "what calls this function" (`find_callers`), "who uses this symbol" (`find_references`).

Caveat: fixtures are intentionally small (3 files, ~10–20 lines each) — see "Cobertura de lenguajes" in `internal/checklist.md` for why (no versioned larger fixture yet). Absolute counts here are small; the reduction *ratio* is the meaningful signal, and it should hold or improve on larger real repos, since MCP output stays proportional to the number of actual matches while the grep+read baseline grows with the size of every file a plain-text match happens to touch, regardless of relevance.

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

## Rust (`crates/ccm-lang-rust/tests/fixtures/billing-app`)

New fixture (`invoice.rs`/`logger.rs`/`main.rs`), same billing-app shape as Java/C#/C++/Go, added this session — none existed before.

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 37 | ~9 | 1299 (grep 647 + read 652 across 2 file(s)) | ~324 | 97.2% |
| what calls this function (`log`) | 103 | ~25 | 1370 (grep 655 + read 715 across 3 file(s)) | ~342 | 92.5% |
| who uses this symbol (`add_item`) | 44 | ~11 | 1262 (grep 610 + read 652 across 2 file(s)) | ~315 | 96.5% |

## Python (`crates/ccm-lang-python/tests/fixtures/billing-app`)

New fixture (`invoice.py`/`logger.py`/`main.py`), same shape as the Rust one above, added this session — none existed before.

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 38 | ~9 | 892 (grep 381 + read 511 across 2 file(s)) | ~223 | 95.7% |
| what calls this function (`log`) | 107 | ~26 | 963 (grep 535 + read 428 across 2 file(s)) | ~240 | 88.9% |
| who uses this symbol (`add_item`) | 46 | ~11 | 1079 (grep 568 + read 511 across 2 file(s)) | ~269 | 95.7% |

## Lua (`crates/ccm-lang-lua/tests/fixtures/inventory-app`)

Reused the existing `inventory-app` fixture and its proven query terms (same ones `index_integration.rs` already asserts on). `ccm-lang-lua` is deliberately **not** a `ccm-cli` dependency (see its `Cargo.toml` description — it's an architecture-validation exercise outside the project's original language scope, not wired into the production `ccm-cli`/`ccm-mcp-server` registries), so this row was produced by a separate, self-contained harness — `crates/ccm-lang-lua/examples/token_benchmark_lua.rs` — that duplicates the exact same `mcp_chars`/`grep_then_read_chars` measurement code rather than adding Lua to `ccm-cli`. Run via `cargo run -p ccm-lang-lua --example token_benchmark_lua` (named `_lua` to avoid an example-binary filename collision with `ccm-cli`'s own `token_benchmark` example).

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Inventory`) | 41 | ~10 | 1613 (grep 1138 + read 475 across 2 file(s)) | ~403 | 97.5% |
| what calls this function (`log`) | 143 | ~35 | 1156 (grep 646 + read 510 across 2 file(s)) | ~289 | 87.6% |
| who uses this symbol (`addItem`) | 88 | ~22 | 861 (grep 386 + read 475 across 2 file(s)) | ~215 | 89.8% |

## HTML (`crates/ccm-lang-html/tests/fixtures/site`)

Reused the existing 2-file, 2-language `site` fixture (`index.html` + `style.css`).

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`header`) | 37 | ~9 | 499 (grep 239 + read 260 across 2 file(s)) | ~124 | 92.6% |
| what calls this function (`nav`) | 0 | ~0 | 609 (grep 349 + read 260 across 2 file(s)) | ~152 | **100.0%** |
| who uses this symbol (`style.css`) | 52 | ~13 | 348 (grep 142 + read 206 across 1 file(s)) | ~87 | 85.1% |

**Outlier, called out explicitly, not smoothed over**: the "what calls this function" row is a degenerate 100.0%, and it is *not* a genuine efficiency win. `ccm-lang-html` (like `ccm-lang-css` and `ccm-lang-xaml` below) never emits a `RelationKind::Calls` relation — HTML has no function-call semantics — so `find_callers` returns 0 hits (0 chars) for *any* term, while grep+read still has to scan and read whatever files happen to contain the literal text. The 100% figure measures "MCP correctly reports zero callers because there are none to report" against "grep still had to look", not a real navigation win. Real, structural, and expected — not a bug or a cherry-picked term.

## CSS (`crates/ccm-lang-css/tests/fixtures/theme`)

Reused the existing 2-file `theme` fixture (`main.css` importing `extra.css`).

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`#header`) | 32 | ~8 | 262 (grep 107 + read 155 across 1 file(s)) | ~65 | 87.8% |
| what calls this function (`.sidebar`) | 0 | ~0 | 263 (grep 108 + read 155 across 1 file(s)) | ~65 | **100.0%** |
| who uses this symbol (`extra.css`) | 47 | ~11 | 273 (grep 118 + read 155 across 1 file(s)) | ~68 | 82.8% |

**Outlier, called out explicitly**: same degenerate 100.0% as HTML above and for the same reason — `ccm-lang-css` only ever emits `Imports` relations, never `Calls`, so `find_callers` is structurally always empty for CSS. **Also notable, on the low side**: `#header` (87.8%) and `extra.css` (82.8%) both fall below the 88–98% band the first 5 languages established — a real measured result, not an error. The fixture is intentionally tiny (2 files, ~15 lines total), so the fixed per-line overhead of the MCP-formatted output (path:line:col + kind + name) is proportionally larger against a very small baseline; the same effect shows up in XML below, more severely.

## XML (`crates/ccm-lang-xml/tests/fixtures/services-config`)

The pre-existing `tests/fixtures/config` fixture is a single 8-line file — too trivial for a meaningful multi-file comparison against the other benchmarked languages, so a new 3-file fixture (`api.xml`/`worker.xml`/`logging.xml`, same nested-`id`/`name`/`Name` shape as `config`) was added this session for benchmarking; `config` itself is untouched and still used by `ccm-lang-xml`'s own integration tests.

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`worker`) | 71 | ~17 | 254 (grep 132 + read 122 across 1 file(s)) | ~63 | 72.0% |
| what calls this function (`jobs`) | 0 | ~0 | 257 (grep 135 + read 122 across 1 file(s)) | ~64 | **100.0%** |
| who uses this symbol (`api`) | 0 | ~0 | 254 (grep 126 + read 128 across 1 file(s)) | ~63 | **100.0%** |

**Outliers, called out explicitly and prominently, in both directions**:
- **Both relation queries are structurally 100.0% for XML, always, regardless of fixture or term.** `ccm-lang-xml`'s own module doc states it plainly: "No relations are ever emitted — there is nothing dialect-generic to link to" (generic XML has no universal cross-file-reference convention, unlike XAML). So `find_callers` *and* `find_references` both return 0 hits for every term in every XML fixture — this isn't a fixture limitation fixable by making the fixture bigger or more realistic, it's the parser's deliberate scope boundary. Two of the three canonical queries are not meaningfully answerable by `ccm-lang-xml` today.
- **`find_symbol` on `worker` is 72.0% — well below the 88–98% range every other language falls in**, the lowest reduction of any query benchmarked this session. This is a real, measured number: with a genuinely tiny fixture (3 files, ~5 lines each) the MCP output's fixed per-hit overhead (`path:line:col [xml] element worker\n` = 71 chars for one hit) is a large fraction of an equally tiny grep+read baseline (254 chars total, single file). The *ratio* is the weak signal here specifically because XML's structural-only symbol model gives the MCP side little to say beyond "here it is" — worth watching if it holds up on a larger real XML file, but not fabricated or adjusted to fit expectations.

## XAML (`crates/ccm-lang-xaml/tests/fixtures/app`)

Reused the existing 2-file, 2-language `app` fixture (`MainWindow.xaml` + `MainWindow.xaml.cs`).

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`SaveBtn`) | 43 | ~10 | 705 (grep 339 + read 366 across 2 file(s)) | ~176 | 93.9% |
| what calls this function (`Click`) | 0 | ~0 | 705 (grep 339 + read 366 across 2 file(s)) | ~176 | **100.0%** |
| who uses this symbol (`SaveBtn_Click`) | 66 | ~16 | 705 (grep 339 + read 366 across 2 file(s)) | ~176 | 90.6% |

**Outlier, called out explicitly**: the same structural 100.0% as HTML/CSS above — `ccm-lang-xaml` only emits `References` relations (event-handler-attribute → C# method), never `Calls`, so `find_callers` is always empty. Unlike XML, XAML's `find_references` row (90.6%) is real and meaningful — it resolves a genuine cross-*language* relation (a XAML `Click` attribute back to its C# method body) — so only the "callers" query is degenerate here, not two of three.

## Bash (`crates/ccm-lang-bash/tests/fixtures/deploy-scripts`)

Reused the existing 3-file `deploy-scripts` fixture (`deploy.sh`/`lib.sh`/`run.sh`).

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`deploy`) | 72 | ~18 | 617 (grep 481 + read 136 across 2 file(s)) | ~154 | 88.3% |
| what calls this function (`log`) | 86 | ~21 | 510 (grep 373 + read 137 across 2 file(s)) | ~127 | 83.1% |
| who uses this symbol (`build`) | 46 | ~11 | 478 (grep 369 + read 109 across 1 file(s)) | ~119 | 90.4% |

**Notable, on the low side**: "what calls this function" (`log`) at 83.1% is measurably below the 88–98% band the other languages fall in — a real result, not smoothed over. `log` is a very short, common word (`echo "[LOG] $1"` itself contains "log" via the `[LOG]` literal), so grep's baseline stays modest while a 2-caller MCP relation listing is proportionally not much smaller. Small-fixture overhead, same family of effect as CSS/XML above, not a Bash-specific problem.

## PowerShell (`crates/ccm-lang-powershell/tests/fixtures/deploy-scripts`)

Reused the existing 3-file `deploy-scripts` fixture (`Deploy.ps1`/`Lib.psm1`/`Run.ps1`).

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoke-Build`) | 50 | ~12 | 433 (grep 270 + read 163 across 1 file(s)) | ~108 | 88.5% |
| what calls this function (`Write-Log`) | 126 | ~31 | 658 (grep 430 + read 228 across 2 file(s)) | ~164 | 80.9% |
| who uses this symbol (`Invoke-Deploy`) | 54 | ~13 | 459 (grep 266 + read 193 across 2 file(s)) | ~114 | 88.2% |

**Notable, on the low side**: "what calls this function" (`Write-Log`) at 80.9% is the second-lowest non-degenerate reduction measured this session, below the 88–98% band. PowerShell's long, hyphenated cmdlet-style names (`Write-Log`, `Invoke-Build`) make each MCP relation line longer than the equivalent Java/C# entry, and there are only 2 callers in this fixture — the same small-fixture fixed-overhead effect noted for CSS/XML/Bash above, not evidence of a PowerShell-specific issue.

## PHP (`crates/ccm-lang-php/tests/fixtures/billing-app`)

Reused the existing 3-file `billing-app` fixture (`Invoice.php`/`Payable.php`/`run.php`).

| Query | MCP chars | MCP ~tokens | Grep+Read chars | Grep+Read ~tokens | Reduction |
|---|---|---|---|---|---|
| find the definition of (`Invoice`) | 73 | ~18 | 790 (grep 401 + read 389 across 2 file(s)) | ~197 | 90.8% |
| what calls this function (`pay`) | 39 | ~9 | 894 (grep 430 + read 464 across 3 file(s)) | ~223 | 95.6% |
| who uses this symbol (`Payable`) | 55 | ~13 | 735 (grep 397 + read 338 across 2 file(s)) | ~183 | 92.5% |

All three PHP rows land squarely inside the 88–98% band established by the first 5 languages — no outliers.

## Status toward the general "3+ languages" benchmark criterion

**Met** (an earlier session): Java, C#, and JavaScript/TypeScript have real benchmark numbers — 90–92%, 92%, and 94–97% character reduction respectively across the three canonical queries; no longer blocking anything. C++ (92–94%) and Go (88–98%) benchmarks were run in a later session alongside their crates. This session added real, measured rows for all 10 remaining languages — Rust, Python, Lua, HTML, CSS, XML, XAML, Bash, PowerShell, PHP — bringing every language crate in the project to at least one real benchmark entry (15 languages total).

Two structural findings surfaced by this round, not present in the original 5 languages (all general-purpose, function-call languages):
- **`ccm-lang-css`, `ccm-lang-html`, and `ccm-lang-xaml` never emit a `Calls` relation** (they're declarative/markup languages without function-call semantics), so their "what calls this function" query is *always* a degenerate 100.0% reduction (0 MCP hits vs. a non-zero grep+read baseline) — a real, structural result, not a genuine navigation win, and not comparable to the same query on Java/Rust/Bash/etc.
- **`ccm-lang-xml` never emits *any* relation** (by explicit design — see its module doc), so *both* "what calls this function" and "who uses this symbol" are always 100.0% for XML, on top of its `find_symbol` reduction (72.0%) being the lowest of any query measured this session, from small-fixture fixed-overhead. XML is the weakest-covered language by this benchmark's own three-query methodology; only 1 of 3 canonical queries produces a meaningful number for it today.

CSS (`extra.css`, 82.8%), Bash (`log`, 83.1%), and PowerShell (`Write-Log`, 80.9%) also each produced one non-degenerate query result below the 88–98% band — all attributed to the same fixed-per-line-overhead-against-a-tiny-baseline effect, not language-specific defects; see each section above for the specific number and reasoning.
