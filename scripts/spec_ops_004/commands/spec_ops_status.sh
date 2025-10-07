#!/usr/bin/env bash
# Display comprehensive status for a SPEC-ID across all stages
# Usage: spec_ops_status.sh <SPEC-ID>

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: spec_ops_status.sh <SPEC-ID>" >&2
  exit 1
fi

SPEC_ID="$1"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
EVIDENCE_ROOT="${REPO_ROOT}/docs/SPEC-OPS-004-integrated-coder-hooks/evidence"

echo "# SPEC Status: ${SPEC_ID}"
echo ""
echo "Generated: $(date)"
echo ""

# Check if SPEC exists
SPEC_DIR=$(find "${REPO_ROOT}/docs" -maxdepth 1 -type d -name "${SPEC_ID}*" -print -quit 2>/dev/null || true)

if [[ -z "${SPEC_DIR}" ]]; then
  echo "❌ SPEC directory not found"
  echo ""
  echo "Expected: docs/${SPEC_ID}*/"
  exit 1
fi

echo "📁 SPEC Directory: ${SPEC_DIR}"
echo ""

# Check core files
echo "## Core Files"
echo ""
for FILE in PRD.md spec.md plan.md tasks.md; do
  if [[ -f "${SPEC_DIR}/${FILE}" ]]; then
    SIZE=$(wc -l < "${SPEC_DIR}/${FILE}")
    echo "  ✓ ${FILE} (${SIZE} lines)"
  else
    echo "  ❌ ${FILE} missing"
  fi
done
echo ""

# Check SPEC.md tracker entry
echo "## SPEC.md Tracker"
echo ""
if grep -q "${SPEC_ID}" "${REPO_ROOT}/SPEC.md" 2>/dev/null; then
  ENTRY=$(grep "${SPEC_ID}" "${REPO_ROOT}/SPEC.md" | head -1)
  echo "  ✓ Entry exists"
  echo "  \`\`\`"
  echo "  ${ENTRY}"
  echo "  \`\`\`"
else
  echo "  ❌ No SPEC.md entry found"
fi
echo ""

# Stage status
STAGES=(plan tasks implement validate audit unlock)

echo "## Stage Status"
echo ""
echo "| Stage | Guardrail | Consensus | Agents | Status |"
echo "|-------|-----------|-----------|--------|--------|"

for STAGE in "${STAGES[@]}"; do
  GUARDRAIL_STATUS="❌ Not run"
  CONSENSUS_STATUS="❌ Not run"
  AGENT_COUNT=0
  OVERALL_STATUS="⏳ Pending"

  # Check guardrail telemetry
  GUARDRAIL_FILES="${EVIDENCE_ROOT}/commands/${SPEC_ID}/spec-${STAGE}_*.json"
  if compgen -G "${GUARDRAIL_FILES}" > /dev/null 2>&1; then
    LATEST_GUARDRAIL=$(ls -t ${GUARDRAIL_FILES} 2>/dev/null | head -1)
    if [[ -f "${LATEST_GUARDRAIL}" ]]; then
      BASELINE_STATUS=$(jq -r '.baseline.status // "unknown"' "${LATEST_GUARDRAIL}" 2>/dev/null || echo "unknown")
      GUARDRAIL_STATUS="✓ ${BASELINE_STATUS}"
    fi
  fi

  # Check consensus
  CONSENSUS_FILES="${EVIDENCE_ROOT}/consensus/${SPEC_ID}/spec-${STAGE}_*_synthesis.json"
  if compgen -G "${CONSENSUS_FILES}" > /dev/null 2>&1; then
    LATEST_CONSENSUS=$(ls -t ${CONSENSUS_FILES} 2>/dev/null | head -1)
    if [[ -f "${LATEST_CONSENSUS}" ]]; then
      CONS_STATUS=$(jq -r '.status // "unknown"' "${LATEST_CONSENSUS}" 2>/dev/null || echo "unknown")
      CONS_CONFLICTS=$(jq -r '.consensus.conflicts | length' "${LATEST_CONSENSUS}" 2>/dev/null || echo "0")
      CONSENSUS_STATUS="✓ ${CONS_STATUS}"
      if [[ "${CONS_CONFLICTS}" != "0" ]]; then
        CONSENSUS_STATUS="${CONSENSUS_STATUS} (${CONS_CONFLICTS} conflicts)"
      fi

      # Count agent artifacts
      AGENT_FILES="${EVIDENCE_ROOT}/consensus/${SPEC_ID}/spec-${STAGE}_*_{gemini,claude,gpt_pro,gpt_codex}.json"
      AGENT_COUNT=$(compgen -G "${AGENT_FILES}" 2>/dev/null | wc -l)
    fi
  fi

  # Overall status
  if [[ "${GUARDRAIL_STATUS}" == *"✓"* ]] && [[ "${CONSENSUS_STATUS}" == *"ok"* ]]; then
    OVERALL_STATUS="✅ Complete"
  elif [[ "${GUARDRAIL_STATUS}" == *"✓"* ]]; then
    OVERALL_STATUS="⏳ In progress"
  fi

  echo "| ${STAGE} | ${GUARDRAIL_STATUS} | ${CONSENSUS_STATUS} | ${AGENT_COUNT} | ${OVERALL_STATUS} |"
done

echo ""

# Recent agent activity
echo "## Recent Agent Activity (last 60 min)"
echo ""
AGENT_DIR="${REPO_ROOT}/.code/agents"
RECENT_AGENTS=$(find "${AGENT_DIR}" -name "result.txt" -mmin -60 2>/dev/null | wc -l)

echo "Total agents: ${RECENT_AGENTS}"
echo ""

if [ "$RECENT_AGENTS" -gt 0 ]; then
  echo "### Latest 5 Agents"
  echo ""
  find "${AGENT_DIR}" -name "result.txt" -mmin -60 -printf "%T@ %p\n" 2>/dev/null | \
    sort -rn | head -5 | while read TIMESTAMP RESULT; do
      AGENT_ID=$(basename $(dirname "$RESULT"))
      SIZE=$(stat -c '%s' "$RESULT")
      SIZE_KB=$((SIZE / 1024))
      MODEL=$(grep "^model:" "$RESULT" 2>/dev/null | head -1 | cut -d: -f2 | tr -d ' ' || echo "unknown")
      FIRST=$(head -1 "$RESULT" | cut -c1-80)
      echo "- \`${AGENT_ID}\` - ${MODEL} - ${SIZE_KB}KB"
      echo "  \`${FIRST}...\`"
    done
fi

echo ""
echo "---"
echo ""
echo "💡 **Usage:**"
echo ""
echo "View detailed agent logs:"
echo "\`\`\`"
echo "bash scripts/spec_ops_004/log_agent_runs.sh 120"
echo "\`\`\`"
echo ""
echo "View guardrail telemetry:"
echo "\`\`\`"
echo "cat docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/${SPEC_ID}/spec-<stage>_*.json | jq ."
echo "\`\`\`"
echo ""
echo "View consensus synthesis:"
echo "\`\`\`"
echo "cat docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/${SPEC_ID}/spec-<stage>_*_synthesis.json | jq ."
echo "\`\`\`"
