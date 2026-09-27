#!/usr/bin/env bash
# shellcheck shell=bash
#
# fs.sh — local filesystem work: building file lists, creating archives,
# checksums, retention and restore.

if [[ -n "${__BACKUP_FS_SH:-}" ]]; then
    return 0
fi
readonly __BACKUP_FS_SH=1

FS_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
. "$FS_DIR/log.sh"
. "$FS_DIR/config.sh"

ARCHIVE_PREFIX="backup"
MANIFEST_NAME="MANIFEST.sha256"
declare -i FILES_SEEN=0
declare -i BYTES_SEEN=0
TMP_ROOT="${TMPDIR:-/tmp}"

# make_workdir — private temp dir, removed on exit.
make_workdir() {
    local dir
    dir="$(mktemp -d "$TMP_ROOT/backup.XXXXXX")" || die "cannot create a temp dir under $TMP_ROOT"
    chmod 700 "$dir"
    on_exit "rm -rf -- '$dir'"
    printf '%s\n' "$dir"
}

# archive_name [WHEN] — backup-<host>-<YYYYmmdd-HHMMSS><ext>
archive_name() {
    local when="${1:-$(date +%Y%m%d-%H%M%S)}" host
    host="$(hostname -s 2>/dev/null || uname -n)"
    printf '%s-%s-%s%s\n' "$ARCHIVE_PREFIX" "${host,,}" "$when" "$(compression_extension)"
}

# Parses the timestamp out of an archive name; prints epoch seconds.
archive_epoch() {
    local name="${1##*/}" stamp
    if [[ "$name" =~ -([0-9]{8})-([0-9]{6})\. ]]; then
        stamp="${BASH_REMATCH[1]} ${BASH_REMATCH[2]:0:2}:${BASH_REMATCH[2]:2:2}:${BASH_REMATCH[2]:4:2}"
        date -d "$stamp" +%s 2>/dev/null || date -j -f "%Y%m%d %H:%M:%S" "$stamp" +%s
    else
        return 1
    fi
}

# exclude_args — prints one --exclude=PATTERN per line for tar.
exclude_args() {
    local file pattern
    file="$(config_get exclude_file)"
    printf -- '--exclude=%s\n' '*.tmp' '*.swp' '.cache' 'node_modules' '.git/objects'
    [[ -n "$file" && -r "$file" ]] || return 0
    while IFS= read -r pattern; do
        [[ -z "$pattern" || "$pattern" == \#* ]] && continue
        printf -- '--exclude=%s\n' "$pattern"
    done <"$file"
}

# collect_files OUT — NUL-separated list of every file to back up.
collect_files() {
    local out="$1" dir max_bytes
    max_bytes=$(( $(config_get max_size_mb) * 1024 * 1024 ))
    : >"$out"
    while read -r dir; do
        [[ -d "$dir" ]] || continue
        find "$dir" -xdev -type f -print0 2>/dev/null
    done < <(config_list source_dirs) |
        while IFS= read -r -d '' file; do
            local size
            size=$(stat -c %s "$file" 2>/dev/null || stat -f %z "$file")
            if (( max_bytes > 0 && size > max_bytes )); then
                log_debug "skipping large file $file ($(format_bytes "$size"))"
                continue
            fi
            FILES_SEEN+=1
            BYTES_SEEN+=size
            printf '%s\0' "$file"
        done >"$out"
    log_info "collected $(tr -cd '\0' <"$out" | wc -c) files"
}

# create_archive LIST DEST — tar + compressor, optionally gpg-encrypted.
create_archive() {
    local list="$1" dest="$2" compressor
    local -a excludes
    mapfile -t excludes < <(exclude_args)
    compressor="$(compression_command)"
    timer_start archive
    if config_is_true dry_run; then
        log_info "[dry-run] would write $dest from $(tr -cd '\0' <"$list" | wc -c) files"
        timer_stop archive >/dev/null
        return 0
    fi
    if config_is_true encrypt; then
        tar --null -T "$list" "${excludes[@]}" -cf - 2>/dev/null |
            $compressor |
            gpg --batch --yes --encrypt --recipient "$(config_get gpg_recipient)" -o "$dest.gpg" ||
            die "archive creation failed"
        dest="$dest.gpg"
    else
        tar --null -T "$list" "${excludes[@]}" -cf - 2>/dev/null | $compressor >"$dest" ||
            die "archive creation failed"
    fi
    timer_stop archive >/dev/null
    log_ok "wrote $(basename "$dest") ($(format_bytes "$(file_size "$dest")"))"
    printf '%s\n' "$dest"
}

file_size() {
    stat -c %s "$1" 2>/dev/null || stat -f %z "$1"
}

# checksum FILE — sha256 using whichever tool exists.
checksum() {
    if command -v sha256sum >/dev/null; then
        sha256sum "$1" | cut -d' ' -f1
    else
        shasum -a 256 "$1" | awk '{ print $1 }'
    fi
}

# write_manifest DIR — records the checksum of every archive in DIR.
write_manifest() {
    local dir="$1" archive
    (
        cd "$dir" || exit 1
        for archive in "$ARCHIVE_PREFIX"-*; do
            [[ -f "$archive" ]] || continue
            printf '%s  %s\n' "$(checksum "$archive")" "$archive"
        done >"$MANIFEST_NAME.new"
        mv -f "$MANIFEST_NAME.new" "$MANIFEST_NAME"
    )
}

# verify_manifest DIR — non-zero when any archive's checksum changed.
verify_manifest() {
    local dir="$1" expected name actual bad=0
    [[ -r "$dir/$MANIFEST_NAME" ]] || { log_warn "no manifest in $dir"; return 1; }
    while read -r expected name; do
        actual="$(checksum "$dir/$name")"
        if [[ "$actual" != "$expected" ]]; then
            log_error "checksum mismatch: $name"
            bad=1
        fi
    done <"$dir/$MANIFEST_NAME"
    return "$bad"
}

# list_archives DIR — newest first.
list_archives() {
    local dir="$1"
    find "$dir" -maxdepth 1 -type f -name "$ARCHIVE_PREFIX-*" -printf '%T@ %p\n' 2>/dev/null |
        sort -rn |
        cut -d' ' -f2-
}

# apply_retention DIR — deletes archives older than retention_days, but
# always keeps the newest retention_min.
apply_retention() {
    local dir="$1" days keep now cutoff archive epoch index=0 removed=0
    days="$(config_get retention_days)"
    keep="$(config_get retention_min)"
    now="$(now_epoch)"
    cutoff=$(( now - days * 86400 ))
    while read -r archive; do
        (( index++ ))
        if (( index <= keep )); then
            continue
        fi
        epoch="$(archive_epoch "$archive")" || continue
        if (( epoch < cutoff )); then
            if config_is_true dry_run; then
                log_info "[dry-run] would delete $(basename "$archive")"
            else
                rm -f -- "$archive" && (( removed++ ))
            fi
        fi
    done < <(list_archives "$dir")
    log_info "retention: removed $removed archive(s) older than $days days"
    write_manifest "$dir"
}

# restore_archive ARCHIVE DEST [PATTERN...]
restore_archive() {
    local archive="$1" dest="$2"
    shift 2
    [[ -f "$archive" ]] || die "no such archive: $archive" 66
    mkdir -p "$dest"
    local -a decompress
    case "$archive" in
        *.gpg) decompress=(gpg --batch --decrypt "$archive") ;;
        *) decompress=(cat "$archive") ;;
    esac
    "${decompress[@]}" | case "$archive" in
        *.zst*) zstd -dc ;;
        *.xz*) xz -dc ;;
        *.gz*) gzip -dc ;;
        *) cat ;;
    esac | tar -xf - -C "$dest" "$@"
    log_ok "restored $(basename "$archive") into $dest"
}

