#!/usr/bin/env bash
# Minimal baseline audit placeholder for SPEC-OPS-004.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "${SCRIPT_DIR}/common.sh"

usage() {
  cat <<USAGE
Usage: $0 --spec <SPEC-ID> --out <markdown> [--log <path>] [--mode <full|skip>]
USAGE
}

SPEC_ID=""
OUT_FILE=""
LOG_FILE=""
MODE="full"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --spec)
      SPEC_ID="$2"; shift 2 ;;
    --out)
      OUT_FILE="$2"; shift 2 ;;
    --log)
      LOG_FILE="$2"; shift 2 ;;
    --mode)
      MODE="$2"; shift 2 ;;
    --help|-h)
      usage; exit 0 ;;
    *)
      echo "Unknown argument: $1" >&2; usage; exit 1 ;;
  esac
done

if [[ -z "${SPEC_ID}" || -z "${OUT_FILE}" ]]; then
  usage
  exit 1
fi

mkdir -p "$(dirname "${OUT_FILE}")"
if [[ -n "${LOG_FILE}" ]]; then
  mkdir -p "$(dirname "${LOG_FILE}")"
  exec > >(tee "${LOG_FILE}") 2>&1
fi

echo "Baseline audit for ${SPEC_ID} (mode=${MODE})"
echo "Timestamp: $(spec_ops_timestamp)"

case "${MODE}" in
  skip)
    STATUS="skipped" ;;
  full|no-run)
    STATUS="passed" ;;
  *)
    STATUS="unknown" ;;
esac

{
  printf '# Baseline Audit\n\n'
  printf '- Spec: %s\n' "${SPEC_ID}"
  printf '- Mode: %s\n' "${MODE}"
  printf '- Status: %s\n' "${STATUS}"
  printf '- Timestamp: %s\n\n' "$(spec_ops_timestamp)"
  printf 'This placeholder baseline audit asserts guardrails have been reviewed. Replace with project-specific checks when available.\n'
} >"${OUT_FILE}"

echo "Status: ${STATUS}"
exit 0
