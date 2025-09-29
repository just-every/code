#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../common.sh"

if [[ $# -lt 1 ]]; then
  echo "Usage: /spec-ops-unlock <SPEC-ID> [--manifest-path <path>]" >&2
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

spec_ops_prepare_stage "spec-unlock" "${SPEC_ID}"
spec_ops_write_log "unlock guardrail ready"
spec_ops_write_log "using manifest $(spec_ops_manifest_path)"

SCHEMA_VERSION=1
UNLOCK_STATUS="unlocked"

read -r -d '' TELEMETRY <<JSON || true
{
  "schemaVersion": ${SCHEMA_VERSION},
  "command": "spec-ops-unlock",
  "specId": "${SPEC_ID}",
  "sessionId": "${SPEC_OPS_SESSION_ID}",
  "timestamp": "$(spec_ops_timestamp)",
  "unlock_status": "${UNLOCK_STATUS}",
  "artifacts": [
    { "path": "${SPEC_OPS_LOG}" }
  ]
}
JSON

spec_ops_emit_telemetry "${TELEMETRY}"
echo "Unlock guardrail executed for ${SPEC_ID}"
echo "Telemetry: ${SPEC_OPS_TELEMETRY}"
