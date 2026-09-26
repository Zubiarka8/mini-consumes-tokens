#!/usr/bin/env bash
# Starts mct-mcp-server over stdio, sends `initialize` + `tools/list` like an
# MCP client, and reports whether it came up and what it advertises. This is
# the check behind Claude Code's opaque `CONNECTION_CLOSED`: when the server
# dies on startup, the real error (e.g. an index migrated by a newer branch)
# is printed here.
#
# Usage:
#   scripts/mcp-smoke.sh                     # the installed binary (~/.cargo/bin)
#   scripts/mcp-smoke.sh --dev               # build and use target/debug
#   scripts/mcp-smoke.sh --expect NAME       # also fail unless tool NAME is listed
#   scripts/mcp-smoke.sh --list              # also print every tool name
#   scripts/mcp-smoke.sh --bin PATH --root DIR
#
# Exit status: 0 when the server answered and every check passed, 1 otherwise.

source "$(dirname "$0")/lib.sh"

bin="$CARGO_BIN/mct-mcp-server$EXE"
root="$ROOT"
dev=0 list=0 expect=()
while [ $# -gt 0 ]; do
  case "$1" in
    --dev) dev=1; shift ;;
    --bin) bin="$(abspath "${2:?--bin needs a path}")"; shift 2 ;;
    --root) root="$(abspath "${2:?--root needs a directory}")"; shift 2 ;;
    --expect) expect+=("${2:?--expect needs a tool name}"); shift 2 ;;
    --list) list=1; shift ;;
    -h | --help) sed -n '2,16p' "$ROOT/scripts/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
    *) die "unknown argument: $1 (see --help)" ;;
  esac
done

if [ "$dev" -eq 1 ]; then
  cargo build -q -p mct-mcp-server >"$LOG_DIR/smoke-build.log" 2>&1 \
    || { echo "build FAILED — see $LOG_DIR/smoke-build.log"; tail -n 20 "$LOG_DIR/smoke-build.log"; exit 1; }
  bin="$ROOT/target/debug/mct-mcp-server$EXE"
fi
[ -x "$bin" ] || die "no server binary at $bin (install it with scripts/reinstall.sh, or pass --dev)"

out="$LOG_DIR/smoke-stdout.jsonl"
err="$LOG_DIR/smoke-stderr.log"
# The server answers each request as it arrives and exits once stdin closes.
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-03-26","capabilities":{},"clientInfo":{"name":"mcp-smoke","version":"0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list"}' \
  | "$bin" --root "$root" >"$out" 2>"$err" || true

if ! grep -q '"id":2' "$out"; then
  echo "server FAILED to answer — binary: $bin"
  grep -vE ' (INFO|DEBUG|TRACE) ' "$err" | cap 20 "$err"
  if grep -q 'migration number that is too high' "$err"; then
    echo "→ the index was migrated by a newer schema; rebuild it: scripts/reinstall.sh --reindex"
  fi
  exit 1
fi

names=$(json_query "$out" \
  'select(.id == 2) | .result.tools[].name' \
  '[t["name"] for l in lines if l.get("id") == 2 for t in l["result"]["tools"]]' | tr -d '\r')
bytes=$(json_query "$out" \
  'select(.id == 2) | [.result.tools[].description // "" | utf8bytelength] | add' \
  'sum(len(t.get("description", "").encode()) for l in lines if l.get("id") == 2 for t in l["result"]["tools"])' | tr -d '\r')
# A description still equal to server.rs's `#[tool(description = ...)]`
# fallback means tools.ttc's expansion never reached the client.
fallbacks=$(json_query "$out" \
  'select(.id == 2) | .result.tools[] | select(.description // "" | endswith("see tools.ttc")) | .name' \
  '[t["name"] for l in lines if l.get("id") == 2 for t in l["result"]["tools"] if t.get("description", "").endswith("see tools.ttc")]' | tr -d '\r')
count=$(printf '%s\n' "$names" | grep -c . || true)

status=0
echo "server ok — $count tool(s), $bytes description bytes ($bin)"
if [ -n "$fallbacks" ]; then
  echo "WARN  $(printf '%s\n' "$fallbacks" | grep -c .) tool(s) advertise the server.rs fallback description, not tools.ttc's"
fi
for name in ${expect[@]+"${expect[@]}"}; do
  if printf '%s\n' "$names" | grep -qx "$name"; then
    echo "ok    $name is listed"
  else
    echo "FAIL  $name is not listed — is the binary older than your change? (--dev, or scripts/reinstall.sh)"
    status=1
  fi
done
if [ "$list" -eq 1 ]; then
  printf '%s\n' "$names" | sort | sed 's/^/  /'
fi
exit "$status"
