# Performance & limits

- **Parallel parsing** (#635): a full reindex parses on `min(available cores, 8)` threads, or `MCT_INDEX_WORKERS` (1 to 64; 1 parses on the writing thread). One thread walks the tree and is the only SQLite writer; it writes each result in walk order, so ids and issue order match a sequential run. At most 2 jobs and 2 results per worker are queued, and the walk waits while that many results wait for an earlier one, which bounds memory. A parser panic is caught on its thread and re-raised on the writer, rolling the run back. A path reindex (`reindex_paths`, the watcher) stays sequential. Measured on this repository, 10 cores: cold `init` 7.48 s → 3.67 s (median of 5), peak RSS 73.7 → 89.8 MB.
- **`MAX_TRAVERSAL_DEPTH = 256`** — every language crate's AST walker stops recursing past this depth. See [[sec-001-php-stack-overflow]].
- **Fuzzing**: `cargo-fuzz`, 30s smoke run per language crate, CI matrix `fuzz-smoke` — Linux/macOS only (ASan DLL/MSVC-sancov issues on Windows).
- **Token benchmark**: 3 canonical queries (`find_symbol`, `find_callers`, `find_references`) vs. Grep+Read baseline, most languages in the high-80s–90s% character reduction; ceiling 97.8% (Go); floor 72.0% (XML `find_symbol`). Full methodology: `benchmarks/token-benchmark.md`.

#performance
