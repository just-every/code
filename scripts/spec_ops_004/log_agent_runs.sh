#!/usr/bin/env bash
# Log and analyze agent runs from actual agent directory structure
# Agents create: UUID-dir/result.txt (no metadata.json)

set -euo pipefail

MINUTES_BACK="${1:-120}"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
AGENT_DIR="${REPO_ROOT}/.code/agents"
OUTPUT_FILE="${REPO_ROOT}/agent_execution_log_$(date +%Y%m%d-%H%M%S).md"

{
  echo "# Agent Execution Log"
  echo ""
  echo "Generated: $(date)"
  echo "Agent directory: ${AGENT_DIR}"
  echo ""

  # Find all agent result files from last N minutes
  RESULTS=$(find "${AGENT_DIR}" -name "result.txt" -mmin -${MINUTES_BACK} 2>/dev/null)
  TOTAL=$(echo "$RESULTS" | grep -c "result.txt" || echo "0")

  echo "## Summary"
  echo ""
  echo "Total agents (last ${MINUTES_BACK} min): **${TOTAL}**"
  echo ""

  if [ "$TOTAL" -eq 0 ]; then
    echo "No recent agent executions found."
    exit 0
  fi

  echo "## Agent List"
  echo ""
  echo "| # | Agent ID | Modified | Size | First Line |"
  echo "|---|----------|----------|------|------------|"

  N=1
  for RESULT in $RESULTS; do
    DIR=$(dirname "$RESULT")
    AGENT_ID=$(basename "$DIR")
    MODIFIED=$(stat -c '%y' "$RESULT" | cut -d'.' -f1)
    SIZE=$(stat -c '%s' "$RESULT")
    SIZE_KB=$((SIZE / 1024))
    FIRST_LINE=$(head -1 "$RESULT" 2>/dev/null | cut -c1-60 || echo "(empty)")

    echo "| $N | \`$AGENT_ID\` | $MODIFIED | ${SIZE_KB}KB | $FIRST_LINE |"
    N=$((N + 1))
  done

  echo ""
  echo "## Analysis"
  echo ""

  # Extract model info from result files
  echo "### Models Used"
  echo '```'
  for RESULT in $RESULTS; do
    grep -m 1 "model:" "$RESULT" 2>/dev/null | sed 's/^/  /' || true
  done | sort | uniq -c | sort -rn
  echo '```'
  echo ""

  # Check for errors
  ERRORS=$(find "${AGENT_DIR}" -name "error.txt" -mmin -${MINUTES_BACK} 2>/dev/null | wc -l)
  echo "### Errors"
  if [ "$ERRORS" -gt 0 ]; then
    echo ""
    echo "⚠️ **${ERRORS} agents failed**"
    echo ""
    find "${AGENT_DIR}" -name "error.txt" -mmin -${MINUTES_BACK} -exec sh -c '
      echo "**Error in** \`$(basename $(dirname {}))\`:"
      echo "\`\`\`"
      head -20 {}
      echo "\`\`\`"
      echo ""
    ' \;
  else
    echo "✓ No errors"
  fi
  echo ""

  # Extract workdir to understand what agents were working on
  echo "### Working Directories"
  echo '```'
  for RESULT in $RESULTS; do
    grep "^workdir:" "$RESULT" 2>/dev/null || true
  done | sort | uniq -c | sort -rn
  echo '```'
  echo ""

  # Check for stage patterns (gemini-plan, claude-tasks, etc.)
  echo "### Detected Stage Patterns"
  echo '```'
  for RESULT in $RESULTS; do
    # Try to extract agent name from prompt or instructions
    grep -E "agent.*gemini|agent.*claude|agent.*gpt|agent.*code" "$RESULT" 2>/dev/null | head -1 || true
  done | sed 's/.*agent[^:]*: *//' | cut -d',' -f1 | sort | uniq -c | sort -rn
  echo '```'

} > "${OUTPUT_FILE}"

echo "Report written to: ${OUTPUT_FILE}"
cat "${OUTPUT_FILE}"
