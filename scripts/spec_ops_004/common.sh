#!/usr/bin/env bash
# Shared helpers for SPEC-OPS-004 guardrail commands.

set -euo pipefail

SPEC_OPS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SPEC_OPS_ROOT}/../.." && pwd)"
EVIDENCE_ROOT="${REPO_ROOT}/docs/SPEC-OPS-004-integrated-coder-hooks/evidence"

SPEC_OPS_HAL_ARTIFACTS=()

spec_ops_timestamp() {
  date -u "+%Y-%m-%dT%H:%M:%SZ"
}

spec_ops_require_clean_tree() {
  if [[ "${SPEC_OPS_ALLOW_DIRTY:-0}" == "1" ]]; then
    return
  fi
  if ! git diff --no-ext-diff --quiet || ! git diff --no-ext-diff --cached --quiet; then
    echo "SPEC-OPS-004: working tree must be clean" >&2
    git status --short >&2
    exit 1
  fi
}

spec_ops_prepare_stage() {
  local stage="$1"; shift
  local spec_id="$1"; shift

  if [[ -z "${spec_id}" ]]; then
    echo "SPEC-OPS-004: SPEC ID required" >&2
    exit 1
  fi

  export SPEC_OPS_SESSION_ID="${SPEC_OPS_SESSION_ID:-$(spec_ops_timestamp)-$RANDOM$RANDOM}"
  export SPEC_OPS_STAGE_DIR="${EVIDENCE_ROOT}/commands/${spec_id}"
  mkdir -p "${SPEC_OPS_STAGE_DIR}"
  export SPEC_OPS_TELEMETRY="${SPEC_OPS_STAGE_DIR}/${stage}_${SPEC_OPS_SESSION_ID}.json"
  export SPEC_OPS_LOG="${SPEC_OPS_STAGE_DIR}/${stage}_${SPEC_OPS_SESSION_ID}.log"
  touch "${SPEC_OPS_LOG}"
}

spec_ops_write_log() {
  printf '%s %s\n' "$(spec_ops_timestamp)" "$*" >>"${SPEC_OPS_LOG}"
}

spec_ops_emit_telemetry() {
  local content="$1"
  printf '%s\n' "${content}" >"${SPEC_OPS_TELEMETRY}"
  spec_ops_write_log "telemetry -> ${SPEC_OPS_TELEMETRY}"
}

spec_ops_capture_hal() {
  local tool="$1"
  local json_args="$2"
  local dest="$3"
  local fallback_status="$4"
  local fallback_note="$5"
  local secret_env="$6"
  local secret_value="$7"

  if ! command -v cargo >/dev/null 2>&1; then
    spec_ops_write_log "cargo not available; skipping HAL tool ${tool}"
    return 1
  fi

  local tmp
  tmp="$(mktemp)"
  local cmd=(cargo run -q -p codex-mcp-client --bin call_tool -- --tool "${tool}")
  if [[ -n "${json_args}" ]]; then
    cmd+=(--args "${json_args}")
  fi
  cmd+=(--env "${secret_env}=${secret_value}")
  cmd+=(-- npx -y hal-mcp)

  if ! "${cmd[@]}" >"${tmp}" 2>>"${SPEC_OPS_LOG}"; then
    spec_ops_write_log "HAL tool ${tool} failed"
    rm -f "${tmp}"
    return 1
  fi

  local text
  text="$(jq -r '.content[0].text // ""' "${tmp}" 2>>"${SPEC_OPS_LOG}" || printf '')"
  rm -f "${tmp}"

  local body="${text#*Body:\n}"
  if [[ "${body}" == "${text}" ]] || [[ -z "${body//[[:space:]]/}" ]]; then
    body=$(printf '{"status":%s,"note":"%s"}' "${fallback_status}" "${fallback_note}")
  fi

  printf '%s' "${body}" >"${dest}"
  spec_ops_write_log "HAL tool ${tool} -> ${dest}"
  SPEC_OPS_HAL_ARTIFACTS+=("${dest}")
}

spec_ops_run_hal_smoke() {
  if [[ "${SPEC_OPS_HAL_SKIP:-0}" == "1" ]]; then
    spec_ops_write_log "SPEC_OPS_HAL_SKIP=1; skipping HAL smoke"
    return 0
  fi

  if ! command -v jq >/dev/null 2>&1; then
    spec_ops_write_log "jq not available; skipping HAL smoke"
    return 1
  fi

  local secret_env="${SPEC_OPS_HAL_SECRET_ENV:-HAL_SECRET_KAVEDARR_API_KEY}"
  local secret_value="${!secret_env:-}"
  if [[ -z "${secret_value}" ]]; then
    spec_ops_write_log "HAL secret ${secret_env} not set; skipping HAL smoke"
    return 1
  fi

  local base_url="${SPEC_OPS_HAL_BASE_URL:-http://127.0.0.1:7878}"
  base_url="${base_url%/}"
  local dest_dir
  dest_dir="$(spec_ops_hal_evidence_dir)"
  local ts
  ts="$(date -u +%Y%m%d-%H%M%SZ)"

  local health_args
  printf -v health_args '{"url":"%s/health"}' "${base_url}"
  spec_ops_capture_hal "http-get" "${health_args}" "${dest_dir}/${ts}-hal-health.json" 503 "Health endpoint returned no body" "${secret_env}" "${secret_value}"

  local list_args
  printf -v list_args '{"url":"%s/api/v3/movie","query":{"page":1,"limit":50}}' "${base_url}"
  spec_ops_capture_hal "http-get" "${list_args}" "${dest_dir}/${ts}-hal-list_movies.json" 500 "List movies returned no body" "${secret_env}" "${secret_value}"

  local indexer_args
  printf -v indexer_args '{"url":"%s/api/v3/indexer/test","headers":{"Content-Type":"application/json"},"body":"{}"}' "${base_url}"
  spec_ops_capture_hal "http-post" "${indexer_args}" "${dest_dir}/${ts}-hal-indexer_test.json" 500 "Indexer test returned no body" "${secret_env}" "${secret_value}"

  local graphql_args
  printf -v graphql_args '{"url":"%s/graphql","headers":{"Content-Type":"application/json"},"body":"{\"query\":\"query { __typename }\"}"}' "${base_url}"
  spec_ops_capture_hal "http-post" "${graphql_args}" "${dest_dir}/${ts}-hal-graphql_ping.json" 401 "GraphQL ping returned no body" "${secret_env}" "${secret_value}"
}

spec_ops_hal_evidence_dir() {
  local dest="${SPEC_OPS_HAL_EVIDENCE_DIR:-${SPEC_OPS_STAGE_DIR}}"
  mkdir -p "${dest}"
  printf '%s\n' "${dest}"
}
