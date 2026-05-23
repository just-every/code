#!/usr/bin/env bash
set -euo pipefail

INTERVAL_SEC=${INTERVAL_SEC:-5}
MATCH_UID=${MATCH_UID:-1000}
MATCH_REGEX=${MATCH_REGEX:-/code-rs/bin/code([[:space:]]|$)}
RSS_KB_LIMIT=${RSS_KB_LIMIT:-8388608}
PRESSURE_RSS_KB_LIMIT=${PRESSURE_RSS_KB_LIMIT:-4194304}
MEM_AVAILABLE_FLOOR_KB=${MEM_AVAILABLE_FLOOR_KB:-786432}
HARD_MEM_AVAILABLE_FLOOR_KB=${HARD_MEM_AVAILABLE_FLOOR_KB:-2097152}
TERM_GRACE_SEC=${TERM_GRACE_SEC:-10}

log() {
    local message=$1
    logger -t code-memory-guard -- "$message" || true
    printf '%s %s\n' "$(date -Is)" "$message" >&2
}

mem_available_kb() {
    awk '/MemAvailable:/ { print $2; exit }' /proc/meminfo
}

select_candidate() {
    ps -eo pid=,uid=,rss=,args= --sort=-rss | awk \
        -v guard_uid="$MATCH_UID" \
        -v guard_regex="$MATCH_REGEX" '
            $2 == guard_uid {
                rss = $3
                cmd = substr($0, index($0, $4))
                if (cmd ~ guard_regex) {
                    print $1, rss, cmd
                    exit
                }
            }
        '
}

terminate_pid() {
    local pid=$1
    local reason=$2
    local rss_kb=$3
    local cmd=$4

    log "stopping pid=${pid} rss_kb=${rss_kb} reason=${reason} cmd=${cmd}"
    kill -TERM "$pid" 2>/dev/null || return 0

    local deadline=$((SECONDS + TERM_GRACE_SEC))
    while kill -0 "$pid" 2>/dev/null; do
        if (( SECONDS >= deadline )); then
            break
        fi
        sleep 1
    done

    if kill -0 "$pid" 2>/dev/null; then
        log "pid=${pid} survived SIGTERM; sending SIGKILL"
        kill -KILL "$pid" 2>/dev/null || true
    fi
}

main() {
    log "starting guard uid=${MATCH_UID} rss_limit_kb=${RSS_KB_LIMIT} pressure_limit_kb=${PRESSURE_RSS_KB_LIMIT} mem_floor_kb=${MEM_AVAILABLE_FLOOR_KB} hard_floor_kb=${HARD_MEM_AVAILABLE_FLOOR_KB}"

    while true; do
        local available_kb
        available_kb=$(mem_available_kb)

        local candidate
        candidate=$(select_candidate || true)
        if [[ -n "$candidate" ]]; then
            local pid rss_kb cmd
            pid=${candidate%% *}
            local rest=${candidate#* }
            rss_kb=${rest%% *}
            cmd=${rest#* }

            if (( rss_kb >= RSS_KB_LIMIT && available_kb <= HARD_MEM_AVAILABLE_FLOOR_KB )); then
                terminate_pid "$pid" "rss_limit_exceeded available_kb=${available_kb}" "$rss_kb" "$cmd"
            elif (( available_kb <= MEM_AVAILABLE_FLOOR_KB && rss_kb >= PRESSURE_RSS_KB_LIMIT )); then
                terminate_pid "$pid" "low_mem_available_kb=${available_kb}" "$rss_kb" "$cmd"
            fi
        fi

        sleep "$INTERVAL_SEC"
    done
}

main "$@"
