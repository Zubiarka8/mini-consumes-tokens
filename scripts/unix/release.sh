#!/usr/bin/env bash
# The scriptable steps of RELEASING.md, in three commands run in order. The
# CHANGELOG entry and the PR review stay human; crates.io is never touched.
#
# Usage:
#   scripts/unix/release.sh bump <x.y.z|patch|minor|major>  # on a clean tree
#   scripts/unix/release.sh dry-run [x.y.z]   # on the release branch
#   scripts/unix/release.sh publish [x.y.z]   # after the PR is merged
#
# patch/minor/major count from the current workspace version (0.2.0 →
# 0.2.1 / 0.3.0 / 1.0.0); any explicit x.y.z is accepted as given.
# Without a version, dry-run uses the checkout's and publish origin/main's.
#
# bump     Sets the workspace version, every internal dependency version and
#          the README badge, refreshes Cargo.lock and the crates/*/fuzz
#          lockfiles (workspace packages only) and commits. On main it first
#          creates the branch release-<x.y.z>.
# dry-run  Checks the dated CHANGELOG section, runs check.sh and the installer
#          test, pushes the branch and opens its PR (publish-branch.sh), then
#          runs the Release workflow by hand and waits: build and package
#          only, no tag, no release.
# publish  Fetches origin/main, checks its version, CHANGELOG section and CI,
#          tags it v<x.y.z>, pushes the tag and waits for the Release run,
#          then checks the release has its five archives. Pushing the tag is
#          what publishes the GitHub Release.
#
# Needs gh, authenticated, for dry-run and publish. Exit status: 0 ok,
# 1 a check or a step failed, 2 usage.

source "$(dirname "$0")/lib.sh"

REPO="Zubiarka8/mini-consumes-tokens"

usage() {
  echo "usage: scripts/unix/release.sh bump <x.y.z|patch|minor|major>"
  echo "       scripts/unix/release.sh dry-run|publish [x.y.z]"
}
fail() {
  echo "failed: $*" >&2
  exit 1
}

workspace_version() {
  cargo pkgid -p mct-cli | sed -E 's/.*[#@]//'
}

case "${1:-}" in
  -h | --help) usage; exit 0 ;;
  bump | dry-run | publish) ;;
  *) usage >&2; exit 2 ;;
esac
cmd="$1"
v="${2:-}"
case "$cmd:$v" in
  bump:) usage >&2; exit 2 ;;
  dry-run:) v="$(workspace_version)" ;;
  publish:)
    git fetch -q origin main
    v="$(git show origin/main:Cargo.toml | sed -nE 's/^version = "(.*)"$/\1/p')" ;;
  bump:patch | bump:minor | bump:major)
    IFS=. read -r major minor patch <<<"$(workspace_version)"
    case "$v" in
      patch) v="$major.$minor.$((patch + 1))" ;;
      minor) v="$major.$((minor + 1)).0" ;;
      major) v="$((major + 1)).0.0" ;;
    esac ;;
esac
echo "$v" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$' || { usage >&2; exit 2; }
tag="v$v"

# `changelog_ok <file>`: the file has a dated `## [x.y.z] - YYYY-MM-DD` heading.
changelog_ok() {
  grep -Eq "^## \[$(echo "$v" | sed 's/\./\\./g')\] - [0-9]{4}-[0-9]{2}-[0-9]{2}$" "$1"
}

