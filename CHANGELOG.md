# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.3.2] - 2026-11-13

This release is mostly about the dashboard and about large Obsidian vaults. Notes now render the way Obsidian renders them, the notes graph shows every note at once and stays readable, and the dashboard can close itself when you stop using it. A few fixes also make the dead-code and duplicate reports more accurate, and releases now come in more formats.

### Upgrading from 0.3.1

- The index moves from schema version 14 to 17 and reparses indexed files. Update every binary that shares an index before reopening it: older binaries cannot open the migrated database. Run `mct-cli --root <project> reindex --force` to refresh the index.
- `mct-cli web` now closes itself after 30 minutes without use. Pass `--idle-timeout <minutes>` to change that for one run (`0` means never), or set it under Settings in the dashboard. A saved setting applies when you do not pass the flag.
- `format::find_dead_code_toon` was removed from `mct-mcp-server`. Nothing in the repository used it, but it was public, so code that called it directly will stop compiling.

### Added

- Symbol queries can report declaration signatures, parameters, visibility and modifiers when the language parser provides them. Context packs also include declaration details.
- Symbol relations can record override relationships and more precise evidence about imports, receivers, scope and external calls.
- Language parsers index more declarations and relationships, including Go fields and package values, Python module values and instance fields, TypeScript parameter properties and decorators, Kotlin top-level declarations, and C++ constants.
- `mct-mcp-server --exit-idle <VALUE>` (or `MCT_EXIT_IDLE`) makes the process exit after that long without a request that uses the index. It is off by default, because an MCP client does not restart a server that has exited. Dashboard Settings, under Background use, has the same choice ("Close MCP servers after no use"); running servers pick it up within a minute.
- Dashboard: Obsidian notes render with their own syntax: callouts, wikilinks with aliases and sections, embeds, tags, footnotes, task lists, tables and highlighted code. Raw HTML in a note is shown as text, never run.
- Dashboard: the notes graph draws every note at once, even in vaults of thousands of notes. Notes are coloured by community, hubs are labelled, and display and force sliders let you tune the layout.
- Dashboard: selecting a note lights up the notes it connects to and fades the rest. A graph-only view gives it the full width, with the note opening beside it. The graph search finds a note as you type.
- Dashboard: a Settings page section for background use. You choose how long the server stays up without use (5 minutes to 3 hours, a custom value, or never) and how long the page keeps checking for a new index while untouched.
- Dashboard: your theme, language and those two times are remembered. They are saved in the browser, shared between projects, and also written to `.mct-index/web-settings.json` so they survive cleared site data and a restart. The theme defaults to your system setting and the language to English, unless your browser lists Spanish first.
- Dashboard: dead-code candidates can be filtered by the project's own folders, and the list shows the total with a link to clear the filters.
- Dashboard: findings are renumbered when the exclusion rules change, and the project logo is the favicon and header brand.
- Releases ship a native Windows on Arm archive (`windows-arm64`), and `install.ps1` picks it on an Arm machine.
- Releases ship static musl archives for Linux (`linux-x86_64-musl`, `linux-arm64-musl`) that run on Alpine and on distros with an older glibc. `install.sh` picks them on a musl system, and `MCT_LIBC=musl` forces them.
- Releases ship `.deb` and `.rpm` packages for Linux x86_64 and arm64. They install `mct-cli` and `mct-mcp-server` into `/usr/bin` on Debian and Ubuntu, and on Fedora, RHEL and openSUSE.

### Changed

- `mct-cli web` stops by itself when nobody is using it. Only your own input and normal requests count as use. The page's automatic checks do not keep it alive, and the page stops making them while the tab is hidden or untouched.
- The dashboard serves several connections at once and reloads its snapshot in the background, so a slow tab no longer delays the others.
- Large vaults load faster. The Obsidian vault is published note by note, unchanged notes are reused on a rescan, note bodies are searched from the snapshot at any vault size, and the notes graph sends its edges as positions instead of names.
- The graph layout is computed once and reused when you come back to it, and it pauses while the tab is hidden.
- Duplicate groups name their members more clearly and count files across the whole group, not only the members the list previews.

### Fixed

- Dashboard: a graph node no longer draws a label box on each of its neighbours, which buried hubs under overlapping labels.
- Dashboard: tags in a note's body are read the same way as links, so tags inside code are ignored.
- Dashboard: a connection slot is freed even when a request thread panics, and a failure to start a thread is reported.
- Dead code: a function that only calls itself is now reported as a candidate. Rust and JavaScript or TypeScript also record type uses and functions used as values, so code that is used no longer shows up as unused. On this repository the candidates dropped from 206 to 90.
- Duplicates: bindings are told apart from selected names.
- PHP: `$cb()` is recorded as a call to the closure bound to `$cb`.
- Indexing: a relation counts as a file import only when its candidates are in one file. On a 20,001-file project that query went from 5.8 s to 0.3 s with the same results.
- `mct-mcp-server` no longer walks the whole project when it starts watching files. On macOS and Windows that walk, build output included, kept a core at 99% during a build.

### Removed

- `format::find_dead_code_toon` from `mct-mcp-server` (see Upgrading).

## [0.3.1] - 2026-11-07

This release adds framework awareness for PHP and Laravel and Lua variables, makes the dashboard's duplicate and dead-code workflows easier to hand to an AI, and stops several processes from fighting over one index.

### Upgrading from 0.3.0

