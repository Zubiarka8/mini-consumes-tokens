# Usage guide

How to use mini-consumes-tokens day to day, once it is installed. The short version is in the [README](../README.md). Every flag and MCP tool parameter is in [COMMANDS.md](../COMMANDS.md).

## The first five minutes

```sh
cd path/to/your/project     # the project you want indexed
mct-cli --root . init       # build the index; connecting an assistant also triggers it
mct-cli --root . status     # files and symbols indexed, last run, any read failures
```

1. **Connect your agent or editor.** Steps for each tool are in [Connect your agent or editor](INSTALL.md#connect-your-agent-or-editor).
2. **Ask it things.** *"Where is `InvoiceService` defined?"* or *"What would break if I change `calculate_total`?"* Your assistant uses the index instead of reading every file. If it does not, paste the [instructions block](INSTALL.md#instructions-for-your-agent) into its rules.
3. **Refresh only if you need to.** While an assistant is connected, the index updates itself in the background once changes settle on disk (15 seconds after the last change by default). Only the files that changed are parsed again, at a few milliseconds per file. If the assistant seems to miss something after a large batch of changes, run `mct-cli --root . reindex --force`.

## Keep files out of the index

`mct-cli --root . ignore-init` writes a starter `.mctignore`. Edit it to exclude things like `docs/` or `*.md`, then run `reindex --force`. A connected server applies the edit before its next tool call, so an excluded file is never served from the old index.

- `--import-gitignore` excludes everything your own `.gitignore` files already exclude: the root one and, as in git, each nested one for the paths below its directory. It works on a new file or an existing one.
- `--generated` adds editable rules for `venv`, caches, `bin`, `obj`, `build` and `dist` at any depth. It is opt-in because `bin` can hold real source.
- Next.js output (`.next/`) is always excluded.
- Existing rules and later overrides are preserved.
- `mct-cli --root . gitignore-init` keeps the generated database itself out of git.

After changing exclusions, reindex and restart the dashboard.

## The local dashboard

```sh
mct-cli --root . web
```

Open the localhost address it prints. No browser opens by itself, and Ctrl-C stops the server. The dashboard uses the existing index, regenerates the Obsidian vault unless you pass `--no-obsidian`, and follows reindexes while it runs.

What is in it:

- A code atlas of directories, files, symbols and imports.
- Duplicate-code review with a side-by-side compare view. Every duplicate member links to its atlas file and every file links back to its groups. Unwanted groups can be moved to **Discarded** as false positives or not relevant, then restored. Decisions are saved locally, and a group whose files changed is reviewed again.
- Dead-code review, search and a dependency graph.
- A notes reader for an Obsidian vault. Notes render with callouts, wikilinks, embeds, tags, footnotes and highlighted code. Large vaults are drawn as one graph, coloured by community, with sliders for display and forces.
- Settings for theme (light, dark or system), language (English or Spanish) and background use.

The dashboard stays up while you use it. By default it closes itself after 30 minutes without use, and the page stops checking for a new index after 5 minutes without input. Change both under **Settings**, or pass `--idle-timeout <minutes>` for one run (`0` means never). Your choices are saved in the browser, shared between projects and written to `.mct-index/web-settings.json`. Details are in [the command reference](../COMMANDS.md#web--web---port-port---no-obsidian---idle-timeout-minutes) and [the dashboard design notes](01-architecture/web-dashboard.md).

## Obsidian

Every `init` and `reindex`, and the server's background refresh, also writes a compact code atlas to `.mct-index/obsidian/`. Its root `index.md` links to directory maps, which link to one metadata note per indexed file. File notes list imports and symbols with line ranges. Source text is not copied into the vault, so code stays reachable through MCP without duplicating your repository.

In Obsidian, choose "Open folder as vault" on that folder. The atlas is regenerable and ignored by git with the rest of `.mct-index/`. Pass `--no-obsidian` to `init`, `reindex` or `mct-mcp-server` to skip it, or run `mct-cli --root . obsidian` to rebuild it by hand. File notes are capped at 100 symbols and 50 imports, with the omitted counts shown, and directory maps paginate at 100 entries. Your own notes and `.obsidian/` survive regeneration, because generated notes are tracked in `.manifest.toon`. Format and recovery details are in [COMMANDS.md](../COMMANDS.md#obsidian).

The vault uses the same project-root `.mctignore` as the index, including `.gitignore` rules when you import them. There is no separate vault ignore file. After editing exclusions, run `mct-cli --root . reindex` to bring the database, notes and links in line. The manual `obsidian` command only filters the existing index, so it does not index newly included files.

## What your assistant can do

- Find everything defined in a file or folder when you do not know the exact name.
- Find where a function or class is defined.
- Find code from a description of what it does (needs the optional `semantic` build).
- Find every place something is used, optionally several hops out.
- Find what a function depends on and who depends on it.
- Answer "what would this change affect?" in one call.
- Show which files a module imports and which import it, spot import cycles and export the graph as Mermaid or DOT.
- Show a file's overall shape without reading all of it.
- Orient itself in an unfamiliar file, directory or the whole project in one call, and browse the plain file layout before picking a file.
- Spot candidate dead code (a heuristic: zero indexed references).
- Check that the index is healthy and up to date.

## Module dependency graph

`impact_analysis` answers "which symbols break if I change this?". `get_dependency_graph` answers the module-level question: which files does this file or directory import, who imports it, and is there an import cycle? It complements `impact_analysis` and does not replace it.

| Argument | Meaning |
|---|---|
| `module` (required) | Repo-relative file or directory, such as `src/orders.rs` or `frontend/` |
| `depth` | Import hops to follow from `module`. Default 2, capped at the project's maximum query depth |
| `limit` | Row cap. The output says `truncated: true` when it cut anything |
| `format` | `toon` (default) or `text` |
| `export` | `mermaid` or `dot` adds a rendered graph to the answer |

Things worth knowing:

- The unit is the file. A directory root expands to the files under it.
- An edge `A -> B` ("A imports B") exists only for a resolved import, so a guess never becomes an edge.
- Imports that resolve to nothing in the index (third party, standard library, unknown) are listed under `external`.
- Cycles are listed one per line, or `circular: none`.

The design is in [ADR-006](01-architecture/adrs/006-dependency-graph-tool.md).

An example call for a Rust or TypeScript project:

```json
{"module": "frontend/apiClient.ts", "depth": 2}
```

An illustrative TOON answer. The exact columns follow the tool's schema.

```text
root: frontend/apiClient.ts (depth 2)
nodes[2]{path,hop}:
  frontend/apiClient.ts,0
  frontend/format.ts,1
edges[1]{from,to}:
  frontend/apiClient.ts,frontend/format.ts
circular: none
external[0]:
truncated: false
```

A cycle, where `a.ts` imports `b.ts` and `b.ts` imports `a.ts`, is reported once per strongly connected component:

```text
circular[1]: src/a.ts -> src/b.ts -> src/a.ts
```

With `"export": "mermaid"` the answer also carries a diagram you can paste (`"export": "dot"` gives Graphviz DOT):

```text
graph LR
  n0["frontend/apiClient.ts"] --> n1["frontend/format.ts"]
```
