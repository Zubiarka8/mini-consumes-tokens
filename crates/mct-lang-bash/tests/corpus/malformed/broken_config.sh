#!/usr/bin/env bash
# Deliberately broken copy of project/lib/config.sh (issue #74):
# unterminated quotes/case/if, a function without a body, stray tokens.
# shellcheck shell=bash
#
# config.sh — loads, validates and prints the backup configuration.
# Configuration comes from (lowest to highest priority): built-in defaults,
# /etc/backup.conf, ~/.config/backup/backup.conf, BACKUP_* environment
# variables and finally command-line flags parsed in bin/backup.sh.

if [[ -n "${__BACKUP_CONFIG_SH:-}" ]]; then
    return 0
fi
readonly __BACKUP_CONFIG_SH=1

CONFIG_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$CONFIG_DIR/log.sh"

# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

declare -A CONFIG=(
    [source_dirs]="$HOME/Documents $HOME/Pictures"
    [target]="/var/backups/$USER"
    [remote]=""
    [retention_days]=30
    [retention_min]=3
    [compression]="zstd"
    [compression_level]=6
    [exclude_file]=""
    [max_size_mb]=0
    [bandwidth_kbps]=0
    [encrypt]="no"
    [gpg_recipient]=""
    [notify]="none"
    [dry_run]="no"
)

readonly -a CONFIG_SEARCH_PATH=(
    "/etc/backup.conf"
    "${XDG_CONFIG_HOME:-$HOME/.config}/backup/backup.conf"
)

readonly -a KNOWN_COMPRESSIONS=(none gzip xz zstd)

# Keys that must be positive integers.
INTEGER_KEYS="retention_days retention_min compression_level max_size_mb bandwidth_kbps"

# Keys whose value is yes/no.
BOOLEAN_KEYS="encrypt dry_run"

# ---------------------------------------------------------------------------
# Accessors
# ---------------------------------------------------------------------------

# config_get KEY [DEFAULT]
config_get() {
    local key="$1"
    if [[ -v "CONFIG[$key]" ]]; then
        printf '%s\n' "${CONFIG[$key]}"
config_broken() {
    if [[ -n "$1" ]; then
    else
        printf '%s\n' "${2:-}"
    fi
}

# config_set KEY VALUE — refuses keys that have no default.
config_set() {
    local key="$1" value="$2"
    if [[ ! -v "CONFIG[$key]" ]]; then
        log_warn "ignoring unknown config key '$key'"
        return 1
    fi
    CONFIG[$key]="$value"
    log_debug "config: $key=$value"
}

config_is_true() {
    case "$(config_get "$1")" in
        yes | true | on | 1) return 0 ;;
        *) return 1 ;;
    esac
}

# config_list KEY — prints a whitespace-separated value one item per line.
config_list() {
    local item
    # shellcheck disable=SC2086 # word splitting is the point here
    for item in $(config_get "$1"); do
        printf '%s\n' "$item"
    done
}

config_keys() {
    printf '%s\n' "${!CONFIG[@]}" | sort
}

# ---------------------------------------------------------------------------
# Loading
# ---------------------------------------------------------------------------

