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
  init                  Build the index for the first time
  reindex               Re-scan the project and update the index
  status                Report index health
  mcp-register          Write (or update) `.mcp.json` so an MCP client can launch this server
  ignore-init           Write a starter `.mctignore` for custom indexing exclusions
  gitignore-init        Keep the generated index out of git
  obsidian              Regenerate the Obsidian vault from the existing index
  web                   Serve the local project dashboard (atlas, duplicates, dead code, search, dependencies)
  dead-code             Write a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) — one row per candidate with its file, function name, kind, language, and start/end line
  duplicates            Report candidate duplicate functions and methods, as TOON
  probe                 Parse files without indexing them and report, per file, either its symbol/relation counts or the first syntax error with its line
  update, -u, --update  Update mct-cli and mct-mcp-server to the latest GitHub release
  help                  Print this message or the help of the given subcommand(s)

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
mct-cli --root . init --no-obsidian
```

Also writes the [Obsidian vault](#obsidian) in `.mct-index/obsidian/`; `--no-obsidian` skips it.

<details>
<summary>Full output</summary>

```
Build the index for the first time.

Equivalent to `reindex` — kept as a separate, discoverable first command for a fresh checkout.

Usage: mct-cli init [OPTIONS]

Options:
      --force
          Re-parse every supported file, even if unchanged since last run

      --no-obsidian
          Do not write the Obsidian vault in `.mct-index/obsidian/`

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
mct-cli --root . reindex --no-obsidian
```

After a successful reindex the [Obsidian vault](#obsidian) in `.mct-index/obsidian/` is brought up to date; `--no-obsidian` skips that. It prints the vault location. If the vault cannot be written, the reindex still succeeds and prints a `warning:` line to stderr.

<details>
<summary>Full output</summary>

```
Re-scan the project and update the index.

Only re-parses files that changed since the last run, unless `--force` is given. Safe to run repeatedly (e.g. from a pre-commit hook or CI step) — a no-op reindex costs one blob-hash comparison per file.

Usage: mct-cli reindex [OPTIONS]

Options:
      --force
          Re-parse every supported file, even if unchanged since last run

      --no-obsidian
          Do not write the Obsidian vault in `.mct-index/obsidian/`

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
mct-cli --root . mcp-register --reindex-delay 30s
```

`--reindex-delay <VALUE>` writes `"--reindex-delay", "<value>"` into the server entry's `args`, so the server waits that long without file changes before updating the index (see [`--reindex-delay`](#--reindex-delay-value)). `VALUE` is `15s`, `30s`, `45s`, `1m`, a custom positive `<N>s` / `<N>m` / `<N>ms` (for example `90s`), or `off` to disable automatic updates. Re-running `mcp-register` without the flag keeps the value already in `.mcp.json`; re-running it with the flag replaces it. `off` is kept the same way.

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
          Project root to operate on. Defaults to the current working directory.

          `init` and `call` accept several roots, as a repeated `--root` or a comma-separated `--roots a,b`; every other command takes exactly one.

          [alias: --roots]

      --reindex-delay <VALUE>
          Quiet period without file changes before the server refreshes the index automatically: `15s`, `30s`, `45s`, `1m`, a custom positive `<N>s` / `<N>m` / `<N>ms`, or `off` to disable automatic updates. Written to the server entry's args as `--reindex-delay` and kept by later `mcp-register` runs that omit the flag

  -h, --help
          Print help (see a summary with '-h')

Examples:
  cargo run -p mct-cli -- --root . mcp-register
  cargo run -p mct-cli -- --root . mcp-register --name my-project
  cargo run -p mct-cli -- --root . mcp-register --reindex-delay 30s
  cargo run -p mct-cli -- --root . mcp-register --reindex-delay off
```

</details>

### `ignore-init` / `ignore-init --import-gitignore`

