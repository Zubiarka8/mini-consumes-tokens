#!/usr/bin/env bash
# Builds the website of one or more branches and serves each on its own port,
# so several branches can be compared in the browser at the same time. Every
# build uses the same base path the site is deployed under, which avoids the
# unstyled page you get when the build and the server disagree about the
# prefix. Each branch must already have a worktree (see branch-worktree.sh).
#
# Usage:
#   scripts/unix/preview-branches.sh <branch>...
#   scripts/unix/preview-branches.sh --no-build <branch>...   # serve the existing out/
#   scripts/unix/preview-branches.sh --stop                   # stop every preview started here
#
# Options: --base-path <path> (default /mini-consumes-tokens), --port-start <n> (default 4700).
# Per branch it syncs the local skills, runs `npm ci` when node_modules is
# missing, builds, starts the server, and checks that the page and its CSS
# answer 200 before printing the URL. Exit status: 0 on success, 2 on usage or failure.

source "$(dirname "$0")/lib.sh"

base_path="/mini-consumes-tokens"
port=4700
build=1
stop=0
branches=()
while [ $# -gt 0 ]; do
  case "$1" in
    --base-path) base_path="${2:?--base-path needs a value}"; shift 2 ;;
    --port-start) port="${2:?--port-start needs a value}"; shift 2 ;;
    --no-build) build=0; shift ;;
    --stop) stop=1; shift ;;
    -*) die "unknown option: $1" ;;
    *) branches+=("$1"); shift ;;
  esac
done

pid_dir="$ROOT/$LOG_DIR/previews"
mkdir -p "$pid_dir"

stop_pidfile() {
  [ -f "$1" ] || return 0
  pid="$(cat "$1")"
  kill "$pid" 2>/dev/null || true
  rm -f "$1"
}

if [ "$stop" = 1 ]; then
  for f in "$pid_dir"/*.pid; do stop_pidfile "$f"; done
  echo "stopped every preview started by this script"
  exit 0
fi
[ ${#branches[@]} -ge 1 ] || die "usage: scripts/unix/preview-branches.sh [--no-build] [--base-path P] [--port-start N] <branch>... | --stop"

port_busy() { lsof -iTCP:"$1" -sTCP:LISTEN >/dev/null 2>&1; }

worktree_of() {
  git worktree list --porcelain | awk -v b="refs/heads/$1" '
    /^worktree /{w=substr($0,10)} /^branch /{if (substr($0,8)==b) print w}' | head -n1
}

for branch in "${branches[@]}"; do
  dir="$(worktree_of "$branch")"
  [ -n "$dir" ] || die "no worktree for $branch (create one with scripts/unix/branch-worktree.sh $branch [base])"
  site="$dir/website"
  [ -d "$site" ] || die "$dir has no website/ directory"
  slug="$(echo "$branch" | tr '/' '-')"

  "$SCRIPTS_DIR/sync-skills.sh" "$dir" >/dev/null
  [ -d "$site/node_modules" ] || { echo "[$branch] npm ci"; npm --prefix "$site" ci >"$ROOT/$LOG_DIR/preview-$slug-ci.log" 2>&1 || die "npm ci failed, see $LOG_DIR/preview-$slug-ci.log"; }
  if [ "$build" = 1 ]; then
    echo "[$branch] build"
    NEXT_PUBLIC_BASE_PATH="$base_path" npm --prefix "$site" run build >"$ROOT/$LOG_DIR/preview-$slug-build.log" 2>&1 ||
      die "build failed for $branch, see $LOG_DIR/preview-$slug-build.log"
  fi
  [ -f "$site/out/index.html" ] || die "$branch has no website/out/index.html (run without --no-build)"

  stop_pidfile "$pid_dir/$slug.pid"
  while port_busy "$port"; do port=$((port + 1)); done
  # `cd` first, on its own: with `cd && cmd &` the pid would be the subshell's, not the server's.
  (cd "$site"; NEXT_PUBLIC_BASE_PATH="$base_path" PORT="$port" nohup node scripts/preview.mjs \
    >"$ROOT/$LOG_DIR/preview-$slug.log" 2>&1 & echo $! >"$pid_dir/$slug.pid")
  sleep 1

  url="http://localhost:$port$base_path/"
  css="$(curl -s "$url" | grep -o "$base_path/_next/static/css/[^\"]*\.css" | head -n1)"
  page_code="$(curl -s -o /dev/null -w '%{http_code}' "$url")"
  css_code="$(curl -s -o /dev/null -w '%{http_code}' "http://localhost:$port$css")"
  if [ "$page_code" = 200 ] && [ "$css_code" = 200 ]; then
    echo "[$branch] $url   (page 200, css 200)"
  else
    die "[$branch] server on port $port answered page=$page_code css=$css_code, see $LOG_DIR/preview-$slug.log"
  fi
  port=$((port + 1))
done
