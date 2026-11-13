# mini-consumes-tokens

<img src="website/public/img/logo.png" alt="mini-consumes-tokens logo" width="120" align="right">

A local MCP server that indexes your code into a symbol graph. Your AI coding agent looks things up there instead of reading whole files, so "where is this defined?", "what calls it?" and "what breaks if I change it?" cost far fewer tokens.

![CI](https://img.shields.io/github/actions/workflow/status/Zubiarka8/mini-consumes-tokens/ci.yml?branch=main&label=CI)
![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)
![Version](https://img.shields.io/badge/version-0.3.1-informational.svg)

[Quick start](#how-to-install) · [Why install it](#why-install-it) · [FAQ](#faq) · [Documentation](#documentation) · [Commands](COMMANDS.md) · [Changelog](CHANGELOG.md) · [Website](https://zubiarka8.github.io/mini-consumes-tokens/)

## How to install

Works on Windows, macOS and Linux. Git and a Rust toolchain ([rustup](https://rustup.rs)) are needed only to build from source. On Windows that also means the "Desktop development with C++" workload from Visual Studio Build Tools.

### 1. Install the programs

The scripts download the latest release for your system and install `mct-cli` and `mct-mcp-server`. No Rust toolchain needed.

```sh
# macOS and Linux (installs into ~/.local/bin; set INSTALL_DIR on bash to change it)
curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash
```

```powershell
# Windows PowerShell (installs into %LOCALAPPDATA%\mct\bin)
irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex
```

To build from source instead, clone the repository and install from the clone:

```sh
git clone https://github.com/Zubiarka8/mini-consumes-tokens.git
cd mini-consumes-tokens
cargo install --locked --path crates/mct-cli
cargo install --locked --path crates/mct-mcp-server
```

Debian, Ubuntu, Fedora and openSUSE packages, updates and other options are in [docs/INSTALL.md](docs/INSTALL.md).

### 2. Index your project

Run this inside the project you want indexed, not inside the clone.

```sh
cd /path/to/your/project
mct-cli --root . init      # builds .mct-index/ in your project
mct-cli --root . status    # how many files and symbols were indexed
```

### 3. Connect your agent

```sh
mct-cli --root . mcp-register --name mini-consumes-tokens
```

This writes or merges a `.mcp.json`, which most MCP clients read. Restart your agent and ask where a function is defined. Per-client steps and an instructions block for your agent's rules are in [Connect your agent or editor](docs/INSTALL.md#connect-your-agent-or-editor).

To try a tool without an agent, run `mct-cli --root . call --tool get_indexing_status`. To open the dashboard, run `mct-cli --root . web`.

## Why install it

- Your agent spends fewer tokens. On our test fixtures, a context pack used about 85% fewer tokens than chaining the lookups by hand, and `batch` used 37 to 42% fewer than separate calls. Results depend on the task and the repository. The measured cases, including the ones where it costs more, are in [`benchmarks/`](benchmarks/).
- It gets short answers instead of files: a symbol, its references, callers and callees, or a ready-made context pack for the code you are about to change.
- One server works with any assistant or editor that supports MCP. It also runs as a plain command line tool.
- One index covers many languages, and projects that mix several of them are fine. What is supported for each language and library is in the [checklists](docs/02-crates/parsers/checklists/by-language.md).
- Your code stays on your machine. The index is one SQLite file in `.mct-index/`, with no cloud service and no telemetry. Delete it any time and it is rebuilt from your source.
- The index stays current. While a server or the dashboard runs, saved files are reindexed in the background and only the changed files are parsed again.
- The same index feeds a local dashboard with a code atlas, duplicate-code and dead-code review, a dependency graph with cycle detection and an Obsidian notes graph. See the [usage guide](docs/USAGE.md#the-local-dashboard).

## FAQ

### Which AI coding agents work with it?

Any agent or editor that supports MCP. Only the place where the client keeps its MCP settings differs. The steps for the common ones are in [docs/INSTALL.md](docs/INSTALL.md#connect-your-agent-or-editor).

### Does my code leave my computer?

No. The index is built and read locally. The optional `semantic` build downloads an embedding model once, then runs it locally.

### Can I use it without an AI agent?

Yes. `mct-cli` handles status checks, dead-code and duplicate reports, and the dashboard on its own.

### What if I delete the index?

`init`, `reindex` or the next server start rebuilds it. Nothing is lost.

### Something does not work.

See [Troubleshooting](docs/INSTALL.md#troubleshooting).

## Documentation

| Document | What it covers |
| --- | --- |
| [Installation](docs/INSTALL.md) | Install options, connecting each client, semantic search, updating, uninstalling, troubleshooting |
| [Usage guide](docs/USAGE.md) | The first five minutes, ignore rules, the dashboard, Obsidian, the dependency graph |
| [Command reference](COMMANDS.md) | Every command, flag and MCP tool parameter |
| [Architecture](ARCHITECTURE.md) | How the crates fit together |
| [Library and framework support](docs/01-architecture/library-support.md) | What is indexed for each framework |
| [Checklist by language](docs/02-crates/parsers/checklists/by-language.md) | One checklist for each language, with done, partial and missing marks |
| [Checklist by feature](docs/02-crates/parsers/checklists/by-feature.md) | Each feature (functions, types, relations, search) across all languages |
| [Checklist by library](docs/02-crates/parsers/checklists/by-library.md) | Framework and package support, one checklist per library |
| [Checklist by feature and library](docs/02-crates/parsers/checklists/by-feature-library.md) | The feature and library grid in one place |
| [Feature definitions](docs/02-crates/parsers/features.md) | What "done" means for each feature ID |
| [Symbol coverage](docs/02-crates/parsers/symbol-coverage.md) | The symbols and relations each language produces |
| [Language gaps](docs/02-crates/parsers/language-gaps.md) | Known gaps per language |
| [Dashboard guide](docs/01-architecture/web-dashboard.md) | Design notes for `mct-cli web` |
| [Benchmarks](benchmarks/) | Token measurements and how they were taken |
| [Roadmap](ROADMAP.md) | What is planned |
| [Changelog](CHANGELOG.md) | What changed in each release, with upgrade steps |
| [Contributing](CONTRIBUTING.md) | Clone, build, test, open a pull request, add a language |
| [Project rules for agents](rules.md) | The single rules file that `AGENTS.md` and `CLAUDE.md` point to |
| [Security](SECURITY.md) | How to report a vulnerability privately |
| [Third-party notices](THIRD_PARTY_NOTICES.md) | Licenses of bundled components |

mini-consumes-tokens was started in September 2026 by Arkaitz Zubizarreta (zubiarka8). Initial versions were released under MIT or Apache License 2.0; the current version is released under the [Apache License 2.0](LICENSE).

A star on [GitHub](https://github.com/Zubiarka8/mini-consumes-tokens) helps other people find it.