Writes a starter `.mctignore` at the project root (if one doesn't already exist) — a `.gitignore`-style file to exclude extra files/directories (e.g. `docs/`, `*.md`) from indexing, on top of the built-in exclusions. A `!pattern` line re-includes what an earlier line excluded (e.g. `*.md` then `!docs/readme.md`; last match wins, nothing inside an excluded directory, never a built-in exclusion). Edit it, then `reindex --force` to apply. A running `mct-mcp-server` applies an edit to `.mctignore` (or to an imported `.gitignore`) before its next tool call, even with the watcher disabled: no tool serves a newly excluded path from rows indexed before the edit. If an existing rule file cannot be read (including invalid UTF-8), the server retains the previous rules and rejects the tool call until the file is readable again; a missing rule file still removes its opt-in rules.

`--import-gitignore` activates `@import-gitignore`: everything the project's own `.gitignore` excludes is excluded from indexing too (read as if its lines, `!` re-includes included, stood where the directive does). On a fresh file it's baked into the generated content; on an existing one, the directive is appended if it isn't already there — one explicitly requested update to an existing file, and only because the flag asked for it. Idempotent either way. `--generated` is another explicit opt-in update, described below.

```sh
mct-cli --root . ignore-init
mct-cli --root . ignore-init --import-gitignore
```

<details>
<summary>Full output</summary>

```
Write a starter `.mctignore` for custom indexing exclusions.

Writes a starter `.mctignore` at the project root, if one doesn't already exist. Lets a project exclude extra files/directories from indexing (e.g. `docs/`, `*.md`) on top of the built-in exclusions, without touching git. An existing file is left untouched unless `--import-gitignore` or `--generated` is requested.

Usage: mct-cli ignore-init [OPTIONS]

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
  cargo run -p mct-cli -- --root . ignore-init
  cargo run -p mct-cli -- --root . ignore-init --import-gitignore
```

</details>

#### Generated-directory preset

```sh
mct-cli --root . ignore-init --generated --import-gitignore
mct-cli --root . reindex
mct-cli --root . web
```

`--generated` adds editable `.mctignore` rules for `venv/`, `.venv/`,
`__pycache__/`, `.pytest_cache/`, `.mypy_cache/`, `.ruff_cache/`, `.tox/`,
`.nox/`, `bin/`, `obj/`, `build/` and `dist/` at any depth. It works on a new
or existing file, preserves existing text and later user overrides, and is
idempotent. It does not change `.gitignore`, create an index or delete files.

This is opt-in: directories such as `bin/`, `build/` and `dist/` can contain
real project source. Remove unwanted rules or add a later `!src/bin/` (including
its directory) to retain a known source tree. Bare `bin/` applies at any depth;
`project/bin/` scopes it to that project, relative to the root. Built-in security
exclusions still cannot be overridden. Symlinked ignore files (and hard-linked files on Unix) are
refused by this preset; the updated regular file is published through a temporary
file without truncating the original on a failed write.

After applying the rules, reindex to remove previously indexed rows and regenerate
the vault, then restart the website. The public navigation snapshot and the web
analysis already filter newly excluded stale rows, so excluded files, symbols and
import links are not exposed by a new web session while awaiting that reindex.
Unreadable project rules prevent that session from starting. The current browser
session still uses its startup snapshot and must be restarted after rule changes.

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

### `obsidian`

Regenerates the Obsidian vault from the existing SQLite index, without reindexing. The vault is `.mct-index/obsidian/`: `index.md` links to directory maps, each directory map links to source-file notes, and each source-file note lists its imports and indexed symbols (kind, name, lines, parent). Markdown files are listed like any other source file, with their headings as symbols; binary assets are left out. Source contents are not copied, so the vault stays compact and code excerpts continue to come from MCP context tools. In Obsidian choose "Open folder as vault" on that folder. `init`, `reindex` and the MCP server's background refresh already keep it current; unchanged generated notes are reused. The folder is regenerable and already covered by `.mct-index/` in `.gitignore`; Obsidian's own `.obsidian/` settings inside it are preserved.

```sh
mct-cli --root . obsidian
```

The vault shares the project-root `.mctignore` with SQLite indexing, including
re-inclusions and opt-in `@import-gitignore`; it has no separate ignore file.
Run `mct-cli --root . reindex` after changing rules to synchronize database rows
and generated notes/links. Manual `obsidian` hides newly excluded stale rows,
but cannot add files that have not been indexed. Unreadable or invalid-UTF-8
rule files stop these operations and preserve the last generated vault.

The generated format is deterministic: file frontmatter includes `title`,
`path`, `language`, `content_hash`, and `symbol_count`. Names use a sanitized,
bounded ASCII path label and a stable SHA-256 suffix. Relative Markdown links
connect notes to their directory map and resolved local imports. Each file
lists at most 100 symbols and 50 unique imports, with omitted counts; maps
paginate after 100 entries. Display labels cap at 160 characters (symbols)
and 240 (links); precise source paths stay in frontmatter. These bounds are
not token estimates. Use SQLite/FTS search first, then retrieve specific
locations with existing context tools; relation links are secondary navigation,
never automatic graph expansion. MCP retains its existing deduplication and
response budget; the vault is never queried as an alternative source.

`.manifest.toon` owns only generated notes. Renames and deletions remove obsolete
owned notes; personal notes and `.obsidian/` are preserved. Modified generated
notes, unmanaged filename collisions, corrupt manifests and symlinks cause an
error rather than overwriting content. Move conflicting personal content out
of generated paths before retrying. Manual `obsidian` returns a nonzero exit on
failure; automatic generation reports a warning and preserves the successful
SQLite reindex. `--no-obsidian` leaves an existing atlas untouched.

Publication stages the complete tree in a unique sibling directory and uses
two renames with rollback; it is not crash-atomic and does not promise power-loss
durability. After a process crash, check for a retained `.mct-obsidian-*` sibling
and recover its `previous` vault if needed. Remove `obsidian.lock` only after
confirming no generator is active, then regenerate. Evals, corpus and library
consumers do not register the hook. See [ADR-013](docs/01-architecture/adrs/013-obsidian-vault-location.md).

### `web` / `web --port <port> --no-obsidian`

Serve the local project dashboard (code atlas, duplicate and dead-code review, search, dependencies, notes):

```sh
mct-cli --root . web
mct-cli --root . web --port 8787
mct-cli --root . web --no-obsidian
```

The command prints a stable `http://localhost:<port>/` URL with no key in it, so
it can be bookmarked. Keep the command running; Ctrl-C stops it. No browser opens
automatically.

**Port.** The project root is the current directory unless `--root` is given. The
port is remembered per project root in `<root>/.mct-index/web-port` (a plain
number; the folder is git-ignored and derived, so deleting it only forgets the port).

- First run: `8787` if free, otherwise a free port chosen by the system.
- Later runs: the remembered port while it is free. If it is busy, a free port is
  chosen, used and remembered instead.
- The port is bound once and the same listener is used, so no other process can take
  it between the check and the use.
- `--port <port>` is an explicit choice: it is used exactly, never remembered, and a
  busy port is an error (`port N on 127.0.0.1 is unavailable ...`), not a silent change.
  `--port 0` takes any free port.
- Each project keeps its own file, so several projects get different ports (the
  first to start gets `8787`). Two instances on the same root: the second finds the
  port busy, takes another and remembers that one, so the next single start uses it.

**Security without a key.** The server binds `127.0.0.1` only. Protection against
other websites relies on the `Host` check (only `localhost:<port>` and
`127.0.0.1:<port>`, which stops DNS rebinding), `Origin` and `Sec-Fetch-Site`
checks, a per-session CSRF token and exact same-origin `Origin` plus
`application/json` for every POST, and a restrictive content policy. Limitation:
the old URL key also kept out other *local* processes; now any process (or other
user) on the same machine that can reach `127.0.0.1:<port>` can read the dashboard
and, with a valid `Origin` and CSRF token it fetches from the API, change review
decisions or `.mctignore`. Use `mct-cli web` only on a machine you trust; on a
shared machine prefer an explicit `--port` while the dashboard is needed, then stop it.

- **Dashboard:** counts, review-state chart, language bars, project health and
  recent activity. Metrics the index does not record (history, tokens,
  confidence, reviewers) are shown as unavailable, never invented.
- **Indexed code:** files and symbols with filters, a source preview and the
  resolved imports/importers of each file.
- **Duplicates / Dead code / Reviews:** filterable lists with a side-by-side
  compare view (aligned diff, synced panes, whitespace/comment/identifier
  normalization), a review panel (state and note) and per-finding history.
- **Search:** keyword search over code, symbols, files, duplicates, dead code,
  Obsidian notes and dependencies; saved and recent searches stay in the
  browser. Semantic search is not offered.
- **Dependencies, Statistics, Obsidian, Unified workspace, Settings:** import
  graph (tree and table views), charts, a safe Markdown note reader and
  diagnostics. The UI is Spanish (Spain) with an i18n-ready string table, dark
  and light themes, keyboard navigation and a responsive layout.
- **Complete navigation:** lists page at 20-100 entries and every filter lives
  in the URL, so views can be bookmarked. Group IDs are positions within this
  startup snapshot, not stable cross-run identifiers; review decisions use
  stable content keys.

An existing index is required. The command opens SQLite read-only and never
reindexes, migrates, changes exclusions or installs MCP hooks. Metadata comes
from the public bulk navigation API; duplicate analysis uses the same typed API,
source validation and default thresholds/policy as `duplicates` (6 lines,
40 tokens, broad normalization, trivial calls demoted, no partial-block search).
It computes the typed report at startup rather than parsing the CLI's TOON cache.
Unsupported languages, skipped candidates and index failures are visible.

By default `web` also regenerates `.mct-index/obsidian/` through `mct-obsidian`.
A failed write prints a warning and leaves the website usable. `--no-obsidian`
leaves that folder untouched while keeping the web atlas available. The web
atlas always comes from SQLite, so edited personal notes and `.obsidian/`
configuration are never served. Code excerpts appear only in review and the source preview. No graph expansion or source-context budget change.

Pages are a fixed startup snapshot held in memory; browser reload does not rerun
analysis. After changes, run `reindex` and restart `web`. The only HTTP mutation is a protected duplicate-review preference action. There
is no reindex/refresh, source edit, arbitrary file serving or external resource. The server
binds only IPv4 loopback, checks Host and Origin,
rejects cross-site requests and applies a restrictive content policy. Requests
use a 16 KiB header limit, a 4 KiB review-form limit and a 2-second read
deadline, one request per connection. Another process on the same machine can reach the port
(there is no URL key); see the security notes above.
Only this process's rendered views are available, never the SQLite file or vault
contents as a directory listing. See [ADR-014](docs/01-architecture/adrs/014-local-project-web.md).

#### Dismiss duplicate findings / false positives

Each group in the web list and its detail page has **Move to discarded**, with
**False positive** or **Other / not relevant** as the reason. The group immediately
leaves the active list. **Discarded** has the same search and pagination, plus
**Restore to active**. Atlas and overview counts show active groups; the raw CLI
analysis and TOON download keep every group and their original verdicts.

Decisions persist in `.mct-index/duplicate-reviews.toon`, an optional local user
preference file, not a second code index. Keys hash the normalization policy,
sorted member paths/names/kinds/line ranges and indexed source-file hashes. A
ranking/member-order change does not attach a decision to a different group.
Changing a member file (even an unrelated comment), its path, membership or the
normalization policy conservatively makes the group active again after reindex
and restart. Old decisions are retained, so reverting to the original content can
restore its prior review state. Only currently detected groups appear in either
view. No source file, SQLite row, raw verdict or Obsidian note is deleted.

Only `POST api/review` can change preferences. It requires the private URL, an exact
same-origin Origin, a per-session CSRF nonce and a bounded JSON body (state, optional note; history is append-only). GET/HEAD never
change state. Writes reload under a per-project exclusive lock, merge other
writers' decisions and atomically replace the TOON file before changing the
visible state. A failed save leaves the group visible and reports an error.
Malformed, oversized or symlinked state is refused, never overwritten. Unix
also rejects hard-linked state; publication replaces the file entry on every platform.
Concurrent editing/renaming of the state directory outside the tool is unsupported.

Preferences update immediately in the current web process; other already-running
processes see them on their next save or restart. A crash may leave
`duplicate-reviews.lock`; remove it only after confirming no writer is active.
Atomic replacement is not a promise of power-loss durability. No bulk purge or
source-deletion operation is provided. See [ADR-015](docs/01-architecture/adrs/015-duplicate-review-decisions.md).

### `dead-code` / `dead-code --path <path> --language <lang> --output <file>`

Writes a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) to `.mct-index/dead-code-report.csv` — one row per candidate with its file, function name, kind, language, and start/end line. Optionally narrowed to a file/directory/crate and/or language, and written to a custom path instead of the default.

