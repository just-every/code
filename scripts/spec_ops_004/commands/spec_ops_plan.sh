#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../common.sh"

if [[ $# -lt 1 ]]; then
  echo "Usage: /spec-ops-plan <SPEC-ID> [--baseline-mode <mode>]" >&2
  exit 1
fi

SPEC_ID="$1"; shift
BASELINE_MODE="no-run"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --baseline-mode)
      BASELINE_MODE="$2"; shift 2 ;;
    --skip-baseline)
      BASELINE_MODE="skip"; shift ;;
    *)
      echo "Unknown argument: $1" >&2; exit 1 ;;
  esac
done

spec_ops_require_clean_tree
spec_ops_prepare_stage "spec-plan" "${SPEC_ID}"
spec_ops_write_log "baseline mode=${BASELINE_MODE}"

BASELINE_OUT="${SPEC_OPS_STAGE_DIR}/baseline_${SPEC_OPS_SESSION_ID}.md"
"${SCRIPT_DIR}/../baseline_audit.sh" --spec "${SPEC_ID}" --out "${BASELINE_OUT}" --mode "${BASELINE_MODE}" >>"${SPEC_OPS_LOG}" 2>&1 || true

read -r -d '' TELEMETRY <<JSON || true
{
  "command": "spec-ops-plan",
  "specId": "${SPEC_ID}",
  "sessionId": "${SPEC_OPS_SESSION_ID}",
  "timestamp": "$(spec_ops_timestamp)",
  "baseline": {
    "mode": "${BASELINE_MODE}",
    "artifact": "${BASELINE_OUT}"
  }
}
JSON

spec_ops_emit_telemetry "${TELEMETRY}"
echo "Plan guardrail executed for ${SPEC_ID}"
echo "Telemetry: ${SPEC_OPS_TELEMETRY}"
