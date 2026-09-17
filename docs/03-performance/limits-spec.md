# Performance & limits

- **`MAX_TRAVERSAL_DEPTH = 256`** — every language crate's AST walker stops recursing past this depth. See [[sec-001-php-stack-overflow]].
- **Fuzzing**: `cargo-fuzz`, 30s smoke run per language crate, CI matrix `fuzz-smoke` — Linux/macOS only (ASan DLL/MSVC-sancov issues on Windows).
- **Token benchmark**: 3 canonical queries (`find_symbol`, `find_callers`, `find_references`) vs. Grep+Read baseline, most languages in the high-80s–90s% character reduction; ceiling 97.8% (Go); floor 72.0% (XML `find_symbol`). Full methodology: `benchmarks/token-benchmark.md`.

#performance
