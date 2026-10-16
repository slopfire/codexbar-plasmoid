#!/usr/bin/env bash
# Publish the CodexBar sync server, and optionally the Android APK download,
# through an Ultradenpa realm tunnel.
#
# `ultradenpa publish` is a foreground command: the service exists only while the
# publisher process lives, and its registration is removed when the process
# exits. Anything started from an interactive shell therefore dies with that
# shell, and the hostname starts answering
#   {"code":"resource_not_found","error":"no published service for this hostname"}
# This script starts every process detached with `setsid`, so the publications
# outlive the shell that created them.
#
# Usage:
#   scripts/publish-via-ultradenpa.sh            # start + publish everything
#   scripts/publish-via-ultradenpa.sh status     # what is listening / published
#   scripts/publish-via-ultradenpa.sh stop       # unpublish and stop
#
# Environment overrides:
#   CODEXBAR_SYNC_PORT (8188)   CODEXBAR_SYNC_NAME (codexbar)
#   CODEXBAR_APK_PORT  (8098)   CODEXBAR_APK_NAME  (codexbar-apk)
#   CODEXBAR_PUBLISH_APK=0      skip the APK download service
#   CODEXBAR_SYNC_TOKEN         passed through to the sync server

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
helper="$repo_root/native-cli/target/release/codexbar-plasmoid"
dist_dir="$repo_root/android/dist"
state_dir="${XDG_RUNTIME_DIR:-/tmp}/codexbar-publish"

sync_port="${CODEXBAR_SYNC_PORT:-8188}"
sync_name="${CODEXBAR_SYNC_NAME:-codexbar}"
apk_port="${CODEXBAR_APK_PORT:-8098}"
apk_name="${CODEXBAR_APK_NAME:-codexbar-apk}"
publish_apk="${CODEXBAR_PUBLISH_APK:-1}"

mkdir -p "$state_dir"

die() {
    printf '%s\n' "$*" >&2
    exit 1
}

listening() {
    ss -ltnH "sport = :$1" 2>/dev/null | grep -q .
}

published() {
    ultradenpa published 2>/dev/null | grep -q "^name=$1 "
}

hostname_for() {
    ultradenpa published 2>/dev/null | awk -v n="$1" '
        $0 ~ "^name=" n " " {
            for (i = 1; i <= NF; i++) {
                if ($i ~ /^private_hostname=/) { sub(/^private_hostname=/, "", $i); print $i }
            }
        }'
}

require_tools() {
    command -v ultradenpa >/dev/null || die "ultradenpa is not on PATH"
    command -v ss >/dev/null || die "ss is not available"
}

start_sync_server() {
    if listening "$sync_port"; then
        printf 'sync server already listening on 127.0.0.1:%s\n' "$sync_port"
        return
    fi
    [[ -x "$helper" ]] || die "missing $helper — build it with scripts/build-native-cli.sh"
    setsid nohup "$helper" sync-server --bind 127.0.0.1 --port "$sync_port" \
        >"$state_dir/sync-server.log" 2>&1 </dev/null &
    for _ in $(seq 1 40); do
        listening "$sync_port" && break
        sleep 0.25
    done
    listening "$sync_port" || die "sync server did not start; see $state_dir/sync-server.log"
    printf 'sync server listening on 127.0.0.1:%s\n' "$sync_port"
}

start_apk_server() {
    if listening "$apk_port"; then
        printf 'APK server already listening on 127.0.0.1:%s\n' "$apk_port"
        return
    fi
    [[ -d "$dist_dir" ]] || die "missing $dist_dir — run ./gradlew :app:assembleRelease first"
    setsid nohup python3 -m http.server "$apk_port" --bind 127.0.0.1 --directory "$dist_dir" \
        >"$state_dir/apk-server.log" 2>&1 </dev/null &
    for _ in $(seq 1 40); do
        listening "$apk_port" && break
        sleep 0.25
    done
    listening "$apk_port" || die "APK server did not start; see $state_dir/apk-server.log"
    printf 'APK server listening on 127.0.0.1:%s (%s)\n' "$apk_port" "$dist_dir"
}

publish_service() {
    local port="$1" name="$2"
    if published "$name"; then
        printf '%s already published\n' "$name"
        return
    fi
    setsid nohup ultradenpa publish "$port" --name "$name" \
        >"$state_dir/publish-$name.log" 2>&1 </dev/null &
    for _ in $(seq 1 60); do
        published "$name" && break
        sleep 0.5
    done
    published "$name" || die "publish of $name did not register; see $state_dir/publish-$name.log"
    printf '%s published\n' "$name"
}

report() {
    printf '\n'
    local host
    host="$(hostname_for "$sync_name")"
    [[ -n "$host" ]] && printf 'sync server:  https://%s\n' "$host"
    if [[ "$publish_apk" == "1" ]]; then
        host="$(hostname_for "$apk_name")"
        [[ -n "$host" ]] && printf 'APK download: https://%s/%s\n' "$host" "$(apk_file_name)"
    fi
    printf '\nBoth are realm visibility: the viewer must be attached to Ultradenpa.\n'
}

apk_file_name() {
    local candidate
    candidate="$(ls -1 "$dist_dir"/*-signed.apk 2>/dev/null | head -n 1)"
    [[ -n "$candidate" ]] && basename "$candidate" || printf 'codexbar-android-1.0-signed.apk'
}

case "${1:-start}" in
start)
    require_tools
    start_sync_server
    publish_service "$sync_port" "$sync_name"
    if [[ "$publish_apk" == "1" ]]; then
        start_apk_server
        publish_service "$apk_port" "$apk_name"
    fi
    report
    ;;
status)
    require_tools
    listening "$sync_port" && printf 'sync server listening on %s\n' "$sync_port" || printf 'sync server NOT listening\n'
    listening "$apk_port" && printf 'APK server listening on %s\n' "$apk_port" || printf 'APK server NOT listening\n'
    printf '\nregistrations:\n'
    ultradenpa published 2>/dev/null | grep -E "^name=($sync_name|$apk_name) " || printf '  none\n'
    ;;
stop)
    for name in "$sync_name" "$apk_name"; do
        published "$name" && ultradenpa unpublish "$name" && printf 'unpublished %s\n' "$name"
    done
    for port in "$sync_port" "$apk_port"; do
        pids="$(ss -ltnpH "sport = :$port" 2>/dev/null | grep -oE 'pid=[0-9]+' | cut -d= -f2 | sort -u)"
        for pid in $pids; do
            kill "$pid" 2>/dev/null && printf 'stopped pid %s on port %s\n' "$pid" "$port"
        done
    done
    ;;
*)
    die "usage: $(basename "$0") [start|status|stop]"
    ;;
esac
