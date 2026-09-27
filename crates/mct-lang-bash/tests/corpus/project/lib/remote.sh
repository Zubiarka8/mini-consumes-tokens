#!/usr/bin/env bash
# shellcheck shell=bash
#
# remote.sh — copying archives off-site (rsync over ssh or S3-compatible
# storage) and sending notifications when a run finishes.

if [[ -n "${__BACKUP_REMOTE_SH:-}" ]]; then
    return 0
fi
readonly __BACKUP_REMOTE_SH=1

REMOTE_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$REMOTE_DIR/log.sh"
source "$REMOTE_DIR/config.sh"
source "$REMOTE_DIR/fs.sh"

SSH_OPTS=(-o BatchMode=yes -o ConnectTimeout=10 -o ServerAliveInterval=30)
RSYNC_OPTS=(--archive --partial --human-readable --compress)
declare -g REMOTE_KIND=""
declare -g REMOTE_HOST=""
declare -g REMOTE_PATH=""
S3_ENDPOINT="${S3_ENDPOINT:-}"
WEBHOOK_TIMEOUT=10

# parse_remote URL — sets REMOTE_KIND/HOST/PATH.
#   ssh://user@host/path, user@host:path, s3://bucket/prefix, file:///path
parse_remote() {
    local url="$1"
    REMOTE_KIND="" REMOTE_HOST="" REMOTE_PATH=""
    case "$url" in
        "")
            return 1
            ;;
        ssh://*)
            url="${url#ssh://}"
            REMOTE_KIND=ssh
            REMOTE_HOST="${url%%/*}"
            REMOTE_PATH="/${url#*/}"
            ;;
        s3://*)
            url="${url#s3://}"
            REMOTE_KIND=s3
            REMOTE_HOST="${url%%/*}"
            REMOTE_PATH="${url#*/}"
            ;;
        file://*)
            REMOTE_KIND=file
            REMOTE_PATH="${url#file://}"
            ;;
        *@*:*)
            REMOTE_KIND=ssh
            REMOTE_HOST="${url%%:*}"
            REMOTE_PATH="${url#*:}"
            ;;
        *)
            log_error "unsupported remote '$url'"
            return 1
            ;;
    esac
    log_debug "remote: kind=$REMOTE_KIND host=$REMOTE_HOST path=$REMOTE_PATH"
}

remote_configured() {
    [[ -n "$(config_get remote)" ]] && parse_remote "$(config_get remote)"
}

# remote_exec COMMAND... — runs a command on the ssh remote.
remote_exec() {
    ssh "${SSH_OPTS[@]}" "$REMOTE_HOST" -- "$@"
}

bandwidth_args() {
    local kbps
    kbps="$(config_get bandwidth_kbps)"
    (( kbps > 0 )) && printf -- '--bwlimit=%d\n' "$kbps"
    return 0
}

# upload_ssh FILE...
upload_ssh() {
    local -a extra
    mapfile -t extra < <(bandwidth_args)
    remote_exec mkdir -p "$REMOTE_PATH" || return 1
    retry 3 5 rsync "${RSYNC_OPTS[@]}" "${extra[@]}" -e "ssh ${SSH_OPTS[*]}" "$@" "$REMOTE_HOST:$REMOTE_PATH/"
}

# upload_s3 FILE...
upload_s3() {
    local file key
    local -a endpoint=()
    [[ -n "$S3_ENDPOINT" ]] && endpoint=(--endpoint-url "$S3_ENDPOINT")
    for file in "$@"; do
        key="${REMOTE_PATH%/}/$(basename "$file")"
        retry 3 5 aws "${endpoint[@]}" s3 cp --only-show-errors "$file" "s3://$REMOTE_HOST/$key" || return 1
    done
}

upload_file_remote() {
    mkdir -p "$REMOTE_PATH" && cp -f -- "$@" "$REMOTE_PATH/"
}

# upload FILE... — dispatches on REMOTE_KIND.
upload() {
    remote_configured || { log_info "no remote configured; skipping upload"; return 0; }
    if config_is_true dry_run; then
        log_info "[dry-run] would upload $# file(s) to $REMOTE_KIND:$REMOTE_HOST$REMOTE_PATH"
        return 0
    fi
    timer_start upload
    case "$REMOTE_KIND" in
        ssh) upload_ssh "$@" ;;
        s3) upload_s3 "$@" ;;
        file) upload_file_remote "$@" ;;
    esac
    local status=$?
    timer_stop upload >/dev/null
    return "$status"
}

# remote_list — archives on the remote, one name per line.
remote_list() {
    remote_configured || return 1
    case "$REMOTE_KIND" in
        ssh) remote_exec ls -1 "$REMOTE_PATH" | grep "^$ARCHIVE_PREFIX-" ;;
        s3) aws s3 ls "s3://$REMOTE_HOST/${REMOTE_PATH%/}/" | awk '{ print $4 }' ;;
        file) ls -1 "$REMOTE_PATH" | grep "^$ARCHIVE_PREFIX-" ;;
    esac
}

# remote_prune KEEP — deletes all but the newest KEEP remote archives.
remote_prune() {
    local keep="$1" name
    local -a names
    mapfile -t names < <(remote_list | sort -r)
    (( ${#names[@]} > keep )) || return 0
    for name in "${names[@]:keep}"; do
        log_info "pruning remote $name"
        case "$REMOTE_KIND" in
            ssh) remote_exec rm -f "$REMOTE_PATH/$name" ;;
            s3) aws s3 rm "s3://$REMOTE_HOST/${REMOTE_PATH%/}/$name" ;;
            file) rm -f -- "$REMOTE_PATH/$name" ;;
        esac
    done
}