# `wait_run <workflow> <event> <since>`: waits for the first run of that
# workflow and event created at or after <since> (UTC, ISO 8601) to complete.
# Exits 1 unless it succeeded.
wait_run() {
  local workflow="$1" event="$2" since="$3" id="" i
  for i in $(seq 1 30); do
    id="$(gh run list -R "$REPO" --workflow "$workflow" --event "$event" --limit 5 \
      --json databaseId,createdAt --jq "[.[] | select(.createdAt >= \"$since\")] | last | .databaseId // empty")"
    [ -n "$id" ] && break
    sleep 10
  done
  [ -n "$id" ] || fail "no $workflow run started after $since"
  echo "waiting for https://github.com/$REPO/actions/runs/$id"
  gh run watch "$id" -R "$REPO" --exit-status >/dev/null ||
    fail "run $id did not succeed — scripts/unix/ci-failures.sh $id"
  echo "run $id: success"
}

bump() {
  git diff --quiet HEAD || fail "the working tree has uncommitted changes"
  local old
  old="$(workspace_version)"
  [ "$old" != "$v" ] || fail "the workspace is already at $v"
  if [ "$(git rev-parse --abbrev-ref HEAD)" = main ]; then
    git switch -c "release-$v"
  fi

  # Only `version = "<old>"` on its own line ([workspace.package]) and in
  # the `mct-* = { path = "crates/…", version = … }` dependency lines.
  sed -i.bak -E \
    -e "s/^version = \"$old\"$/version = \"$v\"/" \
    -e "s/^(mct-[a-z0-9-]+ = \{ path = \"crates\/[^\"]+\", version = )\"$old\"/\1\"$v\"/" \
    Cargo.toml
  sed -i.bak -E "s/badge\/version-$old-/badge\/version-$v-/" README.md
  rm Cargo.toml.bak README.md.bak
  ! grep -Eq "^mct-.*version = \"$old\"" Cargo.toml || fail "Cargo.toml still pins mct-* crates to $old"
  grep -q "badge/version-$v-" README.md || fail "README version badge not updated"

  local log="$LOG_DIR/release-bump.log" d
  new_log "$log"
  cargo update --workspace >>"$log" 2>&1 || fail "cargo update --workspace — see $log"
  for d in crates/*/fuzz; do
    (cd "$d" && cargo update --workspace) >>"$log" 2>&1 || fail "cargo update in $d — see $log"
  done
  [ "$(workspace_version)" = "$v" ] || fail "cargo still reports $(workspace_version)"

  git add Cargo.toml Cargo.lock README.md crates/*/fuzz/Cargo.lock
  git commit -q -m "chore(release): bump the workspace to $v"
  echo "bumped $old → $v on $(git rev-parse --abbrev-ref HEAD)"
  echo "next: write the dated '## [$v] - $(date +%F)' CHANGELOG section, commit it, then"
  echo "      scripts/unix/release.sh dry-run"
}

dry_run() {
  git diff --quiet HEAD || fail "the working tree has uncommitted changes"
  local branch
  branch="$(git rev-parse --abbrev-ref HEAD)"
  [ "$branch" != main ] || fail "run dry-run on the release branch, not main"
  [ "$(workspace_version)" = "$v" ] || fail "the workspace is at $(workspace_version), not $v — run bump first"
  changelog_ok CHANGELOG.md || fail "CHANGELOG.md has no '## [$v] - YYYY-MM-DD' section"

  "$SCRIPTS_DIR/check.sh" || fail "check.sh"
  python3 scripts/unix/tests/test_install.py || fail "installer test"
  "$SCRIPTS_DIR/publish-branch.sh" "$branch" --title "Release $v" || fail "publish-branch.sh"

  local since
  since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  gh workflow run release.yml -R "$REPO" --ref "$branch"
  wait_run release.yml workflow_dispatch "$since"
  echo "next: get the PR reviewed and merged, then scripts/unix/release.sh publish"
}

publish() {
  git fetch -q origin main --tags
  ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null || fail "tag $tag already exists"
  local sha
  sha="$(git rev-parse origin/main)"
  git show "$sha:Cargo.toml" | grep -q "^version = \"$v\"$" || fail "origin/main is not at version $v"
  changelog_ok <(git show "$sha:CHANGELOG.md") || fail "origin/main's CHANGELOG.md has no dated [$v] section"
  local ci
  ci="$(gh run list -R "$REPO" --workflow ci.yml --commit "$sha" --limit 1 --json status,conclusion --jq '.[0] | "\(.status) \(.conclusion)"')"
  [ "$ci" = "completed success" ] || fail "CI on origin/main ($sha) is '${ci:-missing}', not a success"

  local since
  since="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  git tag -a "$tag" -m "$tag" "$sha"
  git push origin "$tag"
  wait_run release.yml push "$since"

  local assets
  assets="$(gh release view "$tag" -R "$REPO" --json assets --jq '.assets | length')"
  [ "$assets" = 5 ] || fail "release $tag has $assets archive(s), expected 5"
  echo "published https://github.com/$REPO/releases/tag/$tag with 5 archives"
}

case "$cmd" in
  bump) bump ;;
  dry-run) dry_run ;;
  publish) publish ;;
esac