# disk_free_mb DIR
disk_free_mb() {
    df -Pm "$1" | awk 'NR == 2 { print $4 }'
}

# ensure_space DIR NEEDED_MB
ensure_space() {
    local free
    free="$(disk_free_mb "$1")"
    if (( free < $2 )); then
        die "only ${free} MiB free in $1, need $2 MiB" 73
    fi
}

# du_summary DIR — largest top-level entries.
du_summary() {
    du -sh "$1"/* 2>/dev/null | sort -rh | head -n "${2:-10}"
}

# Coprocess-based hashing of many files in one sha256sum process.
hash_many() {
    local file line
    coproc HASHER { xargs -0 sha256sum; }
    for file in "$@"; do
        printf '%s\0' "$file" >&"${HASHER[1]}"
    done
    exec {HASHER[1]}>&-
    while read -r line <&"${HASHER[0]}"; do
        printf '%s\n' "$line"
    done
    wait "$HASHER_PID"
}

# Snapshot of the source dirs' metadata, used to skip unchanged runs.
source_fingerprint() {
    local dir
    while read -r dir; do
        find "$dir" -xdev -type f -printf '%p %s %T@\n' 2>/dev/null
    done < <(config_list source_dirs) | sort | checksum /dev/stdin
}

changed_since_last_run() {
    local state="$1/.fingerprint" current
    current="$(source_fingerprint)"
    if [[ -r "$state" && "$(<"$state")" == "$current" ]]; then
        return 1
    fi
    printf '%s\n' "$current" >"$state"
}

# select-based picker for interactive restore.
pick_archive() {
    local dir="$1" choice
    local -a archives
    mapfile -t archives < <(list_archives "$dir")
    (( ${#archives[@]} )) || die "no archives in $dir" 66
    PS3="restore which archive? "
    select choice in "${archives[@]##*/}"; do
        [[ -n "$choice" ]] && break
    done
    printf '%s/%s\n' "$dir" "$choice"
}

# Removes stale temp dirs left by crashed runs (older than a day).
clean_stale_workdirs() {
    find "$TMP_ROOT" -maxdepth 1 -type d -name 'backup.*' -mtime +1 -exec rm -rf {} + 2>/dev/null || true
}

fs_stats() {
    summary_table "files=$FILES_SEEN" "bytes=$(format_bytes "$BYTES_SEEN")"
}

# Keyword form without parentheses; defines a helper function when first
# called (bash functions can be defined inside other functions).
function snapshot_btrfs {
    local subvol="$1" dest="$2"
    function btrfs_available {
        command -v btrfs >/dev/null && btrfs subvolume show "$1" >/dev/null 2>&1
    }
    if ! btrfs_available "$subvol"; then
        log_warn "$subvol is not a btrfs subvolume; skipping snapshot"
        return 1
    fi
    btrfs subvolume snapshot -r "$subvol" "$dest/snap-$(date +%Y%m%d%H%M%S)"
}

# Body in a subshell: cd and umask changes never leak to the caller.
tar_in_dir() (
    cd "$1" || exit 1
    umask 077
    shift
    tar -cf - "$@"
)

# Here-string and process substitution in one place.
count_matching() {
    local pattern="$1" text="$2" n
    n=$(grep -c -- "$pattern" <<<"$text" || true)
    tee >(wc -l >&2) <<<"$text" >/dev/null
    printf '%s\n' "$n"
}
