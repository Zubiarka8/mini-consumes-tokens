# Shared helpers for scripts/windows/*.ps1 -- dot-sourced, not run. Windows
# only (macOS/Linux have scripts/unix/). Works on Windows PowerShell 5.1 and
# PowerShell 7+, so no `??`, ternaries or `&&`. Kept ASCII-only: Windows
# PowerShell 5.1 reads a BOM-less .ps1 in the ANSI code page.
#
# Each script sets these before dot-sourcing this file:
#   $ScriptPath = $MyInvocation.MyCommand.Path
#   $ScriptArgs = $args
#
# Native commands (cargo, git, the mct binaries) never run through
# PowerShell's own redirection: Windows PowerShell 5.1 turns every stderr line
# of a redirected native command into an error record (and, under
# $ErrorActionPreference = 'Stop', a terminating error). They go through
# Invoke-Logged / Invoke-Captured below instead.

$ErrorActionPreference = 'Stop'

if ($env:OS -ne 'Windows_NT') {
  [Console]::Error.WriteLine('error: scripts/windows/ is for Windows; on macOS and Linux use scripts/unix/')
  exit 2
}

$ScriptsDir = $PSScriptRoot
$Root = (Resolve-Path (Join-Path $ScriptsDir '..\..')).Path
# Where the script was invoked from, for resolving relative path arguments.
# The scripts never change the caller's location (a .ps1 runs in the calling
# session, so Set-Location would outlive it); every repo path is joined onto
# $Root instead, and native commands start with $Root as working directory.
$CallerPwd = (Get-Location).ProviderPath
# Full command output goes here; the scripts print only a summary. Under the
# already-gitignored target/ so nothing new needs ignoring. Each log has a
# fixed name and is overwritten by the next run of the same script, so the
# directory never grows; deleting it (or `cargo clean`) is always safe.
$LogDir = 'target/script-logs'
[void](New-Item -ItemType Directory -Force -Path (Join-Path $Root $LogDir))

if ($env:CARGO_HOME) { $CargoHome = $env:CARGO_HOME } else { $CargoHome = Join-Path $env:USERPROFILE '.cargo' }
$CargoBin = Join-Path $CargoHome 'bin'

$Utf8NoBom = New-Object System.Text.UTF8Encoding($false)
$EmDash = [string][char]0x2014

# Absolute path of a repo-relative path.
function Get-RepoPath([string]$Rel) { Join-Path $Root $Rel }

# Absolute form of a path argument given relative to the caller's directory.
function Get-AbsPath([string]$Path) {
  if ([IO.Path]::IsPathRooted($Path)) { return $Path }
  return (Join-Path $CallerPwd $Path)
}

function Stop-Usage([string]$Message) {
  [Console]::Error.WriteLine("error: $Message")
  exit 2
}

# Prints the calling script's leading `#` comment block, like --help.
function Show-Help {
  foreach ($line in [IO.File]::ReadAllLines($ScriptPath)) {
    if ($line -notmatch '^#') { break }
    Write-Host ($line -replace '^# ?', '')
  }
}

# The lines of a text file (UTF-8), or none when it doesn't exist.
function Get-Lines([string]$Path) {
  if (-not [IO.Path]::IsPathRooted($Path)) { $Path = Get-RepoPath $Path }
  if (-not (Test-Path -LiteralPath $Path -PathType Leaf)) { return @() }
  return [IO.File]::ReadAllLines($Path, [Text.Encoding]::UTF8)
}

# `Write-Capped <lines> <max> <log>`: prints at most $Max lines, then a note
# pointing at the full log.
function Write-Capped($Lines, [int]$Max, [string]$Log) {
  $all = @($Lines | Where-Object { $null -ne $_ })
  for ($i = 0; $i -lt [Math]::Min($Max, $all.Count); $i++) { Write-Host $all[$i] }
  if ($all.Count -gt $Max) { Write-Host ("  ... {0} more line(s) in {1}" -f ($all.Count - $Max), $Log) }
}

# Lines matching $Pattern, each followed by up to $After lines of context
# (grep -A without the `--` separators; overlapping contexts are not repeated).
function Select-WithContext($Lines, [string]$Pattern, [int]$After) {
  $all = @($Lines)
  $out = New-Object System.Collections.Generic.List[string]
  $next = 0
  for ($i = 0; $i -lt $all.Count; $i++) {
    if ($all[$i] -cmatch $Pattern) {
      $from = [Math]::Max($i, $next)
      $to = [Math]::Min($i + $After, $all.Count - 1)
      for ($j = $from; $j -le $to; $j++) { $out.Add($all[$j]) }
      $next = $to + 1
    }
  }
  return ,$out.ToArray()
}

