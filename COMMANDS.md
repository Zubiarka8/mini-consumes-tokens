# Command reference

The complete technical reference for `mct-cli`, `mct-mcp-server`, and the MCP tools your AI assistant calls — every flag, every parameter, and the verbatim `--help` output. This is the deep-dive version; for the short version (the handful of commands you'll actually type) and everything else about the project, see [README.md](README.md).

All `mct-cli` commands accept a global `--root <path>` naming the project to operate on (defaults to the current folder), and `--help` prints this same information in the terminal.

---

## 1. `mct-cli` — the human-facing command line

### `--root <path>` (global flag)

Names the project to operate on. Accepted by every subcommand below; defaults to the current working directory when omitted.

```sh
mct-cli --root /path/to/your/project status
```

### `--help`

Prints top-level usage and the list of subcommands.

```sh
mct-cli --help
```

<details>
<summary>Full output</summary>

```
Index a repository and inspect the index from the command line.

The MCP server (`mct-mcp-server`) does the same indexing automatically at startup — this CLI is for manual/scripted use (CI, a pre-commit hook, or just checking coverage before wiring up the plugin).

Usage: mct-cli [OPTIONS] <COMMAND>

Commands:
  init            Build the index for the first time
  reindex         Re-scan the project and update the index
  status          Report index health
  mcp-register    Write (or update) `.mcp.json` so an MCP client can launch this server
  ignore-init     Write a starter `.mctignore` for custom indexing exclusions
  gitignore-init  Keep the generated index out of git
  dead-code       Write a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) — one row per candidate with its file, function name, kind, language, and start/end line
  help            Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -v, --version
          Print version

  -h, --help
          Print help (see a summary with '-h')

Invoked here via `cargo run -p mct-cli --`; once installed on PATH (e.g.
`cargo install --path crates/mct-cli`) the binary is `mct-cli` too, so drop
the `cargo run -p mct-cli --` prefix from every example below.

Typical first run:
  cargo run -p mct-cli -- --root . init
  cargo run -p mct-cli -- --root . ignore-init --import-gitignore
  cargo run -p mct-cli -- --root . gitignore-init
  cargo run -p mct-cli -- --root . status

Run `cargo run -p mct-cli -- <command> --help` for a command's full
description and examples.
```

</details>

### `<command> --help`

Prints one subcommand's full description, flags, and examples.

```sh
mct-cli reindex --help
```

### `-v` / `--version`

Prints the installed `mct-cli` version.

```sh
mct-cli --version
```

### `init`

Builds the index for the first time. Identical to `reindex`; it exists separately to give a fresh checkout an obvious first step.

```sh
mct-cli --root . init
```

<details>
<summary>Full output</summary>

```
Build the index for the first time.

Equivalent to `reindex` — kept as a separate, discoverable first command for a fresh checkout.

Usage: mct-cli init [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- --root . init
```

</details>

### `reindex` / `reindex --force`

Re-scans the project, re-reading only files that changed since the last scan. `--force` re-reads every file regardless of whether it changed — use if you suspect something was missed.

```sh
mct-cli --root . reindex
mct-cli --root . reindex --force
```

<details>
<summary>Full output</summary>

```
Re-scan the project and update the index.

Only re-parses files that changed since the last run, unless `--force` is given. Safe to run repeatedly (e.g. from a pre-commit hook or CI step) — a no-op reindex costs one blob-hash comparison per file.

Usage: mct-cli reindex [OPTIONS]

Options:
      --force
          Re-parse every supported file, even if unchanged since last run

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . reindex
  cargo run -p mct-cli -- --root . reindex --force
```

</details>

### `status`

Reports index health: coverage per language, when it last indexed, languages seen with no support yet, files that failed to read.

```sh
mct-cli --root . status
```

<details>
<summary>Full output</summary>

```
Report index health.

Prints coverage per language, the last indexed time, any unsupported languages seen while walking the tree, and files that failed to parse. Run this after `init`/`reindex` to sanity-check the result, or on its own to check whether the index looks stale.

Usage: mct-cli status [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- --root . status
```

</details>

### `mcp-register` / `mcp-register --name <name>`

Writes (or safely merges into) `.mcp.json` at the project root, so an MCP client can launch the server for this project. Entries for other tools are left untouched. `--name` lets you choose the connection's name instead of using the project folder's name.

```sh
mct-cli --root . mcp-register
mct-cli --root . mcp-register --name my-project
```

<details>
<summary>Full output</summary>

```
Write (or update) `.mcp.json` so an MCP client can launch this server.

Writes (or updates) `.mcp.json` at the project root so Claude Code (or any other client reading that file) can launch `mct-mcp-server` for this project. Merges into an existing file instead of overwriting it, so other servers already configured there are left untouched.

Usage: mct-cli mcp-register [OPTIONS]

Options:
      --name <NAME>
          Server name (the key under `mcpServers`). Defaults to the root directory's own name

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . mcp-register
  cargo run -p mct-cli -- --root . mcp-register --name my-project
```

</details>

### `ignore-init` / `ignore-init --import-gitignore`

Writes a starter `.mctignore` at the project root (if one doesn't already exist) — a `.gitignore`-style file to exclude extra files/directories (e.g. `docs/`, `*.md`) from indexing, on top of the built-in exclusions. Edit it, then `reindex --force` to apply.

`--import-gitignore` activates `@import-gitignore`: everything the project's own `.gitignore` excludes is excluded from indexing too (its `!negation` lines are skipped, same simplification `.mctignore` itself has). On a fresh file it's baked into the generated content; on an existing one, the directive is appended if it isn't already there — the one case `ignore-init` updates a file instead of leaving it alone, and only because the flag asked for it. Idempotent either way.

```sh
mct-cli --root . ignore-init
mct-cli --root . ignore-init --import-gitignore
```

<details>
<summary>Full output</summary>

```
Write a starter `.mctignore` for custom indexing exclusions.

Writes a starter `.mctignore` at the project root, if one doesn't already exist. Lets a project exclude extra files/directories from indexing (e.g. `docs/`, `*.md`) on top of the built-in exclusions, without touching git. Without `--import-gitignore`, an existing file is left untouched.

Usage: mct-cli ignore-init [OPTIONS]

Options:
      --import-gitignore
          Activate `@import-gitignore`, so everything the project's `.gitignore` excludes is excluded from indexing too. On a fresh file this is baked into the generated content; on an existing one, the directive is appended if it isn't already there — otherwise this is the one case `ignore-init` updates a file instead of leaving it alone

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . ignore-init
  cargo run -p mct-cli -- --root . ignore-init --import-gitignore
```

</details>

### `gitignore-init`

Adds `.mct-index/` to the project's `.gitignore` (creating it if needed), so the generated symbol database never ends up committed — it's fully derived from source and regenerated by `reindex`. Additive and idempotent: leaves everything else in the file untouched, and does nothing if it's already covered.

```sh
mct-cli --root . gitignore-init
```

<details>
<summary>Full output</summary>

```
Keep the generated index out of git.

Adds `.mct-index/` to the project's `.gitignore` (creating it if needed), so the generated symbol database — fully derived from source, regenerated by `reindex` — never ends up committed. Leaves everything else already in the file untouched; a no-op if it's already covered.

Usage: mct-cli gitignore-init [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- --root . gitignore-init
```

</details>

### `dead-code` / `dead-code --path <path> --language <lang> --output <file>`

Writes a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) to `.mct-index/dead-code-report.csv` — one row per candidate with its file, function name, kind, language, and start/end line. Optionally narrowed to a file/directory/crate and/or language, and written to a custom path instead of the default.

```sh
mct-cli --root . dead-code
mct-cli --root . dead-code --path src/services --language python --output dead-code.csv
```

<details>
<summary>Full output</summary>

```
Write a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) — one row per candidate with its file, function name, kind, language, and start/end line

Usage: mct-cli dead-code [OPTIONS]

Options:
      --path <PATH>          File, directory, or crate prefix to scan — same matching semantics as `list_symbols`. Omit for the whole project
      --root <ROOT>          Project root to operate on. Defaults to the current working directory
      --language <LANGUAGE>  Exact language id to keep (e.g. `rust`, `python`, `go`)
      --output <OUTPUT>      Where to write the CSV report. Defaults to `<root>/.mct-index/dead-code-report.csv`
  -h, --help                 Print help
```

</details>

---

## 2. `mct-mcp-server` — the program your AI assistant actually talks to

You don't normally run this by hand; your assistant or editor starts it once connected (see [README.md § Connecting it to your AI assistant or editor](README.md#4-installation-and-setup)).

### `mct-mcp-server --root <path>`

Starts the server for that project and waits for an MCP client over stdio. Builds/refreshes the index automatically on startup, and keeps refreshing it silently in the background as files change.

```sh
mct-mcp-server --root /path/to/your/project
```

### `--reindex-debounce-ms <ms>`

Milliseconds of quiet time required after the last detected file write before the background watcher auto-reindexes. Defaults to `1500`. Passing `0` disables the background watcher entirely (index only refreshes on startup or a manual `reindex`).

```sh
mct-mcp-server --root /path/to/your/project --reindex-debounce-ms 3000
```

### `--help`

```sh
mct-mcp-server --help
```

<details>
<summary>Full output</summary>

```
mini-consumes-tokens: MCP server exposing an AST-derived symbol graph of this repository (find_symbol, find_references, find_calls, find_callers)

Usage: mct-mcp-server [OPTIONS]

Options:
      --root <ROOT>
          Project root to index. Defaults to the current working directory
      --reindex-debounce-ms <REINDEX_DEBOUNCE_MS>
          Milliseconds of quiet time required after the last detected file write before auto-reindexing. 0 disables the background watcher entirely, restoring the previous startup-only behavior [default: 1500]
  -h, --help
          Print help
```

</details>

---

## 3. MCP tools — what your AI assistant calls for you

These aren't run by hand; they're the tools an MCP client (Claude Code, Cursor, etc.) calls once connected. Grouped the same way `discover_tool_categories` (§ 3.5) groups them. Response-returning tools share three optional parameters, documented once here instead of per tool: `limit` (max results, default 50), `offset` (skip this many results before applying `limit`, for paging), and `format` (`"text"`, the default human/agent-readable layout, or `"toon"`, a compact token-lean table — see the note at the end of this section).

### 3.1 Discovery — "what exists here, and what does it look like?"

**`list_symbols`** — the discovery step. Lists symbol definitions (name, kind, line range) found under a path: a single file matches exactly, a directory/crate path (no extension) matches as a prefix. Optionally narrowed to one symbol `kind` and/or one `language`. Use this first when you don't know a symbol's exact name yet — it's what feeds `find_symbol`/`find_references`/etc., not a replacement for them.

**`get_file_skeleton`** — a single file's top-level declarations with bodies collapsed to `// ...`, up to ~90% fewer tokens than reading the whole file. Brace-delimited languages get precise body elision; others get a best-effort declaration-line rendering. Takes one file path, not a directory — use `list_symbols` first if you don't know which file you need.

**`get_project_overview`** — the cheapest way to get oriented in an unfamiliar file, directory, or the whole project: a capped, ranked hierarchical digest (modules, key symbols, optionally each symbol's top callers) in one call, instead of chaining `list_symbols` + `get_file_skeleton` + `find_calls` by hand. Coarser than the two tools above, so switch to them once you know which file you care about. Optional `include_relations` (default false) adds top-caller lines at roughly 4x the response cost.

**`get_file_tree`** — a plain directory/file tree, no symbol data, depth-limited (`depth`, default 3) and pruned of the same noise directories (`target`, `node_modules`, `.git`) reindexing skips. Cheaper than `get_project_overview` when you only need the filesystem shape, before you know which file or crate to look at.

### 3.2 Lookup — "where is this, exactly?"

**`find_symbol`** — finds the definition location(s) of a symbol by name, across every indexed language, including polyglot projects. `match` controls how `name` is matched: `exact` (default), `prefix`, or `fuzzy` (substring, case-insensitive — use this instead of guessing the exact name and re-querying). Does not find callers/references — use `find_callers`/`find_references` for that.

### 3.3 Relations — "who touches this, and what does it touch?"

**`find_callers`** — every direct caller of a function/method.

**`find_calls`** — every function/method a given function/method calls.

**`find_references`** — every reference to a symbol: calls, imports, extends/implements — broader than `find_callers`/`find_calls`.

**`impact_analysis`** — the full blast radius of changing or removing a symbol in one call: combines `find_callers` + `find_references` + a heuristic check for affected tests, instead of three separate calls.

All four accept `depth` (how many relation-graph hops to walk beyond the direct hit; default 1 is today's direct-hit behavior, raising it also includes hits-of-hits, each tagged with its hop number, clamped to a hard maximum) — reach for `depth` before manually chaining calls to walk the graph further out.

### 3.4 Maintenance — "is the index okay, and what's unused?"

**`reindex`** — re-scans the project on demand. `force` (default false) re-parses every file regardless of whether its content changed; omit it for the normal incremental behavior. Rarely needed by hand since the index also refreshes automatically at startup and in the background.

**`get_indexing_status`** — reports index health: coverage per language, last indexed time, unsupported languages seen, files that failed to parse. `verbose_dependencies` (default false) lists every manifest's dependencies individually instead of a one-line summary.

**`find_dead_code`** — candidate unused functions/classes/structs/enums/traits/interfaces/type-aliases: a heuristic based on zero indexed references, not true visibility analysis. A starting point for cleanup, not a certainty — sanity-check a hit with `find_references` or `impact_analysis` before deleting anything.

### 3.5 Meta — progressive tool discovery

**`discover_tool_categories`** — lists every registered tool's name and one-line purpose, grouped by category, with no input schemas. A cheap first call for an MCP client that wants to defer loading full schemas. The standard MCP `tools/list` handshake still returns every tool's full schema up front as usual — these two meta tools only help a client built to use them instead.

**`get_tool_schema`** — returns one named tool's full input schema and description on demand, once `discover_tool_categories` (or this document) has told you which tool you need.

### A note on `format: "toon"`

`list_symbols`, `find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, and `find_dead_code` all accept an optional `format` argument — `"text"` (the default, unchanged) or `"toon"`. `toon` renders the result's uniform rows (symbols, references, calls) as a compact TOON table (one header row of column names, then one row per hit, no repeated labels) instead of this server's usual labelled lines — fewer tokens on a large result, at the cost of `list_symbols`' per-kind grouping. It's opt-in and additive: nothing changes for an existing client that never passes `format`.

---

For installation, connecting to your assistant, uninstalling, and troubleshooting, see [README.md](README.md).
