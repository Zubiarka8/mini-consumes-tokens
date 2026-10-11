#!/bin/sh
# Build the static export with the GitHub Pages base path and serve it on :4173.
# Usage: website/scripts/serve.sh   (SKIP_BUILD=1 reuses the existing out/)
set -e
export NEXT_PUBLIC_BASE_PATH=/mini-consumes-tokens
web="$(dirname "$0")/.."
[ -n "$SKIP_BUILD" ] || npm --prefix "$web" run build
(sleep 1 && open "http://127.0.0.1:${PORT:-4173}$NEXT_PUBLIC_BASE_PATH/") &
exec sh -c 'cd "$1" && node scripts/preview.mjs' _ "$web"