`--reviewed show|hide|only` applies the review decisions made in `mct-cli web`
and adds a `review_state` column: `show` keeps every row, `hide` leaves out
false positives and dismissed candidates whose code is unchanged (and says how
many), `only` lists just those. A decision about code that changed since shows
as `needs-reevaluation` and is never hidden. Without the flag no review file is
read and the report is unchanged. If a review file cannot be read, a warning
goes to stderr and nothing is hidden; a decision kept from a readable file while
another source is unreadable also shows as `needs-reevaluation` and hides nothing
(the CLI, MCP and web list report this effective state; the web API keeps the
stored one in `previousState`).

```sh
mct-cli --root . dead-code
mct-cli --root . dead-code --path src/services --language python --output dead-code.csv
mct-cli --root . dead-code --reviewed hide
```

<details>
<summary>Full output</summary>

```
Write a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) — one row per candidate with its file, function name, kind, language, and start/end line

Usage: mct-cli dead-code [OPTIONS]

Options:
      --path <PATH>          File, directory, or crate prefix to scan — same matching semantics as `list_symbols`. Omit for the whole project
      --root <ROOT>          Project root to operate on. Defaults to the current working directory [alias: --roots]
      --language <LANGUAGE>  Exact language id to keep (e.g. `rust`, `python`, `go`)
      --output <OUTPUT>      Where to write the CSV report. Defaults to `<root>/.mct-index/dead-code-report.csv`
      --reviewed <REVIEWED>  Apply review decisions from the web dashboard and add a `review_state` column. Omit for the unreviewed report [possible values: show, hide, only]
  -h, --help                 Print help (see more with '--help')
```

</details>

### `duplicates` / `duplicates --min-lines <N> --min-tokens <N>`