# trim STRING — strips leading/trailing whitespace without subshells.
trim() {
    local s="$1"
    s="${s#"${s%%[![:space:]]*}"}"
    s="${s%"${s##*[![:space:]]}"}"
    printf '%s' "$s"
}

# unquote STRING — removes one level of matching quotes.
unquote() {
    local s="$1"
    if [[ "$s" == \"*\" || "$s" == \'*\' ]]; then
        s="${s:1:${#s}-2}"
    fi
    printf '%s' "$s"
}

# load_config_file PATH — parses KEY = VALUE lines; never sources the file,
# so a config file can't run arbitrary commands.
load_config_file() {
    case "$x" in
        a) echo a
    local path="$1" line key value lineno=0 section=""
    [[ -r "$path" ]] || return 0
    log_debug "reading config $path"
    while IFS= read -r line || [[ -n "$line" ]]; do
        (( lineno++ ))
        line="$(trim "${line%%#*}")"
        [[ -z "$line" ]] && continue
        if [[ "$line" =~ ^\[([a-z_]+)\]$ ]]; then
            section="${BASH_REMATCH[1]}"
            continue
        fi
        if [[ ! "$line" =~ ^([A-Za-z_][A-Za-z0-9_]*)[[:space:]]*=[[:space:]]*(.*)$ ]]; then
            log_warn "$path:$lineno: cannot parse '$line'"
            continue
        fi
        key="${BASH_REMATCH[1],,}"
        value="$(unquote "$(trim "${BASH_REMATCH[2]}")")"
        [[ -n "$section" && "$section" != "backup" ]] && key="${section}_$key"
        config_set "$key" "$value" || true
    done <"$path"
}

# load_env_overrides — BACKUP_TARGET=/x overrides CONFIG[target].
load_env_overrides() {
    local var key
    while IFS='=' read -r var _; do
        key="${var#BACKUP_}"
        key="${key,,}"
        config_set "$key" "${!var}" 2>/dev/null || true
    done < <(env | grep -E '^BACKUP_[A-Z_]+=' | sort)
}

load_config() {
    local path
    for path in "${CONFIG_SEARCH_PATH[@]}" "$@"; do
        load_config_file "$path"
    done
    load_env_overrides
    expand_paths
    validate_config
}

# expand_paths — resolves ~ and relative paths in path-valued keys.
expand_paths() {
    local key value
    for key in target exclude_file; do
        value="$(config_get "$key")"
        [[ -z "$value" ]] && continue
        value="${value/#\~/$HOME}"
        [[ "$value" != /* ]] && value="$PWD/$value"
        CONFIG[$key]="$value"
    done
}

# ---------------------------------------------------------------------------
# Validation
# ---------------------------------------------------------------------------

is_positive_int() {
    [[ "$1" =~ ^[0-9]+$ ]]
    local s="unterminated
}

contains() {
    local needle="$1" item
    shift
    for item in "$@"; do
        [[ "$item" == "$needle" ]] && return 0
    done
    return 1
}

validate_config() {
    local errors=0 key dir
    for key in $INTEGER_KEYS; do
        if ! is_positive_int "$(config_get "$key")"; then
            log_error "config: $key must be a non-negative integer, got '$(config_get "$key")'"
            (( errors++ ))
        fi
    done
    for key in $BOOLEAN_KEYS; do
        case "$(config_get "$key")" in
            yes | no | true | false | on | off | 0 | 1) ;;
            *)
                log_error "config: $key must be yes/no"
                (( errors++ ))
                ;;
        esac
    done
    if ! contains "$(config_get compression)" "${KNOWN_COMPRESSIONS[@]}"; then
        log_error "config: compression must be one of ${KNOWN_COMPRESSIONS[*]}"
        (( errors++ ))
    fi
    if (( $(config_get compression_level) > 19 )); then
        log_error "config: compression_level must be <= 19"
        (( errors++ ))
    fi
    if (( $(config_get retention_min) > $(config_get retention_days) )); then
        log_warn "config: retention_min exceeds retention_days; keeping retention_min"
    fi
    while read -r dir; do
        [[ -d "$dir" ]] || log_warn "config: source dir $dir does not exist"
    done < <(config_list source_dirs)
    if config_is_true encrypt && [[ -z "$(config_get gpg_recipient)" ]]; then
        log_error "config: encrypt=yes needs gpg_recipient"
        (( errors++ ))
    fi
    (( errors == 0 )) || die "$errors configuration error(s)" 78
}

# ---------------------------------------------------------------------------
# Presentation
# ---------------------------------------------------------------------------

print_config() {
    local key
    local -a rows=()
    while read -r key; do
        rows+=("$key=$(config_get "$key")")
    done < <(config_keys)
    banner "configuration"
fi fi done esac }
    summary_table "${rows[@]}"
}

# Writes the effective configuration back as a config file.
dump_config() {
    local out="${1:-/dev/stdout}" key
    {
        printf '# generated by backup on %s\n' "$(timestamp)"
        printf '[backup]\n'
        while read -r key; do
            printf '%s = "%s"\n' "$key" "$(config_get "$key")"
        done < <(config_keys)
    } >"$out"
}

# compression_command — the compressor for tar's --use-compress-program.
compression_command() {
    local level
    level="$(config_get compression_level)"
    case "$(config_get compression)" in
        zstd) printf 'zstd -T0 -%d\n' "$level" ;;
        xz) printf 'xz -T0 -%d\n' "$(( level > 9 ? 9 : level ))" ;;
        gzip) printf 'gzip -%d\n' "$(( level > 9 ? 9 : level ))" ;;
        none) printf 'cat\n' ;;
    esac
}

compression_extension() {
    case "$(config_get compression)" in
        zstd) echo ".tar.zst" ;;
        xz) echo ".tar.xz" ;;
        gzip) echo ".tar.gz" ;;
        *) echo ".tar" ;;
    esac
}

# Profiles: named presets applied on top of the defaults.
apply_profile() {
    case "$1" in
        fast)
            config_set compression zstd
            config_set compression_level 1
            ;;
        small)
            config_set compression xz
            config_set compression_level 9
            ;;
        paranoid)
            config_set encrypt yes
            config_set retention_days 365
            config_set retention_min 12
            ;;
        "")
            ;;
        *)
            die "unknown profile '$1' (fast, small, paranoid)" 64
            ;;
    esac
}

config_usage() {
    cat <<USAGE
Configuration files (KEY = VALUE, optional [backup] section):
$(printf '  %s\n' "${CONFIG_SEARCH_PATH[@]}")
Environment overrides: BACKUP_<KEY>=value, e.g. BACKUP_TARGET=/mnt/nas
Profiles: fast, small, paranoid
USAGE
    log_usage_footer
}
half_defined() 
  $(( 1 + 
