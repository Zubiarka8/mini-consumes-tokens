#!/usr/bin/env bash
# Installs this repo's git hooks from scripts/unix/hooks/ into the hooks
# directory git actually uses (core.hooksPath if set, else .git/hooks).
# Hooks are copied, not symlinked, so they keep working after checking out a
# branch that predates them; re-run this script to update them.
#
# Usage:
#   scripts/unix/install-hooks.sh              # install / update
#   scripts/unix/install-hooks.sh --uninstall  # remove the hooks it installed
#
# A hook of the same name that this script didn't install is left alone and
# reported — merge it by hand.

source "$(dirname "$0")/lib.sh"

uninstall=0
case "${1:-}" in
  "") ;;
  --uninstall) uninstall=1 ;;
  -h | --help) sed -n '2,13p' "$SCRIPTS_DIR/$(basename "$0")" | sed 's/^# \{0,1\}//'; exit 0 ;;
  *) die "unknown argument: $1 (see --help)" ;;
esac

hooks_dir="$(git rev-parse --git-path hooks)"
mkdir -p "$hooks_dir"
# Every hook under scripts/unix/hooks/ carries this marker on its second line.
marker="installed by scripts/unix/install-hooks.sh"
status=0

for source in "$SCRIPTS_DIR"/hooks/*; do
  name="$(basename "$source")"
  target="$hooks_dir/$name"
  ours=0
  [ -f "$target" ] && grep -q "$marker" "$target" && ours=1
  if [ "$uninstall" -eq 1 ]; then
    if [ "$ours" -eq 1 ]; then rm -f "$target"; echo "removed   $target"; fi
    continue
  fi
  if [ -e "$target" ] && [ "$ours" -eq 0 ]; then
    echo "skipped   $target — an existing hook this script didn't install; merge $source into it by hand"
    status=1
    continue
  fi
  if [ "$ours" -eq 1 ] && cmp -s "$source" "$target"; then
    echo "current   $target"
  else
    cp "$source" "$target" && chmod +x "$target"
    echo "installed $target"
  fi
done
exit "$status"
