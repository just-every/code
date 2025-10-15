#!/usr/bin/env bash
# Display comprehensive status for a SPEC-ID across all stages
# Usage: spec_ops_status.sh --spec <SPEC-ID> [--json] [--stale-hours N]

set -euo pipefail

usage() {
  cat <<'EOF_USAGE' >&2
Usage: spec_ops_status.sh --spec <SPEC-ID> [--json] [--stale-hours <hours>]

Flags:
  --spec <SPEC-ID>      Target SPEC identifier (e.g. SPEC-KIT-DEMO)
  --json                Emit structured JSON (schemaVersion 1.1)
  --stale-hours <n>     Override stale threshold (default 24)
EOF_USAGE
  exit 1
}

baseline_scan() {
  local spec_id="$1"
  local spec_dir
  spec_dir=$(find "${REPO_ROOT}/docs" -maxdepth 1 -type d -name "${spec_id}*" -print -quit 2>/dev/null || true)

  if [[ -z "${spec_dir}" ]]; then
    echo "❌ SPEC directory not found"
    echo ""
    echo "Expected: docs/${spec_id}*/"
    return 1
  fi

  echo "📁 SPEC Directory: ${spec_dir}"
  echo ""

  echo "## Core Files"
  echo ""
  for file in PRD.md spec.md plan.md tasks.md; do
    if [[ -f "${spec_dir}/${file}" ]]; then
      local lines
      lines=$(wc -l < "${spec_dir}/${file}")
      echo "  ✓ ${file} (${lines} lines)"
    else
      echo "  ❌ ${file} missing"
    fi
  done
  echo ""

  echo "## SPEC.md Tracker"
  echo ""
  if grep -q "${spec_id}" "${REPO_ROOT}/SPEC.md" 2>/dev/null; then
    local entry
    entry=$(grep "${spec_id}" "${REPO_ROOT}/SPEC.md" | head -1)
    echo "  ✓ Entry exists"
    echo "  \`\`\`"
    echo "  ${entry}"
    echo "  \`\`\`"
  else
    echo "  ❌ No SPEC.md entry found"
  fi
  echo ""

  echo "## Stage Status"
  echo ""
  echo "| Stage | Guardrail | Consensus | Agents | Status |"
  echo "|-------|-----------|-----------|--------|--------|"

  local stages=(plan tasks implement validate audit unlock)
  for stage in "${stages[@]}"; do
    local guardrail_status="❌ Not run"
    local consensus_status="❌ Not run"
    local agent_count=0
    local overall_status="⏳ Pending"

    local guardrail_glob="${EVIDENCE_ROOT}/commands/${spec_id}/spec-${stage}_*.json"
    if compgen -G "${guardrail_glob}" >/dev/null 2>&1; then
      local latest_guardrail
      latest_guardrail=$(ls -t ${guardrail_glob} 2>/dev/null | head -1)
      if [[ -f "${latest_guardrail}" ]]; then
        local baseline_status
        baseline_status=$(jq -r '.baseline.status // "unknown"' "${latest_guardrail}" 2>/dev/null || echo "unknown")
        guardrail_status="✓ ${baseline_status}"
      fi
    fi

    local consensus_glob="${EVIDENCE_ROOT}/consensus/${spec_id}/spec-${stage}_*_synthesis.json"
    if compgen -G "${consensus_glob}" >/dev/null 2>&1; then
      local latest_consensus
      latest_consensus=$(ls -t ${consensus_glob} 2>/dev/null | head -1)
      if [[ -f "${latest_consensus}" ]]; then
        local cons_status cons_conflicts
        cons_status=$(jq -r '.status // "unknown"' "${latest_consensus}" 2>/dev/null || echo "unknown")
        cons_conflicts=$(jq -r '.consensus.conflicts | length' "${latest_consensus}" 2>/dev/null || echo "0")
        consensus_status="✓ ${cons_status}"
        if [[ "${cons_conflicts}" != "0" ]]; then
          consensus_status="${consensus_status} (${cons_conflicts} conflicts)"
        fi

        local agent_glob="${EVIDENCE_ROOT}/consensus/${spec_id}/spec-${stage}_*_{gemini,claude,gpt_pro,gpt_codex}.json"
        agent_count=$(compgen -G "${agent_glob}" 2>/dev/null | wc -l)
      fi
    fi

    if [[ "${guardrail_status}" == *"✓"* ]] && [[ "${consensus_status}" == *"ok"* ]]; then
      overall_status="✅ Complete"
    elif [[ "${guardrail_status}" == *"✓"* ]]; then
      overall_status="⏳ In progress"
    fi

    echo "| ${stage} | ${guardrail_status} | ${consensus_status} | ${agent_count} | ${overall_status} |"
  done

  echo ""
  echo "## Recent Agent Activity (last 60 min)"
  echo ""
  local agent_dir="${REPO_ROOT}/.code/agents"
  local recent_agents
  recent_agents=$(find "${agent_dir}" -name "result.txt" -mmin -60 2>/dev/null | wc -l)

  echo "Total agents: ${recent_agents}"
  echo ""

  if [[ "${recent_agents}" -gt 0 ]]; then
    echo "### Latest 5 Agents"
    echo ""
    find "${agent_dir}" -name "result.txt" -mmin -60 -printf "%T@ %p\n" 2>/dev/null \
      | sort -rn | head -5 | while read -r _timestamp result; do
          local agent_id
          local size
          local size_kb
          local model
          local first_line
          agent_id=$(basename "$(dirname "${result}")")
          size=$(stat -c '%s' "${result}")
          size_kb=$((size / 1024))
          model=$(grep "^model:" "${result}" 2>/dev/null | head -1 | cut -d: -f2- | tr -d ' ' || echo "unknown")
          first_line=$(head -1 "${result}" | cut -c1-80)
          echo "- \`${agent_id}\` - ${model} - ${size_kb}KB"
          echo "  \`${first_line}...\`"
        done
  fi

  echo ""
  echo "---"
  echo ""
  echo "💡 **Usage:**"
  echo ""
  printf 'View detailed agent logs:\n```\n'
  printf 'bash scripts/spec_ops_004/log_agent_runs.sh 120\n'
  printf '```\n\n'
  printf 'View guardrail telemetry:\n```\n'
  printf 'cat docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/%s/spec-<stage>_*.json | jq .\n' "${spec_id}"
  printf '```\n\n'
  printf 'View consensus synthesis:\n```\n'
  printf 'cat docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/%s/spec-<stage>_*_synthesis.json | jq .\n' "${spec_id}"
  printf '```\n'
}

