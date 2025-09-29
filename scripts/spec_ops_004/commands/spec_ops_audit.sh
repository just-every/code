#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../common.sh"

if [[ $# -lt 1 ]]; then
  echo "Usage: /spec-ops-audit <SPEC-ID> [--manifest-path <path>]" >&2
  exit 1
fi

SPEC_ID="$1"; shift || true
MANIFEST_PATH=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --manifest-path)
      if [[ $# -lt 2 ]]; then
        echo "--manifest-path requires a value" >&2
        exit 1
      fi
      MANIFEST_PATH="$2"; shift 2 ;;
    *)
      echo "Unknown argument: $1" >&2
      exit 1 ;;
  esac
done

if [[ -n "${MANIFEST_PATH}" ]]; then
  spec_ops_set_manifest_path "${MANIFEST_PATH}"
fi

spec_ops_prepare_stage "spec-audit" "${SPEC_ID}"
spec_ops_write_log "audit guardrail ready"
spec_ops_write_log "using manifest $(spec_ops_manifest_path)"

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
SCENARIO_STATUS="${SPEC_OPS_HAL_STATUS:-failed}"

hal_summary=""
hal_summary=$(spec_ops_hal_summary_block "${SCENARIO_STATUS}") || hal_summary=""

read -r -d '' TELEMETRY <<JSON || true
{
  "schemaVersion": ${SCHEMA_VERSION},
  "command": "spec-ops-audit",
  "specId": "${SPEC_ID}",
  "sessionId": "${SPEC_OPS_SESSION_ID}",
  "timestamp": "$(spec_ops_timestamp)",
  "scenarios": [
    {
      "name": "audit guardrail bootstrap",
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
  ]${hal_summary}
}
JSON

spec_ops_emit_telemetry "${TELEMETRY}"
echo "Audit guardrail executed for ${SPEC_ID}"
echo "Telemetry: ${SPEC_OPS_TELEMETRY}"

if [[ ${HAL_FAILURE} -ne 0 ]]; then
  exit 1
fi
