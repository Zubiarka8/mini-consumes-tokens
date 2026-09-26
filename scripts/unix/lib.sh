# Shared helpers for scripts/unix/*.sh — sourced, not run. macOS and Linux
# only (Windows gets its own scripts under scripts/windows/). Bash 3.2
# compatible (macOS's /bin/bash), so no associative arrays, mapfile or ${var,,}.

set -euo pipefail

case "$(uname -s)" in
  Darwin | Linux) ;;
  *)
    echo "error: scripts/unix/ is for macOS and Linux; on Windows use scripts/windows/" >&2
    exit 2 ;;
esac

SCRIPTS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT="$(cd "$SCRIPTS_DIR/../.." && pwd)"
# Where the script was invoked from, for resolving relative path arguments;
# every script then runs from the repo root.
CALLER_PWD="$PWD"
cd "$ROOT"
# Full command output goes here; the scripts print only a summary. Under the
# already-gitignored target/ so nothing new needs ignoring. Relative, so the
# paths the scripts print stay short. Each log has a fixed name and is
# overwritten by the next run of the same script, so the directory never
# grows; deleting it (or `cargo clean`) is always safe.
LOG_DIR="target/script-logs"
mkdir -p "$LOG_DIR"

# The command line and checkout a log came from, for its header. A sourced
# file sees the calling script's arguments.
SCRIPT_CMD="scripts/unix/$(basename "$0")${*:+ $*}"
GIT_REV="$(git rev-parse --abbrev-ref HEAD 2>/dev/null || echo '?')@$(git rev-parse --short HEAD 2>/dev/null || echo '?')"
git diff --quiet HEAD 2>/dev/null || GIT_REV="$GIT_REV-dirty"

# `<command> — <date time zone> — <branch>@<commit>[-dirty]`
log_stamp() {
  printf '%s — %s — %s' "$SCRIPT_CMD" "$(date '+%Y-%m-%d %H:%M:%S %z')" "$GIT_REV"
}

# `new_log <file>`: starts a log with a `# <log_stamp>` line, so an old log
# is recognisable at a glance (`head -n1 target/script-logs/*`). Commands
# then append to it with >>.
new_log() {
  printf '# %s\n' "$(log_stamp)" >"$1"
}

CARGO_BIN="${CARGO_HOME:-$HOME/.cargo}/bin"

# Absolute form of a path argument given relative to the caller's directory.
abspath() {
  case "$1" in
    /*) printf '%s\n' "$1" ;;
    *) printf '%s\n' "$CALLER_PWD/$1" ;;
  esac
}

die() {
  echo "error: $*" >&2
  exit 2
}

# Prints at most $1 lines of stdin, then a note pointing at the full log $2.
cap() {
  local max="$1" log="$2"
  awk -v max="$max" -v logfile="$log" '
    NR <= max { print }
    END { if (NR > max) printf "  … %d more line(s) in %s\n", NR - max, logfile }
  '
}

# `json_query <file> <jq filter> <python expression over `lines`>`: jq when
# available, python3 otherwise. `lines` is the file parsed as JSON Lines.
json_query() {
  local file="$1" jq_filter="$2" py_expr="$3"
  if command -v jq >/dev/null 2>&1; then
    jq -r "$jq_filter" "$file"
  elif command -v python3 >/dev/null 2>&1; then
    python3 - "$file" "$py_expr" <<'EOF'
import json, sys
lines = [json.loads(l) for l in open(sys.argv[1], encoding="utf-8") if l.strip()]
out = eval(sys.argv[2])
for item in (out if isinstance(out, list) else [out]):
    print(item)
EOF
  else
    die "needs jq or python3 to read the server's JSON-RPC responses"
  fi
}
