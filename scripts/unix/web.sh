#!/usr/bin/env bash
# Serves the local project dashboard (`mct-cli web`) and opens it in the
# default browser. Keep it running; Ctrl-C stops the server.
#
# Usage:
#   scripts/unix/web.sh                    # the installed mct-cli (~/.cargo/bin)
#   scripts/unix/web.sh --dev              # build and run this checkout (cargo run)
#   scripts/unix/web.sh --port N           # use exactly this port (0 = any free port)
#   scripts/unix/web.sh --no-obsidian      # do not regenerate the Obsidian vault
#   scripts/unix/web.sh --no-open          # print the URL but do not open a browser
#   scripts/unix/web.sh --root DIR         # project to serve (default: this checkout)
#
# The output always shows the concrete link: `dashboard: <url>` when it is
# serving, `dashboard: <url> (NOT running)` with the expected link when it is not.
#
# Exit status: 0 on a clean stop, 1 when the server failed to start, 2 on a usage error.

source "$(dirname "$0")/lib.sh"

root="$ROOT"
port="" dev=0 obsidian=1 open_browser=1
while [ $# -gt 0 ]; do
  case "$1" in
    --dev) dev=1; shift ;;
    --port) port="${2:?--port needs a number}"; shift 2 ;;
    --no-obsidian) obsidian=0; shift ;;
    --no-open) open_browser=0; shift ;;
    --root) root="$(abspath "${2:?--root needs a directory}")"; shift 2 ;;
    -h | --help) sed -n '2,16p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

# The link this project is (or would be) served on: --port, else the port
# remembered in .mct-index/web-port, else the default 8787.
exp_port="$port"
if [ -z "$exp_port" ] && [ -f "$root/.mct-index/web-port" ]; then
  exp_port="$(tr -d '[:space:]' <"$root/.mct-index/web-port")"
fi
exp_port="${exp_port:-8787}"
if [ "$exp_port" = "0" ]; then expected="http://localhost:<free port>/"; else expected="http://localhost:$exp_port/"; fi

if [ "$dev" -eq 0 ] && [ -x "$CARGO_BIN/mct-cli" ]; then
  cmd=("$CARGO_BIN/mct-cli")
else
  cmd=(cargo run -q -p mct-cli --)
fi
args=(--root "$root" web)
[ -n "$port" ] && args+=(--port "$port")
[ "$obsidian" -eq 0 ] && args+=(--no-obsidian)

log="$LOG_DIR/web.log"
new_log "$log"
"${cmd[@]}" "${args[@]}" >>"$log" 2>&1 &
pid=$!
trap 'kill "$pid" 2>/dev/null || true' EXIT INT TERM

# `cargo run` may compile first, so allow up to a minute before giving up.
url=""
for _ in $(seq 1 600); do
  url="$(grep -m1 -Eo 'http://(localhost|127\.0\.0\.1):[0-9]+/?' "$log" || true)"
  [ -n "$url" ] && break
  if ! kill -0 "$pid" 2>/dev/null; then
    echo "web FAILED to start — see $log"
    grep -vE '^# ' "$log" | cap 20 "$log"
    if grep -q 'schema is incompatible' "$log"; then
      echo "→ the index was built by another version; close other MCP clients, then: scripts/unix/reinstall.sh --reindex"
    fi
    echo "dashboard: $expected (NOT running)"
    exit 1
  fi
  sleep 0.1
done
[ -n "$url" ] || { echo "web FAILED: no URL after 60s — see $log"; echo "dashboard: $expected (NOT running)"; exit 1; }

echo "dashboard: $url   (Ctrl-C to stop)"
if [ "$open_browser" -eq 1 ]; then
  if command -v open >/dev/null 2>&1; then
    open "$url" || true
  elif command -v xdg-open >/dev/null 2>&1; then
    xdg-open "$url" >/dev/null 2>&1 || true
  else
    echo "no browser opener found (open / xdg-open); open the URL above by hand"
  fi
fi

wait "$pid" || true
echo "dashboard stopped: $url"
