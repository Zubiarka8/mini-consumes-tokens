# Command reference

Captured from the issue #560 worktree build. Installed versions may differ; check the relevant executable help before suggesting flags.

## CLI

```text
Index a repository and inspect the index from the command line.

The MCP server (`mct-mcp-server`) does the same indexing automatically at startup — this CLI is for manual/scripted use (CI, a pre-commit hook, or just checking coverage before wiring up the plugin).

Usage: mct-cli [OPTIONS] <COMMAND>

Commands:
  init                  Build the index for the first time
  reindex               Re-scan the project and update the index
  obsidian              Regenerate the Obsidian vault from the existing index
  web                   Serve the local project dashboard (code, duplicates, dead code, reviews, dependencies, notes)
  call                  Call one MCP tool once and print its result
  status                Report index health
  mcp-register          Write (or update) the MCP config of every AI agent you use
  init-mctignore           Write a starter `.mctignore` for custom indexing exclusions
  git-exclusions-init        Keep the generated index out of git
  dead-code             Write a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) — one row per candidate with its file, function name, kind, language, and start/end line
  duplicates            Report candidate duplicate functions and methods, as TOON
  probe                 Parse files without indexing them and report, per file, either its symbol/relation counts or the first syntax error with its line
  skills-register       Install the `mct-*` skills for the AI agents you use
  update, -u, --update  Update mct-cli and mct-mcp-server to the latest GitHub release
  help                  Print this message or the help of the given subcommand(s)

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

  -v, --version
          Print version

  -h, --help
          Print help (see a summary with '-h')

Invoked here via `cargo run -p mct-cli --`; once installed on PATH (e.g.
`cargo install --path crates/mct-cli`) the binary is `mct-cli` too, so drop
the `cargo run -p mct-cli --` prefix from every example below.

Typical first run:
  cargo run -p mct-cli -- --root . init
  cargo run -p mct-cli -- --root . init-mctignore --import-gitignore
  cargo run -p mct-cli -- --root . git-exclusions-init
  cargo run -p mct-cli -- --root . status

Query without an MCP client, on one or several projects:
  cargo run -p mct-cli -- --root . call --tool get_indexing_status
  cargo run -p mct-cli -- --roots ../a,../b call --tool list_symbols --args '{"path":"src"}'

Run `cargo run -p mct-cli -- <command> --help` for a command's full
description and examples.
```

## mct-cli init

```text
Build the index for the first time.

Equivalent to `reindex` — kept as a separate, discoverable first command for a fresh checkout. With several roots (`--root a --root b` or `--roots a,b`) each one is indexed independently and gets its own summary. A root whose index is already up to date is not re-parsed unless `--force` is given.

Usage: mct-cli init [OPTIONS]

Options:
      --force
          Re-parse every supported file, even if unchanged since last run

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --no-obsidian
          Do not write the Obsidian vault in `.mct-index/obsidian/`

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . init
  cargo run -p mct-cli -- --roots ../a,../b init
  cargo run -p mct-cli -- --root . init --force
```

## mct-cli reindex

```text
Re-scan the project and update the index.

Only re-parses files that changed since the last run, unless `--force` is given. Safe to run repeatedly (e.g. from a pre-commit hook or CI step) — a no-op reindex costs one blob-hash comparison per file.

Usage: mct-cli reindex [OPTIONS]

Options:
      --force
          Re-parse every supported file, even if unchanged since last run

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --no-obsidian
          Do not write the Obsidian vault in `.mct-index/obsidian/`

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . reindex
  cargo run -p mct-cli -- --root . reindex --force
```

## mct-cli obsidian

```text
Regenerate the Obsidian vault from the existing index.

Writes `.mct-index/obsidian/`: a directory map, one source-file note per indexed file and an entry index. Source content is not copied. Open that folder in Obsidian with "Open folder as vault". `init` and `reindex` already do this (unless given `--no-obsidian`); use this command to rebuild it by hand. It does not reindex, so run `init` or `reindex` first.

Usage: mct-cli obsidian [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- --root . obsidian
```

## mct-cli web

```text
Serve the local project dashboard (code, duplicates, dead code, reviews, dependencies, notes).

Requires an existing index (`init` first). While it runs, it keeps that index up to date like the MCP server does: a reindex at startup, then saved files are reindexed automatically and the whole project is reconciled periodically. Regenerates the Obsidian vault unless disabled. Prints a stable `http://localhost:<port>/` URL with no key in it; keep this command running and press Ctrl-C to stop. It also stops by itself after `--idle-timeout` minutes without use (keys, clicks or scrolling in the dashboard, or any request other than its automatic checks); the dashboard's Settings page changes that time while it runs. A published change, from this process or any other, is picked up on the next request, no restart needed. No browser is opened automatically.

