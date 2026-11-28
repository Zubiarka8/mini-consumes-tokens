# ADR-052: Path catalog of non-source files

## Status
Proposed (2026-11-27). Issue: [#629](https://github.com/Zubiarka8/mini-consumes-tokens/issues/629).

## Context
- `index_file` returns `Seen::Source` without a `files` row for any path no parser claims. Only parsed files and the ADR-003 asset formats (`glb gltf png jpg jpeg webp ktx2 hdr exr`) get a row. A PDF manual, a demo's audio, a Word or Excel file, a font or a CAD model exists only on disk.
- An agent that needs one of them walks the tree (`get_file_tree`, `Glob`) and reads long listings to reach a handful of paths.
- Every `files` row is a symbol container: status coverage, dead code, duplicates, the Obsidian atlas and `relation_candidates` read it by language. ADR-003 D1 already rejected a parallel `assets` table for relation targets, because a relation resolves to a symbol.

## Decision

### D1 Storage
| Option | Verdict |
|---|---|
| (a) A `files` row with language `other` for every unparsed path | Rejected. Every per-language query and report would have to skip it, and `files` would mean "walked" instead of "parsed". |
| (b) A new `path_catalog` table keyed by `relative_path` | **Chosen.** |

Migration 36 adds `path_catalog(relative_path PRIMARY KEY, category, format, evidence, note, size_bytes, modified_ns, rules)` and two indexes, `(category, relative_path)` and `(format, relative_path)`, for the two filters that are equality matches.
- The key is the same canonical, `/`-separated, root-relative path as `files.relative_path`. A parsed file has one row in each table under one identity, so it appears in symbol queries and in catalog queries.
- The migration adds an empty table and invalidates no content hash. The next ordinary reindex fills it, with no `--force`.

### D2 Classification
- One registry, `mct-index/src/catalog/formats.rs`: 20 categories, each canonical format with its suffix aliases. A suffix belongs to one format (unit test). Compound suffixes (`tar.gz`, `drawio.svg`) are matched before their last part; a versioned `libz.so.1.2` is a shared library.
- Matching is case-insensitive; the stored path keeps its spelling.
- A suffix with two common meanings is listed in `CONFLICTING_SUFFIXES` and resolved from the parser that claims the file and a signature read of at most 4096 bytes (`SIGNATURE_BYTES`): `.ts`/`.mts` (TypeScript or MPEG-TS/AVCHD), `.dot` (Word template or Graphviz), `.ttc` (font collection or text), `.obj` (Wavefront or COFF), `.key` (Keynote or PEM), `.db` (SQLite or unknown), `.tif`/`.tiff` (TIFF or GeoTIFF), `.fig` (Figma or XFig). No other file is opened.
- `evidence` says why: `extension`, `signature`, `parser`, `name` (`README`, `LICENSE`), `ambiguous` (signals did not settle it, `note` lists the readings) or `unknown` (no registered suffix, no parser). An unclassified file is category `other` and stays findable by path.
- A registered format wins over a parser that also claims the suffix (`.svg` is `vector` even when a markup parser indexes it). A file only a parser claims is `source`, with the language id as format. SQL is `source` when a parser claims it and a `database` dump otherwise.

### D3 Incremental updates
- A row is rewritten only when its size, its modification time (ns) or `rules` changed. `rules` is `v<CATALOG_RULES_VERSION>:<FNV-1a of the registry's extension → language map>`, so a new parser or a registry change reclassifies once.
- The catalog follows the walk: the same exclusions on the walked and on the canonical path, the same root-escape guard, symlinks indexed under their canonical target.
- A full reindex sweeps rows whose path was not seen, except at or under a path that could not be read. The run is one transaction, so an error or a killed process leaves the previous catalog.
- Watcher updates (`reindex_paths`) drop a deleted path or directory, and the old end of a rename reported by its new path only: a vanished catalog row in the new path's directory, or anywhere with the same size and modification time.

## Consequences
- An older binary stops on the newer migration count, as with every migration (`scripts/unix/reinstall.sh --reindex`).
- One row per walked file. The index grows by a few hundred bytes per file; measured in `benchmarks/path-catalog.md`.
- Out of scope: OCR, transcription, text extraction, previews, decompression and document analysis.

## Verification
- `crates/mct-index/src/catalog/{formats,classify}.rs` unit tests: alias normalization, compound suffixes, each conflict resolver, the bounded signature read.
- `crates/mct-index/tests/path_catalog.rs` over `tests/fixtures/path-catalog/`: one expected row per fixture file, add/rename/delete, an interrupted run, exclusions, symlinks, a sparse 8 GiB file, and an index at schema 35 filled by a plain reindex.
- `crates/mct-mcp-server/tests/find_files.rs` and `crates/mct-cli/tests/cli_mcp_parity.rs`: `find_files` and `mct-cli files` print the same page; invalid filters and an empty index give a clear message.
- Cost and token savings: [`benchmarks/path-catalog.md`](../../../benchmarks/path-catalog.md). Formats and how to add one: [`docs/02-crates/path-catalog-formats.md`](../../02-crates/path-catalog-formats.md).