# One argument quoted for a cmd.exe command line.
function ConvertTo-CmdArg([string]$Arg) {
  if ($Arg -eq '' -or $Arg -match '[\s&|<>^()"]') { return '"' + ($Arg -replace '"', '""') + '"' }
  return $Arg
}

# `Invoke-Captured <exe> <args>`: runs a native command from $Root and returns
# @{ ExitCode; Out; Err } with its stdout/stderr as strings.
function Invoke-Captured([string]$Exe, [string[]]$Arguments) {
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $Exe
  $psi.Arguments = (@($Arguments) | ForEach-Object { ConvertTo-CmdArg $_ }) -join ' '
  $psi.WorkingDirectory = $Root
  $psi.UseShellExecute = $false
  $psi.RedirectStandardOutput = $true
  $psi.RedirectStandardError = $true
  $psi.CreateNoWindow = $true
  try { $p = [Diagnostics.Process]::Start($psi) } catch { return @{ ExitCode = 127; Out = ''; Err = "$_" } }
  $errTask = $p.StandardError.ReadToEndAsync()
  $out = $p.StandardOutput.ReadToEnd()
  $p.WaitForExit()
  return @{ ExitCode = $p.ExitCode; Out = $out; Err = $errTask.Result }
}

# `Invoke-Logged <log> <exe> <args>`: runs a native command from $Root with
# stdout and stderr appended, interleaved, to the log; returns its exit code.
# Goes through cmd.exe, whose `>> log 2>&1` keeps the two streams in order.
function Invoke-Logged([string]$Log, [string]$Exe, [string[]]$Arguments) {
  $line = ((@($Exe) + @($Arguments)) | ForEach-Object { ConvertTo-CmdArg $_ }) -join ' '
  $line += ' >> ' + (ConvertTo-CmdArg (Get-RepoPath $Log)) + ' 2>&1'
  $psi = New-Object System.Diagnostics.ProcessStartInfo
  $psi.FileName = $env:ComSpec
  if (-not $psi.FileName) { $psi.FileName = 'cmd.exe' }
  # /s: strip exactly the outer quotes, so the quoted arguments inside survive.
  $psi.Arguments = '/d /s /c "' + $line + '"'
  $psi.WorkingDirectory = $Root
  $psi.UseShellExecute = $false
  $p = [Diagnostics.Process]::Start($psi)
  $p.WaitForExit()
  return $p.ExitCode
}

function Get-Git([string[]]$Arguments) {
  $r = Invoke-Captured 'git' $Arguments
  if ($r.ExitCode -ne 0) { return $null }
  return $r.Out.Trim()
}

# The command line and checkout a log came from, for its header.
$ScriptCmd = 'scripts/windows/' + (Split-Path -Leaf $ScriptPath)
if (@($ScriptArgs).Count -gt 0) { $ScriptCmd += ' ' + (@($ScriptArgs) -join ' ') }
$gitBranch = Get-Git @('rev-parse', '--abbrev-ref', 'HEAD'); if (-not $gitBranch) { $gitBranch = '?' }
$gitCommit = Get-Git @('rev-parse', '--short', 'HEAD'); if (-not $gitCommit) { $gitCommit = '?' }
$GitRev = "$gitBranch@$gitCommit"
if ((Invoke-Captured 'git' @('diff', '--quiet', 'HEAD')).ExitCode -ne 0) { $GitRev += '-dirty' }

# `<command> - <date time zone> - <branch>@<commit>[-dirty]`
function Get-LogStamp {
  $date = (Get-Date).ToString('yyyy-MM-dd HH:mm:ss ') + (Get-Date).ToString('zzz').Replace(':', '')
  return "$ScriptCmd $EmDash $date $EmDash $GitRev"
}

# Starts a log with a `# <stamp>` line, so an old log is recognisable at a
# glance; commands then append to it.
function New-Log([string]$Log) {
  [IO.File]::WriteAllText((Get-RepoPath $Log), "# $(Get-LogStamp)`n", $Utf8NoBom)
}

# Appends text (plus a newline) to a UTF-8 file.
function Add-Utf8Line([string]$Path, [string]$Text) {
  [IO.File]::AppendAllText($Path, "$Text`n", $Utf8NoBom)
}

# Sums `cargo test`'s `test result:` lines: @{ Passed; Failed; Ignored }.
function Get-TestCounts($Lines) {
  $c = @{ Passed = 0; Failed = 0; Ignored = 0 }
  foreach ($line in @($Lines)) {
    if ($line -notmatch '^test result:') { continue }
    if ($line -match '(\d+) passed') { $c.Passed += [int]$Matches[1] }
    if ($line -match '(\d+) failed') { $c.Failed += [int]$Matches[1] }
    if ($line -match '(\d+) ignored') { $c.Ignored += [int]$Matches[1] }
  }
  return $c
}
