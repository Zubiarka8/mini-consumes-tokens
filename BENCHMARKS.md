# Reproducible token benchmark

Measured on 2026-11-20 with `cl100k_base` from `tiktoken` 0.12.0. The
benchmark uses three public repositories at fixed commits: ripgrep 14.1.1
(`4649aa9700619f94cf9c66876e9549d83420e16c`), Requests 2.32.5
(`b25c87d7cb8d6a18a37fa12442b5f883f9e41741`) and Preact 10.27.0
(`e42b82b17b3bf2ef5ff1f26328582254d7348278`).

For each named symbol, the script runs `search_symbols_like` and requests
`build_context_pack`, both scoped to its definition file. Issue #24's
`search_symbols + get_symbol_source` workflow has no `get_symbol_source`
tool; `build_context_pack` is the tool that returns a symbol's definition and
source, plus its callers, callees and tests. It also runs an exact text search across the repository and
reads every matched file once. The MCP JSON column is the JSON-RPC tool-result
envelope containing the same TOON text, not a separate structured JSON schema.
Tool definitions, prompts and client-side framing are excluded. Token counts
apply to output only; neither precision nor completeness is measured. Context
packs have bounded source and relationship output, while the full-file baseline
reads complete matching files. The two outputs are therefore not equivalent
and these numbers are not a general estimate of an agent's end-to-end savings.

| Repository | Symbol | MCP TOON | MCP JSON envelope | grep | full matched files | grep + read |
|---|---|---:|---:|---:|---:|---:|
| ripgrep | Searcher | 733 | 864 | 7938 | 153786 | 161724 |
| ripgrep | RegexMatcher | 440 | 550 | 6536 | 105666 | 112202 |
| ripgrep | Standard | 452 | 567 | 5811 | 218694 | 224505 |
| requests | Session | 1683 | 1877 | 2845 | 77007 | 79852 |
| requests | PreparedRequest | 801 | 936 | 1269 | 80259 | 81528 |
| requests | Response | 934 | 1076 | 3836 | 95836 | 99672 |
| preact | render | 1297 | 1481 | 75161 | 535877 | 611038 |
| preact | createElement | 1232 | 1424 | 7291 | 285164 | 292455 |
| preact | hydrate | 355 | 460 | 2490 | 72341 | 74831 |

The numbers above were measured with `mct-cli` built from `main` at commit
`b7053663` (ripgrep 14.1.1 as `rg`). Results can change with the indexer, CLI, source
checkout, `rg` version and tokenizer. The script refuses a different repository
revision and aborts if it cannot find the requested context pack.

## Reproduce

Install Rust, Git, Python 3, ripgrep, and `tiktoken==0.12.0` in a Python
environment. The tokenizer downloads its vocabulary on first use. Build
`mct-cli` with `cargo build -p mct-cli`, clone each tagged project into a
directory named `ripgrep`, `requests`, or `preact`, and run:

```sh
python benchmarks/public-token-benchmark.py --cli target/debug/mct-cli --repos /path/to/repositories
```

The script builds an MCT index in each clone. The three repositories can be
obtained from `https://github.com/BurntSushi/ripgrep`,
`https://github.com/psf/requests`, and `https://github.com/preactjs/preact`.
Use the tags and verify the commits above. See
[`benchmarks/token-benchmark.md`](benchmarks/token-benchmark.md) for the
earlier fixture-based measurements and their limitations.
