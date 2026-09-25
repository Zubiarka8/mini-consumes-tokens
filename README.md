# mini-consumes-tokens (mct)

[![CI](https://img.shields.io/github/actions/workflow/status/Zubiarka8/mini-consumes-tokens/ci.yml?branch=main&label=CI)](https://github.com/Zubiarka8/mini-consumes-tokens/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Version](https://img.shields.io/badge/version-0.1.0-informational.svg)](#)

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

**Supported languages:** Rust, Python, Java, C#, Kotlin, JavaScript/TypeScript, C++, Go, HTML, CSS, XML, XAML, Bash, PowerShell, PHP, and Markdown (headings plus [[WikiLink]]/#tag relations).

---

## 2. Prerequisites

- **A computer running Windows, macOS, or Linux** — it works identically on all three.
- **Git**, to download the project (only needed for the build-from-source option).
- **The Rust toolchain** via [rustup](https://rustup.rs) — built and tested with Rust 1.96.0; rustup fetches everything needed automatically. Only needed for the build-from-source option.
- **Windows only:** the "Desktop development with C++" workload from Visual Studio Build Tools, to compile Rust code on Windows. No other Windows-specific setup is required.
- **An AI coding assistant or editor that supports MCP**, if your goal is to connect it to your assistant. Not required if you only want the command-line tool.

---

## 3. Installation and setup

### 3.1 Installing

This project isn't published to a package manager (like `apt`, Homebrew, or `winget`) yet, so there are two ways to install it: a one-line script that downloads a ready-to-run copy, or building it yourself from source. Either one leaves you with the same two programs, `mct-cli` and `mct-mcp-server`.

#### Option A: quick install script — no Rust toolchain required

This downloads the latest prebuilt release for your operating system. Both scripts print a warning if their install folder isn't already on your PATH, along with the exact line to add.

**macOS / Linux** (installs into `$HOME/.local/bin`; override with `INSTALL_DIR`):

```sh
curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash

# custom folder:
INSTALL_DIR=/usr/local/bin curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash
```

**Windows, PowerShell** (installs into `%LOCALAPPDATA%\mct\bin`; override with `MCT_INSTALL_DIR`):

```powershell
irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex

# custom folder:
$env:MCT_INSTALL_DIR = "C:\tools\mct"; irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex
```

If you'd rather not pipe a script straight into your shell, download the archive for your system by hand from the [GitHub Releases page](https://github.com/Zubiarka8/mini-consumes-tokens/releases) and extract `mct-cli`/`mct-mcp-server` into any folder on your PATH.

#### Option B: build from source

Requires the prerequisites above (Rust via rustup, plus the MSVC C++ build tools on Windows). Same four commands on every operating system:

```sh
git clone https://github.com/Zubiarka8/mini-consumes-tokens.git
cd mini-consumes-tokens
cargo install --path crates/mct-cli
cargo install --path crates/mct-mcp-server
```

#### Verifying either option worked

```sh
mct-cli --help
mct-mcp-server --help
```

If both print usage text, the install worked. If you get "command not found" instead, see [Troubleshooting](#6-troubleshooting-common-problems).

### 3.2 Connecting it to your AI assistant or editor

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

### 3.3 Uninstalling completely

First disconnect it from every tool you connected it to, using that tool's own removal method:

- **Claude Code:** `claude mcp remove mini-consumes-tokens --scope project` (match the `--scope` you used when adding it)
- **Any other tool:** remove the `mini-consumes-tokens` entry from that tool's MCP configuration file (`.mcp.json`, `.vscode/mcp.json`, `~/.codex/config.toml`, or wherever it keeps one — see section 3.2), by hand or through its settings screen.

Then remove the programs and data:

```sh
# If you installed with the quick install script, delete the two binaries from the
# install folder ($HOME/.local/bin, %LOCALAPPDATA%\mct\bin, or your INSTALL_DIR/MCT_INSTALL_DIR):
rm "$HOME/.local/bin/mct-cli" "$HOME/.local/bin/mct-mcp-server"                                     # macOS / Linux
Remove-Item "$env:LOCALAPPDATA\mct\bin\mct-cli.exe","$env:LOCALAPPDATA\mct\bin\mct-mcp-server.exe"  # Windows

# If you built from source:
cargo uninstall mct-cli
cargo uninstall mct-mcp-server

# Either way — delete a project's local index (safe to do any time):
rm -rf .mct-index                        # macOS / Linux
Remove-Item -Recurse -Force .mct-index   # Windows PowerShell
```

If you committed a `.mcp.json` (created by `mcp-register`), remove or edit that file too.

---

## 4. Quick usage guide

```sh
cd path/to/your/project     # 1. move into the project you want indexed
mct-cli --root . init       # 2. build the index (once — connecting an assistant also triggers it)
mct-cli --root . status     # 3. check it worked: files and symbols indexed, last run, any read failures
```

4. **Connect your AI assistant or editor** — see section 3.2 for your specific tool.
5. **Just ask.** Things like *"where is `InvoiceService` defined?"* or *"what would break if I change `calculate_total`?"* — your assistant uses the index automatically instead of reading every file.
6. **Refresh if needed:** `mct-cli --root . reindex --force`. Not usually necessary (the index keeps itself up to date), but it's there if your assistant seems to be missing something after a large batch of changes.
7. **Keep noise out (optional):** `mct-cli --root . ignore-init` writes a starter `.mctignore` — edit it to exclude things like `docs/` or `*.md` from the index, then `reindex --force`.

**What your assistant can now do**, once connected:

- Find everything defined in a file or folder, when you don't know the exact name yet
- Find exactly where a specific function or class is defined
- Find every place something is used, anywhere in the project
- Find what a function depends on, and who depends on it
- Get a one-shot answer to "what would this change affect"
- See a file's overall shape without reading the whole thing
- Check whether the index itself is healthy and up to date

---

## 5. Command reference

Every command, in one place. All accept a global `--root <path>` naming the project to operate on (defaults to the current folder), and `--help` prints this same information in the terminal.

### `mct-cli` — the human-facing command line

| Command | What it does |
|---|---|
| `init` | Builds the index for the first time. Identical to `reindex`; it exists separately to give a fresh checkout an obvious first step. |
| `reindex` | Re-scans the project, re-reading only files that changed since the last scan. |
| `reindex --force` | Same, but re-reads every file regardless of whether it changed — use if you suspect something was missed. |
| `status` | Reports index health: coverage per language, when it last indexed, languages seen with no support yet, files that failed to read. |
| `mcp-register` | Writes (or safely merges into) `.mcp.json` at the project root, so an MCP client can launch the server for this project. Entries for other tools are left untouched. |
| `mcp-register --name <name>` | Same, but lets you choose the connection's name instead of using the project folder's name. |
| `ignore-init` | Writes a starter `.mctignore` at the project root (if one doesn't already exist) — a `.gitignore`-style file to exclude extra files/directories (e.g. `docs/`, `*.md`) from indexing, on top of the built-in exclusions. Edit it, then `reindex --force` to apply. |

### `mct-mcp-server` — the program your AI assistant actually talks to

You don't normally run this by hand; your assistant or editor starts it once connected (section 3.2).

| Command | What it does |
|---|---|
| `mct-mcp-server --root <path>` | Starts the server for that project and waits for an MCP client over stdio. Builds/refreshes the index automatically on startup. |
| `mct-mcp-server --help` | Prints usage information. |

**Response format:** `list_symbols`, `find_symbol`, `find_references`, `find_calls`, `find_callers`, `impact_analysis` and `find_dead_code` accept an optional `format` argument — `"text"` (the default, unchanged) or `"toon"`. `toon` renders the result's uniform rows (symbols, references, calls) as a compact TOON table (one header row of column names, then one row per hit, no repeated labels) instead of this server's usual labelled lines — fewer tokens on a large result, at the cost of `list_symbols`' per-kind grouping. It's opt-in and additive: nothing changes for an existing client that never passes `format`.

---

## 6. Troubleshooting common problems

**"`mct-cli`"/"`mct-mcp-server`: command not found" after installing**
Cargo installs programs into a folder (usually `~/.cargo/bin`, or `%USERPROFILE%\.cargo\bin` on Windows) that isn't always on your PATH. Add that folder to your PATH (the rustup installer offers to do this — rerun it, or add the folder manually), open a new terminal, and try again.

**Windows build fails with a `link.exe` or `cl.exe` error**
The Microsoft C++ build tools from Prerequisites aren't installed. Install the "Desktop development with C++" workload via Visual Studio Build Tools, then rerun the `cargo install` commands.

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

For the internal design, see [ARCHITECTURE.md](ARCHITECTURE.md). For contributing, including how to add a new language, see [CONTRIBUTING.md](CONTRIBUTING.md).
