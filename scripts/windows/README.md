# scripts/windows/

The Windows (PowerShell) versions of the scripts in `../unix/`: same names
with a `.ps1` extension, same flags, same output lines, same logs in
`target\script-logs\`, same exit codes (0 ok, 1 a failed check, 2 a usage
error). What each one does is in `../README.md`.

| Script | Unix counterpart |
|---|---|
| `check.ps1 [-p <crate>] [--only test\|clippy\|eval] [--no-eval]` | `check.sh` |
| `reinstall.ps1 [--reindex] [--semantic] [--server-only]` | `reinstall.sh` |
| `mcp-smoke.ps1 [--dev] [--expect T] [--list] [--bin P --root D]` | `mcp-smoke.sh` |
| `new-tool-check.ps1 <tool> [--no-tests]` | `new-tool-check.sh` |
| `new-language-check.ps1 <suffix> [--readme-name N] [--no-tests]` | `new-language-check.sh` |
| `token-report.ps1 [--no-eval] [--markdown F]` | `token-report.sh` |
| `corpus-report.ps1 <lang> [--bless] [--update-progress]` | `corpus-report.sh` |
| `corpus-progress-check.ps1` | `corpus-progress-check.sh` |
| `parse-probe.ps1 <files...>` | `parse-probe.sh` |
| `install-hooks.ps1 [--uninstall]` | `install-hooks.sh` |

## Running them

From the repo root, in PowerShell:

```powershell
scripts\windows\check.ps1 -p mct-core
```

Windows PowerShell's default execution policy (`Restricted`) refuses to run
any `.ps1`. Either allow local scripts once for your user:

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

or bypass it per call, which also works from `cmd.exe`:

```bat
powershell -NoProfile -ExecutionPolicy Bypass -File scripts\windows\check.ps1 -p mct-core
```

`pwsh` (PowerShell 7+) works the same way in place of `powershell`.

## Differences from the Unix scripts

- **`.exe` binaries**: the installed server is
  `%USERPROFILE%\.cargo\bin\mct-mcp-server.exe` (or `%CARGO_HOME%\bin`).
- **Locked files**: Windows can't overwrite a running `.exe` or delete an
  open database. If Claude Code is connected to the server,
  `reinstall.ps1` fails to install or to delete `.mct-index\index.sqlite3`
  and says so — disconnect it (`/mcp`, or quit Claude Code) and re-run.
- **No `jq`/`python3` needed**: `mcp-smoke.ps1` parses the JSON-RPC replies
  with PowerShell's own `ConvertFrom-Json`.
- **Hooks are still bash**: Git for Windows runs every git hook through its
  bundled bash, whatever shell started git, so `hooks\post-checkout` is a sh
  script (the Unix one adapted to `.exe` paths and to Windows paths in
  `CARGO_HOME`/`USERPROFILE`). `install-hooks.ps1` writes it with LF line
  endings, which that bash needs.
- **ASCII output**: the scripts print `->`, `-` and `...` where the Unix
  ones print `→`, `—` and `…`; Windows PowerShell 5.1 reads a BOM-less
  `.ps1` in the ANSI code page, so the sources stay ASCII-only.

## For maintainers

`lib.ps1` holds the shared helpers. Native commands (`cargo`, `git`, the mct
binaries) never go through PowerShell's own `>`/`2>&1`: Windows PowerShell
5.1 turns every stderr line of a redirected native command into an error
record, which under `$ErrorActionPreference = 'Stop'` aborts the script.
Use `Invoke-Logged` (stdout+stderr appended in order to a log, via
`cmd.exe`) or `Invoke-Captured` (both streams returned as strings) instead.
Keep the scripts compatible with 5.1 — no `??`, ternaries or `&&` — and
ASCII-only. When changing a Unix script, change its `.ps1` twin in the same
PR.