- The index moves to schema migration 14 and reparses your files. Update every binary that shares an index before you reopen it, because older binaries cannot open a migrated database. Then run `mct-cli --root <project> reindex --force`.
- Possible framework entry points are evidence for review, not proof that a symbol is used. Look at these hints before you remove an unused-code candidate.

### Added

- Only one background maintainer works on an index. Of all the `mct-mcp-server` sessions and `mct-cli web` dashboards on a project, the one that holds `.mct-index/index.sqlite3.maintainer.lock` watches and reconciles. The others follow its updates and take over when it exits (ADR-028).
- `mct-mcp-server --maintenance-idle <VALUE>` (or `MCT_MAINTENANCE_IDLE`) puts maintenance to sleep after that long without a request that uses the index. It is off by default. The next request waits for one reconciliation before it is answered (ADR-029).
- Dashboard: every duplicate group in a symbol's detail has its own "Copy duplicate for AI" button, and every duplicate and unused-code row has a visible "Copy AI template" button that copies the finding's brief.
- Lua indexing covers variables, class annotations and module paths. LuaRocks dependency analysis reads rockspecs without running them and shows declaration details in the status and dependency tools.
- PHP and Laravel indexing covers routes, Blade templates, migrations, listeners and views. The dead-code workspace shows possible framework entry points as evidence.
- Rust indexing includes local bindings and more attribute and type references.
- Cross-platform scripts launch the local dashboard.

### Changed

- A reindex that finds nothing to change, such as a periodic reconciliation or `mct-cli call` on an up-to-date index, no longer writes to the database. The published generation stays the same, so other processes keep their response caches and the dashboard does not reload. `get_indexing_status` and `mct-cli status` report "Last indexed at" as the last run that changed the index. When a later run found nothing to change, they also show a "Last verified unchanged at" line, kept only inside that process.
- The post-reindex hook (the Obsidian vault) runs after a run that changed the index, and once per process. The explicit `reindex` tool still refreshes it every time.
- `mct-cli web` reloads its snapshot when a change is published or the index file is rebuilt, instead of on any change to the database or WAL file metadata.
- `@import-gitignore` also imports nested `.gitignore` files, each applying below its own directory as in git. Nested files are found on every full reindex and when a `.gitignore` changes.
- `.next/` (Next.js build output) is excluded by default. Existing indexes drop its rows on the next full reindex.
- Dashboard: stacked finding rows list their files in a panel on hover and keyboard focus instead of a native tooltip, and no longer carry the ignore button, which stays in the table.
- Dashboard AI briefs separate evidence from assumptions, keep intentional fixtures and language-specific behaviour, and explain review states. Copied finding lists put each file and line range on its own line and leave out the review-key appendix.
- Duplicate findings recover complete member information and provenance. Shared parser, query and test helpers replace repeated logic while keeping language-specific assertions.

### Fixed

- `mct-cli web --help` no longer says the dashboard runs without reindexing. It keeps the index up to date while it runs.
- Dashboard: a duplicate group's file count covers the whole group (`fileCount` in `api/findings`), not the six members the list previews. The review workspace counts files, not fragments.
- Dashboard: pending-review figures come from the server (`findings.pending`, `health.pendingReviews`) with one definition (unreviewed, in review, or needs re-evaluation), instead of two different sums in the page.
- A reference no longer becomes ambiguous just because a file module shadows a declaration with the same name in that file. Imports and genuinely ambiguous targets behave as before.
- The Windows and Unix language checks accept shared syntax-error assertions as well as inline ones.

## [0.3.0] - 2026-10-30

This release turns `mct-cli web` into a full local dashboard, adds review-aware duplicate and dead-code reports, and makes TOON the default output format.

### Upgrading from 0.2.0

- **Breaking:** MCP tools that accept `format` now return TOON by default. Pass `format: text` if your client parses the previous output.
- Schema migration 11 reparses indexed files. Older binaries cannot open a migrated index, so reinstall the current binaries and reindex before using them.
- Review decisions are saved to the shared version 3 store. Older binaries do not see decisions saved there. Avoid switching versions while you review findings.
- The background reindex now waits 15 s after the last file change (it was 1.5 s). Set it with `--reindex-debounce-ms` or `MCT_REINDEX_DEBOUNCE_MS` (flag first, then variable). `0` still turns it off.
- The tools `search_symbols` and `hybrid_search` are now `search_symbols_like` and `hybrid_search_like`. The old names stay as deprecated aliases for one version and will then be removed, which will break clients that pin them. The catalog is about 1,900 tokens larger while both names are listed.
- Rust code that builds `BuildContextPackArgs`, `SearchSymbolsArgs` or `HybridSearchArgs` with a struct literal must add the new `detail` and `max_tokens` fields.
- The first `mct-cli duplicates` run after the upgrade analyzes again, because the report cache is now TOON.

### Added

**Web dashboard**

- `mct-cli web` is now a complete local dashboard. A read-only JSON API sits over a snapshot of the index, with a dependency-free front end that is responsive and keyboard accessible, with a Spanish UI and dark and light themes. It covers the main dashboard, indexed code, duplicates with a side-by-side compare, dead code, search, a dependency graph, reviews, statistics, Obsidian notes and settings. Features the index cannot back (history, semantic search, tokens, confidence) show as unavailable. The server-rendered pages and the form `POST review` are replaced by `POST api/review` (JSON). See `docs/01-architecture/web-dashboard.md`.
- Review a duplicate group in the dashboard: mark it as a false positive or not relevant, hide it, and restore it from Discarded. Decisions survive restarts and bind to member identities and source hashes, not group ranks, so changed source comes back for review. Posts are protected, writes are atomic and a writer lock keeps the previous state if a save fails. See ADR-015.
- Review decisions gain notes and an append-only history.

