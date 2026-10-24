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
  web                   Serve the local dashboard (duplicates, dead code, Obsidian, reviews, comments)
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

Serve the local dashboard for duplicate code, dead code and the Obsidian notes:

```sh
mct-cli --root . web
mct-cli --root . web --port 8787
mct-cli --root . web --no-obsidian
```

The default port is 8787 and the private key is saved in `.mct-index/web-token`
(mode 0600, git-ignored), so the URL stays the same between runs. Use
`--rotate-token` to issue a new key; old links then stop working.

The command prints a private `http://127.0.0.1:<port>/<random-key>/` URL. Open
that URL in your browser and keep the command running; Ctrl-C stops it. Port `0`
(the default) selects an available port. No browser opens automatically.

Two interfaces share the same private URL and the same data:

- **`<url>app`:** the single-page dashboard (ADR-019): duplicates with a
  side-by-side comparison, dead-code candidates (heuristic, never presented as
  proven), Obsidian (statistics, note list, note detail with links and backlinks,
  focus graph) and review history. It refreshes by polling `api/version` every
  2 seconds, so a reindex shows up without restarting the command.
- **`<url>`:** the server-rendered pages of ADR-018 (Spanish/English). They work
  without JavaScript and stay available as the fallback.

What you can do in the dashboard:

- **Review a finding:** mark a duplicate group or dead-code candidate as
  confirmed, false positive or ignored on purpose, with a reason. Every change is
  kept in a history, and **Restore** returns the finding to pending. A finding
  whose symbol/block text changed is marked "needs re-evaluation" instead of
  being hidden. Unrelated edits and line moves preserve the decision.
- **Comment:** add, edit and delete free-text comments (up to 2,000 bytes, 500 per
  finding) on any finding.
- **Browse:** lists are paged and searchable by path, symbol or note text.

Where state lives:

- `.mct-index/index.sqlite3` is derived data. `web` opens it read-only and never
  reindexes, migrates it or changes exclusions.
- `.mct-index/dashboard.sqlite3` holds your decisions, their history and your
  comments. It survives any reindex or index rebuild because rows are keyed by the
  finding's stable identity, never by an index row id. It is created on first use.
- Besides the decision on the whole group, each member file of a duplicate group
  has its own status (unreviewed, accepted, rejected, in review, ignored) and note,
  keyed by the group's identity and the file path. Setting a file back to
  unreviewed with an empty note removes its row.
- On first open, an existing `.mct-index/reviews.toon` is imported once and left
  untouched; the older `duplicate-reviews.toon` is still read and never rewritten.

Only the indexed project is served; there is no project switcher. An existing
index is required. Duplicate analysis uses the same typed API, thresholds and
policy as `duplicates` (6 lines, 40 tokens, broad normalization, trivial calls
demoted, no partial-block search). After a reindex the analysis snapshot is
rebuilt on a worker thread, at most every 2 seconds; until it finishes the
previous snapshot keeps being served and `api/version` reports `rebuilding`. On a
large repository the rebuild can take several seconds.

By default `web` also regenerates `.mct-index/obsidian/` through `mct-obsidian`.
A failed write prints a warning and leaves the website usable. `--no-obsidian`
leaves that folder untouched and hides the Obsidian view. The dashboard always
reads notes from SQLite, so edited personal notes and `.obsidian/` configuration
are never served.

Security: the server binds only IPv4 loopback and requires the generated URL key.
It checks Host and Origin, rejects cross-site requests and sends a CSP with
`script-src 'self'; style-src 'self'` (no inline code, no `unsafe-eval`, no
external origin: Tailwind, Alpine and the icons are precompiled and embedded).
JSON writes (`POST api/review`, `api/file-review`, `api/comment`, `api/comment/update`,
`api/comment/delete`, `api/review/delete`) require `Content-Type: application/json`, an exact
same-origin `Origin`, the per-session CSRF token in the `X-CSRF-Token` header and
a body of at most 8 KiB. Requests use a 16 KiB header limit and a 2-second read
deadline, one request per connection; responses are `Cache-Control: no-store`.
Keep the private URL local; another process running as your user remains within
the trust boundary. See [ADR-014](docs/01-architecture/adrs/014-local-project-web.md),
[ADR-018](docs/01-architecture/adrs/018-web-dashboard.md) and
[ADR-019](docs/01-architecture/adrs/019-dashboard-sqlite-store.md).

