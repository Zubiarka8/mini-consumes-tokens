# Repository rules

## Table of contents

- [Project coordination](#project-coordination)
- [Contributor workflow](#contributor-workflow)
- [Choosing an AI model](#choosing-an-ai-model)
  - [Model routing](#model-routing)
  - [Optional cross-provider worker handoff](#optional-cross-provider-worker-handoff)
  - [Handoff checkpoints](#handoff-checkpoints)
  - [Context and token management](#context-and-token-management)
  - [Usage-limit recovery](#usage-limit-recovery)
  - [Command and capability selection](#command-and-capability-selection)
- [What this is](#what-this-is)
- [Dogfooding: how to explore this repository's source code](#dogfooding-how-to-explore-this-repositorys-source-code)
- [Commands](#commands)
- [Architecture](#architecture)

Shared rules for Codex, Claude Code, and any agent working in this repository. This file is the single source of truth for project instructions; `AGENTS.md` and `CLAUDE.md` only point here.

## Project coordination

Read [PROJECT_MANAGER.md](PROJECT_MANAGER.md) at the start of a substantial work session. It defines task ownership, branch isolation, durable checkpoints, review, integration, and recovery. This file remains authoritative for architecture, technical restrictions, model selection, effort, billing, and client commands. Use only project-local skills unless the user explicitly authorizes another scope.

Write all repository content in English: file and branch names, documentation, code, comments, task records, commit messages, PRs, and issues. Preserve required external identifiers and literal fixture data when translation would change behavior.

Whenever a project MCP tool reports an error (including connection failures, invalid tool arguments, indexing/parse failures, or query failures), notify the user and record it in [errors/REGISTRY.md](errors/REGISTRY.md), following [errors/README.md](errors/README.md). Record the date, tool/operation, relevant arguments without secrets, exact diagnostic, impact, and verification or next action. Update an existing entry for the same problem and append the observation or status change to [errors/HISTORY.md](errors/HISTORY.md), including errors corrected during the same session. Separate intentional malformed-fixture rejection from unexpected failures and caller mistakes from server defects; retain resolved entries and require verification before closing them. When investigating documented parser limitations, maintain these same records.

## Contributor workflow

Contribute using your preferred editor or assistant. No particular AI provider, model, paid subscription, or multiagent setup is required. Use the tools and permissions available in your environment while following the engineering and source-exploration rules below.

Keep changes scoped and reviewable. Preserve unexplained local work, record acceptance criteria, verify relevant behavior when authorized, and report any checks that could not be completed. If multiple writers are used, give each an exclusive branch/worktree and coordinate overlapping changes. Do not add personal model selections, subscription limits, or private agent state to shared documentation.

Write repository content in English: file and branch names, documentation, code, comments, commit messages, PRs, and issues. Preserve required external identifiers and literal fixture data when translation would change behavior.

If a local LOCAL_INSTRUCTIONS.md file exists, read it as additional instructions for that checkout. Its absence is normal and must not block contributions. Local instructions do not override the user's instructions, client permissions, or shared engineering constraints.

## Choosing an AI model

Project model policy, binding for every contributor and client:

- **Prohibited: GPT-6 Astra** (`gpt-6-astra`), in any variant and at any reasoning effort (High, Medium, and so on).
- **Prohibited: Claude Fable** (`claude-fable-5-1`), at any effort level. Do not use Claude Code's `best` alias either, because it can resolve to Fable.
- **Allowed:** every other GPT and Claude model, at any reasoning effort the client exposes.

Within that policy, choose the model and effort that meet the task's quality bar at the lowest reasonable usage. Model choice never replaces tests or review.

### Model routing

Use one provider by default. The preferred cross-provider arrangement, only when the user asks for it and both clients are available, is ChatGPT/Codex as coordinator and Claude Code as one bounded worker. Do not duplicate a task across providers or spawn parallel agents just because the tools are installed.

| Task shape | ChatGPT/Codex options | Claude Code options | Starting effort |
|---|---|---|---|
| Bounded, repeatable work: summarize, classify, mechanical edits, or a well-specified small fix | GPT-6 Luna or GPT-5.6 Luna, if available | Sonnet; Haiku for simple extraction or classification | The lowest effort the model exposes |
| Ordinary implementation, multi-file fixes, planning, and integration | GPT-5.6 Terra or GPT-6.1 Sol, if available | Sonnet | Medium (default) |
| Difficult architecture, root-cause analysis, or high-risk tradeoffs | GPT-6.1 Sol or GPT-5.6 Sol, if available | Opus | Medium (default); raise only when verification fails |

These names are examples of model families, not guarantees of availability. Check the client's model picker for what is available. If a listed model is unavailable, use a currently available, lower-cost allowed model, never a prohibited one. Parallel agents and ultra or high-effort modes add usage; require a clear task benefit before choosing them.

OpenAI's current guidance describes Luna as efficient for focused tasks, Terra as balanced for everyday work, and Sol as suited to complex coding; Claude's model aliases and exact versions vary by provider. Treat the catalog and client picker as authoritative: [OpenAI model catalog](https://learn.chatgpt.com/docs/models?surface=app), [OpenAI model selection](https://learn.chatgpt.com/docs/model-selection), [OpenAI pricing and usage](https://learn.chatgpt.com/docs/pricing), [Claude Code model configuration](https://code.claude.com/docs/en/model-config), [Claude Code cost guidance](https://code.claude.com/docs/en/costs), and [Claude Code interactive usage-limit behavior](https://code.claude.com/docs/en/interactive-mode).

### Optional cross-provider worker handoff

Use this workflow only when the user explicitly asks to coordinate providers and both tools are available. It is optional: contributors using Cursor or another single client continue with that client and follow the repository's shared engineering rules. Do not start another agent by default or repeat the same task in two providers.

For the requested ChatGPT/Codex-led, Claude Code-worker pattern:

1. The lead turns the request into one bounded worker brief: goal, in-scope paths, acceptance criteria, applicable checks, and what to return. Send only relevant context and links; do not paste the full conversation or large files when paths/tools can supply them.
2. Confirm the `claude` CLI is installed and authenticated before dispatch. If not, report the limitation and continue only if the user wants a single-provider fallback.
3. Start one Claude Code non-interactive worker in the foreground with `claude -p --output-format json` and request a structured final report (`completed`, `blocked`, or `failed`; summary; changed paths; checks and results; follow-up needed). The process result is the completion message to the lead. Wait for it before dependent integration. For API-billed runs, use a user-approved `--max-budget-usd` cap and a reasonable `--max-turns` limit; for subscription use, check Claude Code's `/usage` command and the account's usage limits. Never invent a dollar cap or enable paid overage without the user's authorization.
4. Give the worker its own branch/worktree. Do not have the lead and worker modify the same checkout concurrently. Add workers only for independent, disjoint tasks that justify the extra context and usage; give each its own worktree. Do not use Claude Agent Teams for cross-provider messaging: they coordinate Claude sessions, not a separate ChatGPT/Codex session, and each teammate adds its own context and usage.
5. The lead reviews the worker's diff and report, runs the required integration checks, and owns integration, pushes, and PRs unless the user explicitly delegates those actions; workers may create scoped commits when authorized in their task brief. If the report is blocked, send one specific follow-up and wait for its result. Do not claim success while work is blocked or checks failed.

`claude -p` is a one-shot process, so its returned report is the reliable handoff; it does not send messages to an unrelated interactive Claude session. For a continuing Claude CLI session, address follow-up prompts to its explicit session rather than assuming cross-client messaging.

### Handoff checkpoints

Use the durable records and recovery protocol in [PROJECT_MANAGER.md](PROJECT_MANAGER.md#checkpoints-and-handoffs). The coordinator maintains `project-management/`; workers checkpoint their assigned task in their own branch and return the commit, report, and remaining work. Ignored `target/agent-handoffs/` files may hold temporary process output, but must never be the only copy of essential project state. A hard stop may prevent a final checkpoint; recover from the latest persisted record and inspect Git before continuing.

The coordinator may wait for a bounded worker without sending periodic prompts. Claude Code's automatic subscription-reset waiting is limited to supported interactive sessions; background sessions and `claude -p` runs cannot use that menu. A worker that can still report must return `blocked` when it cannot continue. Never assume an API-billed process can wait for a subscription reset. Providers cannot message unrelated chats without an exposed, authorized handoff route.

### Context and token management

- Give each worker only the task brief and necessary repository context. Prefer targeted symbol/context tools, small diffs, and concise test output over dumping whole files or transcripts.
- A context window is per conversation/session; a provider's usage allowance or rate limit is separate. A context warning can be handled by compacting or continuing from a checkpoint. A usage limit cannot be fixed by `/compact`, clearing a chat, or opening another surface on the same account.
- In Claude Code, use Claude Code `/context` for context consumers and Claude Code `/usage` for account usage. Let automatic compaction handle a near-full context window, or use Claude Code `/compact` to continue the same task. The summary must retain the objective, constraints, decisions, changed paths, verification, next action, and open questions. Use Claude Code `/clear` only when starting an unrelated task or after the current task is complete; it starts a new conversation and does not replenish the account allowance. A finished `claude -p` invocation exits and needs no `/clear`.
- In ChatGPT/Codex, use ChatGPT/Codex `/compact` to continue the same task with a long history; use a new task/chat for unrelated work. Preserve the handoff checkpoint before changing sessions. Do not clear or abandon context while a worker result is pending. `/compact` is available in both Claude Code and ChatGPT/Codex, but each command only affects its own conversation.
- Do not compact at fixed intervals. Check context and usage on long tasks; compact only when it helps continue useful work. Higher effort, larger prompts, repeated reviews, parallel agents, and long-running workers can increase usage.
- Do not use JSON for data that this project's agents, scripts, or reports produce or consume. Use TOON (Token-Oriented Object Notation) or another compact, token-efficient format, such as the `format: toon` option of the MCP tools. JSON is allowed only where an external protocol, tool, or file format requires it (for example MCP JSON-RPC messages, `claude -p --output-format json`, `.mcp.json`, `skills-lock.json`, `package.json`). Do not add new JSON output to project scripts.

### Usage-limit recovery

When a request is blocked, first read the client's message and usage/status display to identify whether it is a context warning, a model-specific limit, an account/workspace limit, or a billing/spend cap. Do not infer one from another. In Claude Code, `Claude Code /rate-limit-options` exposes interactive limit choices when that client offers them; supported interactive subscription sessions may wait for automatic continuation after reset. Background sessions and `claude -p` runs cannot use that wait menu. Waiting does not make exhausted shared account usage available sooner.

| Situation | Recovery |
|---|---|
| Context is nearly full, but requests still work | Write/update the handoff checkpoint, then compact the current session. Continue from that summary; do not start a duplicate task. |
| A model-specific limit is reached | Check the provider's exact reset/limit message. If the provider exposes another already-authorized model outside that model-family limit, switch only if it fits this routing policy. Keep the same task checkpoint and worktree. |
| An account, subscription, or organization limit is exhausted | Check the provider's usage display and reset time. Preserve the current diff and checkpoint; wait for reset. ChatGPT Work and Codex share usage, so do not treat them as separate allowances. Claude Code distinguishes model-family limits from subscription session/weekly limits; switching model only helps for a model-family limit. Do not assume changing model, chat, app, or provider sign-in will bypass a shared limit. |
| A billing cap or paid-credit prompt appears | Stop before accepting charges, enabling credits/overage, using an API key, or increasing an organization cap. Continue only after explicit user authorization. |
| The error type is unclear or the session/process ended without a report | Preserve all work. Record `blocked` or `failed`, the exact error, worktree/branch, completed changes, checks, and the next safe action in the checkpoint. The coordinator inspects the diff before retrying. |

If ChatGPT/Codex is coordinating Claude Code and ChatGPT/Codex reaches a limit while Claude is working, Claude may finish only its assigned worker task and write its checkpoint/report. The coordinator resumes after its own context is compacted or its usage resets, then inspects and integrates the report. If Claude reaches its limit while waiting for ChatGPT/Codex, leave Claude's session and checkpoint intact; the coordinator can continue only after Claude is available again, or through a fallback already authorized by the user. Changing the coordinator role requires explicit user authorization; follow the recovery protocol in `PROJECT_MANAGER.md`. Neither side should poll with repeated model requests or start unapproved paid usage just to send a “task finished” message.

### Command and capability selection

Project instructions do not grant access to commands, tools, models, MCP servers, or permissions. The active client, version, plan, installed extensions, workspace policy, and approval settings determine what is available. Before relying on a command, inspect the current client's command menu (`/`) or CLI help and the available tool list. ChatGPT web, the ChatGPT desktop app/Codex app, Codex CLI, Cursor, and Claude Code do not share one command namespace. In the table below, each column names the client that owns its commands; a repeated spelling (for example, `/compact`, `/model`, or `/plan`) is a separate client-specific command, not a shared command that switches clients. Use only commands and tools exposed by the current environment; never claim a command ran when it was only suggested.

Think through the relevant command categories for both the lead and worker, then use only the commands that directly help with the current task:

| Need | ChatGPT desktop / Codex app (except where marked CLI) | Claude Code |
|---|---|---|
| Select model and effort | ChatGPT/Codex `/model`; ChatGPT/Codex `/reasoning`; confirm with ChatGPT/Codex `/status` | Claude Code `/model`; Claude Code `/effort`; confirm with Claude Code `/effort status` or the session header |
| Plan a substantial task | ChatGPT/Codex `/plan`; use ChatGPT/Codex `/goal` only for a persistent multi-session objective | Claude Code `/plan` |
| Isolate concurrent changes | ChatGPT/Codex `/worktree` or ChatGPT/Codex `/fork` into a worktree | Claude Code `/fork` for a separate background session; use a separate Git worktree for concurrent code edits |
| Inspect context and usage | ChatGPT/Codex `/status` for session/context; Codex CLI `/usage` for account limits | Claude Code `/context` and Claude Code `/usage` |
| Manage long context | ChatGPT/Codex `/compact` to continue the same task; start a new chat for unrelated work | Claude Code `/compact` to continue the same task; Claude Code `/clear` between unrelated tasks |
| Check integrations or review work | ChatGPT/Codex `/mcp` to inspect connected servers; ChatGPT/Codex `/review` for a code review | Claude Code `/mcp` for MCP controls; Claude Code `/diff` to inspect edits; Claude Code `/review` or Claude Code `/code-review` for a review |
| Track active work | ChatGPT/Codex `/goal` for a durable objective; Codex CLI `/ps` for background terminal processes | Claude Code `/tasks` for current tasks/background work; Claude Code `/resume` to return to a session |
| Recover from a usage limit | ChatGPT/Codex `/status`; Codex CLI `/usage` for account limits and reset details | Claude Code `/usage`; Claude Code `/rate-limit-options` in supported interactive sessions |

These are candidates, not a checklist to execute on every request. Prefer one lead and one worker; avoid Claude Code `/batch`, agent teams, broad client-specific `/fork` fan-out, or repeated second-model reviews unless the task is divisible and the expected quality or elapsed-time benefit warrants their added context and usage. For a one-shot Claude Code worker, use CLI options such as `claude -p --model <available-model> --effort <available-level> --output-format json`; use `--max-budget-usd` and `--max-turns` only when supported and appropriate for the billing mode. A fresh `claude -p` process needs no Claude Code `/clear` or `/compact`.

Commands that broaden permissions or cause external side effects are not convenience switches. Keep the current least-privilege boundary; do not use bypass-approval/sandbox flags to make a handoff work. Do not relax permissions, post review comments, push, or start an auto-fix workflow unless the user authorized that action. For exact command availability and behavior, consult the [ChatGPT/Codex app slash command reference](https://learn.chatgpt.com/docs/reference/slash-commands), [Codex CLI commands](https://learn.chatgpt.com/docs/developer-commands), [Claude Code commands](https://code.claude.com/docs/en/commands), and [Claude Code MCP commands](https://code.claude.com/docs/en/mcp).

## What this is

An MCP server (`mct-mcp-server`) and CLI (`mct-cli`) that index a code repository — any supported language, identically on any OS — via tree-sitter AST parsing into a symbol graph stored in SQLite. The index is published as MCP tools so an agent gets precise project context instead of reading whole files with `Read`/`Grep`/`Glob`.

## Dogfooding: how to explore this repository's source code

**The rule:** for *exploration/lookup* questions about this repo's own source, the only permitted tools are this project's own software — the `mct-mcp-server` MCP tools and `mct-cli` (`crates/mct-cli`) subcommands. Nothing else: no `Grep`, `Read`, `Bash cat|ls|grep`, `python`, or any other method outside `crates/`. This project's entire point is that an agent queries a symbol graph instead of grepping whole files; doing it the old way here undercuts the thing being built.

**Scope:** exploration/lookup means "what's defined in X", "where is Y", "who calls Z", "what does Z call", "what would break if I change Z", "what symbols does file/crate X have". It does **not** cover reading a file as a precondition for editing it (`Edit` requires a prior `Read`), or general code-modification work — `Read`/`Grep`/`Edit` remain normal there.

| Question | Tool |
|---|---|
| What symbols does a file/crate have, without knowing a name yet? | `list_symbols` — the discovery step; a file path matches exactly, a directory/crate path (no extension) matches as a prefix, `kind`/`language` narrow it |
| What is one file's overall shape (top-level declarations, bodies collapsed)? | `get_file_skeleton` — single file; use `list_symbols` first if you don't know which file |
| What does an unfamiliar file/directory/crate/project look like as a whole? | `get_project_overview` — capped hierarchical digest (modules, key symbols, top callers) in one call; coarser, so switch to the two above once you know the file |
| What's the directory/file layout, before you know which file or crate to look at? | `get_file_tree` — plain directory tree (no symbol data), depth-limited, pruned of the same noise dirs (`target`, `node_modules`, `.git`) reindexing skips |
| Where is X defined? | `find_symbol` |
| Exact name unknown — words or a partial identifier in any style (`parse request`, `parseReq`, `http server`)? | `search_symbols` — BM25-ranked FTS5 match over each name split at camelCase/snake_case/kebab-case/acronym boundaries, exact name always first; default `limit` 10 (max 100), `offset`, optional `snippet_lines`. Not exhaustive — graph tools stay the source of truth for relations |
| Only know what the code *does*, not words of its name (`load settings from disk`)? | `hybrid_search` — `search_symbols` fused with local-embedding similarity by weighted Reciprocal Rank Fusion; `alpha` 0 = lexical only … 1 = semantic only (omitted → routed by query shape: 0.1 identifier / 0.75 prose / 0.5 otherwise), `top_k` 10 (max 100), `offset`, `snippet_lines`. Its lexical side also expands dev-verb synonyms and resolves `Type::member` / `module.fn` qualifiers. A `"double-quoted"` query is an exact phrase: lexical only whatever `alpha`, its words consecutive and in order (no prefix/synonym/any-word matching), a literal name match boosted to #1, plus a second section listing prose string literals (error/log messages) holding the phrase as `path:line in <kind> <name> "text"` — case/diacritic-insensitive, currently extracted by `mct-lang-rust` (string literals) and `mct-lang-md` (paragraphs and table rows) only. Needs a `--features semantic` build of `mct-mcp-server` (`bge-small-en-v1.5` by default, `MCT_EMBEDDING_MODEL` to switch; downloaded on first use into `.mct-index/models`); otherwise it returns the lexical ranking and says so on its first line |
| Who calls X directly? | `find_callers` |
| What does X call? | `find_calls` |
| Every reference to X (calls, imports, extends/implements) | `find_references` |
| Full blast radius before changing/removing X | `impact_analysis` |
| About to work on X — its definition, doc comment and source plus callers/callees/dependencies/tests, without reading whole files? | `build_context_pack` — one call: X's definition(s) with the comment/attribute block above and the first `source_lines` (40, max 200) lines, then every related symbol **once** (roles merged, e.g. `caller,test`) with its location and one-line signature, file-level `use`/`import`ers and unresolved callee names as footers, ambiguous callees in a separate uncertain section (never picked); `depth` walks both ways, `path`/`language` pick the definition of an ambiguous name, `limit` 30, `format` `toon`. ~85% fewer tokens than find_symbol + find_calls + find_callers + impact_analysis + reading the files (`crates/mct-mcp-server/tests/context_pack.rs`) |
| Candidate unused functions/classes/structs/enums/traits/interfaces/type-aliases (heuristic — zero indexed references, not true visibility) | `find_dead_code` |
| Several lookups whose queries you already know (e.g. `find_symbol` + `find_callers` + `get_file_skeleton`)? | `batch` — `queries: [{tool, args}]`, up to 25, any read-only tool (not `reindex`, not a nested `batch`); each result under its own `[n] tool` header, a failing sub-query reported inline without failing the rest. Drops the per-call envelope (≥20% fewer tokens, measured in `crates/mct-mcp-server/tests/batch.rs`) |
| Is the index stale / healthy? | `get_indexing_status`, and `reindex` only if it looks stale |

`find_references`/`find_calls`/`find_callers`/`impact_analysis` also take `depth` (multi-hop BFS beyond the direct hit, default 1, clamped to 32) and `offset` (pagination past `limit`) — reach for `depth` before manually chaining calls to walk the relation graph further out.

**Markdown literal search:** use `hybrid_search` with a double-quoted phrase;
`search_symbols` only searches symbols, even with quotes. Paragraphs and pipe-table
headers/body rows retain their Markdown source text with whitespace collapsed.
Headings remain symbols; frontmatter and code blocks are excluded from literals.
The shared prose filter requires at least eight characters and two whitespace-separated
words containing letters, and rejects suspected secrets. It keeps the first 256
characters of each block and at most 500 distinct literals per file (the first
occurrence supplies the line). Identifier-only or URL-only rows can therefore be
excluded. Files without a supported extension are not treated as Markdown.
After upgrading an existing index to Markdown literal extraction, run
`mct-cli --root <project> reindex --force` once: unchanged files are otherwise
skipped. This parser change requires no schema migration. Exact-phrase literal
search also works without the optional semantic feature.

**Progressive tool discovery:** `discover_tool_categories` lists every registered tool's name and one-line purpose grouped by category, with no input schemas — a cheap first call for a client that wants to defer the schema payload. `get_tool_schema` then returns one named tool's full input schema and description on demand. These are additive: the standard MCP `tools/list` handshake still returns every tool's full schema up front as usual, so ordinary MCP clients are unaffected — `discover_tool_categories`/`get_tool_schema` only help a client built to use them instead.

**`.mct-index/index.sqlite3` may only be touched through this project's own software** — the MCP tools above or `mct-cli` subcommands. Never an external tool: no `sqlite3` CLI, no `python3`/`sqlite3` module, no DB browser. If this codebase doesn't ship it, it doesn't get to touch the index.

**When `Read`/`Grep` are still correct:** the index captures symbols and relations, not comment/doc-string prose — checking whether a doc-comment's *wording* still matches something requires `Grep`/`Read`. `Glob` for listing/finding files by pattern is fine (it's not a symbol lookup).

**If a question falls outside every tool's coverage:** say so explicitly and ask the user whether to allow `Read`/`Grep` for that specific instance. Never fall back automatically, and never treat one approval as a standing exception.

**If the MCP tools are absent from the deferred-tools list** (not shown as "failed to connect" — simply missing): the session started before `.mcp.json` was written/updated, or before `mct-mcp-server` was built. Tell the user to restart Codex or Claude Code / reconnect MCP rather than treating direct SQLite queries as the steady state — direct SQLite access is a same-session fallback only, and even then only through this project's own tooling.

**Enforcement hooks:** `.codex/hooks.json` wires the hooks in `internal/claude-hooks/` (see its `README.md`): `Bash grep|cat|find` over `crates/` and any `sqlite3` / `.mct-index/index.sqlite3` access are denied, edited `.rs` files are rustfmt'd, and Stop runs `scripts/unix/check.sh --only clippy`. Claude Code can enable its optional `PreToolUse` guard from `internal/claude-hooks/dogfood_mcp_guard.py`, as documented in that README.

**If the MCP server shows `CONNECTION_CLOSED`** (a genuine connect failure, not "absent from the list" above): this usually means `.mct-index/index.sqlite3` was migrated by a branch with more `M::up` entries in `crates/mct-index/src/schema.rs` than the branch currently checked out — the checked-out binary sees a migration number "from the future" and aborts on startup with `migration error: Attempt to migrate a database with a migration number that is too high`. Confirm with `scripts/unix/mcp-smoke.sh` (it runs the installed server binary, feeds it a JSON-RPC `initialize` over stdin and prints the startup error). Fix: `scripts/unix/reinstall.sh --reindex`, which reinstalls the binaries from the checked-out branch and rebuilds `.mct-index/index.sqlite3` for its schema (by hand: delete the file, then `cargo run -p mct-cli -- --root . init` — the index is fully derived from source, safe to delete), then reconnect the MCP client. An installed binary older than the index fails the same way, e.g. after running a newer `target/debug` server against this repo — `scripts/unix/reinstall.sh` covers that too. This will recur any time you switch between branches with a different migration count without reindexing first — `scripts/unix/install-hooks.sh` installs a `post-checkout` hook that does that rebuild automatically.

## Commands

```sh
cargo build --workspace --all-targets                              # build everything
cargo test --workspace                                              # run all 222+ tests
cargo test -p mct-lang-go                                            # run one crate's tests
cargo test -p mct-lang-go idiomatic_syntax                           # run one test by name
cargo clippy --workspace --all-targets --all-features -- -D warnings # lint (CI-gating, zero warnings)
cargo fmt --all                                                      # format (CI runs `cargo fmt --all --check`)
# NOTE: the line above is the everyday lint command; the actual CI job (.github/workflows/ci.yml)
# additionally denies unwrap/expect/panic project-wide:
#   cargo clippy --workspace --all-targets --all-features -- -D warnings \
#     -D clippy::unwrap_used -D clippy::expect_used -D clippy::panic
# Any src/tests/examples file that legitimately uses those (test/bench code, never a path
# processing repo-input content) needs its own #![allow(clippy::unwrap_used, clippy::expect_used,
# clippy::panic)] — see crates/mct-mcp-server/tests/*.rs or the inline `mod *_tests` blocks in
# crates/mct-mcp-server/src/format.rs for the existing convention.

cargo run -p mct-cli -- --root . init                                # first index of a project
cargo run -p mct-cli -- --root . status                              # coverage / health report
cargo run -p mct-cli -- --root . reindex --force
cargo run -p mct-cli -- --root . mcp-register [--name N]             # write/merge .mcp.json for this project
cargo run -p mct-mcp-server -- --root <project>                      # run the MCP server over stdio
cargo run -p mct-eval [-- --verbose]                                 # quality suite vs. baseline (accuracy/MRR/success/latency/tokens)
cargo run -p mct-eval -- --write-baseline                            # refresh crates/mct-eval/baseline.json after an intended change
```

**Prefer `scripts/unix/` (macOS/Linux) or `scripts/windows/` (PowerShell) over the raw commands above** — same work, a few lines of summary instead of hundreds of lines of cargo output (full logs in `target/script-logs/`; see `scripts/README.md`). Every script exists in both, same name and flags — on Windows it's `scripts\windows\<name>.ps1` (e.g. `scripts\windows\check.ps1 -p mct-core`):

```sh
scripts/unix/check.sh [-p <crate>]                 # verify before committing: fmt + tests + CI clippy + mct-eval, one line per step; summary kept for pr-body.sh
scripts/unix/branch-worktree.sh <branch> [base]    # a new branch in its own worktree, from origin/main; use one per concurrent change
scripts/unix/pr-body.sh [file]                     # PR description draft: Verification from the last check.sh, token figures from token-report.sh; warns if stale
scripts/unix/reinstall.sh [--reindex]              # after switching branches / merging a server change: install binaries, fix index schema, smoke-test; then inspect/reconnect MCP from the active client's controls (ChatGPT/Codex: /mcp; Claude Code: /mcp)
scripts/unix/mcp-smoke.sh [--dev] [--expect T]     # CONNECTION_CLOSED? prints the server's real startup error; --expect checks tool T is listed
scripts/unix/new-tool-check.sh <tool>              # adding an MCP tool: which of the ~10 places still don't mention it, then the catalog tests
scripts/unix/new-language-check.sh <suffix>        # adding crates/mct-lang-<suffix>: CONTRIBUTING.md's checklist, what's missing, then its tests
scripts/unix/token-report.sh [--markdown F]        # every token measurement (per language, formats, composite tools, catalog) in one screen
scripts/unix/corpus-report.sh <lang> [--bless] [--update-progress]  # issue #74 corpus: tests + totals + heuristic bug checks, instead of reading expected.snap
scripts/unix/ci-failures.sh [<run-id>]             # why a CI run failed: failing jobs + first error lines (default: latest failed run on this branch)
scripts/unix/pr-status.sh <number>                 # a PR in a few lines: state, review, checks passed/failed/running, latest comment
scripts/unix/corpus-progress-check.sh              # issue #74 table vs GitHub: In PR / Done rows whose PR state is out of date (needs gh)
scripts/unix/parse-probe.sh <files…>               # parse files without indexing: counts or first syntax error + its line
scripts/unix/install-hooks.sh                      # once per clone: post-checkout hook that rebuilds the index when a branch switch breaks it
```

CI (`.github/workflows/ci.yml`) runs `build-test` (build+test+clippy, which includes the `mct-eval` quality gate) on Linux/macOS/Windows, `cargo-audit` on Linux, and `fuzz-smoke` (30s `cargo-fuzz` runs per language crate) on Linux/macOS only — fuzzing is deliberately excluded on Windows (ASan DLL/MSVC-sancov issues, see the comment above that job). `.github/workflows/quality-report.yml` re-runs the `mct-eval` suite monthly and publishes the report (see `benchmarks/quality-eval.md`).

**Windows:** no system deps beyond the standard Rust MSVC toolchain (`link.exe`/`cl.exe` via VS Build Tools' "Desktop development with C++", which `rustup` already prompts for). `git2` builds with `default-features = false` (no ssh/https transport, no OpenSSL) — this project only reads local repo state for blob-hash change detection, never clones/fetches.

## Architecture

**The plugin boundary is the whole design.** `mct-core` defines `LanguageParser` (`language_id()`, `file_extensions()`, `parse()`) and `LanguageRegistry` (extension → parser lookup), and knows nothing about tree-sitter, SQLite, or MCP. Each `crates/mct-lang-*` crate implements that trait for one language via its own tree-sitter grammar. `mct-index` and `mct-mcp-server` never match on language names or extensions directly — everything routes through the registry.

```
mct-core     LanguageParser trait, SymbolRecord/SymbolRelation model, LanguageRegistry
mct-index    SQLite schema/migrations (a single schema for every language — a `language`
             column on `files`, not per-language tables), reindex orchestration, queries.
             Knows no language's grammar.
mct-lang-*   One crate per language, each a LanguageParser impl over its tree-sitter grammar
mct-mcp-server  MCP tools over stdio (rmcp): list_symbols/find_symbol/search_symbols/hybrid_search/find_references/
                find_calls/find_callers/impact_analysis/build_context_pack/find_dead_code/reindex/
                get_indexing_status/get_file_skeleton/get_project_overview/get_file_tree/batch
mct-cli      init/reindex/status/mcp-register subcommands for manual/scripted use
mct-eval     Quality evaluation of the MCP tools against a fixture suite and a checked-in
             baseline (issue #21); dev-only, not published
```

**Adding a language touches exactly these places** (full checklist, including fuzz harnesses, in `CONTRIBUTING.md`):

1. New crate `crates/mct-lang-<name>`, depending on `mct-core` + `tree-sitter-<name>`.
2. Implement `LanguageParser::parse()` — a pure AST walk that never executes/evals input; a syntax error returns `ParseError::Syntax`, never a panic (this runs over arbitrary third-party source).
3. Register in exactly two places: `mct-mcp-server/src/registry.rs::build_registry` and `mct-cli/src/main.rs::build_registry`. Nothing else in `mct-core`, `mct-index`, or `mct-mcp-server` changes.
4. Tests in `crates/mct-lang-<name>/tests/parse.rs`: function/call extraction, one idiomatic-syntax case (generics, decorators, whatever the language's equivalent is), one syntax-error case.
5. Update the language table in `README.md` and `internal/checklist.md`.

**Changing the `LanguageParser` trait, the SQLite schema, or a published MCP tool signature** is cross-cutting (every language crate, every index, and connected MCP clients' live tool contracts) — open an issue first, don't drive-by PR it.

**Code-style invariants across all crates:** no `unwrap()`/`panic!` on any path processing repo-input content (`.unwrap_or_default()` / early return instead) — parsers run over arbitrary, potentially adversarial source; single responsibility per crate (`mct-core` doesn't know SQLite, a `mct-lang-*` crate doesn't know SQLite or MCP).

Commit format: `type(scope): short description` (`feat`/`fix`/`refactor`/`docs`/`test`/`chore`/`perf`), breaking changes marked explicitly.
