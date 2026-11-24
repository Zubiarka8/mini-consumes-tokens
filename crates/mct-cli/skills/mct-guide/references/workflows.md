# Workflow scripts

Run from the skill directory with `python3 scripts/mct_<name>.py`. Every script accepts `--root <project>` and `--cli <binary>` before its positional arguments. Use `--help` to see its arguments. Optionally set `--timeout <seconds>` before positional arguments to limit the subprocess runtime; it exits with status 124 on timeout and 130 on interruption. The Python scripts use only the standard library and delegate to the installed `mct-cli`; tool availability and arguments depend on that binary's version.

The scripts validate the project directory, executable, nonempty primary value, JSON structure, duplicate JSON keys, non-finite JSON numbers, and common numeric, string and boolean parameter types. Unknown tool parameters are forwarded to the installed CLI. The installed CLI remains authoritative for tool-specific parameters.

For query scripts, provide additional tool parameters with `--args '<JSON object>'`; the positional value supplies the primary parameter and cannot also appear in the JSON. For example:

```sh
python3 scripts/mct_search.py --root . 'request handler' --args '{"limit":5}'
python3 scripts/mct_callers.py --root . parse --args '{"depth":2}'
python3 scripts/mct_tree.py --root . --args '{"depth":2}'
```

| Script | Primary value | Purpose |
| --- | --- | --- |
| `mct_tree.py` | optional path | File and directory layout |
| `mct_overview.py` | optional path | Ranked overview of a project or path |
| `mct_symbols.py` | path | Symbols in a file or directory |
| `mct_skeleton.py` | file path | Declarations with bodies collapsed |
| `mct_find.py` | symbol name | Find definitions |
| `mct_search.py` | query | Search by words or partial name |
| `mct_meaning.py` | query | Hybrid search; inspect output for lexical-only mode |
| `mct_callers.py` | function | Incoming calls |
| `mct_calls.py` | function | Outgoing calls |
| `mct_refs.py` | symbol | References across relation kinds |
| `mct_impact.py` | symbol | Candidate impact and related tests |
| `mct_context.py` | symbol | Definition and related context |
| `mct_deps.py` | module | Import graph and cycles |
| `mct_dead_code.py` | optional path | Dead-code candidates, not proof |
| `mct_duplicates.py` | optional path | Duplicate candidates, not proof |

These query scripts use `mct-cli call`, which performs an incremental index refresh before querying. They do not guarantee that the index is current if parsing fails; inspect returned issues and unresolved or ambiguous relations. Use installed MCP tool schemas or the project MCP reference for supported `--args` keys.

`mct_status.py`, `mct_reindex.py`, `mct_obsidian.py`, `mct_probe.py`, `mct_web.py`, and `mct_update.py` invoke the matching CLI command. Pass command options after `--`, for example `python3 scripts/mct_probe.py --root . -- src/main.rs` or `python3 scripts/mct_reindex.py --root . -- --force`. Reindex and Obsidian write local data; web starts a long-running server; update replaces installed binaries and should run only when requested.

`mct_setup.py` executes one named setup operation per invocation: `init`, `mcp-register`, `skills-register`, `git-exclusions-init`, `init-mctignore`, or `status`. For example, `python3 scripts/mct_setup.py --root . mcp-register -- --dry-run`. Review the relevant CLI help and intended effects before running a write or using `--force` or `--global`; this script does not silently run the entire setup sequence.
