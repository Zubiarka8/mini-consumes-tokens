#!/usr/bin/env bash
# shellcheck shell=bash
#
# log.sh — logging, colours, timing and error handling shared by every
# script of the backup tool. Sourced first; depends on nothing else.

if [[ -n "${__BACKUP_LOG_SH:-}" ]]; then
    return 0
fi
readonly __BACKUP_LOG_SH=1

# ---------------------------------------------------------------------------
# Levels and colours
# ---------------------------------------------------------------------------

declare -A LOG_LEVELS=([debug]=10 [info]=20 [warn]=30 [error]=40 [fatal]=50)
LOG_LEVEL="${LOG_LEVEL:-info}"
LOG_FILE="${LOG_FILE:-}"
LOG_TIMESTAMPS=1
LOG_PREFIX="backup"
export LOG_LEVEL LOG_FILE

if [[ -t 2 && -z "${NO_COLOR:-}" ]]; then
    C_RESET=$'\033[0m'
    C_DIM=$'\033[2m'
    C_RED=$'\033[31m'
    C_YELLOW=$'\033[33m'
    C_GREEN=$'\033[32m'
    C_BLUE=$'\033[34m'
else
    C_RESET=""
    C_DIM=""
    C_RED=""
    C_YELLOW=""
    C_GREEN=""
    C_BLUE=""
fi

# Seconds since the epoch, with a fallback for shells without EPOCHSECONDS.
now_epoch() {
    if [[ -n "${EPOCHSECONDS:-}" ]]; then
        printf '%s\n' "$EPOCHSECONDS"
    else
        date +%s
    fi
}

# ISO-8601 timestamp in UTC.
timestamp() {
    date -u +"%Y-%m-%dT%H:%M:%SZ"
}

# Numeric value of a level name; unknown names map to "info".
level_value() {
    local name="${1,,}"
    printf '%s\n' "${LOG_LEVELS[$name]:-${LOG_LEVELS[info]}}"
}

# Whether a message of level $1 passes the configured LOG_LEVEL.
log_enabled() {
    local wanted current
    wanted=$(level_value "$1")
    current=$(level_value "$LOG_LEVEL")
    (( wanted >= current ))
}

colour_for() {
    case "$1" in
        debug) printf '%s' "$C_DIM" ;;
        info) printf '%s' "$C_BLUE" ;;
        warn) printf '%s' "$C_YELLOW" ;;
        error | fatal) printf '%s' "$C_RED" ;;
        ok) printf '%s' "$C_GREEN" ;;
        *) printf '' ;;
    esac
}

# log LEVEL MESSAGE... — the one function every other logger goes through.
log() {
    local level="$1"
    shift
    log_enabled "$level" || return 0
    local line="$*"
    if (( LOG_TIMESTAMPS )); then
        line="$(timestamp) ${level^^} [$LOG_PREFIX] $line"
    else
        line="${level^^} [$LOG_PREFIX] $line"
    fi
    printf '%s%s%s\n' "$(colour_for "$level")" "$line" "$C_RESET" >&2
    if [[ -n "$LOG_FILE" ]]; then
        printf '%s\n' "$line" >>"$LOG_FILE"
    fi
}

log_debug() { log debug "$@"; }
log_info() { log info "$@"; }
log_warn() { log warn "$@"; }
log_error() { log error "$@"; }

log_ok() {
    printf '%s✔ %s%s\n' "$(colour_for ok)" "$*" "$C_RESET" >&2
}

# die MESSAGE [STATUS] — log a fatal error and exit.
die() {
    local message="$1" status="${2:-1}"
    log fatal "$message"
    print_stack 1
    exit "$status"
}

