#!/usr/bin/env bash
# Installs the latest release of ccm-cli and ccm-mcp-server for Linux/macOS.
#
# Usage:
#   curl -sSL https://raw.githubusercontent.com/Zubiarka8/mini-consumes-tokens/main/install.sh | bash
#
# Override the install directory (default: $HOME/.local/bin):
#   INSTALL_DIR=/usr/local/bin curl -sSL .../install.sh | bash
#
# Windows: use install.ps1 instead (irm .../install.ps1 | iex).
set -euo pipefail

REPO="Zubiarka8/mini-consumes-tokens"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

log() { printf '%s\n' "$*" >&2; }
die() { log "error: $*"; exit 1; }

command -v curl >/dev/null 2>&1 || die "curl is required"
command -v tar >/dev/null 2>&1 || die "tar is required"

os="$(uname -s)"
arch="$(uname -m)"

case "$os" in
    Linux) platform_os="linux" ;;
    Darwin) platform_os="macos" ;;
    *) die "unsupported OS: $os (prebuilt binaries cover Linux and macOS only — see install.ps1 for Windows, or 'cargo install ccm-cli ccm-mcp-server' from source)" ;;
esac

case "$arch" in
    x86_64|amd64) platform_arch="x86_64" ;;
    arm64|aarch64) platform_arch="arm64" ;;
    *) die "unsupported architecture: $arch" ;;
esac

asset_name="${platform_os}-${platform_arch}"

log "Fetching latest release info for $REPO..."
release_json="$(curl -sSL "https://api.github.com/repos/$REPO/releases/latest")"

tag="$(printf '%s' "$release_json" | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
[ -n "$tag" ] || die "could not determine the latest release tag — check https://github.com/$REPO/releases"

download_url="$(printf '%s' "$release_json" | grep -o '"browser_download_url": *"[^"]*"' | sed -E 's/.*"(https:[^"]+)"/\1/' | grep -- "-${asset_name}\.tar\.gz$" || true)"
[ -n "$download_url" ] || die "no prebuilt archive found for $asset_name in release $tag — see https://github.com/$REPO/releases/tag/$tag"

log "Installing $tag ($asset_name) into $INSTALL_DIR..."
mkdir -p "$INSTALL_DIR"

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

archive="$tmp_dir/$(basename "$download_url")"
curl -sSL -o "$archive" "$download_url"
tar -xzf "$archive" -C "$tmp_dir"

extracted_dir="$(find "$tmp_dir" -maxdepth 1 -type d -name 'mini-consumes-tokens-*' | head -n1)"
[ -n "$extracted_dir" ] || die "unexpected archive layout — could not find the extracted directory"

install -m 755 "$extracted_dir/ccm-cli" "$INSTALL_DIR/ccm-cli"
install -m 755 "$extracted_dir/ccm-mcp-server" "$INSTALL_DIR/ccm-mcp-server"

log "Installed ccm-cli and ccm-mcp-server $tag to $INSTALL_DIR"

case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        log ""
        log "$INSTALL_DIR is not on your PATH. Add this to your shell profile:"
        log "  export PATH=\"$INSTALL_DIR:\$PATH\""
        ;;
esac

log ""
log "Next steps, from inside a project you want indexed:"
log "  ccm-cli --root . init"
log "  ccm-cli --root . mcp-register"