Reports candidate duplicate functions and methods in Rust and Python, as TOON.
Groups are leads for human review, not proof of redundant or equivalent behavior.
Run `init` or `reindex` first: analysis uses the existing index and checks current
file hashes, without reindexing, migrating or changing indexed data.

- `--normalization broad` (default) erases identifier names and literal values.
- `--normalization consistent` preserves identifier repetition: `a+a` matches
  `x+x`, but differs from `x+y`. It retains f-string interpolation structure.
  It compares spellings, not resolved scopes; shadowing is not modelled.
- `--normalization exact` retains emitted identifier/literal text, including
  declaration names and f-string expressions. Trivia is ignored, including Rust
  visibility. It can match equal functions in different scopes or files.
- `--path` selects a relative file or directory; `--language` selects an indexed
  language id. Filters apply before reading/tokenizing files.
- `--ranking tokens` ranks by tokens times additional copies (an estimate);
  `--ranking lines` retains the default line-based order.
- `--limit`, `--offset` and `--no-excerpts` control output. Totals cover all groups
  in the filtered scope. `omitted_groups` counts groups outside the output page.
- `--output <FILE>` also writes the TOON report to a file (relative paths use the
  project root); stdout is unchanged.
- A repeat run reuses the last report when nothing it depends on changed: the
  options, the binary version, the index database, `.mctignore`/`.gitignore` and
  the current bytes of every indexed file. It is stored as
  `.mct-index/duplicates-cache.toon` (the report behind one `cache_key` field)
  and announced on stderr. `--no-cache` neither reads nor writes it.

`status: complete` means no candidate failed analysis. Separately, `index_coverage`
reports whole-index failures, even outside filters: `partial`, `unknown`, or
`no-recorded-errors`. The last value does not discover new, unindexed files.
Files edited/deleted since indexing are skipped and mark analysis partial.
Unsupported languages are listed. Exact callable byte ranges exclude surrounding
syntax and preceding decorators/attributes; ambiguous boundaries are skipped.

Each group has an `alignment` block. Members share one normalized sequence, so
their identifier and literal tokens are compared position by position:
- `identifier_pattern`: `identical`, `one-to-one` (a consistent renaming) or
  `not-one-to-one`, with an example. It compares spellings, not bindings, so
  reusing one name for two shadowed variables can read as `not-one-to-one`.
- `differing_positions` and `differences` list the original texts and lines per
  member, in member order, separated by ` / `. Kinds come from syntactic roles:
  `declaration`, `parameter`, `name` (no role), `member` (attribute, field,
  method or keyword name), `callee`, `type`, `literal-value`,
  `literal-category` (for example number versus string), `interpolated-string`,
  `docstring`, `mixed-role`. At most 12 distinct differences are listed.
- `undeclared_name_positions` counts differing `name` positions where some
  member's spelling is not one of its own parameters: a renamed local, or a
  reference to a different outside item (path, constant, unit struct, enum
  variant, function). Bindings are not resolved, so the two are not told apart.
- `warnings` explain differences worth a reviewer's attention: different
  members, callees or type names, literal values or categories, interpolated
  strings that broad compares whole, and a non-one-to-one identifier pattern.
- `types` counts all aligned positions with a type role, including equal texts.
  `compatibility` is always `unknown`. `primitive_spelling_conflicts` counts
  positions containing distinct recognized primitive spellings; `conflicts`
  lists at most 12 of them with per-member values and lines. Only complete direct
  parameter, return and local annotations are recognized. Compound types,
  qualified paths, generic arguments, unions, Optional, string annotations and
  custom names stay unknown. An unknown member does not hide a conflict between
  two other members; mixed roles are not compared. The warning is
  `primitive-type-spellings-differ`. Names can be shadowed in both languages,
  including Rust `type u32 = String`, so neither differing nor equal spellings
  establish type compatibility. No group is removed, split or reordered.

Each group also has a `context` block for what the compared spans exclude:
- `status`: `same`, `differs`, or `not-evaluated` when a member's language
  reports no callable context (with a `context-unavailable` warning).
- `aspects`: `decorators` (Python decorators, Rust outer attributes, comments
  removed) and `enclosing` (constructs around the callable, outermost first,
  such as `@dataclass class`, `try > except` or `#[cfg(test)] mod`). Each row
  shows the shared value, or one value per member separated by ` / `. `(none)`
  and `(top level)` stand for an empty aspect.
- A differing aspect adds a `decorators-differ` or `enclosing-context-differs`
  warning. Labels name kinds only: conditions, context managers, class names and
  impl targets are not compared.

Each group also has a `priority` block, computed before pagination:
- `call_shape`: `call-only` when every member's body is only flat calls (one
  call or macro invocation per statement, whose arguments hold no further call,
  closure, comprehension, conditional or block, and no nested macro
  delimiters), `mixed` when only some members' are, `other`, or
  `not-evaluated`. Callee names are never read: `print`, `log` or `assert` may
  be redefined or matter.
- `level` and `reason`: with `--trivial-calls demote` (the default) a call-only
  group is `demoted` and ranked after every other group, keeping its relative
  order; `--trivial-calls keep` leaves the ranking order. No group is removed:
  `total_groups` and pagination include demoted groups, and `demoted_groups`
  counts them.

Each group also has a `subgroups` block, computed before pagination, that
partitions its members by the evidence separating them. It is an extra view:
the group's `members` table, totals, order and pagination do not change.
- Two members share a subgroup when they agree on every separating item: the
  text at each differing aligned `member`, `callee` and `type` position, the
  category at each `literal-category` position, and their identifier pattern
  (each spelling numbered by first use: equal patterns are exactly consistent
  renamings, so `value * factor` and `value * value` differ). A `member`,
  `callee` or `type` position is not separating when every member's text there
  is a name that member declares or takes as a parameter (a called parameter, a
  recursive call). Individual names, parameters, literal values, interpolated
  strings, docstrings and mixed roles never separate, so a consistent renaming
  never splits. Decorators and enclosing constructs stay in the `context`
  block and never separate.
- `status`: `single` (nothing separates the members), `split`, or
  `not-evaluated` (aligned tokens missing for a member). `basis` lists the
  separating kinds; `count` the subgroups; `separating_items` the separating
  positions, plus one when identifier patterns differ.
