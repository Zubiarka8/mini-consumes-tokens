#!/usr/bin/env bash
#
# backup — entry point of the backup tool.
#
#   backup [global options] <command> [command options]
#
# Commands: run, list, verify, restore, prune, doctor, config, speedtest.

set -Eeuo pipefail
shopt -s nullglob extglob

BIN_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
LIB_DIR="${BACKUP_LIB_DIR:-$BIN_DIR/../lib}"

# shellcheck source=../lib/log.sh
source "$LIB_DIR/log.sh"
# shellcheck source=../lib/config.sh
source "$LIB_DIR/config.sh"
# shellcheck source=../lib/fs.sh
. "$LIB_DIR/fs.sh"
# shellcheck source=../lib/remote.sh
. "$LIB_DIR/remote.sh"

VERSION="2.4.1"
PROGRAM="${0##*/}"
PROFILE=""
declare -a EXTRA_CONFIGS=()
declare -a RESTORE_PATTERNS=()
COMMAND=""
FORCE=0
QUIET=0

usage() {
    cat <<EOF
$PROGRAM $VERSION — incremental-friendly tar backups

Usage: $PROGRAM [options] <command> [args]

Commands:
  run                 create a new archive and upload it
  list [--remote]     list local (or remote) archives
  verify              check archive checksums against the manifest
  restore [ARCHIVE] DEST [PATTERN...]
  prune               apply retention locally and remotely
  doctor              check dependencies, config and remote
  config [--dump F]   print the effective configuration
  speedtest [MB]      measure upload throughput

Options:
  -c, --config FILE   extra config file (repeatable)
  -p, --profile NAME  fast | small | paranoid
  -n, --dry-run       show what would happen
  -f, --force         run even if nothing changed
  -q, --quiet         only warnings and errors
  -v, --verbose       debug output
  -h, --help          this help
      --version       print the version
EOF
    config_usage
}

version() {
    printf '%s %s (bash %s)\n' "$PROGRAM" "$VERSION" "${BASH_VERSION}"
}

# parse_args "$@" — sets globals; leaves the command's own args in ARGS.
declare -a ARGS=()
parse_args() {
    while (( $# )); do
        case "$1" in
            -c | --config)
                [[ -n "${2:-}" ]] || die "$1 needs a file" 64
                EXTRA_CONFIGS+=("$2")
                shift 2
                ;;
            --config=*)
                EXTRA_CONFIGS+=("${1#*=}")
                shift
                ;;
            -p | --profile)
                PROFILE="${2:-}"
                shift 2
                ;;
            -n | --dry-run)
                DRY_RUN_FLAG=1
                shift
                ;;
            -f | --force)
                FORCE=1
                shift
                ;;
            -q | --quiet)
                QUIET=1
                LOG_LEVEL=warn
                shift
                ;;
            -v | --verbose)
                LOG_LEVEL=debug
                shift
                ;;
            -h | --help)
                usage
                exit 0
                ;;
            --version)
                version
                exit 0
                ;;
            --)
                shift
                ARGS+=("$@")
                break
                ;;
            -*)
                die "unknown option $1 (see --help)" 64
                ;;
            *)
                if [[ -z "$COMMAND" ]]; then
                    COMMAND="$1"
                else
                    ARGS+=("$1")
                fi
                shift
                ;;
        esac
    done
    [[ -n "$COMMAND" ]] || { usage >&2; exit 64; }
}

setup() {
    load_config "${EXTRA_CONFIGS[@]}"
    apply_profile "$PROFILE"
    if [[ "${DRY_RUN_FLAG:-0}" == 1 ]]; then
        config_set dry_run yes
    fi
    install_traps
    rotate_log
    log_dump_settings
}

# ---------------------------------------------------------------------------
# Commands
# ---------------------------------------------------------------------------

cmd_run() {
    local target work list archive
    target="$(config_get target)"
    mkdir -p "$target"
    clean_stale_workdirs
    if (( ! FORCE )) && ! changed_since_last_run "$target"; then
        log_info "sources unchanged since the last run; use --force to back up anyway"
        return 0
    fi
    ensure_space "$target" 512
    work="$(make_workdir)"
    list="$work/files"
    (( QUIET )) || banner "backup $(hostname) → $target"
    timer_start run
    collect_files "$list"
    archive="$(create_archive "$list" "$target/$(archive_name)")"
    if [[ -n "$archive" ]] && ! config_is_true dry_run; then
        write_manifest "$target"
        upload "$archive" "$target/$MANIFEST_NAME"
    fi
    apply_retention "$target"
    local elapsed
    elapsed="$(timer_stop run)"
    (( QUIET )) || fs_stats
    notify ok "backup finished in $(format_duration "$elapsed")"
}

