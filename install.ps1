#!/usr/bin/env pwsh
# Installs the latest release of mct-cli and mct-mcp-server for Windows.
#
# Usage:
#   irm https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.ps1 | iex
#
# Override the install directory (default: $env:LOCALAPPDATA\mct\bin):
#   $env:MCT_INSTALL_DIR = "C:\tools\mct"; irm .../install.ps1 | iex

$ErrorActionPreference = "Stop"

$Repo = "Zubiarka8/mini-consumes-tokens"
$InstallDir = if ($env:MCT_INSTALL_DIR) { $env:MCT_INSTALL_DIR } else { Join-Path $env:LOCALAPPDATA "mct\bin" }
$AssetName = "windows-x86_64"

Write-Host "Fetching latest release info for $Repo..."
$release = Invoke-RestMethod -Uri "https://api.github.com/repos/$Repo/releases/latest"
$tag = $release.tag_name
if (-not $tag) {
    throw "could not determine the latest release tag - check https://github.com/$Repo/releases"
}

$asset = $release.assets | Where-Object { $_.name -like "*-$AssetName.zip" } | Select-Object -First 1
if (-not $asset) {
    throw "no prebuilt archive found for $AssetName in release $tag - see https://github.com/$Repo/releases/tag/$tag"
}

Write-Host "Installing $tag ($AssetName) into $InstallDir..."
New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null

$tmpDir = Join-Path ([System.IO.Path]::GetTempPath()) ([System.IO.Path]::GetRandomFileName())
New-Item -ItemType Directory -Force -Path $tmpDir | Out-Null
try {
    $archivePath = Join-Path $tmpDir $asset.name
    Invoke-WebRequest -Uri $asset.browser_download_url -OutFile $archivePath

    Expand-Archive -Path $archivePath -DestinationPath $tmpDir -Force

    $extractedDir = Get-ChildItem -Path $tmpDir -Directory -Filter "mini-consumes-tokens-*" | Select-Object -First 1
    if (-not $extractedDir) {
        throw "unexpected archive layout - could not find the extracted directory"
    }

    Copy-Item (Join-Path $extractedDir.FullName "mct-cli.exe") (Join-Path $InstallDir "mct-cli.exe") -Force
    Copy-Item (Join-Path $extractedDir.FullName "mct-mcp-server.exe") (Join-Path $InstallDir "mct-mcp-server.exe") -Force
} finally {
    Remove-Item -Recurse -Force $tmpDir -ErrorAction SilentlyContinue
}

Write-Host "Installed mct-cli and mct-mcp-server $tag to $InstallDir"

$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
if ($userPath -notlike "*$InstallDir*") {
    Write-Host ""
    Write-Host "$InstallDir is not on your PATH. Add it for future sessions with:"
    Write-Host "  [Environment]::SetEnvironmentVariable('Path', `$env:Path + ';$InstallDir', 'User')"
    Write-Host "(then restart your terminal), or add it manually to your PATH."
}

Write-Host ""
Write-Host "Next steps, from inside a project you want indexed:"
Write-Host "  mct-cli --root . init"
Write-Host "  mct-cli --root . mcp-register"