- `all_distinct: true` means every member is its own subgroup: no rows are
  listed, because the member table already shows that partition.
- Otherwise `rows` lists at most 8 subgroups, in order of their first member:
  `id`, `size`, `members` (1-based positions in the group's member table, at
  most 10), their `locations` (`path:start_line`), and `values`: the
  subgroup's value at the first three separating items, as `kind=text`
  (`identifier-pattern=#N` numbers patterns in member order).
  `omitted_subgroups` counts the unlisted rest.
- The header's `split_groups` counts split groups among all `total_groups`.


Each reported group also has a `verdict` block, derived only from the blocks
above by a fixed rule table (no score, weight or probability):
- `level`: `SUSPICIOUS` when any rule fires, otherwise `REAL`. `FALSE_POSITIVE`
  is part of the output contract, but no current rule emits it: no available
  evidence alone shows that members are not duplicates.
- `reasons`: every rule that fired, in a stable order. Suspicion reasons are
  `alignment-unavailable`, `identifier-pattern-differs`,
  `undeclared-names-differ`, `mixed-role-differences`, `member-names-differ`,
  `callee-names-differ`, `type-names-differ`,
  `primitive-type-spellings-differ`, `literal-categories-differ`,
  `literal-values-differ`, `interpolated-strings-differ`,
  `context-unavailable`, `decorators-differ`, `enclosing-context-differs`,
  `call-only-bodies`, `call-shape-mixed` and `body-shape-not-evaluated`. A
  `REAL` group lists `identical-tokens` or `consistent-renaming` (only the
  declaration name and the members' own parameters differ), `same-context`,
  and `docstrings-differ` when docstrings differ.
- The header's `verdict_counts` (`real`, `suspicious`, `false_positive`) cover
  the reported groups only, after `--limit`/`--offset`.

Verdicts are heuristic explanations, not proof: `REAL` does not prove equal
behavior (bindings, types and call effects are unresolved), and renamed locals
are conservatively `SUSPICIOUS`. A verdict never removes, splits, reorders or
filters a group; demotion and ranking are unchanged.

Evidence never changes grouping: no domain is inferred from names, type
compatibility is not evaluated, and `alignment_not_evaluated` lists the
remaining unevaluated criteria.

`--partial-blocks` (off by default) also reports **repeated token sequences**
shared by two or more different functions of one language, below the
whole-function level, under the selected `--normalization`. Without the flag
the report is unchanged. A block is a lead, not a refactoring proposal:
extraction feasibility (values in/out, control flow, side effects) is not
evaluated, and `alignment_not_evaluated` says so in place of
`partial-block repetition`.
- Blocks are trimmed to whole lines with balanced `()`, `[]`, `{}` and Python
  block markers, and must have `--block-min-tokens` source tokens (default 40)
  and `--block-min-lines` token-bearing lines in every occurrence (default 6).
- Only maximal blocks are listed: a block whose every occurrence lies inside
  members of one reported group is left out, and a shorter block is listed only
  when one of its occurrences does not overlap a larger listed block.
  Repetition inside a single function is not reported; each function shows
  its first occurrence.
- Budgets, all deterministic counts: `--block-max-windows` (default 2,000,000,
  memory: 16 bytes per hashed window), `--block-max-comparisons` (default
  20,000,000 token comparisons, time) and `--block-max-blocks` (default 200
  listed blocks). The header's `partial_blocks_budget` shows each limit, its use
  and `budget_exhausted`: `false` or the first budget hit (`max-windows`,
  `max-comparisons`, `max-blocks`). An exhausted budget makes the block list
  partial; groups stay complete. Each `--block-*` flag requires
  `--partial-blocks`.
- Output: `partial_block_count`, then a `partial_blocks` list after `groups`,
  each with `id`, `language`, `tokens`, `lines` and
  `occurrences{path,start_line,end_line,function}`, ordered by tokens, then
  occurrence count, then first location.

```sh
mct-cli --root . duplicates
mct-cli --root . duplicates --normalization consistent --path crates --language rust --ranking tokens --limit 20 --no-excerpts
mct-cli --root . duplicates --normalization exact --min-lines 10 --min-tokens 80
mct-cli --root . duplicates --partial-blocks --limit 0 --block-max-blocks 50
mct-cli --root . duplicates --reviewed hide --limit 20
```

`--reviewed show|hide|only` applies the web dashboard's review decisions to
the groups before paging: `total_groups`, `split_groups`, `demoted_groups` and
the verdict counts describe the kept groups, a `reviewed` field names the mode,
`hide` adds `hidden_by_review`, and a final `review_states[n]{group,state}`
table gives each shown group's state. The analysis then runs over every group
and the report cache is not used. Partial blocks still consider every group.
Without the flag the report and its cache are unchanged.

Design: [ADR-007](docs/01-architecture/adrs/007-duplicate-function-report.md),
[comparison policies](docs/01-architecture/adrs/008-token-twins-comparison-policies.md),
[aligned evidence](docs/01-architecture/adrs/009-duplicate-group-aligned-evidence.md),
[evidence subgroups](docs/01-architecture/adrs/011-duplicate-subgroups.md). and [repeated partial blocks](docs/01-architecture/adrs/012-repeated-partial-blocks.md).
Reproducible measurements: [token twins evaluation](benchmarks/token-twins.md).

<details>
<summary>Full output</summary>

```
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

</details>

### `probe <files...>`

Parses files without indexing them and reports, per file, either its symbol/relation counts or the first syntax error with its line. No index is opened or written, the files need not be under `--root`, and the extension picks the language. Exits non-zero if any file fails to parse. Useful for finding which construct a grammar rejects.

```sh
mct-cli probe snippet.cpp other.cpp
```

<details>
<summary>Full output</summary>

```
Parse files without indexing them and report, per file, either its symbol/relation counts or the first syntax error with its line.

For narrowing down which construct a grammar rejects: no index is opened or written, and the files need not be under `--root`. Exits non-zero if any file fails to parse.

Usage: mct-cli probe [OPTIONS] <FILES>...

Arguments:
  <FILES>...
          Files to parse; the extension picks the language

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Example:
  cargo run -p mct-cli -- probe snippet.cpp other.cpp
```

</details>

### `update` / `-u` / `--update`

Updates `mct-cli` and `mct-mcp-server` to the latest [GitHub release](https://github.com/Zubiarka8/mini-consumes-tokens/releases) by running the official installer (`install.sh`, or `install.ps1` on Windows) with the folder `mct-cli` runs from as the install folder, so both programs are replaced where they already are. Check the result with `mct-cli --version`.

```sh
mct-cli update      # same as: mct-cli -u, mct-cli --update
```

<details>
<summary>Full output</summary>

```
Update mct-cli and mct-mcp-server to the latest GitHub release.

Runs the official installer (`install.sh`, or `install.ps1` on Windows) with this binary's own folder as the install directory, so both programs are replaced where they already are. Needs network access, plus `curl` and `bash` on macOS/Linux. Release binaries are built without the `semantic` feature: a source install with `--features semantic` is replaced by one without it. Close the MCP client first on Windows, which cannot overwrite a running `mct-mcp-server.exe`.

Usage: mct-cli {update|--update|-u} [OPTIONS]

Options:
      --root <ROOT>
          Project root to operate on. Defaults to the current working directory

  -h, --help
          Print help (see a summary with '-h')

Examples:
  mct-cli update
  mct-cli -u
  mct-cli --update
```

</details>

---

## 2. `mct-mcp-server` — the program your AI assistant actually talks to

You don't normally run this by hand; your assistant or editor starts it once connected (see [README.md § Connecting it to your AI assistant or editor](README.md#43-connecting-it-to-your-ai-assistant-or-editor)).

### `mct-mcp-server --root <path>`

Starts the server for that project and waits for an MCP client over stdio. Builds/refreshes the index automatically on startup, and keeps refreshing it silently in the background as files change.

```sh
mct-mcp-server --root /path/to/your/project
```

### `--reindex-delay <VALUE>`

How long the server waits **without any relevant file change** before it updates the index in the background. `VALUE` is one of:

| Value | Meaning |
| --- | --- |
| `15s`, `30s`, `45s`, `1m` | Common presets |
| `<N>s`, `<N>m`, `<N>ms` | Any custom positive duration, for example `90s` |
| `off` | Disables automatic updates (same as `--reindex-debounce-ms 0`) |

It conflicts with `--reindex-debounce-ms`; give only one. With neither, `MCT_REINDEX_DEBOUNCE_MS` sets the wait; if the variable is absent or invalid, the default is 15000 ms (15 seconds). `--reindex-delay` overrides the variable.

Behaviour (quiet-period semantics):

- Every relevant file change restarts the wait. A burst of saves collapses into a single update.
- Changes that arrive while an update is running are processed right afterwards: updates never overlap and no change is lost.
- Paths excluded by `.mctignore`, `.gitignore` or the built-in exclusions do not trigger or extend the wait.

Example with `--reindex-delay 30s`:

1. You save a file. The server starts a 30 s wait.
2. After 20 s you save another file. The wait starts over from 30 s.
3. Nothing changes for 30 s. The server updates the index and persists it.

```sh
mct-mcp-server --root /path/to/your/project --reindex-delay 30s
mct-mcp-server --root /path/to/your/project --reindex-delay 90s
mct-mcp-server --root /path/to/your/project --reindex-delay off
```

To set it for an MCP client, run `mct-cli --root . mcp-register --reindex-delay 30s`; the generated `.mcp.json` entry then contains:

```json
"args": ["--root", "/path/to/your/project", "--reindex-delay", "30s"]
```

### `--reindex-debounce-ms <ms>`

Milliseconds of quiet time required after the last detected file write before the background watcher auto-reindexes. Defaults to `15000` (15 seconds). Conflicts with [`--reindex-delay`](#--reindex-delay-value). Passing `0` disables the background watcher entirely (index only refreshes on startup or a manual `reindex`). Edits to the ignore rules are the exception: every tool call first checks `.mctignore` (and an imported `.gitignore`) and, if the rules changed, reindexes before answering. The `MCT_REINDEX_DEBOUNCE_MS` environment variable sets the same value; precedence is flag, then variable, then default, and `--reindex-delay` overrides the variable.

```sh
mct-mcp-server --root /path/to/your/project --reindex-debounce-ms 5000
```

### `--no-obsidian`

Stops the server from regenerating the Obsidian vault in `.mct-index/obsidian/` after each reindex (see [`obsidian`](#obsidian)).

```sh
mct-mcp-server --root /path/to/your/project --no-obsidian
```

### `--rebuild-embeddings`

Reindexes, drops every stored embedding, re-embeds every symbol with the build's model (needs a build with `--features semantic`), then exits without serving.

```sh
mct-mcp-server --root /path/to/your/project --rebuild-embeddings
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
          Milliseconds of quiet time required after the last detected file write before auto-reindexing. 0 disables the background watcher entirely, restoring the previous startup-only behavior. Precedence: this flag, then the MCT_REINDEX_DEBOUNCE_MS environment variable, then the default of 15000 Conflicts with --reindex-delay.
      --reindex-delay <VALUE>
          Quiet time without file changes before the index updates itself. Presets: 15s, 30s, 45s, 1m. Custom: <N>s, <N>m or <N>ms (N > 0), e.g. 90s, 2m, 500ms. 'off' disables automatic updates. Conflicts with --reindex-debounce-ms
      --rebuild-embeddings
          Reindex, drop every stored embedding, re-embed every symbol with the build's model (`--features semantic`), then exit without serving
  -h, --help
          Print help

Examples:
  --reindex-delay 30s     update the index after 30s without file changes
  --reindex-delay 2m      custom delay (<N>s, <N>m or <N>ms)
  --reindex-delay off     disable automatic updates
```

</details>

---

## 3. MCP tools — what your AI assistant calls for you

These aren't run by hand; they're the tools an MCP client (Claude Code, Cursor, etc.) calls once connected. Grouped the same way `discover_tool_categories` (§ 3.5) groups them. Most response-returning tools share three optional parameters, documented once here instead of per tool: `limit` (max results; default 50, except `search_symbols_like` and `hybrid_search_like` which default to 10 and clamp to 100 — the latter names it `top_k`), `offset` (skip this many results before applying `limit`, for paging; not offered by `list_symbols` or `find_symbol`), and `format` (`"toon"`, the default compact table, or `"text"`, the opt-in labelled layout — see the note at the end of this section).

### 3.1 Discovery — "what exists here, and what does it look like?"

**`list_symbols`** — the discovery step. Lists symbol definitions (name, kind, line range) found under a path: a single file matches exactly, a directory/crate path (no extension) matches as a prefix. Optionally narrowed to one symbol `kind` and/or one `language`. Use this first when you don't know a symbol's exact name yet — it's what feeds `find_symbol`/`find_references`/etc., not a replacement for them.

**`get_file_skeleton`** — a single file's top-level declarations with bodies collapsed to `// ...`, up to ~90% fewer tokens than reading the whole file. Brace-delimited languages get precise body elision; others get a best-effort declaration-line rendering. Takes one file path, not a directory — use `list_symbols` first if you don't know which file you need.

**`get_project_overview`** — the cheapest way to get oriented in an unfamiliar file, directory, or the whole project: a capped, ranked hierarchical digest (modules, key symbols, optionally each symbol's top callers) in one call, instead of chaining `list_symbols` + `get_file_skeleton` + `find_calls` by hand. Coarser than the two tools above, so switch to them once you know which file you care about. Optional `include_relations` (default false) adds top-caller lines at roughly 4x the response cost; `max_symbols_per_module` (default 8) caps the top-level symbols shown per file, and `path`/`language` narrow the scope.

**`get_file_tree`** — a plain directory/file tree, no symbol data, depth-limited (`depth`, default 3) and pruned of the same noise directories (`target`, `node_modules`, `.git`) reindexing skips. Cheaper than `get_project_overview` when you only need the filesystem shape, before you know which file or crate to look at.

### 3.2 Lookup — "where is this, exactly?"

**`find_symbol`** — finds the definition location(s) of a symbol by name, across every indexed language, including polyglot projects. `match` controls how `name` is matched: `exact` (default), `prefix`, or `fuzzy` (substring, case-insensitive — use this instead of guessing the exact name and re-querying). Does not find callers/references — use `find_callers`/`find_references` for that.

**`search_symbols_like`** — for when you're unsure of the exact name: give words or a partial identifier in any case style (`parse request`, `parseReq`, `parse_request_body`) and get BM25-ranked hits, exact matches first. Each word prefix-matches a word of the symbol's name. Optional `path`, `language`, `limit` (default 10, max 100), `offset`, `snippet_lines` (source lines shown under each hit, default 0, max 50) and `cache` (`false` skips the query cache, which otherwise answers a repeated unchanged call with a one-line "unchanged"). Not exhaustive — use `find_symbol`/`list_symbols` for that. Optional `detail` (`overview`, `summary`, `detailed`) and `max_tokens` (an *estimated* token budget for the whole response, 1–1,000,000; not a model limit) return a lower-detail page that fits it, flag what was lowered or cut, and name the `offset` that continues; a budgeted call bypasses the query cache.

**`hybrid_search_like`** — finds code by meaning: `search_symbols_like` fused with embedding similarity (RRF). `alpha` weights semantic against lexical (0 = lexical only, 1 = semantic only; omitted, the query's shape picks it). A `"double-quoted"` query is an exact phrase, lexical only, and also lists string literals (error/log messages) that hold it as `path:line`. Without an embedding model (build without `--features semantic`) it falls back to lexical only and says so. Takes the same `path`, `language`, `top_k`, `offset`, `snippet_lines`, `cache`, `format`, `detail` and `max_tokens` options as `search_symbols_like`; with a budget the first-line note and the string-literal section count toward it and are never cut.

`search_symbols` and `hybrid_search` remain deprecated aliases for one version, with the same behavior as their corresponding `*_like` tools.

### 3.3 Relations — "who touches this, and what does it touch?"

**`find_callers`** — every direct caller of a function/method.

**`find_calls`** — every function/method a given function/method calls.

**`find_references`** — every reference to a symbol: calls, imports, extends/implements — broader than `find_callers`/`find_calls`. Optional `sort` accepts `relevance` (default) or `execution` (graph hop, file, line and column, sorted before pagination). Execution order is a bounded static approximation, not runtime control flow.

**`impact_analysis`** — the full blast radius of changing or removing a symbol in one call: combines `find_callers` + `find_references` + a heuristic check for affected tests, instead of three separate calls.

**`build_context_pack`** — the composite for working on one symbol: its definition with doc comment and capped source (`source_lines`, default 40, 1–200), plus every caller, callee, dependency and related test listed once with its signature — one call in place of `find_symbol` + `find_calls` + `find_callers` + `impact_analysis` + reading the files. Takes the exact `symbol` name; `path`/`language` pick the definition when the name is ambiguous, `depth` (default 1) sets the call-graph hops in both directions, `limit` (default 30) caps the related symbols listed. Optional `detail` (`overview`: locations only; `summary`: signatures; `detailed`: the default output) and `max_tokens` (an *estimated* budget for the whole response; not a model limit): over budget the least relevant rows lose detail and then are dropped, and a `Budget:` footer says what was lowered, what exists (`avail:`), the cost of the levels above and, when a row was cut, what showing everything would cost (`full=`).

`find_callers`, `find_calls`, `find_references` and `impact_analysis` accept `depth` (how many relation-graph hops to walk beyond the direct hit; default 1 is today's direct-hit behavior, raising it also includes hits-of-hits, each tagged with its hop number, clamped to a hard maximum) — reach for `depth` before manually chaining calls to walk the graph further out.

### 3.4 Maintenance — "is the index okay, and what's unused?"

**`reindex`** — re-scans the project on demand. `force` (default false) re-parses every file regardless of whether its content changed; omit it for the normal incremental behavior. Rarely needed by hand since the index also refreshes automatically at startup and in the background.

**`get_indexing_status`** — reports index health: coverage per language, last indexed time, unsupported languages seen, files that failed to parse. `verbose_dependencies` (default false) lists every manifest's dependencies individually instead of a one-line summary; `dependency` (an exact package name, e.g. `three`) also lists each manifest that declares it, with its version. When either is set and the project has `*.rockspec` files, the response also carries a **LuaRocks detail** section (issue #267): per rockspec, every declared entry with scope, constraint and platform, set against what the project-local `lua_modules` trees list (`lib/luarocks/rocks-<lua>/manifest`) and what `luarocks.lock` pins, plus the rockspec's `build.modules` and `build.install.lua` maps. Everything is read statically: nothing is installed, executed or fetched. "Not found" means not found in the project-local trees (user and system trees are never inspected); an unreadable, oversized, corrupt or excluded tree or lockfile is reported as incomplete or unknown, never as absent; a rockspec containing code is `INCOMPLETE` in every section. Platform entries are shown as declared under a platform, never as active. The report says what is declared, installed, pinned and compatible; it does not predict the version LuaRocks would load at run time. `build.install.lua` is interpreted as validated only for LuaRocks 3.13.0 with build type `none`.

**`get_dependency_graph`** (Lua addition) — each external import made by a `.lua` file in the walk also gets an origin: the rockspecs whose directory contains the importer are listed separately (never merged), each with the project module that maps it, the installed rock that lists it and whether that rock is declared (`declared`, `declared only under platform`, `installed but NOT declared`, or `declaration unproven` when the rockspec's declarations were not read completely), or `origin unknown` with the declared rocks whose module lists could not be read. A sole candidate is claimed only when every relevant source was read.

**`find_dead_code`** — candidate unused functions/classes/structs/enums/traits/interfaces/type-aliases: a heuristic based on zero indexed references, not true visibility analysis. A starting point for cleanup, not a certainty — sanity-check a hit with `find_references` or `impact_analysis` before deleting anything.

**`find_duplicate_code`** — read-only, review-aware duplicate candidates: groups of functions/methods with the same normalized tokens, as compact references (stable group `id`, members with `path`, symbol, kind, lines). All inputs are optional: `path` (root-relative file or directory), `language`, `normalization` (`broad` default, `consistent`, `exact`), `min_lines` (default 6), `min_tokens` (default 40), `limit` (1..100, default 10), `offset`, `include_reviewed` (default false) and `max_tokens` (estimated response budget, 512..16000, default 4000). A group hidden by a `false-positive` or `dismissed` decision that still matches its exact code is filtered out **before** `limit`/`offset`; `include_reviewed` shows it with `actionable: false`. The response counts `detected`, `actionable` and `suppressed` groups, so "nothing found" (`status: no_groups`) differs from "everything reviewed" (`status: all_suppressed`). `review_store: degraded` means a review file could not be read and nothing was hidden. Matching structure is not proof of redundant behavior. It never writes review state. Decision record: [ADR-022](docs/01-architecture/adrs/022-mcp-duplicate-findings.md).

### 3.5 Meta — progressive tool discovery and batching

**`discover_tool_categories`** — lists every registered tool's name and one-line purpose, grouped by category, with no input schemas. A cheap first call for an MCP client that wants to defer loading full schemas. The standard MCP `tools/list` handshake still returns every tool's full schema up front as usual — these two meta tools only help a client built to use them instead.

**`get_tool_schema`** — returns one named tool's full input schema and description on demand, once `discover_tool_categories` (or this document) has told you which tool you need.

**`batch`** — runs 1 to 25 read-only queries in one call (`queries`: a list of `{tool, args}`, each `args` exactly as the tool would take it directly). Each result is reported under its own `[n] tool` header; a failing query is reported inline and the rest still run. `reindex` and nested `batch` are rejected. Use it whenever you already know the queries you need, to drop the per-call overhead.

**`select_tools`** — opt-in (`MCT_TOOL_ATTENTION=1`); not listed otherwise. With it set, `tools/list` starts with this tool alone. Call it with `query` (the user's request): the server narrows `tools/list` to the tools that request needs and sends `tools/list_changed`. An unclear request keeps the full list, and every tool stays callable by name. Figures: [`benchmarks/intent-selection.md`](benchmarks/intent-selection.md).

### A note on `format`

`list_symbols`, `find_symbol`, `search_symbols_like`, `hybrid_search_like`, `find_references`, `find_calls`, `find_callers`, `impact_analysis`, `find_dead_code`, `build_context_pack`, `get_dependency_graph`, and the two deprecated search aliases accept an optional `format` argument — `"toon"` (the default) or `"text"`. Omitting `format`, or sending `null` or a blank string, selects `toon`; any other value is rejected with `format must be one of text, toon`. `toon` renders the result's uniform rows (symbols, references, calls) as a compact TOON table (one header row of column names, then one row per hit, no repeated labels) — fewer tokens on a large result, at the cost of `list_symbols`' per-kind grouping. Pass `"format":"text"` for this server's labelled lines. `batch` sub-queries follow the same default, and `semantic_search` and the other tools without a `format` argument are unchanged. MCP arguments and the JSON-RPC envelope remain JSON.

**Compatibility:** the default used to be `text`. A client or script that parsed the old default output must now pass `"format":"text"`.

---

For installation, connecting to your assistant, uninstalling, and troubleshooting, see [README.md](README.md).

### Review-aware MCP dead-code findings

`find_dead_code` reads the shared TOON review decisions. Only a matching
`false-positive` or `dismissed` decision hides a candidate; filtering occurs
before `limit` (default 50) and `offset`. Optional `include_reviewed: true`
shows hidden rows with `actionable: false`. Both formats report `status`
(`no_candidates`, `all_suppressed`, `candidates`), `detected`, `actionable`,
`suppressed`, `review_store` (`ok` or `degraded`), `returned`, `truncated` and
`next_offset`. Unreadable stores hide nothing. Rows expose identity and review
label, never private notes or history. `max_tokens` is an estimated output
budget, 512..16000 (default 4000); it bounds the response, not analysis work.
Restart pagination after a reindex or a review change. Existing `path`,
`language`, `limit`, `offset` and `format` parameters retain their defaults.

### Source revision verification

MCP source consumers compare the bytes read with the indexed Git blob hash
before using indexed line ranges. Changed, deleted, binary or unindexed files
supply no search snippets or context-pack signatures/source. Metadata remains
queryable. `get_file_skeleton` rejects stale revisions with a reindex instruction.
Restoring the indexed bytes or reindexing restores source retrieval; cached
rankings do not bypass the byte check. Reads retain existing size/path limits.
