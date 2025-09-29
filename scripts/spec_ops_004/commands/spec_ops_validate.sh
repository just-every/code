#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../common.sh"

if [[ $# -lt 1 ]]; then
  echo "Usage: /spec-ops-validate <SPEC-ID>" >&2
  exit 1
fi

SPEC_ID="$1"; shift || true

spec_ops_prepare_stage "spec-validate" "${SPEC_ID}"
spec_ops_write_log "validate guardrail ready"

SPEC_OPS_HAL_ARTIFACTS=()

HAL_FAILURE=0
if ! spec_ops_run_hal_smoke; then
  HAL_FAILURE=1
  if [[ ${#SPEC_OPS_HAL_FAILED_CHECKS[@]} -gt 0 ]]; then
    spec_ops_write_log "HAL smoke failed checks: ${SPEC_OPS_HAL_FAILED_CHECKS[*]}"
  else
    spec_ops_write_log "HAL smoke skipped/failed"
  fi
fi

SCHEMA_VERSION=1
SCENARIO_STATUS="passed"

if [[ ${HAL_FAILURE} -ne 0 ]]; then
  SCENARIO_STATUS="failed"
fi

read -r -d '' TELEMETRY <<JSON || true
{
  "schemaVersion": ${SCHEMA_VERSION},
  "command": "spec-ops-validate",
  "specId": "${SPEC_ID}",
  "sessionId": "${SPEC_OPS_SESSION_ID}",
  "timestamp": "$(spec_ops_timestamp)",
  "scenarios": [
    {
      "name": "validate guardrail bootstrap",
      "status": "${SCENARIO_STATUS}"
    }
  ],
  "artifacts": [
    { "path": "${SPEC_OPS_LOG}" }$(
      for artifact in "${SPEC_OPS_HAL_ARTIFACTS[@]}"; do
        [[ -z "${artifact}" ]] && continue
        printf ', { "path": "%s" }' "${artifact}"
      done
    )
  ]
}
JSON

spec_ops_emit_telemetry "${TELEMETRY}"
echo "Validate guardrail executed for ${SPEC_ID}"
echo "Telemetry: ${SPEC_OPS_TELEMETRY}"

if [[ ${HAL_FAILURE} -ne 0 ]]; then
  exit 1
fi
