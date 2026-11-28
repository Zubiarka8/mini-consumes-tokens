# Path catalog: cost and token savings

Issue #629, [ADR-052](../docs/01-architecture/adrs/052-path-catalog.md). Measured on 2026-11-28.

## Setup

- Machine: Apple M4 (10 cores), macOS 27.0, release builds, Rust 1.96.0.
- Base: `31ac486f` (main before the catalog). Branch: `feat/non-source-file-index` at `ec3f9448`.
- Corpus: this repository at `31ac486f` (`git archive`), plus the path catalog fixture (`crates/mct-index/tests/fixtures/path-catalog`, about one file per family) under `catalog-fixture/`. 2044 files, 46 MB. Each binary indexes its own copy.
- Another session was running `cargo test` on the same machine during the run, so expect a few percent of noise in the timings. Base and branch ran back to back under the same load.
- Tokens are characters / 4, as the `mct-eval` report estimates them.
- Script: `measure.py` (kept out of the repository). It runs `init` 3 times on an empty `.mct-index`, an unchanged `reindex` 5 times, and each query 10 times, and takes the median.

## Index cost

| Measure | Base (31ac486f) | Branch | Change |
|---|---|---|---|
| Full index (`init`, median of 3) | 9.44 s | 9.07 s | -4.0% (noise) |
| Unchanged reindex (median of 5) | 0.169 s | 0.175 s | +3.4% (noise) |
| Index size (sqlite + wal) | 36648 KiB | 37312 KiB | +1.8% (+664 KiB, about 330 bytes per file) |

The classification adds no measurable time to a full index. Only conflicting suffixes open a file, for at most 4096 bytes. An unchanged reindex compares size and modification time per row and writes nothing.

## Finding files: tokens per task

| Task | Old flow: `get_file_tree` depth 8 | Old flow: Glob / `find` with the right patterns | `find_files` | `find_files` latency (CLI process, median of 10) |
|---|---|---|---|---|
| A logo (`query: "logo"`) | ~6024 tokens, truncated at the 24000-byte budget | ~70 tokens (8 paths, one under the excluded `node_modules/`) | ~114 tokens (7 rows) | 8 ms |
| Excel files (`format` xlsx, xls, xlsm in one `batch`) | ~6024 tokens, truncated | ~25 tokens (3 paths) | ~138 tokens (3 rows) | 7 ms per query |
| Excel files, as a category (`category: "spreadsheet"`) | same | — | ~806 tokens (42 rows: 3 Excel, 39 CSV/TSV) | 7 ms |
| Demo audio (`category: "audio", path: "catalog-fixture/demo/"`) | ~6024 tokens, truncated | ~46 tokens (5 paths) | ~100 tokens (5 rows) | 8 ms |

Reading the table:

- **Against walking the tree**, `find_files` costs 1.7 to 2.3% of the tokens: about 100 instead of about 6000. The walk is also incomplete: `get_file_tree` stops at its byte budget, so on a larger repository the file may not be in the answer at all.
- **Against a Glob that already names the right extensions**, `find_files` costs about 1.5 to 5 times more tokens. Each row also carries category, format and size, and the page carries a header. The Glob is cheaper only when the agent already knows every extension of the family (`.mp3 .wav .flac .opus .mid .m4a …`) and accepts excluded paths (`node_modules/`, `secrets/`). It can also read a `.ts` video as source. `find_files` needs no extension list, applies the index exclusions, and settles the doubtful suffixes.
- **A category is broader than a product name.** "Excel" is three formats. `category: "spreadsheet"` also returns CSV and TSV and cost ~806 tokens here; three `format` queries in one `batch` cost ~138.

## Quality gate

`mct-eval` (`cargo run -p mct-eval`): accuracy 1.000 on all suites, with two new `find_files` cases in `crates/mct-eval/suite.json` (~166 and ~175 tokens). The tool catalog grows from ~12319 to ~12890 tokens (+571) for the new tool's schema and description. The TTC description guard moves from 4800 to 5000 bytes. The `get_indexing_status` case of the portfolio-3d suite grows from ~189 to ~222 tokens for the new "Files by category" line.

## Reproduce

```sh
# From a clean checkout of each revision:
cargo build --release -p mct-cli -p mct-mcp-server
mct-cli --root <corpus> init --no-obsidian
mct-cli --root <corpus> reindex --no-obsidian
mct-cli --root <corpus> files --query logo
mct-cli --root <corpus> files --category audio --path catalog-fixture/demo/
mct-cli --root <corpus> call --tool find_files --args '{"category":"document","format":"pdf","limit":20}'
```