cmd_list() {
    local target archive
    if [[ "${ARGS[0]:-}" == "--remote" ]]; then
        remote_list
        return
    fi
    target="$(config_get target)"
    while read -r archive; do
        printf '%-60s %10s\n' "$(basename "$archive")" "$(format_bytes "$(file_size "$archive")")"
    done < <(list_archives "$target")
}

cmd_verify() {
    local target
    target="$(config_get target)"
    if verify_manifest "$target"; then
        log_ok "all archives in $target match the manifest"
    else
        notify failed "manifest verification failed in $target"
        return 1
    fi
}

cmd_restore() {
    local target archive dest
    target="$(config_get target)"
    case "${#ARGS[@]}" in
        0) die "restore needs a destination" 64 ;;
        1)
            archive="$(pick_archive "$target")"
            dest="${ARGS[0]}"
            ;;
        *)
            archive="${ARGS[0]}"
            dest="${ARGS[1]}"
            RESTORE_PATTERNS=("${ARGS[@]:2}")
            ;;
    esac
    if [[ ! -f "$archive" && -f "$target/$archive" ]]; then
        archive="$target/$archive"
    elif [[ ! -f "$archive" ]]; then
        log_info "$archive not found locally; downloading"
        download "$archive" "$target"
        archive="$target/$archive"
    fi
    if [[ -d "$dest" && -n "$(ls -A "$dest")" ]] && ! confirm "$dest is not empty. Restore into it anyway?"; then
        die "restore cancelled" 1
    fi
    restore_archive "$archive" "$dest" "${RESTORE_PATTERNS[@]}"
}

cmd_prune() {
    apply_retention "$(config_get target)"
    remote_configured && remote_prune "$(config_get retention_min)"
}

cmd_doctor() {
    local problems=0
    banner "doctor"
    require_commands tar find sort awk
    local tool
    for tool in zstd xz gzip gpg rsync aws curl; do
        if command -v "$tool" >/dev/null; then
            log_ok "$tool: $(command -v "$tool")"
        else
            log_warn "$tool: not installed"
        fi
    done
    (validate_config) || (( problems++ ))
    check_remote || (( problems++ ))
    local target
    target="$(config_get target)"
    [[ -d "$target" ]] && du_summary "$target" 5
    (( problems == 0 )) && log_ok "no problems found" || log_error "$problems problem(s) found"
    return "$problems"
}

cmd_config() {
    if [[ "${ARGS[0]:-}" == "--dump" ]]; then
        dump_config "${ARGS[1]:-/dev/stdout}"
    else
        print_config
    fi
}

cmd_speedtest() {
    remote_speedtest "${ARGS[0]:-8}"
}

# dispatch — looks up cmd_<command>, accepting unambiguous prefixes.
dispatch() {
    local fn="cmd_${COMMAND//-/_}" candidate
    local -a matches=()
    if ! declare -F "$fn" >/dev/null; then
        while read -r _ _ candidate; do
            [[ "$candidate" == "cmd_${COMMAND}"* ]] && matches+=("$candidate")
        done < <(declare -F)
        case "${#matches[@]}" in
            0) die "unknown command '$COMMAND' (see --help)" 64 ;;
            1) fn="${matches[0]}" ;;
            *) die "ambiguous command '$COMMAND': ${matches[*]#cmd_}" 64 ;;
        esac
    fi
    log_debug "dispatching to $fn ${ARGS[*]:-}"
    "$fn"
}

main() {
    parse_args "$@"
    setup
    local lock
    lock="$(config_get target)/.lock"
    mkdir -p "$(dirname "$lock")"
    case "$COMMAND" in
        list | config | doctor) dispatch ;;
        *) with_lock "$lock" dispatch ;;
    esac
}

# Allow sourcing this file from tests without running main.
if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    LC_ALL=C TZ=UTC main "$@"
fi

# `backup version` works as a command too.
function cmd_version {
    version
}

cmd_help() {
    usage
}
