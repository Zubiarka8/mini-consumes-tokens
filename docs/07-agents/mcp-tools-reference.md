# MCP tools reference

Full reference for exploring this repository with its own MCP tools. The short version is in the Dogfooding section of [rules.md](../../rules.md#dogfooding-exploring-this-repositorys-source).

| Question | Tool and parameters |
|---|---|
| What symbols does a file or crate have, name unknown? | `list_symbols`. A file path matches exactly. A directory or crate path (no extension) matches as a prefix. `kind` and `language` narrow it. Use it first. |
| What is one file's shape (top-level declarations, bodies collapsed)? | `get_file_skeleton`, one file at a time. Use `list_symbols` first if you do not know which file. |
| What does an unfamiliar file, directory, crate or project look like? | `get_project_overview`: a capped, hierarchical digest (modules, key symbols, top callers) in one call. It is coarse, so switch to the two tools above once you know the file. |
| What is the directory and file layout? | `get_file_tree`: a plain, depth-limited tree with no symbol data, pruned of the noise directories reindexing skips (`target`, `node_modules`, `.git`). |
| Where is X defined? | `find_symbol` |
| Exact name unknown (`parse request`, `parseReq`, `http server`)? | `search_symbols_like`: BM25-ranked FTS5 match over each name split at camelCase, snake_case, kebab-case and acronym boundaries. An exact name always comes first. `limit` defaults to 10 (max 100). Also `offset`, `snippet_lines`, and optional `detail` (`overview`, `summary`, `detailed`) with `max_tokens`. `max_tokens` is an estimated budget for the whole response, not a model limit. A cut page says `next: offset=N`, and budgeted calls skip the cache. It is not exhaustive, so graph tools stay the source of truth for relations. |
| Only know what the code does (`load settings from disk`)? | `hybrid_search_like`: `search_symbols_like` fused with local-embedding similarity by weighted Reciprocal Rank Fusion. `alpha` runs from 0 (lexical only) to 1 (semantic only). If omitted, the query shape picks it: 0.1 for an identifier, 0.75 for prose, 0.5 otherwise. Also `top_k` (10, max 100), `offset`, `snippet_lines`, `detail` and `max_tokens`. The lexical side expands developer-verb synonyms and resolves `Type::member` and `module.fn` qualifiers. A `"double-quoted"` query is an exact phrase: lexical only whatever `alpha`, words consecutive and in order, with no prefix, synonym or any-word matching. A literal name match is boosted to first place. A second section lists prose string literals (error and log messages) that hold the phrase, as `path:line in <kind> <name> "text"`, case and diacritic insensitive. Today only `mct-lang-rust` (string literals) and `mct-lang-md` (paragraphs and table rows) extract them. The semantic half needs a `--features semantic` build of `mct-mcp-server` (`bge-small-en-v1.5` by default, `MCT_EMBEDDING_MODEL` to switch, downloaded on first use into `.mct-index/models`). Without it the tool returns the lexical ranking and says so on its first line. |
| Search by meaning only, with a cosine score and signature per hit? | `semantic_search`: the `top_k` (10, max 100) symbols nearest the query embedding, with no lexical side. `path` and `language` narrow it. It uses the same `semantic` build and stored vectors as `hybrid_search_like`. `mct-mcp-server --rebuild-embeddings` re-embeds everything and exits. |
| Who calls X directly? | `find_callers` |
| What does X call? | `find_calls` |
| Every reference to X (calls, imports, extends, implements, overrides)? | `find_references` |
| Full blast radius before changing or removing X? | `impact_analysis` |
| Which files does this file or directory import, who imports it, any import cycle? | `get_dependency_graph`: `module` (repo-relative file or directory), `depth` (default 2, clamped to the maximum query depth), `limit`, `format` (`toon`), `export` (`dot` or `mermaid`). Only resolved imports are edges. Unresolved and third-party imports are listed separately. It complements the symbol-level `impact_analysis`. |
| About to work on X: its definition, comment, source, callers, callees, dependencies and tests, without reading whole files? | `build_context_pack`: one call. It returns X's definitions with the comment or attribute block above them and the first `source_lines` lines (40, max 200). A longer body ends with a footer naming the hidden lines (`L491-L827`) to read from the file (paging: #371). Then every related symbol once (roles merged, for example `caller,test`) with its location and a one-line signature. File-level importers and unresolved callee names come as footers, and ambiguous callees go in a separate uncertain section, never picked. `depth` walks both ways, `path` and `language` pick the definition of an ambiguous name, `limit` is 30, `format` is `toon`. Optional `detail` and `max_tokens` work as above: use `overview` to locate, and `detailed` with a budget when you need source. `overview` then `detailed` costs more than one call (ADR-016). It uses about 85% fewer tokens than `find_symbol` plus `find_calls`, `find_callers`, `impact_analysis` and reading the files (`crates/mct-mcp-server/tests/context_pack.rs`). |
| Candidate unused functions, classes, structs, enums, traits, interfaces or type aliases? | `find_dead_code`. A heuristic: zero indexed references, not true visibility. A reference resolved to another symbol, or marked external, does not count; an ambiguous one counts for every same-named symbol. |
| Server started with `MCT_TOOL_ATTENTION=1` (only `select_tools` listed)? | `select_tools` with the user's request first. It narrows `tools/list` and sends `tools/list_changed`. Every tool stays callable by name. |
| Several lookups whose queries you already know (`find_symbol`, `find_callers`, `get_file_skeleton`)? | `batch`: `queries: [{tool, args}]`, up to 25, any read-only tool (not `reindex`, not a nested `batch`). Each result sits under its own `[n] tool` header, and a failing sub-query is reported inline without failing the rest. It drops the per-call envelope, for at least 20% fewer tokens (measured in `crates/mct-mcp-server/tests/batch.rs`). |
| Is the index stale or healthy? | `get_indexing_status`. Use `reindex` only if it looks stale. |

`find_references`, `find_calls`, `find_callers` and `impact_analysis` also take `depth` (multi-hop breadth-first search beyond the direct hit, default 1, clamped to 32) and `offset` (pagination past `limit`). Try `depth` before you chain calls by hand to walk the relation graph further out.

## Markdown literal search

Use `hybrid_search_like` with a double-quoted phrase. `search_symbols_like` searches symbols only, even with quotes.

- Paragraphs and pipe-table header and body rows keep their Markdown source text with whitespace collapsed.
- Headings stay symbols. Frontmatter and code blocks are excluded from literals.
- The shared prose filter wants at least eight characters and two whitespace-separated words with letters, and rejects suspected secrets. It keeps the first 256 characters of each block and at most 500 distinct literals per file (the first occurrence supplies the line). Identifier-only or URL-only rows can therefore drop out.
- Files without a supported extension are not treated as Markdown.
- After upgrading an existing index to Markdown literal extraction, run `mct-cli --root <project> reindex --force` once, because unchanged files are otherwise skipped. The parser change needs no schema migration.
- Exact-phrase literal search works without the optional semantic feature.

## Progressive tool discovery

`discover_tool_categories` lists every registered tool's name and one-line purpose, grouped by category, with no input schemas. It is a cheap first call for a client that wants to defer the schema payload. `get_tool_schema` then returns one named tool's full input schema, description and contract (`PRE`, `POST` and `EFFECTS` from `tools.ttc`) on demand.

Both are additive. The standard MCP `tools/list` handshake still returns every tool's full schema up front, so ordinary clients are unaffected. These two tools only help a client built to use them.