**Reports and MCP tools**

- `find_duplicate_code` is a new read-only MCP tool. It reads the shared review decisions (ADR-019), so a group marked `false-positive` or `dismissed` is filtered before `limit` and `offset` while its code is unchanged. `include_reviewed` shows it. Output is compact and bounded, and `review_store: degraded` appears when a review file cannot be read. See ADR-022 and `COMMANDS.md`.
- `find_dead_code` applies the shared review decisions before pagination. `include_reviewed` exposes hidden candidates, and responses report review labels, counts, store health and budgeted continuation.
- `mct-cli dead-code --reviewed show|hide|only` and `mct-cli duplicates --reviewed show|hide|only` apply the dashboard's decisions before paging. Without the flag no review file is read and both reports are unchanged.
- `build_context_pack`, `search_symbols_like` and `hybrid_search_like` (and their deprecated aliases) take optional `detail` (`overview`, `summary`, `detailed`) and `max_tokens`. The budget is an estimate over the whole response, not a model limit. Over budget, the least relevant rows lose detail first and are then dropped. A `Budget:` footer, shown only when it says something, reports `complete` or `partial`, what was lowered or dropped, how to continue (`next: offset=N`) and what the higher levels cost. Leave both arguments out and output is unchanged. See ADR-016 and `benchmarks/context-budget.md`.
- `find_references` takes an optional `sort`: `relevance` (default, unchanged) or `execution` (by hop, file and line, a static approximation of execution order).
- `mct-cli duplicates` reports Rust and Python candidate copies with exact callable boundaries. It has opt-in consistent-renaming and exact-token policies, f-string expression comparison, path and language filters, token ranking, pagination and optional excerpts. Coverage diagnostics are separate from analysis status, and analysis opens existing indexes without migrating them. Broad normalization remains the default. See [COMMANDS.md](COMMANDS.md#duplicates--duplicates---min-lines-n---min-tokens-n). The report grew in layers, none of which removes, splits or reorders groups:
  - **Aligned evidence.** Each group lists every member's original identifier and literal texts with lines, classified by syntactic role (member, callee, type, parameter, literal category), plus warnings and an identifier-pattern check. A `context` block compares decorators, outer attributes and enclosing constructs (`class`, `try`, `#[cfg(test)] mod`, and so on) and warns when they differ. `types` evidence counts conflicts between primitive spellings in complete direct annotations. Compatibility stays `unknown`, and aliases, imports and compound types are not resolved. Python `*args` and `**kwargs` parameters are reported as `parameter` differences.
  - **Trivial calls.** A `priority` block marks groups whose bodies are only flat calls, read from syntax and never from callee names. `--trivial-calls demote` (default) ranks them last with the reason and a `demoted_groups` count. `--trivial-calls keep` keeps the ranking order.
  - **Verdicts.** Each group carries an explainable `verdict` (`REAL`, `SUSPICIOUS` or `FALSE_POSITIVE`) with kebab-case reason codes, derived from that evidence by a fixed rule table (ADR-010). The header adds `verdict_scope` and `verdict_counts`. `REAL` requires identical tokens or a renaming limited to the declaration name and the members' own parameters, the same context, and bodies that are not only flat calls. Anything else is `SUSPICIOUS`. No current rule emits `FALSE_POSITIVE`. The `alignment.undeclared_name_positions` count shows renamed references to outside items.
  - **Subgroups.** A `subgroups` block partitions each group's members by the evidence that separates them, with a `split_groups` header count. A consistent renaming never splits a group, and context never separates one. See [ADR-011](docs/01-architecture/adrs/011-duplicate-subgroups.md).
  - **Partial blocks.** `--partial-blocks` (opt-in) also lists repeated token sequences shared by different functions below whole-function level. They are trimmed to whole lines with balanced delimiters, maximal only, and left out when a reported group already covers them. Deterministic budgets bound the search (`--block-max-windows`, `--block-max-comparisons`, `--block-max-blocks`, with sizes from `--block-min-tokens` and `--block-min-lines`), and the header names any exhausted budget. See [ADR-012](docs/01-architecture/adrs/012-repeated-partial-blocks.md).

**Indexing, ignore rules and Obsidian**

- `ignore-init --generated` adds an editable, idempotent preset for virtual environments, caches and `bin`, `obj`, `build` and `dist` output at any depth. Your own overrides keep priority, and ambiguous names such as `bin` stay opt-in. Navigation snapshots filter excluded stale rows and reject unreadable project rules, so file, symbol and import views match the current exclusions.
- Obsidian vault: `init`, `reindex` and the server's background refresh also write `.mct-index/obsidian/`, with a root index, directory maps and one note per source file listing imports and symbols. Source text is not copied. `mct-cli obsidian` rebuilds it by hand, and `--no-obsidian` on `init`, `reindex` and `mct-mcp-server` skips it. A failed write is a warning, not a failed reindex. Notes are capped at 100 symbols and 50 imports, and maps paginate, using stable safe names and relative links. Publishing keeps unmanaged files, checks on-disk hashes and rolls back failed renames. Symlinks and ownership conflicts fail closed. The work lives in the new `mct-obsidian` crate, with a generic post-reindex hook in `mct-index` (`Index::set_post_reindex_hook`), `ReindexReport::warnings`, `Index::indexed_files` and `Index::take_warnings`. See ADR-013.
- Markdown wikilinks resolve by frontmatter alias (issue #135, ADR-005). A bare `[[Alpha]]` or `[[Alpha#Section]]` reaches the note that declares `aliases: [Alpha]`, by exact, case-sensitive text. An alias shared by several notes, or equal to another note's stem, is `ambiguous` and lists every candidate. A `title` is never an alias, and path links ignore aliases. Schema migration 11 adds the `symbol_aliases` table and resets every `content_hash`, so the next normal reindex reparses everything. Run `scripts/unix/reinstall.sh --reindex` if an older binary fails with `CONNECTION_CLOSED` on a migrated index.
- Asset files (`glb gltf png jpg jpeg webp ktx2 hdr exr`) are indexed as `files` rows with language `asset` and one `asset` symbol each (ADR-003 P0b). Images are only stat'ed, and glTF JSON is read under an 8 MiB cap (a `.glb` BIN chunk never). Bad glTF records a `syntax_error` and keeps the asset. New symbol kinds are `asset`, `model_node`, `material`, `animation` and `finding`. `get_indexing_status` takes an optional `dependency` to list the manifests that declare a package, and `build_context_pack` prints no source for an asset. No migration or `reindex --force` is needed, because the next normal reindex adds the assets.

**Server and CLI**

- `mct-mcp-server --reindex-delay <15s|30s|45s|1m|N s/m/ms|off>` updates the index once no relevant file has changed for that long. Every change restarts the wait, bursts collapse into one update, changes during an update run afterwards, and ignore rules apply. `mct-cli mcp-register --reindex-delay <value>` writes it into `.mcp.json` and keeps it on later runs. See [COMMANDS.md](COMMANDS.md#--reindex-delay-value).
- The server's automatic update log (`auto-reindex after file changes settled`) also reports `symbols_added`, `symbols_removed` and the removed names (`removed_symbols`, up to 20, as `kind name (path)`), so a deleted function shows up and not only a deleted file. Tracking is off unless the watcher turns it on (`Index::track_symbol_changes`).
- `mct-cli call --tool <name> [--args <json> | --args-file <path>] [--json] [--timeout <seconds>]` runs one MCP tool in-process and prints the result, so scripts no longer have to spawn `mct-mcp-server` and speak JSON-RPC. `init` and `call` accept several roots (`--roots a,b` or a repeated `--root`) with a labelled result per root, and `init` gained `--force`. `MctServer::call_tool_by_name` is the new public entry point.
- `mct-cli update` (also `-u` or `--update`) installs the latest GitHub release of `mct-cli` and `mct-mcp-server` into the folder `mct-cli` runs from, through the official installer. `mct-cli --version` (`-v`) shows the installed version.
- `scripts/unix/release.sh` (`bump`, `dry-run`, `publish`) automates the scriptable steps of `RELEASING.md`.

**Website, benchmarks and tooling**

- `website/` holds the official landing page and documentation, with the project logo and a light, dark or system theme selector. It is not part of the released binaries (see `website/README.md`).
- A labelled duplicate-report corpus (`benchmarks/token-twins-labels/`, 31 human-labelled Python and Rust groups and 3 partial blocks, with negatives) and a standard-library scorer (`benchmarks/score_duplicate_labels.py`). It reports grouping recall, a verdict confusion matrix, exact subgroup partitions and partial-block precision and recall, and marks absent fields `not-implemented`. See the [token twins evaluation](benchmarks/token-twins.md#labeled-corpus-for-verdicts-subgroups-and-partial-blocks).
- `examples/retrieval_eval.rs` and `tests/retrieval_quality.rs` run labelled retrieval cases (recall, silent misses, calls and estimated tokens per strategy). `examples/token_calibration.rs` optionally compares the estimate with the model's `count_tokens` endpoint.
- `examples/adaptive_eval.rs`, `examples/catalog_report.rs` and `tests/extended_eval.rs` add a 57-case labelled set (Dev/Eval split), an offline simulation of adaptive level selection (not integrated, see `benchmarks/adaptive-detail.md`) and a per-tool catalog size report. `detail` and `max_tokens` add about 523 estimated tokens to the catalog (+3.7%), all in the schema properties of three tools.

### Changed

- **Breaking:** the ten tools with a `format` argument (`list_symbols`, `find_symbol`, `search_symbols`, `hybrid_search`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `find_dead_code`, `build_context_pack`) and their `batch` sub-queries default to TOON when `format` is omitted or null. Pass `"format":"text"` for the old labelled text. Invalid values are still rejected.
- `mct-cli web` serves a stable `http://localhost:<port>/` URL with no key. The port is remembered per project in `.mct-index/web-port` and reused while it is free (first run `8787`). A busy remembered port is replaced by a free one. `--port <port>` is exact and never remembered. The Host, Origin, Fetch Metadata and CSRF checks are unchanged, but the URL key no longer keeps out other local processes.
- `mct-cli web` pins navigation, findings, review hashes and reference counts to one index generation while loading, even during an external reindex. It also follows reindexes: when the index file is republished, the next request reloads the snapshot (duplicates, dead code and decisions), so the dashboard needs no restart. The page's CSRF token stays valid.
- Review decisions move to one shared module (`mct_index::reviews`) and a new file, `.mct-index/review-decisions.toon`, the only one written (version 3, Unix milliseconds). Decisions in `duplicate-reviews.toon` (v1 and v2) and in the dashboard draft's `reviews.toon` keep applying, and those files are no longer rewritten, so older binaries keep working on them. The reading is one-way: older binaries do not see decisions saved here, and this version does not see decisions they save later. Default command output is unchanged. See `docs/01-architecture/adrs/019-shared-review-decisions.md`.
  - A decision about code that changed shows as "needs re-evaluation" instead of hiding the finding, and a resolved finding that is still detected shows as such.
  - A save is rejected (409) when another process changed the decision meanwhile. History is append-only and checked before every save, and the previous file is kept as `review-decisions.toon.prev`.
  - An unreadable review file no longer stops `mct-cli web`. It starts read-only and hides nothing.
  - A writer killed mid-save no longer blocks later saves. The operating system releases the lock, and the next save removes the temporary files the killed save left.
- Review decisions load faster and use less memory. A store of 100,000 rows loads in about 41 ms instead of about 52 ms, and one load peaks about 52 MiB above a run with no store instead of about 94 MiB. The store is read one line at a time, and lines and fields are split with `memchr`, already in the tree through the lock file. See `benchmarks/review-decisions.md`.
- `init`, `reindex` and manual Obsidian generation share project-aware `.mctignore` reloads with MCP and the web. Unreadable direct or imported rules fail closed instead of silently dropping exclusions. Regression tests cover matching SQLite and vault paths, re-inclusions and stale-note and import cleanup.
- The new `mct-languages` crate holds the one `build_registry()` that `mct-cli`, `mct-mcp-server` and `mct-eval` index with. Adding a language now takes one dependency and one `register` line there, instead of two copies kept in sync by a parity test. `mct_mcp_server::registry::build_registry` still works as a re-export. The shipped language set is unchanged.
- The reference manual moved from the hand-written `site/index.html` to a Fumadocs site in `website/` (one MDX page per section, built-in search, `llms.txt`), deployed to GitHub Pages by `.github/workflows/docs.yml`.
- Docs: the troubleshooting page is gone, `querying` and `tools` are rewritten in plainer language, and a `development` page is new.

### Fixed

- Duplicate findings in MCP pin detection and review hashes to the same published index generation, even while another process reindexes.
- MCP source retrieval checks the indexed revision before it applies line ranges. Stale snippets and signatures are omitted, and stale skeleton requests ask for a reindex.
- MCP duplicate queries keep a continuation offset for partially returned groups and tell clients to raise the budget. Degraded-store metadata is kept when the metadata exceeds the budget.
- A finding whose stored content hash is empty is treated as unknown content. The literal-search migration clears every hash until the next reindex, and a file that is not reindexed keeps the empty value. A decision saved in that state could hide a finding whose content was unknown. Such a finding is now never hidden and shows as needing review.
- `mct-cli web` no longer serves source lines from a file that changed after it was indexed. `api/source` answers 409 ("this file changed since it was indexed; reindex and restart the dashboard") instead of marking old line numbers on new text, so the dead-code and duplicate views cannot highlight the wrong lines. Other endpoints that embed source omit it for such a file.
- `mct-cli duplicates` stores its report cache as TOON (`.mct-index/duplicates-cache.toon`, the report behind one `cache_key` field) instead of JSON, and deletes a leftover `duplicates-cache.json`. ADR-007 and `COMMANDS.md` now document the cache, `--no-cache` and `--output`.
- Rust: a path in value position (`Arc::new(m::Parser)`, `Severity::Warning`, `.any(char::is_control)`) is now a `references` relation with the same module and type evidence as a call through that path. `find_references`, `impact_analysis` and `find_dead_code` see it. Run `reindex --force` once so unchanged Rust files pick it up.

### Deprecated

- `search_symbols` and `hybrid_search` (see the upgrade notes above).

## [0.2.0] - 2026-10-09

The second release, and the first meant to ship prebuilt binaries: the `v0.1.0` tag never produced a GitHub Release. It has breaking renames (crates, binaries, index directory and Windows installer settings), so read [Upgrading from 0.1.0](#upgrading-from-010) first.

### Upgrading from 0.1.0

Go straight to the final 0.2.0 state. The intermediate `ccm-*` and `.ccm-index` names used during development were never released and need no step of their own.

1. **Install the new binaries.** They are now `mct-cli` and `mct-mcp-server` (they were `ccm-cli` and `ccm-mcp-server`), and the crates are `mct-*`. From source, run `cargo install --locked --path crates/mct-cli` and `cargo install --locked --path crates/mct-mcp-server` in an updated clone. Prebuilt archives come from the 0.2.0 GitHub Release through `install.sh` or `install.ps1`. The crates are not on crates.io (see `RELEASING.md`), so `cargo install mct-cli` from the registry does not work. If you installed the old binaries with cargo, remove them with `cargo uninstall ccm-cli ccm-mcp-server`.
2. **Point your MCP configuration at the new server.** In `.mcp.json` (or your client's equivalent) change `"command"` from `ccm-mcp-server` to `mct-mcp-server`. The server key and `--root` arguments can stay. `mct-cli --root . mcp-register` writes the correct entry and keeps other servers in the file. Edit this file, do not delete it: it is configuration, not index data.
3. **Rebuild the index.** It now lives in `.mct-index/index.sqlite3`. Version 0.1.0 used `.claude-index/`, which 0.2.0 does not read, migrate or index (`.claude-index/` and `.ccm-index/` are both on the built-in exclusion list). Run `mct-cli --root . init`, or start the new server, which indexes at startup. The index comes entirely from your source files, so nothing is lost. Once the new index works, delete `.claude-index/` (or `.ccm-index/`) to reclaim the space, and add `.mct-index/` to `.gitignore` (`mct-cli --root . gitignore-init` does it).
4. **Restart or reconnect your MCP client** (in Claude Code and Codex, `/mcp`) so it launches `mct-mcp-server` and loads the new tool list.
5. **Windows installer.** The install directory override is now `MCT_INSTALL_DIR` (it was `CCM_INSTALL_DIR`), and the default directory is `%LOCALAPPDATA%\mct\bin` (it was `%LOCALAPPDATA%\ccm\bin`). Add the new directory to your `PATH`. The Linux and macOS installer still uses `INSTALL_DIR` (default `$HOME/.local/bin`). No 0.1.0 archives were ever published, so this only affects scripts or notes that set the old variable.

Schema migrations run forward automatically when a newer binary opens an older `.mct-index`. An older binary refuses to open an index migrated by a newer one (`migration number that is too high`). In that case delete `.mct-index/` and run `init` again with the binary you plan to use. If you ran a development build from `main` against a project, run `mct-cli --root . reindex --force` once after upgrading so unchanged Markdown files gain the new literal and note data.

### Added

**New MCP tools** (18 in total, up from 8 in 0.1.0)

- `get_file_skeleton`: one file's top-level declarations with bodies collapsed to `// ...`. Brace-delimited languages get precise body elision, and others fall back to declaration lines. Nested members are not expanded.
- `get_project_overview`: a capped, ranked digest of a file, directory, crate or the whole project (modules, key symbols, top callers).
- `get_file_tree`: a depth-limited directory and file tree without symbol data, pruned of the same noise directories that reindexing skips.
- `search_symbols`: BM25-ranked full-text search over symbol names split at camelCase, snake_case, kebab-case and acronym boundaries. An exact name comes first. It takes `limit`, `offset` and `snippet_lines`.
- `hybrid_search`: `search_symbols` fused with local-embedding similarity by weighted Reciprocal Rank Fusion, with `alpha` picked from the query shape when omitted. The lexical side expands common developer-verb synonyms and resolves `Type::member` and `module.fn` qualifiers. The semantic half needs a `--features semantic` build of `mct-mcp-server`. Without it (the prebuilt release binaries included) the tool returns the lexical ranking and says so on its first line.
- Exact-phrase search: a `"double-quoted"` `hybrid_search` query matches the words consecutively and in order (lexical only, whatever `alpha`), ranks a literal name match first, and lists prose string literals holding the phrase as `path:line`. Literals come from Rust string literals and from Markdown paragraphs and pipe-table rows only. It works without the semantic feature.
- `build_context_pack`: one symbol's definition, doc comment and capped source, plus each caller, callee, dependency and related test listed once. Ambiguous callees are reported separately instead of guessed.
- `batch`: up to 25 read-only queries in one call, each under its own header. A failing sub-query is reported inline and does not fail the rest.
- `find_dead_code`: top-level symbols with zero indexed references. It is a heuristic: dynamic dispatch, reflection, exports used outside the indexed tree and framework conventions are invisible to it, so check a hit with `find_references` or `impact_analysis` before deleting anything. `mct-cli dead-code` writes the same candidates as a CSV report.
- Progressive tool discovery: `discover_tool_categories` (names and one-line purposes, grouped, no schemas) and `get_tool_schema` (one tool's full schema on demand). The standard `tools/list` handshake is unchanged.

**Better existing tools**

- `find_references`, `find_calls`, `find_callers` and `impact_analysis` take `depth` (multi-hop, default 1, clamped to 32, cycle-guarded) and `offset` (pagination). Hits beyond depth 1 are tagged `[depth N]`.
- `find_symbol` takes `match: exact|prefix|fuzzy`.
- An optional `format: toon` gives compact table output on the tabular tools.
- A query cache for `search_symbols` and `hybrid_search`: exact repeats (and, with the semantic build, near-identical queries) reuse a validated earlier ranking, and an unchanged repeated reply collapses to one line. `MCT_CACHE=off` disables it. See `docs/03-performance/query-cache.md`.
- Shorter tool descriptions in the catalog served over `tools/list`.

**Indexing**

- A background watcher in `mct-mcp-server`: debounced file-system events reindex only the changed paths, and fall back to a full rescan when a batch cannot be mapped to paths. The index stays current without manual `reindex` calls.
- `.mctignore`: gitignore-style project exclusions on top of the built-in ones, scaffolded by `mct-cli ignore-init`. `@import-gitignore` (`ignore-init --import-gitignore`) also excludes everything the project's `.gitignore` excludes. `!pattern` lines re-include what an earlier line excluded (last match wins, never inside an excluded directory, never a built-in exclusion). `mct-cli gitignore-init` adds `.mct-index/` to `.gitignore`.
- Markdown notes are first-class graph nodes, including notes with no heading. That covers nested ATX and Setext sections, heading levels, frontmatter metadata, `[[wikilinks]]` (with `#Heading` anchors), Markdown links and `![[embeds]]` resolved by path, and note-aware context and backlinks.
- Relations resolve to qualified symbol identities from parser evidence. Ambiguous or unresolved targets are reported as such and not pinned on an arbitrary homonym.
- Read diagnostics: files that cannot be read, and files above the size limit (16 MiB by default, `MCT_MAX_FILE_BYTES` to change it), are recorded as read failures. Their last indexed data is kept and marked as possibly stale. Ignore-file read or pattern errors go to stderr while the built-in exclusions stay active. (`get_indexing_status` does not list read failures yet, see issue #133.)
- Lua is registered in `mct-cli` and `mct-mcp-server` (in 0.1.0 it was a test-only parser), which brings the indexed languages to 17.

**CLI**

- New subcommands `ignore-init`, `gitignore-init`, `dead-code` and `probe` (parses files without indexing them and reports counts or the first syntax error). `-v` works as an alias for `--version`, and `--help` is fuller, with examples.

**Parsers and tests**

- A long multi-file fixture corpus for each of the 17 language parsers, with malformed-input cases and snapshot checks, plus framework fixtures for CSS and Python. It turned up many parser fixes, among them C++ scope awareness, Python decorator relations and CSS compound and combinator selectors and hex escapes. Framework coverage is still partial (issue #51).
- `mct-eval`: a quality suite for the MCP tools against a fixture set and a checked-in baseline, run in CI.

### Changed

- **Breaking:** every crate, binary and Rust module path is renamed from the `ccm` prefix to `mct` (`ccm-core` to `mct-core`, `ccm-cli` to `mct-cli`, `ccm-mcp-server` to `mct-mcp-server`, every `ccm-lang-*` to `mct-lang-*`, `ccm_core::` to `mct_core::`, `CcmServer` to `MctServer`). MCP tool names are unchanged.
- **Breaking:** the index directory is `.mct-index/` (it was `.claude-index/` in 0.1.0). There is no automatic migration, so follow the upgrade steps above.
- **Breaking:** the Windows installer reads `MCT_INSTALL_DIR` (it was `CCM_INSTALL_DIR`) and defaults to `%LOCALAPPDATA%\mct\bin`.
- License: the project's own code and documentation are Apache-2.0. Third-party components keep their own licenses, listed in `THIRD_PARTY_NOTICES.md`.
- Relation queries page in the database instead of loading every relation, for direct and multi-hop queries alike. Multi-hop results report a lower bound when the total was not computed.
- Embedding refresh (semantic build only) runs in batches without holding the index lock, which lowers peak memory.

### Fixed

- Relation pagination returned wrong or duplicated windows in some cases. `impact_analysis` now filters and deduplicates affected tests by symbol identity before it pages them.
- A renamed directory left stale rows behind.
- `get_file_skeleton` and `get_project_overview` treated file-level containers in some languages as nested symbols.
- An MCP client that probed `discover_tool_categories` first could have its later standard requests rejected.
- `tools/list` served the long descriptions instead of the compact catalog.
- A duplicate dependency in a manifest aborted the whole reindex.
- Installer docs: the custom install directory now applies to the process that runs the installer.

### Security

These close specific crash classes found by fuzzing and review. They do not claim the parsers are free of vulnerabilities (see issue #22).

- Stack exhaustion on deeply nested input: every parser's AST walk and syntax-error traversal is depth-bounded (`mct_core::MAX_TRAVERSAL_DEPTH`) and prunes below the limit instead of recursing.
- Markdown: `tree-sitter-md` 0.5.3 is replaced by a patched copy in `vendor/tree-sitter-md`. It fixes two memory-safety bugs in the block scanner that ordinary Markdown can reach: a `serialize` buffer overflow on deep nesting, and `isdigit` called with full Unicode code points. Upstream has released neither fix, which is why the crates are not published to crates.io.
- Kotlin: inputs that sent tree-sitter's error recovery into a time and memory blowup are now bounded by a parse budget, and an end-of-file scanning hang is fixed.

### Known limitations

- Type resolution is AST-only (tree-sitter). Relations are matched from syntax, not resolved types.
- Release binaries do not include the `semantic` feature.
- Parser limitations tracked upstream or in open issues: Bash `coproc` (#79), some valid C++ rejected by tree-sitter-cpp (#82), tree-sitter-css 0.25 syntax gaps including Tailwind v4 preludes and nesting (#87, #115, #116), and Markdown frontmatter aliases not resolving wikilinks (#135, fixed in 0.3.0).
- Framework-specific structure (React JSX usage, Vue SFCs and so on) is only partly indexed (#51).
- Linux release binaries are built on the `ubuntu-latest` runner and need a glibc at least as new as that image's.

## [0.1.0] - 2026-09-17

The first release. Work happened in the order below, so later languages really did arrive after earlier ones.

### Added

**Engine and index**

- Core engine: a language-agnostic `LanguageParser` trait (`ccm-core`) as the single place to add a language, and a `LanguageRegistry` that wires parsers in without touching the engine.
- Index (`ccm-index`): one SQLite schema with no per-language tables, versioned migrations, WAL and FTS5 over symbols. Reindexing is incremental through `git2::Oid::hash_object` blob hashing, so no real git repository is needed. Secret-shaped paths are excluded by default (`.env`, `*.pem`, `*.key`, `secrets/`, `credentials.json`, `.aws/`, `node_modules/` and more), and paths are validated against traversal (canonicalization plus root containment, with symlinks outside the root rejected).

**MCP server and CLI**

- MCP server (`ccm-mcp-server`, on the official `rmcp` SDK over stdio) with 7 tools, each classified as atomic, composite or index administration, with guidance that separates it from its closest sibling: `find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis` (composite: `find_callers` plus `find_references` plus a test-name heuristic), `reindex` and `get_indexing_status`.
- `list_symbols(path, kind?, language?, limit?)` lists symbol definitions under a file (exact match) or a directory or crate (prefix match). Until then every other tool needed an exact symbol name. To support it, `end_line` tracking was added to every `SymbolRecord` in all 16 language crates, so results report real `L<start>-L<end>` ranges.
- `ccm-cli` with `init`, `reindex` and `status` subcommands.
- `ccm-cli mcp-register [--name N]` writes (or merges into) `.mcp.json` at the project root with the correct `ccm-mcp-server` entry, as an alternative to `claude mcp add` or editing the JSON by hand. It keeps any other server already in the file and strips Windows' `\\?\` verbatim-path prefix from the written `--root`.
- Prebuilt-binary installers: `install.sh` (Linux and macOS, `curl -sSL .../install.sh | bash`) and `install.ps1` (Windows, `irm .../install.ps1 | iex`). Both download the latest GitHub Release archive for the detected OS and architecture and install `ccm-cli` and `ccm-mcp-server` into a user-writable directory.

**Languages**

- Rust and Python came first. The engine and MCP server were built and proven against them.
- Lua (`ccm-lang-lua`) was added on purpose to test the architecture. As a language outside the original 7-language scope, it confirmed that the `LanguageParser` trait generalizes with zero changes to `ccm-core`, `ccm-index` or `ccm-mcp-server`. It was not wired into the production registries, so it stayed a test-only proof.
- Java and C#, including method overloads (each overload is its own indexed symbol) and, for C#, get and set property pairs modeled as one `Field` symbol instead of two unrelated methods. Fuzzing with `cargo-fuzz` was added for Rust and Python at this point, with fresh setups for Java and C#.
- JavaScript and TypeScript (`ccm-lang-js-ts`): one crate with three tree-sitter grammars chosen by file extension and reported under a single combined `language_id`, because real projects mix `.js` and `.ts` freely. It handles ES module and CommonJS symbols, imports and exports in the same file. JSX and TSX are left unstructured on purpose, and only the logic inside components and handlers is indexed. It was written in this period but committed with the C++ and Go change below.
- C++ (`ccm-lang-cpp`): the central case is a class member declared in a `.h` and defined out of line in a `.cpp`. Both resolve to one logical symbol through shared name and parent, with no new relation kind and no core or index change.
- Go (`ccm-lang-go`): structs, interfaces (with their declared methods indexed), receiver methods (pointer and value receivers normalize to the same parent type), packages, imports and calls. `find_implementations` is deliberately not implemented for Go. Interface satisfaction is structural and the compiler decides it from the method set, so an AST heuristic would not be reliable. That waits for real type resolution.
- The C++ and Go change is also where the finished `ccm-lang-js-ts` crate landed, so the next CI run covered all 8 real language crates in one push.
- Kotlin (`ccm-lang-kotlin`, on `tree-sitter-kotlin-ng`): classes and interfaces (the grammar shares one node kind, told apart by an anonymous `interface` token), `object` declarations indexed as singleton classes, extension functions attached to their receiver type, and primary-constructor `val` and `var` property promotion. `Extends` and `Implements` are told apart by constructor call versus bare type in the supertype list, with no positional heuristic as C# needs.

**Quality**

- Multiplatform CI on GitHub Actions (Linux, macOS, Windows): build, test and clippy (default lint group) on all three, `cargo-audit` on Linux, and short `cargo-fuzz` smoke campaigns on Linux and macOS. Fuzzing is deliberately skipped on Windows, where `cargo fuzz run` fails with `STATUS_DLL_NOT_FOUND` whatever the sanitizer flags (documented in `.github/workflows/ci.yml`). At this point the fuzz matrix covered the 5 real language crates that existed (Rust, Python, Lua, Java, C#).
- `clippy::unwrap_used`, `clippy::expect_used` and `clippy::panic` are resolved across the workspace and enforced in CI (`-D`, alongside the default lint group). One risky `.expect()` was fixed for real (`ccm-index/src/indexer.rs`). The remaining ones in `src/` carry a narrow `#[allow]` with a `// SAFETY:` comment at the exact site (grammar setup in each language crate, two static-pattern glob builds). Every `tests/` and `examples/` file got an explicit file-level `#![allow(...)]` that says why panicking on a broken test precondition is correct there.
- Versioned integration fixtures for all 8 real languages (each a small fixture written for the project under `tests/fixtures/`, never a cloned external repository), plus a polyglot fixture (`ccm-mcp-server/tests/fixtures/polyglot-app/`: a Go backend, a TypeScript frontend and a Python deploy script). A dedicated integration test confirms the index holds and queries symbols from several languages in one project without them bleeding into each other.
- A token-usage benchmark (`ccm-cli/examples/token_benchmark.rs`, results in `benchmarks/token-benchmark.md`) compares MCP tool queries against a Read, Grep and Glob baseline. It showed an 88 to 98% character reduction across three canonical queries, run for each language as it landed (Java, C#, JavaScript and TypeScript, C++, Go).

### Known limitations

These did not block the release and were tracked for 0.2.0 and later.

- Type resolution is AST-only (tree-sitter) for all 8 languages, with no LSP integration yet.
- The secret-exclusion patterns cover Rust, Python, Java, C#, JS and TS, C++ and Go conventions. Ruby, PHP and Swift have no dedicated patterns because there is no language crate for them yet.
- `find_implementations` is not implemented for Go (see above).
- HTML, CSS and JSX are not indexed structurally. Only the script and logic embedded in them is, through the same generic recursion as any unrecognized node.
