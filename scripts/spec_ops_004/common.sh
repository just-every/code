#!/usr/bin/env bash
# Shared helpers for SPEC-OPS-004 guardrail commands.

set -euo pipefail

SPEC_OPS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SPEC_OPS_ROOT}/../.." && pwd)"
EVIDENCE_ROOT="${REPO_ROOT}/docs/SPEC-OPS-004-integrated-coder-hooks/evidence"

SPEC_OPS_HAL_ARTIFACTS=()
SPEC_OPS_HAL_FAILED_CHECKS=()

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
  local fallback_used=0

  if ! command -v cargo >/dev/null 2>&1; then
    spec_ops_write_log "cargo not available; skipping HAL tool ${tool}"
    return 1
  fi

  local tmp
  tmp="$(mktemp)"
  local manifest_path="${SPEC_OPS_CARGO_MANIFEST:-${REPO_ROOT}/codex-rs/Cargo.toml}"
  if [[ ! -f "${manifest_path}" ]]; then
    spec_ops_write_log "cargo manifest ${manifest_path} not found; skipping HAL tool ${tool}"
    rm -f "${tmp}"
    return 1
  fi

  local cmd=(cargo run --manifest-path "${manifest_path}" --quiet -p codex-mcp-client --bin call_tool -- --tool "${tool}")
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
  local body
  body="$(jq -r 'try (.content[0].text | gsub("\\r\\n"; "\\n") | capture("Body:\\n(?<body>[\\s\\S]*)") | .body) catch ""' "${tmp}" 2>>"${SPEC_OPS_LOG}" || printf '')"
  rm -f "${tmp}"

  if [[ -z "${body//[[:space:]]/}" ]]; then
    body=$(printf '{"status":%s,"note":"%s"}' "${fallback_status}" "${fallback_note}")
    fallback_used=1
  fi

  printf '%s' "${body}" >"${dest}"
  if [[ ${fallback_used} -eq 1 ]]; then
    spec_ops_write_log "HAL tool ${tool} produced fallback body -> ${dest}"
  else
    spec_ops_write_log "HAL tool ${tool} -> ${dest}"
  fi
  SPEC_OPS_HAL_ARTIFACTS+=("${dest}")

  if [[ ${fallback_used} -eq 1 ]]; then
    return 1
  fi

  return 0
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
  local failures=0
  local failed_checks=()
  SPEC_OPS_HAL_FAILED_CHECKS=()

  local health_args
  printf -v health_args '{"url":"%s/health"}' "${base_url}"
  if ! spec_ops_capture_hal "http-get" "${health_args}" "${dest_dir}/${ts}-hal-health.json" 503 "Health endpoint returned no body" "${secret_env}" "${secret_value}"; then
    failures=$((failures + 1))
    failed_checks+=("health")
  fi

  local list_args
  printf -v list_args '{"url":"%s/api/v3/movie","query":{"page":1,"limit":50},"headers":{"X-Api-Key":"%s","apikey":"%s"}}' "${base_url}" "${secret_value}" "${secret_value}"
  if ! spec_ops_capture_hal "http-get" "${list_args}" "${dest_dir}/${ts}-hal-list_movies.json" 500 "List movies returned no body" "${secret_env}" "${secret_value}"; then
    failures=$((failures + 1))
    failed_checks+=("list_movies")
  fi

  local indexer_args
  printf -v indexer_args '{"url":"%s/api/v3/indexer/test","headers":{"Content-Type":"application/json","X-Api-Key":"%s","apikey":"%s"},"body":"{}"}' "${base_url}" "${secret_value}" "${secret_value}"
  if ! spec_ops_capture_hal "http-post" "${indexer_args}" "${dest_dir}/${ts}-hal-indexer_test.json" 500 "Indexer test returned no body" "${secret_env}" "${secret_value}"; then
    failures=$((failures + 1))
    failed_checks+=("indexer_test")
  fi

  local graphql_body
  graphql_body=$(printf '%s' '{"query":"query { __typename }"}' | jq -aRs .)
  local graphql_args
  printf -v graphql_args '{"url":"%s/graphql","headers":{"Content-Type":"application/json","X-Api-Key":"%s","apikey":"%s"},"body":%s}' "${base_url}" "${secret_value}" "${secret_value}" "${graphql_body}"
  if ! spec_ops_capture_hal "http-post" "${graphql_args}" "${dest_dir}/${ts}-hal-graphql_ping.json" 401 "GraphQL ping returned no body" "${secret_env}" "${secret_value}"; then
    failures=$((failures + 1))
    failed_checks+=("graphql_ping")
  fi

  if [[ ${failures} -gt 0 ]]; then
    SPEC_OPS_HAL_FAILED_CHECKS=("${failed_checks[@]}")
    spec_ops_write_log "HAL smoke failures: ${failed_checks[*]}"
    return 1
  fi

  return 0
}

spec_ops_hal_evidence_dir() {
  local dest="${SPEC_OPS_HAL_EVIDENCE_DIR:-${SPEC_OPS_STAGE_DIR}}"
  mkdir -p "${dest}"
  printf '%s\n' "${dest}"
}
