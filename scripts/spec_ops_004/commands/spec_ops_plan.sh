#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../common.sh"

if [[ $# -lt 1 ]]; then
  echo "Usage: /spec-ops-plan <SPEC-ID> [--baseline-mode <mode>] [--allow-fail]" >&2
  exit 1
fi

SPEC_ID="$1"; shift
BASELINE_MODE="no-run"
ALLOW_BASELINE_FAIL=0

if [[ "${SPEC_OPS_ALLOW_DIRTY:-0}" == "1" || "${SPEC_OPS_BASELINE_ALLOW_FAIL:-0}" == "1" ]]; then
  ALLOW_BASELINE_FAIL=1
fi

SCHEMA_VERSION=1

baseline_status() {
  case "$1" in
    skip)
      printf 'skipped'
      ;;
    quick|no-run|full)
      printf 'passed'
      ;;
    *)
      printf 'unknown'
      ;;
  esac
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --baseline-mode)
      BASELINE_MODE="$2"; shift 2 ;;
    --skip-baseline)
      BASELINE_MODE="skip"; shift ;;
    --allow-fail)
      ALLOW_BASELINE_FAIL=1; shift ;;
    *)
      echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

spec_ops_require_clean_tree
spec_ops_prepare_stage "spec-plan" "${SPEC_ID}"
spec_ops_write_log "baseline mode=${BASELINE_MODE}"

BASELINE_OUT="${SPEC_OPS_STAGE_DIR}/baseline_${SPEC_OPS_SESSION_ID}.md"
BASELINE_EXIT=0

if ! "${SCRIPT_DIR}/../baseline_audit.sh" --spec "${SPEC_ID}" --out "${BASELINE_OUT}" --mode "${BASELINE_MODE}" >>"${SPEC_OPS_LOG}" 2>&1; then
  BASELINE_EXIT=$?
  spec_ops_write_log "baseline audit exited with code ${BASELINE_EXIT}"
fi

BASELINE_STATUS=""
if [[ -f "${BASELINE_OUT}" ]]; then
  BASELINE_STATUS="$(awk -F': ' '/^[[:space:]-]*Status:/ {print tolower($2); exit}' "${BASELINE_OUT}" 2>/dev/null | tr -d '\r')"
fi

if [[ -z "${BASELINE_STATUS}" ]]; then
  BASELINE_STATUS="$(baseline_status "${BASELINE_MODE}")"
fi

if [[ ${BASELINE_EXIT} -ne 0 ]]; then
  BASELINE_STATUS="failed"
fi

if [[ ! -s "${BASELINE_OUT}" ]]; then
  printf '# Baseline Audit\nStatus: %s\n' "${BASELINE_STATUS}" >"${BASELINE_OUT}"
fi

HOOK_SESSION_START="ok"

read -r -d '' TELEMETRY <<JSON || true
{
  "schemaVersion": ${SCHEMA_VERSION},
  "command": "spec-ops-plan",
  "specId": "${SPEC_ID}",
  "sessionId": "${SPEC_OPS_SESSION_ID}",
  "timestamp": "$(spec_ops_timestamp)",
  "baseline": {
    "mode": "${BASELINE_MODE}",
    "artifact": "${BASELINE_OUT}",
    "status": "${BASELINE_STATUS}"
  },
  "hooks": {
    "session.start": "${HOOK_SESSION_START}"
  },
  "artifacts": [
    { "path": "${BASELINE_OUT}" },
    { "path": "${SPEC_OPS_LOG}" }
  ]
}
JSON

spec_ops_emit_telemetry "${TELEMETRY}"
echo "Plan guardrail executed for ${SPEC_ID}"
echo "Telemetry: ${SPEC_OPS_TELEMETRY}"

if [[ "${BASELINE_STATUS}" == "failed" ]]; then
  if [[ "${ALLOW_BASELINE_FAIL}" == "1" ]]; then
    spec_ops_write_log "baseline failure ignored via allow-fail"
  else
    echo "Baseline audit failed; rerun with --allow-fail to override" >&2
    exit $([[ ${BASELINE_EXIT} -ne 0 ]] && printf '%s' "${BASELINE_EXIT}" || printf '1')
  fi
fi
