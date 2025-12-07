#!/usr/bin/env bash
# Capture macOS memory diagnostics for a process once its RSS exceeds a threshold.
#
# Usage examples:
#   scripts/memdump.sh --pid 12345
#   scripts/memdump.sh --pid 12345 --threshold 4096 --keep 5 --no-core

set -euo pipefail

usage() {
  cat <<'EOF'
Usage: memdump.sh [OPTIONS]

Options:
  -p, --pid PID           Process ID to inspect (required)
  -t, --threshold MB      RSS threshold in MB (default: 5120)
  -k, --keep N            Keep at most N dumps (default: 10)
  -d, --dir PATH          Base directory for dumps (default: ~/.code/memdumps)
      --sample-secs N     Seconds for `sample` (default: 3)
      --sample-interval MS  Sampling interval in milliseconds (default: 20)
      --no-core           Skip lldb core dump
  -h, --help              Show this help

Artifacts (best effort):
  ps snapshot, footprint JSON, vmmap summary, sample stacks, optional lldb core
EOF
}

log() { echo "$(date '+%Y-%m-%d %H:%M:%S') [memdump] $*" >&2; }
warn() { log "WARN: $*"; }
error() { log "ERROR: $*"; }

require_darwin() {
  if [[ "$(uname -s)" != "Darwin" ]]; then
    error "This helper is macOS-only"; exit 1; fi
}

require_binary() {
  if ! command -v "$1" >/dev/null 2>&1; then
    warn "'$1' not found; skipping"; return 1; fi
  return 0
}

check_process_exists() {
  local pid="$1"
  if ! ps -p "$pid" >/dev/null 2>&1; then
    error "Process $pid is not running"; return 1; fi
  return 0
}

get_rss_mb() {
  local pid="$1"
  local rss_kb
  rss_kb=$(ps -o rss= -p "$pid" 2>/dev/null | awk '{print $1}')
  [[ -z "$rss_kb" ]] && echo "0" && return 1
  echo $((rss_kb / 1024))
}

capture_ps_snapshot() {
  local pid="$1" output="$2"
  if ps -p "$pid" -o pid,ppid,user,%cpu,%mem,vsz,rss,etime,command >"$output" 2>&1; then
    log "ps -> $output"
  else
    warn "ps snapshot failed"
  fi
}

capture_footprint() {
  local pid="$1" output="$2"
  require_binary footprint || return 0
  if footprint -j "$pid" >"$output" 2>&1; then
    log "footprint -> $output"
  else
    warn "footprint failed (permissions?)"
  fi
}

capture_vmmap() {
  local pid="$1" output="$2"
  require_binary vmmap || return 0
  if vmmap -summary "$pid" >"$output" 2>&1; then
    log "vmmap -> $output"
  else
    warn "vmmap failed (permissions?)"
  fi
}

capture_sample() {
  local pid="$1" output="$2" secs="$3" interval_ms="$4"
  require_binary sample || return 0
  if sample "$pid" "$secs" "$interval_ms" -file "$output" >"$output.log" 2>&1; then
    log "sample -> $output"
  else
    warn "sample failed (permissions?)"
  fi
}

capture_lldb_core() {
  local pid="$1" dump_dir="$2"
  require_binary lldb || return 0

  local core_path="$dump_dir/core.$pid"
  local script_path="$dump_dir/lldb_script.txt"
  cat >"$script_path" <<LLDB
process attach --pid $pid
process save-core "$core_path"
detach
quit
LLDB

  log "lldb core attempt (timeout 60s)"
  lldb --batch --source "$script_path" >"$dump_dir/lldb.log" 2>&1 &
  local lldb_pid=$!
  (
    sleep 60
    if kill -0 "$lldb_pid" 2>/dev/null; then
      warn "lldb timed out; killing"
      kill "$lldb_pid" 2>/dev/null || true
    fi
  ) &
  local watchdog=$!
  wait "$lldb_pid" || true
  kill "$watchdog" 2>/dev/null || true

  if [[ -f "$core_path" ]]; then
    log "lldb core -> $core_path"
  else
    warn "lldb did not produce a core (see $dump_dir/lldb.log)"
  fi

  rm -f "$script_path"
}

rotate_dumps() {
  local base_dir="$1" keep="$2"
  [[ -d "$base_dir" ]] || return 0
  local dumps
  dumps=$(find "$base_dir" -mindepth 1 -maxdepth 1 -type d -name "*-pid*" | sort -r)
  local idx=0
  while IFS= read -r d; do
    idx=$((idx + 1))
    if [[ $idx -gt $keep ]]; then
      log "prune old dump $d"
      rm -rf "$d"
    fi
  done <<<"$dumps"
}

PID=""
THRESHOLD_MB=5120
KEEP=10
BASE_DIR="$HOME/.code/memdumps"
SKIP_CORE=false
SAMPLE_SECS=3
SAMPLE_INTERVAL_MS=20

while [[ $# -gt 0 ]]; do
  case "$1" in
    -p|--pid) PID="${2:-}"; shift 2 ;;
    -t|--threshold) THRESHOLD_MB="${2:-}"; shift 2 ;;
    -k|--keep) KEEP="${2:-}"; shift 2 ;;
    -d|--dir) BASE_DIR="${2:-}"; shift 2 ;;
    --sample-secs) SAMPLE_SECS="${2:-}"; shift 2 ;;
    --sample-interval) SAMPLE_INTERVAL_MS="${2:-}"; shift 2 ;;
    --no-core) SKIP_CORE=true; shift ;;
    -h|--help) usage; exit 0 ;;
    *) error "unknown option $1"; usage; exit 1 ;;
  esac
done

require_darwin

if [[ -z "$PID" ]]; then error "--pid is required"; usage; exit 1; fi
check_process_exists "$PID" || exit 1

RSS_MB=$(get_rss_mb "$PID")
log "PID $PID RSS=${RSS_MB}MB (threshold ${THRESHOLD_MB}MB)"
if [[ "$RSS_MB" -lt "$THRESHOLD_MB" ]]; then
  log "below threshold; nothing to do"
  exit 0
fi

mkdir -p "$BASE_DIR"
TIMESTAMP=$(date '+%Y%m%d-%H%M%S')
DUMP_DIR="$BASE_DIR/$TIMESTAMP-pid$PID"
mkdir -p "$DUMP_DIR"

log "writing dump to $DUMP_DIR"

cat >"$DUMP_DIR/manifest.txt" <<MANIFEST
Memory Dump Manifest
====================
Timestamp: $(date '+%Y-%m-%d %H:%M:%S')
PID: $PID
RSS (MB): $RSS_MB
Threshold (MB): $THRESHOLD_MB
Hostname: $(hostname)
macOS: $(sw_vers -productVersion 2>/dev/null || echo "unknown")
Command: $(ps -o command= -p "$PID" 2>/dev/null)
MANIFEST

capture_ps_snapshot "$PID" "$DUMP_DIR/ps.txt"
capture_footprint "$PID" "$DUMP_DIR/footprint.json"
capture_vmmap "$PID" "$DUMP_DIR/vmmap_summary.txt"
capture_sample "$PID" "$DUMP_DIR/sample.txt" "$SAMPLE_SECS" "$SAMPLE_INTERVAL_MS"
if [[ "$SKIP_CORE" != true ]]; then
  capture_lldb_core "$PID" "$DUMP_DIR"
fi

rotate_dumps "$BASE_DIR" "$KEEP"

log "done"
