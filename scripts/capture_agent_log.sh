#!/usr/bin/env bash
# Capture end-to-end orchestrator output for analysis
# Usage: ./capture_agent_log.sh <agent-id>

AGENT_ID="${1:-latest}"
AGENT_DIR="/home/thetu/code/.code/agents"
OUTPUT_DIR="/home/thetu/code/test-logs"

mkdir -p "${OUTPUT_DIR}"

if [[ "${AGENT_ID}" == "latest" ]]; then
  AGENT_ID=$(ls -t "${AGENT_DIR}" | head -1)
fi

AGENT_PATH="${AGENT_DIR}/${AGENT_ID}"

if [[ ! -d "${AGENT_PATH}" ]]; then
  echo "Agent directory not found: ${AGENT_PATH}"
  exit 1
fi

TIMESTAMP=$(date +%Y%m%d-%H%M%S)
LOG_FILE="${OUTPUT_DIR}/agent-${AGENT_ID}-${TIMESTAMP}.log"

{
  echo "=== AGENT ${AGENT_ID} ==="
  echo "Timestamp: ${TIMESTAMP}"
  echo ""
  
  if [[ -f "${AGENT_PATH}/result.txt" ]]; then
    echo "=== RESULT ==="
    cat "${AGENT_PATH}/result.txt"
  fi
  
  if [[ -f "${AGENT_PATH}/error.txt" ]]; then
    echo ""
    echo "=== ERRORS ==="
    cat "${AGENT_PATH}/error.txt"
  fi
  
  if [[ -f "${AGENT_PATH}/prompt.txt" ]]; then
    echo ""
    echo "=== PROMPT ==="
    head -100 "${AGENT_PATH}/prompt.txt"
  fi
} > "${LOG_FILE}"

echo "Log captured: ${LOG_FILE}"
cat "${LOG_FILE}"