OUTPUT_JSON=0
SPEC_ID=""
STALE_HOURS_VALUE=24
STALE_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --json)
      OUTPUT_JSON=1
      shift
      ;;
    --spec)
      SPEC_ID="${2:-}"
      shift 2 || usage
      ;;
    --stale-hours)
      HOURS="${2:-}"
      [[ -z "${HOURS}" ]] && usage
      STALE_HOURS_VALUE="${HOURS}"
      STALE_ARGS+=("--stale-hours" "${HOURS}")
      shift 2
      ;;
    --help|-h)
      usage
      ;;
    --*)
      echo "Unknown flag: $1" >&2
      usage
      ;;
    *)
      if [[ -z "${SPEC_ID}" ]]; then
        SPEC_ID="$1"
      else
        echo "Unexpected argument: $1" >&2
        usage
      fi
      shift
      ;;
  esac
done

[[ -z "${SPEC_ID}" ]] && usage

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
EVIDENCE_ROOT="${REPO_ROOT}/docs/SPEC-OPS-004-integrated-coder-hooks/evidence"

STATUS_BIN="${SPEC_STATUS_DUMP_BIN:-}"
if [[ -z "${STATUS_BIN}" ]]; then
  PROFILE="${CODEX_PROFILE:-dev-fast}"
  CANDIDATES=(
    "${REPO_ROOT}/codex-rs/target/${PROFILE}/spec-status-dump"
    "${REPO_ROOT}/codex-rs/target/debug/spec-status-dump"
  )
  for candidate in "${CANDIDATES[@]}"; do
    if [[ -x "${candidate}" ]]; then
      STATUS_BIN="${candidate}"
      break
    fi
  done
fi

REPORT_JSON=""
DEGRADED_MSG=""

if [[ -n "${STATUS_BIN}" && -x "${STATUS_BIN}" ]]; then
  if ! REPORT_JSON="$(${STATUS_BIN} "${SPEC_ID}" "${STALE_ARGS[@]}")"; then
    DEGRADED_MSG="spec-status-dump binary failed"
    REPORT_JSON=""
  fi
fi

if [[ -z "${REPORT_JSON}" ]]; then
  if ! REPORT_JSON="$(cd "${REPO_ROOT}" && scripts/env_run.sh cargo run --quiet --bin spec-status-dump -- "${SPEC_ID}" "${STALE_ARGS[@]}")"; then
    [[ -z "${DEGRADED_MSG}" ]] && DEGRADED_MSG="cargo run --bin spec-status-dump failed"
    REPORT_JSON=""
  fi
fi

if [[ -z "${REPORT_JSON}" ]]; then
  if [[ ${OUTPUT_JSON} -eq 1 ]]; then
    NOW_TS="$(date --iso-8601=seconds)"
    env \
      SPEC_ID="${SPEC_ID}" \
      DEG_MSG="${DEGRADED_MSG:-spec-status-dump unavailable}" \
      NOW="${NOW_TS}" \
      STALE_HOURS="${STALE_HOURS_VALUE}" \
      python3 - <<'PY'
import json
import os
import sys

payload = {
    "schemaVersion": "1.1",
    "specId": os.environ["SPEC_ID"],
    "generatedAt": os.environ["NOW"],
    "staleCutoffHours": int(os.environ.get("STALE_HOURS", "24")),
    "degraded": True,
    "warnings": [os.environ["DEG_MSG"]],
    "render": {
        "markdown": [f"⚠ {os.environ['DEG_MSG']}"],
        "degradedWarning": f"⚠ {os.environ['DEG_MSG']}"
    },
    "tracker": None,
    "stages": [],
    "evidence": {
        "footprintBytes": 0,
        "commandsBytes": 0,
        "consensusBytes": 0,
        "latestArtifact": None,
        "threshold": None,
        "topEntries": []
    }
}
json.dump(payload, sys.stdout)
PY
    exit 0
  fi

  echo "⚠ ${DEGRADED_MSG:-spec-status-dump unavailable}" >&2
  baseline_scan "${SPEC_ID}" || exit 1
  exit 0
else
  if [[ ${OUTPUT_JSON} -eq 1 ]]; then
    echo "${REPORT_JSON}"
    exit 0
  fi

  if command -v jq >/dev/null 2>&1; then
    echo "${REPORT_JSON}" | jq -r "
      if .render.degradedWarning then .render.degradedWarning else empty end,
      (.render.markdown[] // empty)
    " | sed '/^$/d'
  else
    echo "jq not available; falling back to raw JSON" >&2
    echo "${REPORT_JSON}"
  fi

  baseline_scan "${SPEC_ID}" || exit 1
fi
