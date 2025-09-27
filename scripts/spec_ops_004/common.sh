#!/usr/bin/env bash
# Shared helpers for SPEC-OPS-004 guardrail commands.

set -euo pipefail

SPEC_OPS_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SPEC_OPS_ROOT}/../.." && pwd)"
EVIDENCE_ROOT="${REPO_ROOT}/docs/SPEC-OPS-004-integrated-coder-hooks/evidence"

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
