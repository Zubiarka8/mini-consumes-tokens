# scripts/windows/

Reserved for the Windows (PowerShell) versions of the scripts in
`../unix/`. None are written yet; until then, run the underlying commands
directly (listed in the root `CLAUDE.md` under **Commands**).

When adding one, mirror the Unix script it replaces: same name with a
`.ps1` extension (`check.ps1`, `reinstall.ps1`, `mcp-smoke.ps1`,
`new-tool-check.ps1`), same flags and output lines, same exit codes
(0 ok, 1 a failed check, 2 a usage error), full output to
`target/script-logs/`. Mind the `.exe` suffix on the installed binaries
(`%USERPROFILE%\.cargo\bin\mct-mcp-server.exe`).
