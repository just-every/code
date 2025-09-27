#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/../common.sh"

if [[ $# -lt 1 ]]; then
  echo "Usage: /spec-ops-tasks <SPEC-ID>" >&2
  exit 1
fi

SPEC_ID="$1"; shift || true

spec_ops_prepare_stage "spec-tasks" "${SPEC_ID}"
spec_ops_write_log "initialising tasks guardrail"

SCHEMA_VERSION=1
TOOL_STATUS="ready"

read -r -d '' TELEMETRY <<JSON || true
{
  "schemaVersion": ${SCHEMA_VERSION},
  "command": "spec-ops-tasks",
  "specId": "${SPEC_ID}",
  "sessionId": "${SPEC_OPS_SESSION_ID}",
  "timestamp": "$(spec_ops_timestamp)",
  "tool": {
    "status": "${TOOL_STATUS}"
  },
  "artifacts": [
    { "path": "${SPEC_OPS_LOG}" }
  ]
}
JSON

spec_ops_emit_telemetry "${TELEMETRY}"
echo "Tasks guardrail executed for ${SPEC_ID}"
echo "Telemetry: ${SPEC_OPS_TELEMETRY}"
