# Installation and setup

How to install mini-consumes-tokens, connect it to an AI coding agent or editor, remove it and fix common problems. The short version is in the [README](../README.md#how-to-install).

## Requirements

- Windows, macOS or Linux.
- Git and the Rust toolchain from [rustup](https://rustup.rs), only if you build from source. It is built and tested with Rust 1.96.0.
- On Windows, the "Desktop development with C++" workload from Visual Studio Build Tools, also only for building from source.
- An AI coding agent or editor that supports MCP, if you want to connect one. The command line works without it.

## Install

The project is not in a package manager yet (apt, Homebrew, winget or crates.io). Either way you end up with the same two programs: `mct-cli` and `mct-mcp-server`.

### Option A: install script, no Rust needed

The script downloads the latest prebuilt release for your system from [GitHub Releases](https://github.com/Zubiarka8/mini-consumes-tokens/releases). Releases ship archives for Linux and macOS (x86_64 and arm64) and Windows (x86_64 and arm64), static musl builds for Linux, and `.deb` and `.rpm` packages. If a release has no archive for your platform, the script stops with an error and installs nothing; use option B. If the install folder is not on your PATH, the script prints the line to add.

The prebuilt binaries leave out the optional `semantic` feature, so `hybrid_search_like` matches by name only. To add it, see [Search by meaning](#search-by-meaning).

macOS and Linux install into `$HOME/.local/bin`:

```sh
curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash
```

To pick another folder, put `INSTALL_DIR` on `bash`. Set on `curl` it is ignored, and the script falls back to `$HOME/.local/bin` without telling you.

```sh
curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | INSTALL_DIR=/usr/local/bin bash
```

On Alpine, `install.sh` picks the static musl build by itself. Set `MCT_LIBC=musl` to force it when your glibc is too old.

Windows PowerShell installs into `%LOCALAPPDATA%\mct\bin`:

```powershell
irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex
```

```powershell
# another folder:
$env:MCT_INSTALL_DIR = "C:\tools\mct"; irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex
```

Linux packages install both programs into `/usr/bin`. They contain the static musl build, so they run on any release of those distros. Download the one for your architecture from the releases page, then:

```sh
sudo apt install ./mini-consumes-tokens-<tag>-linux-x86_64.deb      # Debian, Ubuntu, Mint
sudo dnf install ./mini-consumes-tokens-<tag>-linux-x86_64.rpm      # Fedora, RHEL
sudo zypper install ./mini-consumes-tokens-<tag>-linux-x86_64.rpm   # openSUSE
```

On an ARM machine use `linux-arm64`. The packages are in no distro repository, so `apt upgrade` and `dnf upgrade` skip them. Install the next release's package the same way.

If you prefer not to pipe a script into your shell, download the archive for your system from the releases page and extract `mct-cli` and `mct-mcp-server` into a folder on your PATH.

### Option B: build from source

The same commands work on every operating system:

```sh
git clone https://github.com/Zubiarka8/mini-consumes-tokens.git
cd mini-consumes-tokens
cargo install --locked --path crates/mct-cli
cargo install --locked --path crates/mct-mcp-server
```

`--locked` builds with the dependency versions the repository was tested with. Install from the clone. `cargo install mct-cli` from crates.io will not work, because the crates are not published there: the build needs a patched Markdown parser that lives in this repository (see [RELEASING.md](../RELEASING.md)).

To run from the checkout without installing anything, replace `mct-cli` with `cargo run -p mct-cli --` in any example, and `mct-mcp-server` with `cargo run -p mct-mcp-server --`:

```sh
cargo run -p mct-cli -- --root . init
cargo run -p mct-cli -- --root . status
```

If you are upgrading from 0.1.0 (`ccm-cli`, `ccm-mcp-server`, `.claude-index/`), follow the [upgrade steps in the changelog](../CHANGELOG.md#upgrading-from-010).

### Check the install, the version and updates

```sh
mct-cli --help
mct-mcp-server --help
mct-cli --version    # or -v
mct-cli update       # or -u or --update
```

If both `--help` commands print usage, the install worked. "Command not found" is covered in [Troubleshooting](#troubleshooting).

`update` runs the option A installer into the folder `mct-cli` already lives in, so both programs are replaced in place. It needs network access, plus `curl` and `bash` on macOS and Linux. Release binaries have no `semantic` feature, so if you built with it, pull your clone and run the same `cargo install` again. On Windows, close your MCP client first, because a running `mct-mcp-server.exe` cannot be replaced. Restart the client afterwards (`/mcp` in Claude Code) so it starts the new server.

## Connect your agent or editor

Every client launches the same program, `mct-mcp-server`. Only the config file and its format change. From inside your project, this writes a `.mcp.json`, or merges into one, without touching other servers already in it:

```sh
mct-cli --root . mcp-register --name mini-consumes-tokens
```

To make the server reindex only after a quiet period without file changes, add `--reindex-delay 30s` (or any duration; `off` disables automatic updates). Details are in [COMMANDS.md](../COMMANDS.md#--reindex-delay-value).

### Claude Code

```sh
claude mcp add mini-consumes-tokens --scope project -- mct-mcp-server --root "<absolute-path-to-your-project>"
```

`--scope` decides who sees the connection: `local` is only you on this machine, `project` is everyone on the team through a file you can commit, and `user` is all your projects.

If Claude Code was open when you added the server, it will not notice. New connections load at startup, and `claude mcp list` shows "Pending approval". Type `exit` and start `claude` again.

### Cursor

Cursor reads the file `mcp-register` writes. Run the command above inside your project and restart Cursor. To add it by hand, open Cursor's MCP settings and point them at the `.mcp.json` in your project.

### Codex CLI

Codex uses TOML. Put the entry in `~/.codex/config.toml` for every project, or in `.codex/config.toml` inside a trusted project for that project only:

```toml
[mcp_servers.mini-consumes-tokens]
command = "mct-mcp-server"
args = ["--root", "<absolute-path-to-your-project>"]
```

A secret-free copy is in [`.codex/config.example.toml`](../.codex/config.example.toml). `codex mcp add mini-consumes-tokens -- mct-mcp-server --root <path>` writes the same entry. Note that `mcp-register` writes JSON to `.mcp.json` and never creates Codex's `.codex/config.toml`, so the two are set up separately.

### GitHub Copilot in VS Code

Copilot uses `.vscode/mcp.json` for one project, or your VS Code user settings for all of them, with a top-level `"servers"` key instead of `"mcpServers"`:

```json
{
  "servers": {
    "mini-consumes-tokens": {
      "type": "stdio",
      "command": "mct-mcp-server",
      "args": ["--root", "${workspaceFolder}"]
    }
  }
}
```

### Any other MCP client

JetBrains AI Assistant, Gemini, Grok Build, Open Code, Open Claw and the rest need two things: the program to run (`mct-mcp-server`) and the project folder (`--root <path>`). Run `mct-cli --root . mcp-register --name <name>` once to generate a block, then paste it where that tool keeps its MCP settings. We have not verified the exact steps for Gemini, Grok Build, Open Code and Open Claw, so follow each tool's own MCP documentation for where the block goes.

### Keep credentials out of these files

This server needs none. If another MCP server in the same file needs a token, pass it through your shell environment or your client's credential store, never as a literal in the file. `.gitignore` ignores `.codex/config.toml` and `.mcp.json`, so a broad `git add` will not commit them, but that does not protect a copy made elsewhere. A token that was ever written in plain text is exposed. Revoke it in the provider's settings (for GitHub: Settings, Developer settings, Personal access tokens), create a new one and inject it from outside. Deleting it from the file does not revoke it.

## Instructions for your agent

Paste this block into whichever instruction file your agent reads: `CLAUDE.md`, `.cursorrules` or `.cursor/rules/*.mdc`, `AGENTS.md`, or straight into a chat. It tells the agent to check that the server is installed and connected, to keep the index fresh, and to prefer the indexed tools over grepping or reading whole files.

```
This project uses mini-consumes-tokens (mct), an MCP server that indexes
the codebase into a symbol graph, so I can look things up instead of
grepping/reading whole files.

Before relying on it in a session:
- If the `mini-consumes-tokens` MCP tools aren't available, check whether
  `mct-cli`/`mct-mcp-server` are installed (`mct-cli --help`). If not,
  install them (see docs/INSTALL.md) and register the server
  with `mct-cli --root . mcp-register --name mini-consumes-tokens`, then
  ask me to restart/reconnect so the connection loads.
- If the tools are available, run `get_indexing_status` (or
  `mct-cli --root . status`) once per session. If it reports the index is
  stale, missing, or unhealthy, rebuild it with `reindex` (or
  `mct-cli --root . reindex --force`) before trusting its answers.

Once it's connected and healthy, prefer it over grep/reading whole files:
- "Where is X defined?" -> find_symbol
- "Something like `parse request`, exact name unknown?" -> search_symbols_like (`search_symbols` still works for one version as a deprecated alias)
- "Code that does X, no idea what it's called?" -> hybrid_search_like (`hybrid_search` likewise)
- "Only the meaning, with a similarity score per hit?" -> semantic_search
- "Who calls X?" -> find_callers
- "What does X call?" -> find_calls
- "Every reference to X" -> find_references
- "What would break if I changed/removed X?" -> impact_analysis
- "Which files/modules does this module depend on, and is there an import cycle?" -> get_dependency_graph
- About to work on X, want its code + callers/callees/tests in one go -> build_context_pack
- Exploring a file/folder before knowing a name -> list_symbols,
  get_file_skeleton, get_project_overview, get_file_tree
- Candidate unused code -> find_dead_code (shared TOON reviews; `include_reviewed` shows hidden candidates)
- Duplicate code, skipping groups a person already rejected -> find_duplicate_code
- Several of the above at once, queries already known -> batch

Still read/edit files normally for everything else (writing code, checking
comment wording, anything the index doesn't cover).
```

## Search by meaning

By default `hybrid_search_like` matches names only and says so. To search by description (*"load the settings from disk"*) even when no word of it is in a name, install the server with the `semantic` feature:

```sh
cargo install --locked --path crates/mct-mcp-server --features semantic
```

The build downloads a prebuilt ONNX Runtime library, so it needs network access and a platform ONNX Runtime supports. It adds a small local embedding model, `bge-small-en-v1.5` (about 130 MB). The model downloads once, on the first `hybrid_search_like` call, into `.mct-index/models` (or `$FASTEMBED_CACHE_DIR` if set). `MCT_EMBEDDING_MODEL=all-MiniLM-L6-v2` picks a smaller, faster one. Each symbol is embedded together with its signature, doc comment and the functions it calls.

`alpha` tunes the mix: 0 matches by name only, 1 by meaning only, fused with Reciprocal Rank Fusion. If you leave it out, it follows the shape of the query: 0.1 for an identifier (`parse_request`, `HttpServer`, `Index::open`), 0.75 for a plain description and 0.5 otherwise.

Wrap the query in double quotes (`"parse request"`) for an exact phrase: name matching only, those words in that order, the literal name first. An exact phrase also searches string literals such as error and log messages, ignoring case and accents (`"database connection failed"` finds `src/db.rs:3 in function connect "database connection failed"`). Literals are indexed only for Rust string literals and Markdown paragraphs and pipe-table rows, at least eight characters and two words, the first 256 characters of each, up to 500 per file, and suspected secrets are skipped. Exact-phrase search needs no `semantic` build.

`semantic_search` uses the same model and stored vectors with no name matching at all. It returns the `top_k` symbols closest to the query by cosine similarity (default 10, maximum 100), each with its signature and score, and `path` and `language` narrow it. It needs the `semantic` build and says so without it. Vectors refresh incrementally: only symbols added or changed since the last reindex are embedded on the next call. To re-embed everything, after changing `MCT_EMBEDDING_MODEL` or if the vectors look stale, run `mct-mcp-server --root . --rebuild-embeddings`. It reindexes, drops every stored vector, re-embeds all symbols and exits.

Real output on the generated project of about 10,000 symbols used by the ignored test `real_model_finds_validate_token_among_10k_symbols` in `crates/mct-mcp-server/tests/semantic_search.rs`. These are the first three hits of ten. `validate_token` came 8th, and the best of the 400 generated `*_token` distractors, `validate_admin_token`, came 37th.

```text
semantic_search { "query": "validate user authentication", "top_k": 10 }

semantic: 10425/10425 symbols embedded (fastembed/bge-small-en-v1.5)
10 match(es) for `validate user authentication`, most similar first:
src/auth.rs:L17-L19 [rust] function authenticate_user  similarity 0.748
    pub fn authenticate_user(name: &str, password: &str) -> bool
src/search.rs:L1-L3 [rust] function validate_search_user  similarity 0.738
    pub fn validate_search_user(input: &str) -> usize
src/profile.rs:L1-L3 [rust] function validate_profile_user  similarity 0.737
    pub fn validate_profile_user(input: &str) -> usize
```

That test ran a release build on an Apple M4 with an in-memory index and a warm model, 30 runs, showing median and p95 with the range over five runs. Query embedding took 7 to 20 ms (p95 9 to 90 ms). The similarity scan over 10,425 vectors plus signatures took 11 to 25 ms (p95 14 to 42 ms). A whole warm tool call took 24 to 49 ms (p95 39 to 83 ms). Loading the model, about 5 s on the first call only, and embedding every symbol the first time, 105 to 218 s for 10,000 symbols, are not included. Later calls embed only what changed.

## Limits and caching

**Input size.** Indexing, source tools and CLI probes read regular files up to 16 MiB. Set `MCT_MAX_FILE_BYTES` to a positive byte count to change it. Oversized source files and manifests are reported as read failures, and any last good indexed data is kept and marked as possibly stale. Ignore-file read or compile failures go to stderr, and the built-in exclusions stay active when a custom glob cannot compile.

**Relation pagination.** Direct queries report an exact total and fetch only the rows you asked for. Multihop queries fetch one extra relation to detect that more exist. When there are more, they report a lower bound and say the total was not computed. `impact_analysis` filters and deduplicates affected tests by symbol identity before applying their own pagination window.

**Query cache** (`search_symbols_like` and `hybrid_search_like`). It is on by default. A repeated query reuses its earlier ranking after checking that every symbol in it still exists unchanged and that nothing else in the index changed the ranking. For `hybrid_search_like`, a query that means the same also hits (embedding similarity of 0.95 or more, with a clear margin over the next closest cached query). Results are always rendered from the current index and files, never from stored code. When a response is identical to one already sent in the session, the reply is a single `cache: …` line, about 90% fewer tokens. Pass `cache: false` to get the full text. `MCT_CACHE=off` disables the cache, and `MCT_SEMANTIC_CACHE=off` keeps only the exact match. Threshold, margin, reply mode and size are configurable too. See [the query cache notes](03-performance/query-cache.md) for validation, invalidation, benchmarks and limits.

## Uninstall

1. **Disconnect it from every tool you connected it to.** For Claude Code, match the `--scope` you used when adding it:

   ```sh
   claude mcp remove mini-consumes-tokens --scope project
   ```

   For any other tool, remove the `mini-consumes-tokens` entry from its MCP config (`.mcp.json`, `.vscode/mcp.json`, `~/.codex/config.toml`, or wherever it keeps one), by hand or from its settings screen.

2. **Remove the programs.** If you used the install script, delete both from the install folder (`$HOME/.local/bin`, `%LOCALAPPDATA%\mct\bin`, or your `INSTALL_DIR` or `MCT_INSTALL_DIR`):

   ```sh
   rm "$HOME/.local/bin/mct-cli" "$HOME/.local/bin/mct-mcp-server"    # macOS and Linux
   ```

   ```powershell
   Remove-Item "$env:LOCALAPPDATA\mct\bin\mct-cli.exe","$env:LOCALAPPDATA\mct\bin\mct-mcp-server.exe"
   ```

   If you built from source:

   ```sh
   cargo uninstall mct-cli
   cargo uninstall mct-mcp-server
   ```

3. **Delete each project's index.** It is safe any time: it is derived from your source and rebuilt on the next `init` or `reindex`.

   ```sh
   rm -rf .mct-index                        # macOS and Linux
   ```

   ```powershell
   Remove-Item -Recurse -Force .mct-index   # Windows PowerShell
   ```

4. **Optional cleanup.** If you committed a `.mcp.json`, or added a `.mctignore` or a `.mct-index/` line to `.gitignore`, edit or remove those. They are leftovers, and the uninstall is complete without this step.

## FAQ

**Does my code leave my computer?** No. The index is one local file, `.mct-index/index.sqlite3`, built and read on your machine. There is no cloud service and no telemetry. The optional `semantic` build downloads its embedding model once from Hugging Face and then runs it locally.

**Will it slow down my assistant or my machine?** Not noticeably. The index builds once and then updates incrementally, re-reading only the files that changed.

**Do I need an AI assistant?** No. `mct-cli status`, `reindex`, `dead-code` and `web` work on their own.

**What if my assistant does not support MCP?** It cannot connect, because MCP is how an assistant calls these tools. You can still use `mct-cli` by hand.

**What if I delete the index?** Nothing is lost. It is rebuilt from your source on the next `init` or `reindex`, or when your assistant connects.

**Does it work on a project with several languages?** Yes. One index covers every supported language, including files that reference each other across languages.

## Troubleshooting

**"mct-cli: command not found" or "mct-mcp-server: command not found" after installing.** Cargo installs into `~/.cargo/bin` (`%USERPROFILE%\.cargo\bin` on Windows), which is not always on your PATH. Add that folder to your PATH (the rustup installer offers to; rerun it or add it by hand), open a new terminal and try again. With the install script, add the folder it printed.

**The Windows build fails with a `link.exe` or `cl.exe` error.** The C++ build tools from [Requirements](#requirements) are missing. Install the "Desktop development with C++" workload from Visual Studio Build Tools and run the `cargo install` commands again.

**Claude Code shows "Pending approval" and the assistant cannot see the tools.** The server was added while Claude Code was running. Type `exit`, start `claude` again and check with `claude mcp list`.

**The assistant gives wrong, confused or empty answers about your code.** Almost always a path problem. Check that the `--root` you connected with is your project folder, not a subfolder or another project, and register again if it is not.

**Answers look out of date after many changes.** Run `mct-cli --root . reindex --force` for a full refresh, or `mct-cli --root . status` to see when it last indexed and whether anything failed.

**The assistant says it cannot reach the tool, or nothing happens.** Most MCP connections run over stdio, a program's standard input and output, not a network address. Check that your config entry uses `"type": "stdio"` with a local `command`, not a URL or a port. Running `mct-cli --root . mcp-register` again regenerates a working entry.
