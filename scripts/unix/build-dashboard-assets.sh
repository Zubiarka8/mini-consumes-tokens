#!/usr/bin/env bash
# Rebuilds the committed dashboard assets under crates/mct-cli/src/web/spa/ (ADR-019 D7).
#
# `cargo build` never runs this and never needs Node: the outputs are checked in
# and embedded with include_str!. Run it only after changing spa/spa.src.css,
# spa/tailwind.config.js, spa/index.html, spa/spa.js or spa/icons.txt, then
# commit the regenerated files. Needs node + npm and network access.
#
# Outputs:  spa/spa.css            Tailwind, compiled and minified
#           spa/vendor/alpine.js   Alpine.js CSP build (no eval, no inline expressions)
#           spa/sprite.svg         Lucide icons listed in spa/icons.txt, as <symbol>s
set -euo pipefail

TAILWIND=3.4.17
ALPINE=3.14.8
LUCIDE=0.468.0

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
spa="$root/crates/mct-cli/src/web/spa"
work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

(
  cd "$work"
  npm init -y >/dev/null
  npm install --silent --no-audit --no-fund \
    "tailwindcss@$TAILWIND" "@alpinejs/csp@$ALPINE" "lucide-static@$LUCIDE"
)

# Tailwind scans the shell and the script for class names (also the ones built in JS).
"$work/node_modules/.bin/tailwindcss" \
  -c "$spa/tailwind.config.js" -i "$spa/spa.src.css" -o "$spa/spa.css" --minify

cp "$work/node_modules/@alpinejs/csp/dist/cdn.min.js" "$spa/vendor/alpine.js"

{
  while read -r name; do
    [ -z "$name" ] && continue
    # lucide icons start with a comment and the <svg ...> tag spread over lines
    inner="$(node -e '
      const s = require("fs").readFileSync(process.argv[1], "utf8");
      process.stdout.write(s.replace(/^[\s\S]*?<svg[^>]*>/, "").replace(/<\/svg>\s*$/, "").trim());
    ' "$work/node_modules/lucide-static/icons/$name.svg")"
    printf '<symbol id="i-%s" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">%s</symbol>\n' "$name" "$inner"
  done < "$spa/icons.txt"
} > "$spa/sprite.svg"

echo "built: $(wc -c < "$spa/spa.css") B css, $(wc -c < "$spa/vendor/alpine.js") B alpine, $(wc -l < "$spa/sprite.svg") icons"