# download NAME DEST
download() {
    local name="$1" dest="$2"
    remote_configured || die "no remote configured" 64
    case "$REMOTE_KIND" in
        ssh) retry 3 5 rsync "${RSYNC_OPTS[@]}" -e "ssh ${SSH_OPTS[*]}" "$REMOTE_HOST:$REMOTE_PATH/$name" "$dest/" ;;
        s3) aws s3 cp "s3://$REMOTE_HOST/${REMOTE_PATH%/}/$name" "$dest/" ;;
        file) cp -f -- "$REMOTE_PATH/$name" "$dest/" ;;
    esac
}

# check_remote — connectivity test used by `backup doctor`.
check_remote() {
    remote_configured || { log_info "remote: not configured"; return 0; }
    case "$REMOTE_KIND" in
        ssh)
            if remote_exec true; then
                log_ok "ssh to $REMOTE_HOST works"
            else
                log_error "cannot ssh to $REMOTE_HOST"
                return 1
            fi
            ;;
        s3)
            require_commands aws
            aws s3 ls "s3://$REMOTE_HOST" >/dev/null && log_ok "bucket $REMOTE_HOST reachable"
            ;;
        file)
            [[ -w "$REMOTE_PATH" ]] && log_ok "$REMOTE_PATH writable"
            ;;
    esac
}

# ---------------------------------------------------------------------------
# Notifications
# ---------------------------------------------------------------------------

json_escape() {
    local s="$1"
    s="${s//\\/\\\\}"
    s="${s//\"/\\\"}"
    s="${s//$'\n'/\\n}"
    s="${s//$'\t'/\\t}"
    printf '%s' "$s"
}

# json_object KEY=VALUE... — flat JSON object with string values.
json_object() {
    local pair sep="" out="{"
    for pair in "$@"; do
        out+="$sep\"$(json_escape "${pair%%=*}")\":\"$(json_escape "${pair#*=}")\""
        sep=","
    done
    printf '%s}\n' "$out"
}

notify_webhook() {
    local url="$1" payload="$2"
    curl --silent --show-error --fail --max-time "$WEBHOOK_TIMEOUT" \
        -H 'Content-Type: application/json' \
        --data "$payload" "$url" >/dev/null
}

notify_mail() {
    local to="$1" subject="$2" body="$3"
    if command -v mail >/dev/null; then
        printf '%s\n' "$body" | mail -s "$subject" "$to"
    else
        log_warn "mail(1) not installed; cannot notify $to"
        return 1
    fi
}

notify_desktop() {
    if command -v notify-send >/dev/null; then
        notify-send "backup" "$1"
    elif command -v osascript >/dev/null; then
        osascript -e "display notification \"$(json_escape "$1")\" with title \"backup\""
    fi
}

# notify STATUS MESSAGE — sends through the configured channel.
notify() {
    local status="$1" message="$2" channel target
    channel="$(config_get notify)"
    target="${channel#*:}"
    case "$channel" in
        none | "") return 0 ;;
        webhook:*)
            notify_webhook "$target" "$(json_object "status=$status" "message=$message" "host=$(hostname)" "at=$(timestamp)")"
            ;;
        mail:*)
            notify_mail "$target" "backup $status on $(hostname)" "$message"
            ;;
        desktop)
            notify_desktop "$status: $message"
            ;;
        *)
            log_warn "unknown notify channel '$channel'"
            ;;
    esac || log_warn "notification via $channel failed"
}

# Measures upload throughput to the remote with a throwaway file.
remote_speedtest() {
    local size_mb="${1:-8}" work file started elapsed
    work="$(make_workdir)"
    file="$work/speedtest.bin"
    head -c "$(( size_mb * 1024 * 1024 ))" /dev/urandom >"$file"
    started="$(now_epoch)"
    upload "$file" || return 1
    elapsed=$(( $(now_epoch) - started ))
    (( elapsed > 0 )) || elapsed=1
    log_info "upload speed: $(( size_mb * 1024 / elapsed )) KiB/s"
}

# remote_verify NAME — compares the remote copy's checksum with the local one.
remote_verify() {
    local name="$1" local_sum remote_sum
    local_sum="$(checksum "$(config_get target)/$name")"
    case "$REMOTE_KIND" in
        ssh) remote_sum="$(remote_exec sha256sum "$REMOTE_PATH/$name" | cut -d' ' -f1)" ;;
        file) remote_sum="$(checksum "$REMOTE_PATH/$name")" ;;
        *)
            log_info "remote_verify: not supported for $REMOTE_KIND"
            return 0
            ;;
    esac
    if [[ "$local_sum" != "$remote_sum" ]]; then
        log_error "remote copy of $name differs (local $local_sum, remote $remote_sum)"
        return 1
    fi
    log_ok "remote copy of $name verified"
}

# Loads an ssh key into a short-lived agent for the duration of the run.
function start_ssh_agent {
    local key="${1:-$HOME/.ssh/id_ed25519}"
    [[ -r "$key" ]] || return 0
    eval "$(ssh-agent -s)" >/dev/null
    on_exit "ssh-agent -k >/dev/null"
    ssh-add -q "$key" || log_warn "could not add $key to the agent"
}

# Retries notify with exponential backoff, via the generic retry helper.
notify_reliably() {
    retry 4 2 notify "$@"
}

# Remote free space in MiB (ssh and file remotes only).
remote_free_mb() {
    case "$REMOTE_KIND" in
        ssh) remote_exec df -Pm "$REMOTE_PATH" | awk 'NR == 2 { print $4 }' ;;
        file) disk_free_mb "$REMOTE_PATH" ;;
        *) printf '%s\n' "-1" ;;
    esac
}
