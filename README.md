# mini-consumes-tokens (mct)

![CI](https://img.shields.io/github/actions/workflow/status/Zubiarka8/mini-consumes-tokens/ci.yml?branch=main&label=CI)
![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)
![Version](https://img.shields.io/badge/version-0.1.0-informational.svg)

**A local map of your code that answers "where is this defined", "what uses it", and "what would break if I change it" — without an AI assistant having to re-read your files over and over.**

---

## 1. What is this and why does it exist?

When an AI coding assistant helps you with a project, it usually has to *open and read* files, or *search through* your whole codebase, just to answer simple questions like "where is this function defined?" or "what else calls this?". That works, but it's slow and it uses up a large chunk of the assistant's available "attention" (technically: tokens) — attention that could otherwise go toward actually solving your problem.

`mini-consumes-tokens` fixes that by doing the reading once, in advance. It scans your project and builds a small local map — an index — of everything meaningful in your code: functions, classes, and how they call and reference each other. That map lives entirely on your own computer, in a single plain file (no cloud service, no external database, nothing leaves your machine).

Once that map exists, your AI assistant can ask it precise questions instead of hunting through files by hand. In our own measurements, this cut the amount of text an assistant needed to read by roughly **70–98%**, depending on the language and the question — for example, finding where something is defined in a Go project used about **97.8% less text** than reading the matching files directly.

A few things that make this different from a typical single-purpose plugin:

- **It understands 16 programming and markup languages at once**, including projects that mix several languages together.
- **It isn't tied to one editor or assistant.** It speaks a common protocol called **MCP** (Model Context Protocol) — think of it as a shared language that lets an AI assistant ask a tool for information directly, the same way different apps on your phone can all talk to the same calendar. Any MCP-capable assistant can connect to it.
- **It also works as a plain command-line tool**, for anyone who just wants to check the health of the index or rebuild it by hand, with no AI assistant involved at all.
- **15 MCP tools, grouped by purpose** (discovery, lookup, relations, maintenance, meta) — from a single symbol lookup or a ranked search by partial name to a one-shot "what would this change affect" blast-radius report, plus a project-wide overview and a plain directory tree for getting oriented before you know which file you need.
- **Progressive tool discovery**: an MCP client can ask for a cheap one-line-per-tool catalog first (`discover_tool_categories`) and fetch a tool's full input schema only when it actually needs it (`get_tool_schema`), instead of always loading every schema up front.
- **Multi-hop relation queries**: `find_references`, `find_calls`, `find_callers`, and `impact_analysis` can walk multiple hops through the call/reference graph in one call (`depth`) and page through large result sets (`offset`), instead of chaining single-hop calls by hand.
- **Compact `toon` output format**: any result-returning tool can render its output as a token-lean table instead of the default labelled text, opt-in per call.

**Supported languages:** Rust, Python, Java, C#, Kotlin, JavaScript/TypeScript, C++, Go, HTML, CSS, XML, XAML, Bash, PowerShell, PHP, and Markdown (headings plus [[WikiLink]]/#tag relations).

### See it in action

Without an index, asking "where is `InvoiceService` defined, and what would break if I changed its `calculate_total` method?" usually sends an assistant off to open and read a pile of files just to find out. With the index connected, it asks two precise questions instead — *where is this defined* and *what depends on it* — gets back a short, exact answer, and moves straight on to actually helping you, instead of spending its attention re-discovering your codebase from scratch.

### Table of contents

1. [What is this and why does it exist?](#1-what-is-this-and-why-does-it-exist)
2. [Quick start](#2-quick-start-copy-paste)
3. [Prompt for your AI assistant](#3-prompt-for-your-ai-assistant-copy-paste)
4. [Installation and setup](#4-installation-and-setup)
5. [Uninstalling completely](#5-uninstalling-completely)
6. [Quick usage guide](#6-quick-usage-guide)
7. [Common commands](#7-common-commands)
8. [FAQ](#8-faq)
9. [Troubleshooting common problems](#9-troubleshooting-common-problems)

---

## 2. Quick start (copy-paste)

The fastest path from zero to a connected AI assistant: five steps, one project, no configuration decisions to make up front. Each step below links to the section with the full details, in case something doesn't go as expected.

> **Note on paths:** steps 1–2 happen inside a clone of *this* project (`mini-consumes-tokens`), to build/install the two programs. Step 3 onward happens inside *your own* project — the one you actually want indexed. Don't run `mct-cli` from inside the `mini-consumes-tokens` checkout by mistake.

**1. Clone this project** (skip this if you're only installing the prebuilt binaries instead — see § 4.2, option A, no Rust toolchain needed):

```sh
git clone https://github.com/Zubiarka8/mini-consumes-tokens.git
cd mini-consumes-tokens
```

**2. Build and install the two programs** this project ships — `mct-cli` (the command-line tool) and `mct-mcp-server` (what your AI assistant talks to):

```sh
cargo install --path crates/mct-cli
cargo install --path crates/mct-mcp-server
```

**3. Move into the project you actually want indexed and build its index** — this creates a `.mct-index/` folder there, safe to delete/rebuild anytime:

```sh
cd /path/to/your/project
mct-cli --root . init
```

**4. Register the MCP server for this project** — writes/merges a `.mcp.json`, works out of the box for Claude Code, Cursor, and most MCP clients (see § 4.3 if your tool needs a different config file/format):

```sh
mct-cli --root . mcp-register --name mini-consumes-tokens
```

**5. Verify everything worked** — prints how many files/symbols got indexed:

```sh
mct-cli --root . status
```

**Last step:** restart or reconnect your AI assistant (tool-specific steps in § 4.3 — some need a manual restart to pick up the new MCP connection), then just ask it something like *"where is `InvoiceService` defined?"*. If it keeps grepping/reading whole files instead of using the index, paste the [prompt in § 3](#3-prompt-for-your-ai-assistant-copy-paste) into its instructions.

Hit an error along the way? Jump straight to [§ 9 Troubleshooting](#9-troubleshooting-common-problems).

---

## 3. Prompt for your AI assistant (copy-paste)

This block is plain text, not a specific tool's syntax — paste the same one into whichever instruction file your assistant reads: `CLAUDE.md` (Claude Code), `.cursorrules`/`.cursor/rules/*.mdc` (Cursor), `AGENTS.md` (Codex and other agents that support it), or straight into a chat. It does three things: makes sure the MCP server is actually installed and connected before relying on it, keeps the index itself up to date, and makes the assistant prefer the indexed tools over grepping/reading whole files.

```
This project uses mini-consumes-tokens (mct) — an MCP server that indexes
the codebase into a symbol graph, so I can look things up instead of
grepping/reading whole files.

Before relying on it in a session:
- If the `mini-consumes-tokens` MCP tools aren't available, check whether
  `mct-cli`/`mct-mcp-server` are installed (`mct-cli --help`). If not,
  install them (see the project's README § 4.2) and register the server
  with `mct-cli --root . mcp-register --name mini-consumes-tokens`, then
  ask me to restart/reconnect so the connection loads.
- If the tools are available, run `get_indexing_status` (or
  `mct-cli --root . status`) once per session. If it reports the index is
  stale, missing, or unhealthy, rebuild it with `reindex` (or
  `mct-cli --root . reindex --force`) before trusting its answers.

Once it's connected and healthy, prefer it over grep/reading whole files:
- "Where is X defined?" -> find_symbol
- "Something like `parse request`, exact name unknown?" -> search_symbols
- "Who calls X?" -> find_callers
- "What does X call?" -> find_calls
- "Every reference to X" -> find_references
- "What would break if I changed/removed X?" -> impact_analysis
- Exploring a file/folder before knowing a name -> list_symbols,
  get_file_skeleton, get_project_overview, get_file_tree
- Candidate unused code -> find_dead_code

Still read/edit files normally for everything else (writing code, checking
comment wording, anything the index doesn't cover).
```

---

## 4. Installation and setup

### 4.1 Minimum requirements

- **A computer running Windows, macOS, or Linux** — it works identically on all three.
- **Git**, to download the project (only needed for the build-from-source option).
- **The Rust toolchain** via [rustup](https://rustup.rs) — built and tested with Rust 1.96.0; rustup fetches everything needed automatically. Only needed for the build-from-source option.
- **Windows only:** the "Desktop development with C++" workload from Visual Studio Build Tools, to compile Rust code on Windows. No other Windows-specific setup is required.
- **An AI coding assistant or editor that supports MCP**, if your goal is to connect it to your assistant. Not required if you only want the command-line tool.

### 4.2 Installing

This project isn't published to a package manager (like `apt`, Homebrew, or `winget`) yet, so there are two ways to install it: a one-line script that downloads a ready-to-run copy, or building it yourself from source. Either one leaves you with the same two programs, `mct-cli` and `mct-mcp-server`.

#### Option A: quick install script — no Rust toolchain required

This downloads the latest prebuilt release for your operating system. Both scripts print a warning if their install folder isn't already on your PATH, along with the exact line to add.

**macOS / Linux** (installs into `$HOME/.local/bin`; override with `INSTALL_DIR`):

```sh
curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash
```

```sh
# custom folder:
INSTALL_DIR=/usr/local/bin curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash
```

**Windows, PowerShell** (installs into `%LOCALAPPDATA%\mct\bin`; override with `MCT_INSTALL_DIR`):

```powershell
irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex
```

```powershell
# custom folder:
$env:MCT_INSTALL_DIR = "C:\tools\mct"; irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex
```

If you'd rather not pipe a script straight into your shell, download the archive for your system by hand from the [GitHub Releases page](https://github.com/Zubiarka8/mini-consumes-tokens/releases) and extract `mct-cli`/`mct-mcp-server` into any folder on your PATH.

#### Option B: build from source

Requires the prerequisites above (Rust via rustup, plus the MSVC C++ build tools on Windows). Same commands on every operating system:

```sh
git clone https://github.com/Zubiarka8/mini-consumes-tokens.git
cd mini-consumes-tokens
cargo install --path crates/mct-cli
cargo install --path crates/mct-mcp-server
```

**Working on the repo itself and don't want to install anything?** Skip `cargo install` and run straight from the checkout with `cargo run -p <crate> --`. Every `mct-cli ...` example in this README becomes:

```sh
cargo run -p mct-cli -- --root . init
cargo run -p mct-cli -- --root . status
cargo run -p mct-cli -- --root . ignore-init --import-gitignore
```

(and `mct-mcp-server` the same way: `cargo run -p mct-mcp-server -- --root <project>`). This also matches what `mct-cli --help`/`mct-cli <command> --help` show when built this way.

#### Verifying either option worked

```sh
mct-cli --help
mct-mcp-server --help
```

If both print usage text, the install worked. If you get "command not found" instead, see [Troubleshooting](#9-troubleshooting-common-problems).

### 4.3 Connecting it to your AI assistant or editor

All of these launch the same program (`mct-mcp-server`) — only the configuration file and its format differ per tool. A shortcut that works for most of them: run this from inside your project folder to write (or safely merge into) a `.mcp.json` with the right settings, leaving any other tool already configured there untouched.

```sh
mct-cli --root . mcp-register --name mini-consumes-tokens
```

**Claude Code** has built-in, first-class support. From your project folder:

```sh
claude mcp add mini-consumes-tokens --scope project -- mct-mcp-server --root "<absolute-path-to-your-project>"
```

`--scope` controls who can see the connection: `local` (just you, on this machine), `project` (everyone on the team, via a config file you can commit), or `user` (all of your projects, everywhere).

> **Gotcha:** if Claude Code was already open when you added the server, it won't notice — new connections load only at startup, and `claude mcp list` will show "⏸ Pending approval". Close Claude Code (`exit`) and reopen it (`claude`) to finish connecting.

**Cursor** reads the same kind of file `mcp-register` produces. Run the command above inside your project, then restart Cursor. To add it by hand instead, open Cursor's MCP settings and point them at the `.mcp.json` in your project folder.

**Codex (OpenAI's Codex CLI)** uses TOML instead of JSON, usually at `~/.codex/config.toml`:

```toml
[mcp_servers.mini-consumes-tokens]
command = "mct-mcp-server"
args = ["--root", "<absolute-path-to-your-project>"]
```

**GitHub Copilot (VS Code)** uses `.vscode/mcp.json` (one project) or your VS Code user settings (every project), with a top-level `"servers"` key instead of `"mcpServers"`:

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

**Any other MCP-compatible tool** — JetBrains AI Assistant, Gemini, Grok Build, Open Code, Open Claw, and the rest — needs the same two details: a program to run (`mct-mcp-server`) and the project folder to point it at (`--root <path>`). Run `mct-cli --root . mcp-register --name <a-name-you-choose>` once to generate a ready-made block, then paste the relevant part wherever that tool keeps its MCP settings. Exact steps for Gemini, Grok Build, Open Code and Open Claw could not be verified for this documentation pass — follow that tool's own MCP documentation for where the block goes.

---

## 5. Uninstalling completely

**Step 1 — disconnect it from every tool you connected it to**, using that tool's own removal method:

- **Claude Code:**

  ```sh
  claude mcp remove mini-consumes-tokens --scope project
  ```

  (match the `--scope` you used when adding it)

- **Any other tool:** remove the `mini-consumes-tokens` entry from that tool's MCP configuration file (`.mcp.json`, `.vscode/mcp.json`, `~/.codex/config.toml`, or wherever it keeps one — see § 4.3), by hand or through its settings screen.

**Step 2 — remove the binaries:**

```sh
# If you installed with the quick install script, delete the two binaries from the
# install folder ($HOME/.local/bin, %LOCALAPPDATA%\mct\bin, or your INSTALL_DIR/MCT_INSTALL_DIR):
rm "$HOME/.local/bin/mct-cli" "$HOME/.local/bin/mct-mcp-server"    # macOS / Linux
```

```powershell
# Windows:
Remove-Item "$env:LOCALAPPDATA\mct\bin\mct-cli.exe","$env:LOCALAPPDATA\mct\bin\mct-mcp-server.exe"
```

```sh
# If you built from source instead:
cargo uninstall mct-cli
cargo uninstall mct-mcp-server
```

**Step 3 — delete each project's local index** (safe to do any time — it's fully derived from source and gets rebuilt on the next `init`/`reindex`):

```sh
rm -rf .mct-index                        # macOS / Linux
```

```powershell
Remove-Item -Recurse -Force .mct-index   # Windows PowerShell
```

**Step 4 — clean up config files (optional):** if you committed a `.mcp.json` (created by `mcp-register`) or added a `.mctignore`/a `.mct-index/` entry in `.gitignore`, remove or edit those files too — none of them are required for the uninstall to be complete, they're just leftover configuration.

---

## 6. Quick usage guide

```sh
cd path/to/your/project     # 1. move into the project you want indexed
mct-cli --root . init       # 2. build the index (once — connecting an assistant also triggers it)
mct-cli --root . status     # 3. check it worked: files and symbols indexed, last run, any read failures
```

1. **Connect your AI assistant or editor** — see § 4.3 for your specific tool.
2. **Just ask.** Things like *"where is* `InvoiceService` *defined?"* or *"what would break if I change* `calculate_total`*?"* — your assistant uses the index automatically instead of reading every file. Paste the [prompt in § 3](#3-prompt-for-your-ai-assistant-copy-paste) if it doesn't yet.
3. **Refresh if needed:** `mct-cli --root . reindex --force`. Not usually necessary (the index also refreshes itself automatically in the background as changes settle on disk), but it's there if your assistant seems to be missing something after a large batch of changes.
4. **Keep noise out (optional):** `mct-cli --root . ignore-init` writes a starter `.mctignore` — edit it to exclude things like `docs/` or `*.md` from the index, then `reindex --force`. Pass `--import-gitignore` (works on a fresh file or an existing one) to exclude everything your project's own `.gitignore` already excludes. And `mct-cli --root . gitignore-init` keeps the generated database itself out of git.

**What your assistant can now do**, once connected:

- Find everything defined in a file or folder, when you don't know the exact name yet
- Find exactly where a specific function or class is defined
- Find every place something is used, anywhere in the project — optionally several hops out
- Find what a function depends on, and who depends on it
- Get a one-shot answer to "what would this change affect"
- See a file's overall shape without reading the whole thing
- Get oriented in an unfamiliar file, directory, or the whole project in one call
- Browse the plain directory/file layout before picking a file
- Spot candidate dead code (heuristic: zero indexed references)
- Check whether the index itself is healthy and up to date

---
## 7. Common commands

The handful of commands you'll actually type by hand. For the full technical reference — every flag, every MCP tool parameter, and the raw `--help` output — see **[COMMANDS.md](COMMANDS.md)**.

| I want to...                                    | Run this                                   |
| ------------------------------------------------ | ------------------------------------------ |
| Build the index for the first time                | `mct-cli --root . init`                    |
| Check whether the index is healthy and up to date | `mct-cli --root . status`                  |
| Force a full re-index after a lot of changes       | `mct-cli --root . reindex --force`         |
| Connect an AI assistant/editor to this project     | `mct-cli --root . mcp-register`            |
| Keep the generated index files out of git          | `mct-cli --root . gitignore-init`          |
| Exclude extra files/folders from indexing          | `mct-cli --root . ignore-init`             |
| Find candidate unused code                         | `mct-cli --root . dead-code`               |

Everything above accepts `--help` for its full description and examples (e.g. `mct-cli reindex --help`), and every command works the same way on Windows, macOS, and Linux.

Your AI assistant doesn't use this table at all — once connected, it calls the indexed tools directly (see § 3 for the prompt that nudges it to do so, and [COMMANDS.md](COMMANDS.md) for what each tool actually does).

---

## 8. FAQ

**Does my code ever leave my computer?**
No. The index is a single local file (`.mct-index/index.sqlite3`) built and read entirely on your machine — no cloud service, no external server, no telemetry.

**Will this slow down my AI assistant or my machine?**
No noticeably — the index builds once, then updates incrementally (only re-reading files that changed) instead of re-scanning your whole project every time.

**Do I have to use an AI assistant to get value from this?**
No — `mct-cli status`/`reindex`/`dead-code` work standalone from the command line with no assistant involved (§ 7).

**What if my assistant doesn't support MCP?**
Then this project can't connect to it directly — MCP (§ 1) is what lets an assistant call these tools. You can still use `mct-cli` by hand.

**What happens if I delete the index by accident?**
Nothing is lost — it's entirely derived from your source code and gets rebuilt the next time you run `init`/`reindex`, or the next time your assistant connects.

**Does it work on a project that mixes several languages?**
Yes — the same index covers all 16 supported languages (§ 1) in one project, including files that reference each other across languages.
---

## 9. Troubleshooting common problems

**"**`mct-cli`**"/"**`mct-mcp-server`**: command not found" after installing**
Cargo installs programs into a folder (usually `~/.cargo/bin`, or `%USERPROFILE%\.cargo\bin` on Windows) that isn't always on your PATH. Add that folder to your PATH (the rustup installer offers to do this — rerun it, or add the folder manually), open a new terminal, and try again.

**Windows build fails with a** `link.exe` **or** `cl.exe` **error**
The Microsoft C++ build tools from § 4.1 aren't installed. Install the "Desktop development with C++" workload via Visual Studio Build Tools, then rerun the `cargo install` commands.

**Claude Code shows "⏸ Pending approval" and the assistant can't see the tools**
The server was added while Claude Code was running. Type `exit`, relaunch with `claude`, and check with `claude mcp list`.

**The assistant gives wrong, confused, or empty answers about your code**
Almost always a path problem — check that the `--root` used when connecting matches your actual project folder, not a subfolder or a different project. Re-register if it doesn't.

**Answers seem out of date after a lot of recent changes**
Run `mct-cli --root . reindex --force` for a full refresh, or `mct-cli --root . status` to see when it last indexed and whether anything failed.

**Nothing happens at all / the assistant says it can't reach the tool**
Most MCP connections talk over "stdio" — a program's normal input and output — not a network address. Check that your configuration entry uses `"type": "stdio"` with a local `command`, not a URL or port. Running `mct-cli --root . mcp-register` again regenerates a known-good entry.

---

## License

Dual-licensed under either the [MIT license](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your choice.

For the complete command/flag/tool-parameter reference, see [COMMANDS.md](COMMANDS.md). For the internal design, see [ARCHITECTURE.md](ARCHITECTURE.md). For contributing, including how to add a new language, see [CONTRIBUTING.md](CONTRIBUTING.md).
