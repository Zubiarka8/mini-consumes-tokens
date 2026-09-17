# mini-consumes-tokens (ccm)

[![CI](https://img.shields.io/github/actions/workflow/status/zubiarka8/mini-consumes-tokens/ci.yml?branch=main&label=CI)](https://github.com/zubiarka8/mini-consumes-tokens/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
[![Version](https://img.shields.io/badge/version-0.1.0-informational.svg)](#)

**A local map of your code that answers "where is this defined", "what uses it", and "what would break if I change it" — without an AI assistant having to re-read your files over and over.**

---

## 1. What is this and why does it exist?

When an AI coding assistant helps you with a project, it usually has to *open and read* files, or *search through* your whole codebase, just to answer simple questions like "where is this function defined?" or "what else calls this?". That works, but it's slow and it uses up a large chunk of the assistant's available "attention" (technically: tokens) — attention that could otherwise go toward actually solving your problem.

`mini-consumes-tokens` fixes that by doing the reading once, in advance. It scans your project and builds a small local map — an index — of everything meaningful in your code: functions, classes, and how they call and reference each other. That map lives entirely on your own computer, in a single plain file (no cloud service, no external database, nothing leaves your machine).

Once that map exists, your AI assistant can ask it precise questions instead of hunting through files by hand. In our own measurements, this cut the amount of text an assistant needed to read by roughly **70–98%**, depending on the language and the question — for example, finding where something is defined in a Go project used about **97.8% less text** than reading the matching files directly.

A few things that make this different from a typical single-purpose plugin:

- **It understands 16 programming and markup languages at once** (see the list below), including projects that mix several languages together.
- **It isn't tied to one editor or assistant.** It speaks a common protocol called **MCP** (Model Context Protocol) — think of it as a shared language that lets an AI assistant ask a tool for information directly, the same way different apps on your phone can all talk to the same calendar. Any MCP-capable assistant can connect to it.
- **It also works as a plain command-line tool** for anyone who just wants to check the health of the index or rebuild it by hand, with no AI assistant involved at all.

**Supported languages:** Rust, Python, Java, C#, Kotlin, JavaScript/TypeScript, C++, Go, HTML, CSS, XML, XAML, Bash, PowerShell, PHP, and Markdown (headings plus [[WikiLink]]/#tag relations).

---

## 2. Prerequisites

Before installing, make sure you have:

- **A computer running Windows, macOS, or Linux** — it works identically on all three.
- **Git** installed, to download the project.
- **The Rust toolchain**, installed via [rustup](https://rustup.rs) — this project is built and tested with Rust 1.96.0, and rustup will fetch everything needed automatically.
- **Windows only:** the "Desktop development with C++" workload from Visual Studio Build Tools (needed to compile Rust code on Windows). No other Windows-specific setup is required.
- **An AI coding assistant or editor that supports MCP**, if your goal is to connect it to your assistant. This isn't required if you only want to use the command-line tool by itself.

---

## 3. Installation and setup

### 3.1 Installing

This project isn't published to a package manager yet, so today the supported way to install it is to build it from source. It's the same four commands on every operating system:

```sh
git clone https://github.com/zubiarka8/mini-consumes-tokens.git
cd mini-consumes-tokens
cargo install --path crates/ccm-cli
cargo install --path crates/ccm-mcp-server
```

Then check that both programs are ready:

```sh
ccm-cli --help
ccm-mcp-server --help
```

If either command prints usage text, the install worked. If you get a "command not found" error instead, see [Troubleshooting](#6-troubleshooting-common-problems) below.

> This project also has one-line install scripts (`install.sh` for macOS/Linux, `install.ps1` for Windows) and ready-made downloads on its GitHub Releases page. Those exist in the repository already, but building from source above is the currently documented and recommended path — the scripts and downloads aren't covered step-by-step in this guide yet.

### 3.2 Uninstalling completely

Uninstalling has two parts: disconnecting it from whichever tool(s) you connected it to (step 1), and removing the programs and data themselves (step 2). Step 1 is different per tool — do it for every tool you connected, using that tool's own removal method:

- **Claude Code:** `claude mcp remove mini-consumes-tokens --scope project` (match whatever `--scope` you used when you added it — see section 3.3)
- **Cursor, VS Code/Copilot, Codex, or any other tool:** remove the `mini-consumes-tokens` entry from that tool's own MCP configuration file (`.mcp.json`, `.vscode/mcp.json`, `~/.codex/config.toml`, or wherever that specific tool stores it — see section 3.3 for each one's exact location), either by hand or through that tool's own settings screen if it has one.

Once every connection is removed, clean up the shared parts:

```sh
# Remove the two programs
cargo uninstall ccm-cli
cargo uninstall ccm-mcp-server

# Delete the local index for a given project (safe to do any time)
rm -rf .ccm-index                        # macOS / Linux
Remove-Item -Recurse -Force .ccm-index   # Windows PowerShell
```

If you committed a `.mcp.json` file to your project (created automatically when registering the server — see below), remove or edit that file too.

### 3.3 Connecting it to your AI assistant or editor

All of these connect the same underlying program (`ccm-mcp-server`) — only the exact configuration file and format differ per tool. A quick shortcut that works for most of them: run

```sh
ccm-cli --root . mcp-register --name mini-consumes-tokens
```

from inside your project folder. This writes (or safely merges into) a `.mcp.json` file with the right settings, without touching any other tool already configured there.

#### Claude Code

Claude Code has built-in, first-class support. From your project folder, run:

```sh
claude mcp add mini-consumes-tokens --scope project -- ccm-mcp-server --root "<absolute-path-to-your-project>"
```

The `--scope` flag controls who can see this connection:

| Scope | Who it applies to |
|---|---|
| `local` | Just you, on this machine |
| `project` | Everyone on the team, via a shared config file you can commit |
| `user` | All of your projects, everywhere |

**Gotcha:** if Claude Code is already open when you add the server, it won't notice right away — it only loads new connections when it starts up. You'll see it listed as "⏸ Pending approval" if you run `claude mcp list`. Close Claude Code (`exit`) and reopen it (`claude`) to finish connecting.

#### Cursor

Cursor reads the same kind of file `mcp-register` produces. Run the command from the top of this section inside your project, then restart Cursor. If you'd rather add it by hand, open Cursor's MCP settings and point it at the `.mcp.json` file created in your project folder.

#### Codex (OpenAI's Codex CLI)

Codex CLI uses a different file format — a text configuration file (TOML) instead of JSON, usually at `~/.codex/config.toml`. Add a block like this:

```toml
[mcp_servers.mini-consumes-tokens]
command = "ccm-mcp-server"
args = ["--root", "<absolute-path-to-your-project>"]
```

#### GitHub Copilot (VS Code)

VS Code's Copilot integration also uses a slightly different shape than the default: it lives in `.vscode/mcp.json` (for one project) or your VS Code user settings (for every project), and its top-level key is `"servers"` instead of `"mcpServers"`:

```json
{
  "servers": {
    "mini-consumes-tokens": {
      "type": "stdio",
      "command": "ccm-mcp-server",
      "args": ["--root", "${workspaceFolder}"]
    }
  }
}
```

#### Grok Build

Configuration steps for Grok Build could not be verified for this documentation pass. If it supports MCP through a JSON configuration file, try the generic pattern under "Other editors and IDEs" below, or run `ccm-cli --root . mcp-register` to generate a standard configuration block you can adapt to whatever format it expects.

#### Open Code

Configuration steps for Open Code could not be verified for this documentation pass. If it supports MCP through a JSON configuration file, try the generic pattern under "Other editors and IDEs" below, or run `ccm-cli --root . mcp-register` to generate a standard configuration block you can adapt to whatever format it expects.

#### Open Claw

Configuration steps for Open Claw could not be verified for this documentation pass. If it supports MCP through a JSON configuration file, try the generic pattern under "Other editors and IDEs" below, or run `ccm-cli --root . mcp-register` to generate a standard configuration block you can adapt to whatever format it expects.

#### Gemini

Configuration steps for Gemini could not be verified for this documentation pass. If it supports MCP through a JSON configuration file, try the generic pattern under "Other editors and IDEs" below, or run `ccm-cli --root . mcp-register` to generate a standard configuration block you can adapt to whatever format it expects.

#### Other editors and IDEs (JetBrains, and any other MCP-compatible tool)

Most MCP-compatible tools — including JetBrains' AI Assistant — accept the same basic connection details: a program to run (`ccm-mcp-server`), and the project folder to point it at (`--root <path>`), described in that tool's own configuration file or settings screen. Run `ccm-cli --root . mcp-register --name <a-name-you-choose>` once to generate a ready-made block, then paste the relevant part into that tool's own MCP settings, following its documentation for exactly where that goes.

---

## 4. Quick usage guide

1. **Move into the project you want indexed:**
   ```sh
   cd path/to/your/project
   ```
2. **Build the index for the first time:**
   ```sh
   ccm-cli --root . init
   ```
   This only needs to happen once — after this, connecting an AI assistant will trigger the same process automatically if needed.
3. **Check that it worked:**
   ```sh
   ccm-cli --root . status
   ```
   This prints a plain-language health report: how many files and symbols were indexed, when it last ran, and whether anything failed to read.
4. **Connect your AI assistant or editor** — see section 3.3 above for your specific tool.
5. **Just ask.** Once connected, you can ask your assistant things like *"where is `InvoiceService` defined?"* or *"what would break if I change `calculate_total`?"* — it will use the index automatically instead of reading every file in the project.
6. **Refresh the index if needed:**
   ```sh
   ccm-cli --root . reindex --force
   ```
   This isn't usually necessary — the index keeps itself up to date — but it's here if your assistant ever seems to be missing something after a large batch of changes.

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

Every command this project provides, in one place. All of them accept a global `--root <path>` flag telling them which project to operate on (it defaults to the current folder if left out), and `--help` on any of them prints this same information from the terminal.

### `ccm-cli` — the human-facing command line

| Command | What it does |
|---|---|
| `ccm-cli --root . init` | Builds the index for the first time. Behaves identically to `reindex` below — it exists as its own command just to give a fresh checkout an obvious first step. |
| `ccm-cli --root . reindex` | Re-scans the project and updates the index, but only re-reads files that changed since the last scan. |
| `ccm-cli --root . reindex --force` | Same as above, but re-reads every file regardless of whether it changed — use this if you suspect something was missed. |
| `ccm-cli --root . status` | Reports index health: how much of each language is covered, when it was last indexed, any languages seen with no support yet, and any files that failed to read. |
| `ccm-cli --root . mcp-register` | Writes (or safely merges into) a `.mcp.json` file at the project root, so an MCP client can launch the server for this project. Existing entries for other tools are left untouched. |
| `ccm-cli --root . mcp-register --name <name>` | Same as above, but lets you choose the connection's name instead of using the project folder's own name. |
| `ccm-cli --help` | Prints this same command list from the terminal. |

### `ccm-mcp-server` — the program your AI assistant actually talks to

You don't normally run this one by hand — your assistant or editor starts it automatically once it's connected (see section 3.3). It only has one option:

| Command | What it does |
|---|---|
| `ccm-mcp-server --root <path>` | Starts the server for the given project and waits for an MCP client to connect over stdio. Builds/refreshes the index automatically on startup. |
| `ccm-mcp-server --help` | Prints usage information from the terminal. |

---

## 6. Troubleshooting common problems

**"`ccm-cli`" or "`ccm-mcp-server`: command not found" after installing**
Cargo installs programs into a folder (usually `~/.cargo/bin`, or `%USERPROFILE%\.cargo\bin` on Windows) that isn't always automatically added to your system's PATH. Add that folder to your PATH (the rustup installer usually offers to do this — you can rerun it, or add the folder manually), open a new terminal window, and try again.

**Windows build fails with a `link.exe` or `cl.exe` error**
This means the Microsoft C++ build tools mentioned in Prerequisites aren't installed. Install the "Desktop development with C++" workload via Visual Studio Build Tools, then run the `cargo install` commands again.

**Claude Code shows "⏸ Pending approval" and the assistant can't see the tools**
The server was added while Claude Code was already running. Type `exit`, relaunch Claude Code with `claude`, and check again with `claude mcp list`.

**The assistant gives wrong, confused, or empty answers about your code**
This is almost always a path problem — double-check that the `--root` path used when connecting matches your actual project folder, not a subfolder or a different project. Re-register if it doesn't.

**Answers seem out of date after a lot of recent changes**
Run `ccm-cli --root . reindex --force` to force a full refresh, or `ccm-cli --root . status` to see exactly when it last indexed and whether anything failed.

**Nothing happens at all / the assistant says it can't reach the tool**
Most MCP connections talk over "stdio" — the same channel a program's normal input and output use — not a network address. Check that your configuration entry uses `"type": "stdio"` with a local `command`, not a URL or port. Running `ccm-cli --root . mcp-register` again will regenerate a known-good entry.

---

## License

Dual-licensed under either the [MIT license](LICENSE-MIT) or the [Apache License, Version 2.0](LICENSE-APACHE), at your choice.

For details on the internal design, see [ARCHITECTURE.md](ARCHITECTURE.md). For contributing, including how to add a new language, see [CONTRIBUTING.md](CONTRIBUTING.md).