The port is remembered per project in `.mct-index/web-port` and reused while it is free (first run: 8787). If it is busy, a free port is chosen and remembered instead. Always binds 127.0.0.1 only; requests with a foreign Host or Origin are refused.

Usage: mct-cli web [OPTIONS]

Options:
      --port <PORT>
          Use exactly this port (0 = any free port). Fails if it is busy and is not remembered. Omit it to reuse the project's remembered port

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --no-obsidian
          Do not regenerate the Obsidian vault; the dashboard still works

      --idle-timeout <MINUTES>
          Stop after this many minutes without use (0 = never). Without it, the time saved from the dashboard's Settings (`.mct-index/web-settings.json`) applies, then 30

  -h, --help
          Print help (see a summary with '-h')

Examples:
  mct-cli web
  mct-cli --root ../app web --no-obsidian
  mct-cli web --port 9000   # explicit: fails if busy, never remembered
```

## mct-cli call

```text
Call one MCP tool once and print its result.

Runs the same tool code `mct-mcp-server` serves, in this process: no server to start, no JSON-RPC to write. Each root is brought up to date first (an incremental reindex, like the server's startup), then the tool is called with `--args`. With several roots the call runs on each and every result is labelled `== <root> ==`; a failure on one root is reported and the rest still run, then the exit status is non-zero. Tool names: `list_symbols`, `get_file_skeleton`, `get_project_overview`, `get_file_tree`, `find_symbol`, `search_symbols`, `hybrid_search`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `build_context_pack`, `find_dead_code`, `batch`, `get_indexing_status`, `reindex`. Add `"format":"toon"` to `--args` where a tool supports it for the most compact output.

Usage: mct-cli call [OPTIONS] --tool <TOOL>

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --tool <TOOL>
          Tool to call (see the list above)

      --args <ARGS>
          The tool's arguments as one JSON object; `-` reads it from stdin. Defaults to `{}`

      --args-file <ARGS_FILE>
          Read the arguments JSON object from this file; `-` is stdin

      --json
          Print the raw MCP result as JSON instead of its text. With several roots, an array of `{root, result}` / `{root, error}` objects

      --timeout <SECONDS>
          Give up on a root after this many seconds (indexing included)

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . call --tool get_indexing_status
  cargo run -p mct-cli -- --root . call --tool list_symbols --args '{"path":"src/main.rs"}'
  cargo run -p mct-cli -- --root . call --tool find_symbol --args-file query.json --json
  echo '{"symbol":"main"}' | cargo run -p mct-cli -- --roots ../a,../b call --tool build_context_pack --args -
```

## mct-cli status

```text
Report index health.

Prints coverage per language, the last indexed time, any unsupported languages seen while walking the tree, and files that failed to parse. Run this after `init`/`reindex` to sanity-check the result, or on its own to check whether the index looks stale.

Usage: mct-cli status [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- --root . status
```

## mct-cli mcp-register

```text
Write (or update) the MCP config of every AI agent you use.

Writes (or updates) `.mcp.json` at the project root so Claude Code (or any other client reading that file) can launch `mct-mcp-server` for this project. The agents detected in the project or your home directory get their own config too: `.cursor/mcp.json` (Cursor), `.vscode/mcp.json` (GitHub Copilot in VS Code), `opencode.json` (OpenCode), and with `--global` `~/.codex/config.toml` (Codex). Every file is merged into, never overwritten, so other servers already configured there are left untouched.

Usage: mct-cli mcp-register [OPTIONS]

Options:
      --name <NAME>
          Server name (the key under `mcpServers`). Defaults to the root directory's own name

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --reindex-delay <VALUE>
          Quiet period without file changes before the server refreshes the index automatically: `15s`, `30s`, `45s`, `1m`, a custom positive `<N>s` / `<N>m` / `<N>ms`, or `off` to disable automatic updates. Written to the server entry's args as `--reindex-delay` and kept by later `mcp-register` runs that omit the flag

      --agent <AGENT>
          Register for these agents (claude, codex, cursor, copilot, opencode), comma-separated, instead of the ones detected

      --global
          Also write the user-level configs: Codex's `~/.codex/config.toml`

      --dry-run
          Print what was detected and which files would change, and write nothing

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . mcp-register
  cargo run -p mct-cli -- --root . mcp-register --name my-project
  cargo run -p mct-cli -- --root . mcp-register --reindex-delay 30s
  cargo run -p mct-cli -- --root . mcp-register --reindex-delay off
```

## mct-cli init-mctignore

```text
Write a starter `.mctignore` for custom indexing exclusions.

Writes a starter `.mctignore` at the project root, if one doesn't already exist. Lets a project exclude extra files/directories from indexing (e.g. `docs/`, `*.md`) on top of the built-in exclusions, without touching git. An existing file is left untouched unless `--import-gitignore` or `--generated` is requested.

Usage: mct-cli init-mctignore [OPTIONS]

Options:
      --import-gitignore
          Activate `@import-gitignore`, so everything the project's `.gitignore` excludes is excluded from indexing too. On a fresh file this is baked into the generated content; on an existing one, the directive is appended if it isn't already there — existing patterns are preserved. `--generated` can also update this file with an optional preset

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --generated
          Add editable exclusions for venv, caches, bin, obj, build and dist. Applies at any depth; bin may contain real source, so review the rules

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . init-mctignore
  cargo run -p mct-cli -- --root . init-mctignore --import-gitignore
```

## mct-cli git-exclusions-init

```text
Keep the generated index out of git.

Adds `.mct-index/` to the project's `.gitignore` (creating it if needed), so the generated symbol database — fully derived from source, regenerated by `reindex` — never ends up committed. Leaves everything else already in the file untouched; a no-op if it's already covered.

Usage: mct-cli git-exclusions-init [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- --root . git-exclusions-init
```

## mct-cli dead-code

```text
Write a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) — one row per candidate with its file, function name, kind, language, and start/end line

Usage: mct-cli dead-code [OPTIONS]

Options:
      --path <PATH>
          File, directory, or crate prefix to scan — same matching semantics as `list_symbols`. Omit for the whole project

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --language <LANGUAGE>
          Exact language id to keep (e.g. `rust`, `python`, `go`)

      --output <OUTPUT>
          Where to write the CSV report. Defaults to `<root>/.mct-index/dead-code-report.csv`

      --reviewed <REVIEWED>
          Apply review decisions from the web dashboard and add a `review_state` column. Omit for the unreviewed report

          Possible values:
          - show: Every finding, with its review state
          - hide: Only findings that still need a person: false positives and dismissed findings whose code is unchanged are left out
          - only: Only the findings `hide` leaves out

  -h, --help
          Print help (see a summary with '-h')
```

## mct-cli duplicates

```text
Report candidate duplicate functions and methods, as TOON.

Groups functions and methods of the same language whose token sequences are identical once identifiers and literals are normalized (Type-2 clones). Every group is a CANDIDATE for human review: matching structure does not prove the code is redundant or behaves the same. Runs on demand against the existing index and the current files; it never reindexes or migrates the index, so run `init` or `reindex` first. Reuses an exact-input report cache in `.mct-index`; use `--no-cache` to bypass it or `--output FILE` to save the TOON report. Candidates that cannot be analyzed are listed as skipped and mark the report partial. Only languages with a token source are analyzed; the report names the rest.

Usage: mct-cli duplicates [OPTIONS]

Options:
      --output <FILE>
          Write the TOON report to this path (relative paths use the project root)

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --no-cache
          Skip reading or writing the duplicate-report cache

      --min-lines <MIN_LINES>
          Minimum source-token-bearing lines per candidate

          [default: 6]

      --min-tokens <MIN_TOKENS>
          Minimum source tokens per candidate

          [default: 40]

      --normalization <NORMALIZATION>
          Comparison policy: broad, consistent identifier spelling, or exact token text

          [default: broad]

      --ranking <RANKING>
          Rank by lines times members, or tokens times additional copies (estimate)

          [default: lines]

      --path <PATH>
          Exact file or directory prefix, relative to the repository root

      --language <LANGUAGE>
          Exact indexed language id

      --limit <LIMIT>
          Maximum groups to display; totals describe all filtered groups

      --offset <OFFSET>
          Skip this many ranked groups

          [default: 0]

      --no-excerpts
          Omit source excerpts

      --trivial-calls <TRIVIAL_CALLS>
          Groups whose bodies are only flat calls: rank them last (demote) or keep the ranking order (keep). Never removes a group

          [default: demote]

      --partial-blocks
          Also report repeated token sequences shared by different functions below the whole-function level (opt-in; bounded by the --block-* budgets). Extraction feasibility is never evaluated

      --block-min-tokens <BLOCK_MIN_TOKENS>
          Minimum source tokens per partial block

          [default: 40]

      --block-min-lines <BLOCK_MIN_LINES>
          Minimum token-bearing lines per partial block, in every occurrence

          [default: 6]

      --block-max-windows <BLOCK_MAX_WINDOWS>
          Most token windows hashed (memory budget, 16 bytes each)

          [default: 2000000]

      --block-max-comparisons <BLOCK_MAX_COMPARISONS>
          Most token comparisons spent extending blocks (deterministic time budget)

          [default: 20000000]

      --block-max-blocks <BLOCK_MAX_BLOCKS>
          Most partial blocks reported

          [default: 200]

      --reviewed <REVIEWED>
          Apply review decisions from the web dashboard before paging, and list each shown group's review state. Bypasses the report cache. Omit for the unreviewed report

          Possible values:
          - show: Every finding, with its review state
          - hide: Only findings that still need a person: false positives and dismissed findings whose code is unchanged are left out
          - only: Only the findings `hide` leaves out

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . duplicates
  cargo run -p mct-cli -- --root . duplicates --min-lines 10 --min-tokens 80
  cargo run -p mct-cli -- --root . duplicates --output duplicates.toon
```

## mct-cli probe

```text
Parse files without indexing them and report, per file, either its symbol/relation counts or the first syntax error with its line.

For narrowing down which construct a grammar rejects: no index is opened or written, and the files need not be under `--root`. Exits non-zero if any file fails to parse.

Usage: mct-cli probe [OPTIONS] <FILES>...

Arguments:
  <FILES>...
          Files to parse; the extension picks the language

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- probe snippet.cpp other.cpp
```

## mct-cli skills-register

```text
Install the `mct-*` skills for the AI agents you use.

Detects Claude Code, Codex, Cursor, GitHub Copilot and OpenCode, copies the skills to `.agents/skills/` (and `.claude/skills/` for Claude Code) and adds a marked pointer block to `AGENTS.md` / `CLAUDE.md`. A skill you edited is never overwritten without `--force`.

Usage: mct-cli skills-register [OPTIONS]

Options:
      --agent <AGENT>
          Skip detection and write for these agents (claude, codex, cursor, copilot, opencode), comma-separated

      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --global
          Write to the home directory (`~/.agents/skills`, `~/.claude/skills`) instead of the project

      --dry-run
          Print what was detected and what would be written, and write nothing

      --force
          Rewrite skills you edited

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- --root . skills-register --dry-run
```

## mct-cli update

```text
Update mct-cli and mct-mcp-server to the latest GitHub release.

Runs the official installer (`install.sh`, or `install.ps1` on Windows) with this binary's own folder as the install directory, so both programs are replaced where they already are. Needs network access, plus `curl` and `bash` on macOS/Linux. Release binaries are built without the `semantic` feature: a source install with `--features semantic` is replaced by one without it. Close the MCP client first on Windows, which cannot overwrite a running `mct-mcp-server.exe`.

Usage: mct-cli {update|--update|-u} [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

  -h, --help
          Print help (see a summary with '-h')

Examples:
  mct-cli update
  mct-cli -u
  mct-cli --update
```

## MCP server

```text
mini-consumes-tokens: MCP server exposing an AST-derived symbol graph of this repository (find_symbol, find_references, find_calls, find_callers)

Usage: mct-mcp-server [OPTIONS]

Options:
      --root <ROOT>
          Project root to index. Defaults to the current working directory
      --reindex-debounce-ms <REINDEX_DEBOUNCE_MS>
          Milliseconds of quiet time required after the last detected file write before auto-reindexing (default 15000). 0 disables the background watcher entirely, restoring the previous startup-only behavior. Precedence: this flag, then the MCT_REINDEX_DEBOUNCE_MS environment variable, then the default. Conflicts with --reindex-delay
      --reconcile-interval-seconds <RECONCILE_INTERVAL_SECONDS>
          Seconds between full reconciliation scans while the filesystem watcher is active (default 300; minimum 60). Invalid environment values use the default
      --reindex-delay <VALUE>
          Quiet time without file changes before the index updates itself. Presets: 15s, 30s, 45s, 1m. Custom: <N>s, <N>m or <N>ms (N > 0), e.g. 90s, 2m, 500ms. 'off' disables automatic updates. Conflicts with --reindex-debounce-ms
      --rebuild-embeddings
          Reindex, drop every stored embedding, re-embed every symbol with the build's model (`--features semantic`), then exit without serving
      --no-obsidian
          Do not regenerate the Obsidian vault in `.mct-index/obsidian/` after each reindex
      --maintenance-idle <VALUE>
          Put background maintenance to sleep after this long without an index-using request: the watcher and periodic reconciliation stop and the maintainer lease is released. The next such request waits for one reconciliation before it is answered. Written like --reindex-delay (e.g. 10m, 90s) or 'off' (the default). Precedence: this flag, then the MCT_MAINTENANCE_IDLE environment variable
      --exit-idle <VALUE>
          Exit the whole process after this long without an index-using request. The MCP client does not restart a server that has exited, so the next tool call fails until the client reconnects. Written like --reindex-delay (e.g. 30m) or 'off' (the default). Precedence: this flag, then the MCT_EXIT_IDLE environment variable
  -h, --help
          Print help

Examples:
  --reindex-delay 30s     update the index after 30s without file changes
  --reindex-delay 2m      custom delay (<N>s, <N>m or <N>ms)
  --reindex-delay off     disable automatic updates
```