Finding and orphan lists support `q`, `status`, `offset`, `limit` and `snapshot`.
The SPA requests 50 results at a time; the API caps each page at 200. A stale
snapshot returns 409 so the client can restart without mixing generations.
`api/reviews?status=orphan` includes retained restored decisions and annotations;
`api/finding` permits read-only inspection of those orphan identities.
`api/duplicates?id=...&full=1&source_offset=0&source_limit=400` returns a bounded
source chunk with availability/truncation and next-offset metadata.

Decision writes require the displayed `fingerprint` and `expected_revision`;
file-label writes and `POST api/review/delete` require `expected_revision`.
HTTP 409 preserves the existing decision when the editor is stale. Finding
verdicts and per-file working labels are separate controls. Unsaved SPA drafts
remain in their tab during navigation and conflicts; they are not durable backups.

MCP `find_dead_code` and CLI `dead-code`/`duplicates` omit unchanged false positives
and deliberately ignored findings before paginating their reports. Changed or
uncertain findings remain visible. Use CLI `--include-reviewed` for raw diagnostic
candidates. Reindexing does not delete decisions from `dashboard.sqlite3`.

SPA regression checks: `node --test crates/mct-cli/tests/dashboard_spa.cjs`.

Rebuilding the precompiled assets (only needed when editing the dashboard UI, not
to build `mct-cli`): `scripts/unix/build-dashboard-assets.sh`.

Decisions and comments are never written to the source files, the index or the
Obsidian vault, and no bulk purge or source-deletion operation exists.
### `dead-code` / `dead-code --path <path> --language <lang> --output <file>`

Writes a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) to `.mct-index/dead-code-report.csv` — one row per candidate with its file, function name, kind, language, and start/end line. Optionally narrowed to a file/directory/crate and/or language, and written to a custom path instead of the default. Omits unchanged candidates saved as false positive or ignored in the `web` dashboard; `--include-reviewed` writes the raw candidates.

```sh
mct-cli --root . dead-code
mct-cli --root . dead-code --path src/services --language python --output dead-code.csv
```

<details>
<summary>Full output</summary>

```
Write a CSV report of dead-code candidates (heuristic: symbols with zero indexed references) — one row per candidate with its file, function name, kind, language, and start/end line.

Omits unchanged candidates saved as false positive or ignored in the `web` dashboard; pending, confirmed, edited and ambiguous ones stay. `--include-reviewed` writes the raw candidates for diagnostics.

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

      --include-reviewed
          Include unchanged findings explicitly discarded in the dashboard

  -h, --help
          Print help (see a summary with '-h')
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
```

Design: [ADR-007](docs/01-architecture/adrs/007-duplicate-function-report.md),
[comparison policies](docs/01-architecture/adrs/008-token-twins-comparison-policies.md),
[aligned evidence](docs/01-architecture/adrs/009-duplicate-group-aligned-evidence.md),
[evidence subgroups](docs/01-architecture/adrs/011-duplicate-subgroups.md). and [repeated partial blocks](docs/01-architecture/adrs/012-repeated-partial-blocks.md).
Reproducible measurements: [token twins evaluation](benchmarks/token-twins.md).

<details>
<summary>Full output</summary>

```
Report candidate duplicate functions and methods, as TOON.

Groups functions and methods of the same language whose token sequences are identical once identifiers and literals are normalized (Type-2 clones). Every group is a CANDIDATE for human review: matching structure does not prove the code is redundant or behaves the same. Runs on demand against the existing index and the current files; it never reindexes or migrates the index, so run `init` or `reindex` first. Omits unchanged groups saved as false positive or ignored in the `web` dashboard before counting and paging; `--include-reviewed` keeps them. Reuses an exact-input report cache in `.mct-index` only while no dashboard review state exists; use `--no-cache` to bypass it or `--output FILE` to save the TOON report. Candidates that cannot be analyzed are listed as skipped and mark the report partial. Only languages with a token source are analyzed; the report names the rest.

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

      --include-reviewed
          Include unchanged findings explicitly discarded in the dashboard

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

**`get_indexing_status`** — reports index health: coverage per language, last indexed time, unsupported languages seen, files that failed to parse. `verbose_dependencies` (default false) lists every manifest's dependencies individually instead of a one-line summary; `dependency` (an exact package name, e.g. `three`) also lists each manifest that declares it, with its version.

**`find_dead_code`** — candidate unused functions/classes/structs/enums/traits/interfaces/type-aliases: a heuristic based on zero indexed references, not true visibility analysis. A starting point for cleanup, not a certainty — sanity-check a hit with `find_references` or `impact_analysis` before deleting anything.

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
