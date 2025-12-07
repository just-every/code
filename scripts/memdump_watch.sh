#!/usr/bin/env bash
# LaunchAgent-friendly watcher that triggers memdump.sh when Auto Drive exceeds a memory threshold.

set -euo pipefail

THRESHOLD_MB=${MEMWATCH_THRESHOLD_MB:-5120}
KEEP=${MEMWATCH_KEEP:-10}
COOLDOWN_SECS=${MEMWATCH_COOLDOWN_SECS:-1800}
BASE_DIR=${MEMWATCH_DIR:-"$HOME/.code/memdumps"}
STATE_DIR="$BASE_DIR/.state"
AUTO_DIR=${MEMWATCH_AUTO_DIR:-"$HOME/.code/auto-drive"}
HELPER=${MEMWATCH_HELPER:-"$HOME/.code/memdump.sh"}
LOG_FILE="$BASE_DIR/memdump.log"
CMD_REGEX=${MEMWATCH_CMD_REGEX:-"code"}
ENABLE_CORE=${MEMWATCH_ENABLE_CORE:-0}

mkdir -p "$BASE_DIR" "$STATE_DIR"

log() {
  local msg="$(date '+%Y-%m-%d %H:%M:%S') [memwatch] $*"
  echo "$msg" | tee -a "$LOG_FILE" >/dev/null
}

read_pid_from_file() {
  local file="$1"
  python3 - "$file" <<'PY'
import json, sys
path = sys.argv[1]
try:
    with open(path) as f:
        data = json.load(f)
    pid = data.get("pid")
    print(pid if pid is not None else "")
except Exception:
    print("")
PY
}

discover_auto_drive_pid() {
  local newest
  newest=$(ls -t "$AUTO_DIR"/pid-*.json 2>/dev/null | head -n1 || true)
  if [[ -z "$newest" ]]; then
    return 1
  fi

  local pid
  pid=$(read_pid_from_file "$newest")
  if [[ -z "$pid" ]]; then
    log "auto-drive pid file missing pid: $newest"
    return 1
  fi

  if kill -0 "$pid" 2>/dev/null; then
    echo "$pid"
    return 0
  fi

  log "stale auto-drive pid file $newest (pid $pid)"
  rm -f "$newest"
  return 1
}

discover_fallback_pid() {
  local line
  line=$(ps -axo pid,rss,command | grep -i "$CMD_REGEX" | grep -v grep | sort -nrk2 | head -n1 || true)
  if [[ -z "$line" ]]; then
    return 1
  fi
  echo "$line" | awk '{print $1}'
}

pid=$(discover_auto_drive_pid || true)
if [[ -z "$pid" ]]; then
  pid=$(discover_fallback_pid || true)
fi

if [[ -z "$pid" ]]; then
  log "no target pid found"
  exit 0
fi

rss_kb=$(ps -o rss= -p "$pid" 2>/dev/null | awk '{print $1}')
if [[ -z "$rss_kb" ]]; then
  log "unable to read rss for pid $pid"
  exit 0
fi

rss_mb=$((rss_kb / 1024))
log "pid $pid rss=${rss_mb}MB (threshold ${THRESHOLD_MB}MB)"
if [[ "$rss_mb" -lt "$THRESHOLD_MB" ]]; then
  exit 0
fi

last_stamp="$STATE_DIR/last_${pid}.stamp"
if [[ -f "$last_stamp" ]]; then
  last_time=$(stat -f %m "$last_stamp" 2>/dev/null || true)
  now=$(date +%s)
  if [[ -n "$last_time" && $((now - last_time)) -lt $COOLDOWN_SECS ]]; then
    log "cooldown active for pid $pid; skipping"
    exit 0
  fi
fi

if [[ ! -x "$HELPER" ]]; then
  log "helper $HELPER missing or not executable"
  exit 1
fi

core_flag=( )
if [[ "$ENABLE_CORE" != 1 ]]; then
  core_flag=(--no-core)
fi

if "$HELPER" --pid "$pid" --threshold "$THRESHOLD_MB" --keep "$KEEP" --dir "$BASE_DIR" "${core_flag[@]}"; then
  date +%s > "$last_stamp"
else
  log "helper failed for pid $pid"
fi