# Prints the call stack, skipping $1 frames (default 0).
print_stack() {
    local skip="${1:-0}" i
    for (( i = skip + 1; i < ${#FUNCNAME[@]}; i++ )); do
        log_debug "  at ${FUNCNAME[$i]} (${BASH_SOURCE[$i]}:${BASH_LINENO[$((i - 1))]})"
    done
}

# ---------------------------------------------------------------------------
# Timing
# ---------------------------------------------------------------------------

declare -A __TIMERS=()

timer_start() {
    __TIMERS[$1]=$(now_epoch)
}

timer_stop() {
    local name="$1" started elapsed
    started="${__TIMERS[$name]:-}"
    if [[ -z "$started" ]]; then
        log_warn "timer '$name' was never started"
        return 1
    fi
    elapsed=$(( $(now_epoch) - started ))
    unset '__TIMERS[$name]'
    log_info "$name took $(format_duration "$elapsed")"
    printf '%s\n' "$elapsed"
}

# format_duration SECONDS → "1h 02m 03s"
format_duration() {
    local total="$1" h m s
    h=$(( total / 3600 ))
    m=$(( (total % 3600) / 60 ))
    s=$(( total % 60 ))
    if (( h > 0 )); then
        printf '%dh %02dm %02ds\n' "$h" "$m" "$s"
    elif (( m > 0 )); then
        printf '%dm %02ds\n' "$m" "$s"
    else
        printf '%ds\n' "$s"
    fi
}

# format_bytes BYTES → human-readable size.
format_bytes() {
    local bytes="$1" unit=0
    local -a units=(B KiB MiB GiB TiB)
    while (( bytes >= 1024 && unit < ${#units[@]} - 1 )); do
        bytes=$(( bytes / 1024 ))
        (( unit++ ))
    done
    printf '%d %s\n' "$bytes" "${units[$unit]}"
}

# ---------------------------------------------------------------------------
# Error handling and cleanup
# ---------------------------------------------------------------------------

declare -a __CLEANUP_HOOKS=()

# on_exit COMMAND — registers COMMAND to run (LIFO) when the script exits.
on_exit() {
    __CLEANUP_HOOKS+=("$*")
}

run_cleanup() {
    local status=$? i
    for (( i = ${#__CLEANUP_HOOKS[@]} - 1; i >= 0; i-- )); do
        log_debug "cleanup: ${__CLEANUP_HOOKS[$i]}"
        eval "${__CLEANUP_HOOKS[$i]}" || log_warn "cleanup hook failed: ${__CLEANUP_HOOKS[$i]}"
    done
    __CLEANUP_HOOKS=()
    return "$status"
}

on_error() {
    local status=$? line="${1:-?}"
    log_error "command failed with status $status at line $line: ${BASH_COMMAND}"
    print_stack 1
    return "$status"
}

install_traps() {
    set -o errtrace
    trap 'on_error "$LINENO"' ERR
    trap run_cleanup EXIT
    trap 'log_warn "interrupted"; exit 130' INT TERM
}

# retry ATTEMPTS DELAY COMMAND... — reruns COMMAND until it succeeds.
retry() {
    local attempts="$1" delay="$2" n=1
    shift 2
    until "$@"; do
        if (( n >= attempts )); then
            log_error "giving up after $n attempts: $*"
            return 1
        fi
        log_warn "attempt $n/$attempts failed: $* — retrying in ${delay}s"
        sleep "$delay"
        (( n++ ))
        delay=$(( delay * 2 ))
    done
}

# with_lock FILE COMMAND... — runs COMMAND holding an exclusive flock.
with_lock() {
    local lockfile="$1"
    shift
    (
        flock -n 9 || die "another run holds $lockfile" 75
        "$@"
    ) 9>"$lockfile"
}

# require_commands NAME... — dies listing every missing executable.
require_commands() {
    local missing=() cmd
    for cmd in "$@"; do
        command -v "$cmd" >/dev/null 2>&1 || missing+=("$cmd")
    done
    if (( ${#missing[@]} )); then
        die "missing required commands: ${missing[*]}" 127
    fi
}

# confirm PROMPT — yes/no question, "no" when stdin is not a terminal.
confirm() {
    local answer
    [[ -t 0 ]] || return 1
    read -r -p "$1 [y/N] " answer
    case "$answer" in
        [yY] | [yY][eE][sS] | [sS][iI] | sí) return 0 ;;
        *) return 1 ;;
    esac
}

# progress CURRENT TOTAL LABEL — single-line progress bar on stderr.
progress() {
    local current="$1" total="$2" label="${3:-}" width=30 filled
    (( total > 0 )) || total=1
    filled=$(( current * width / total ))
    printf '\r%s [%-*s] %3d%% %s' \
        "$LOG_PREFIX" "$width" "$(printf '%*s' "$filled" '' | tr ' ' '#')" \
        $(( current * 100 / total )) "$label" >&2
    if (( current >= total )); then
        printf '\n' >&2
    fi
}

# summary_table KEY=VALUE... — aligned two-column table.
summary_table() {
    local pair key value width=0
    for pair in "$@"; do
        key="${pair%%=*}"
        (( ${#key} > width )) && width=${#key}
    done
    for pair in "$@"; do
        key="${pair%%=*}"
        value="${pair#*=}"
        printf '  %-*s  %s\n' "$width" "$key" "$value"
    done
}

# banner TEXT — boxed heading, unicode box-drawing on purpose.
banner() {
    local text="$*" line
    line=$(printf '%*s' $(( ${#text} + 2 )) '' | tr ' ' '─')
    printf '┌%s┐\n│ %s │\n└%s┘\n' "$line" "$text" "$line"
}

# Writes a multi-line usage text via heredoc, with parameter expansion.
log_usage_footer() {
    cat <<EOF
Logging:
  LOG_LEVEL=${LOG_LEVEL} (debug, info, warn, error)
  LOG_FILE=${LOG_FILE:-<stderr only>}
  NO_COLOR=${NO_COLOR:-} disables colours
EOF
}

# Quoted heredoc: nothing inside is expanded or executed.
log_help_literal() {
    cat <<'EOF'
Use $LOG_LEVEL to change verbosity; `commands` here are not run.
EOF
}

# Rotates LOG_FILE when it grows past $1 bytes (default 1 MiB).
rotate_log() {
    local limit="${1:-1048576}" size
    [[ -n "$LOG_FILE" && -f "$LOG_FILE" ]] || return 0
    size=$(wc -c <"$LOG_FILE")
    if (( size > limit )); then
        mv -f "$LOG_FILE" "$LOG_FILE.1"
        : >"$LOG_FILE"
        log_info "rotated $LOG_FILE ($(format_bytes "$size"))"
    fi
}

# Dumps every LOG_* variable, for --debug output.
log_dump_settings() {
    local var
    for var in "${!LOG_@}"; do
        log_debug "$var=${!var}"
    done
}
