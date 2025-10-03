#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
PROMPTS_FILE="${REPO_ROOT}/docs/spec-kit/prompts.json"
OUTPUT_BASE="${REPO_ROOT}/docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus"
RENDERER="${SCRIPT_DIR}/render_prompt.py"
CODEX_BIN_DEFAULT="${REPO_ROOT}/codex-rs/target/dev-fast/code"

usage() {
  cat <<'USAGE' >&2
Usage: consensus_runner.sh --stage <stage> --spec <SPEC-ID> [options]

Options:
  --stage <stage>            Stage name (spec-plan, spec-tasks, spec-implement, spec-validate,
                             spec-audit, spec-unlock)
  --spec <SPEC-ID>          SPEC identifier (e.g. SPEC-KIT-DEMO)
  --context-file <path>     Additional context file to append
  --output-dir <path>       Override consensus evidence directory
  --dry-run                 Render prompts and write prompt files without executing models
  --execute                 Invoke Codex CLI (`code exec`) for each agent
  --allow-conflict          Exit 0 even if consensus reports conflicts
  --help                    Show this help message

Environment overrides:
  CODEX_BIN                 Path to Codex CLI binary (default: codex-rs/target/dev-fast/code)
  CONSENSUS_MODEL_GEMINI    Override model id for Gemini agent
  CONSENSUS_MODEL_CLAUDE    Override model id for Claude agent
  CONSENSUS_MODEL_GPT_PRO   Override model id for GPT Pro agent
  CONSENSUS_MODEL_GPT_CODEX Override model id for GPT Codex agent
USAGE
  exit 1
}

stage=""
spec=""
context_file=""
output_dir=""
dry_run=0
execute=0
allow_conflict=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --stage)
      stage="$2"; shift 2 ;;
    --spec)
      spec="$2"; shift 2 ;;
    --context-file)
      context_file="$2"; shift 2 ;;
    --output-dir)
      output_dir="$2"; shift 2 ;;
    --dry-run)
      dry_run=1; shift ;;
    --execute)
      execute=1; shift ;;
    --allow-conflict)
      allow_conflict=1; shift ;;
    --help|-h)
      usage ;;
    *)
      echo "Unknown argument: $1" >&2
      usage ;;
  esac
done

if [[ -z "${stage}" || -z "${spec}" ]]; then
  echo "ERROR: --stage and --spec are required" >&2
  usage
fi

timestamp() {
  date -u +"%Y-%m-%dT%H:%M:%SZ"
}

slugify() {
  echo "$1" \
    | tr '[:upper:]' '[:lower:]' \
    | tr -cs '[:alnum:]' '-' \
    | sed 's/^-//; s/-$//'
}

CODEX_BIN="${CODEX_BIN:-${CODEX_BIN_DEFAULT}}"

stage_slug="$(slugify "${stage}")"

if [[ -z "${output_dir}" ]]; then
  output_dir="${OUTPUT_BASE}/${spec}"
fi

mkdir -p "${output_dir}"

if [[ ! -x "${RENDERER}" ]]; then
  echo "ERROR: prompt renderer missing at ${RENDERER}" >&2
  exit 1
fi

if ! agents=$(python3 "${RENDERER}" agents "${stage}" "${PROMPTS_FILE}"); then
  exit 1
fi

read -r prompt_version < <(python3 -c "import json,sys; data=json.load(open('${PROMPTS_FILE}')); print(data['${stage}']['version']) if 'version' in data['${stage}'] else sys.exit('Missing version')")

context_data=""

collect_context() {
  local spec_id="$1"
  local ctx=""

  local spec_dir
  spec_dir=$(find "${REPO_ROOT}/docs" -maxdepth 1 -type d -name "${spec_id}*" -print -quit || true)
  if [[ -n "${spec_dir}" ]]; then
    for fname in spec.md plan.md tasks.md; do
      if [[ -f "${spec_dir}/${fname}" ]]; then
        ctx+=$'\n\n'
        ctx+="===== ${fname} =====\n"
        ctx+="$(cat "${spec_dir}/${fname}")"
      fi
    done
  fi

  if [[ -n "${context_file}" ]]; then
    if [[ -f "${context_file}" ]]; then
      ctx+=$'\n\n'
      ctx+="===== context-file =====\n"
      ctx+="$(cat "${context_file}")"
    else
      echo "WARNING: context file not found: ${context_file}" >&2
    fi
  fi

  echo "${ctx}"
}

context_data=$(collect_context "${spec}")

declare -A agent_model_id
declare -A agent_model_release
declare -A agent_reasoning

agent_model_id[gemini]="${CONSENSUS_MODEL_GEMINI:-gemini-2.5-pro}"
agent_model_release[gemini]="2025-08-06"
agent_reasoning[gemini]="thinking"

agent_model_id[claude]="${CONSENSUS_MODEL_CLAUDE:-claude-4.5-sonnet}"
agent_model_release[claude]="2025-09-29"
agent_reasoning[claude]="auto"

agent_model_id[gpt_pro]="${CONSENSUS_MODEL_GPT_PRO:-gpt-5}"
agent_model_release[gpt_pro]="2025-09-29"
agent_reasoning[gpt_pro]="high"

agent_model_id[gpt_codex]="${CONSENSUS_MODEL_GPT_CODEX:-gpt-5-codex}"
agent_model_release[gpt_codex]="2025-09-29"
agent_reasoning[gpt_codex]="high"

declare -A previous_outputs
declare -A prompt_files

timestamp_run="$(timestamp)"

render_prompt() {
  local agent="$1"
  local agent_prev_json="${previous_outputs[$agent]:-}" || true
  local all_prev="{"
  local first=1
  for key in "${!previous_outputs[@]}"; do
    if [[ -n "${previous_outputs[$key]}" ]]; then
      if [[ ${first} -eq 0 ]]; then
        all_prev+=" ,"
      fi
      all_prev+="\"${key}\": ${previous_outputs[$key]}"
      first=0
    fi
  done
  all_prev+="}"

  python3 "${RENDERER}" render "${stage}" "${agent}" "${PROMPTS_FILE}" "${spec}" "${prompt_version}" \
    "${agent_model_id[$agent]}" "${agent_model_release[$agent]}" "${agent_reasoning[$agent]}" \
    "${context_data}" "${all_prev}" "${agent_prev_json}"
}

write_prompt_file() {
  local agent="$1"
  local prompt_content="$2"
  local file_path="${output_dir}/${stage_slug}_${timestamp_run}_${agent}_prompt.txt"
  printf '%s\n' "${prompt_content}" >"${file_path}"
  prompt_files[$agent]="${file_path}"
  echo "Prompt written to ${file_path}"
}

run_agent() {
  local agent="$1"
  local prompt_content="$2"
  local output_file="${output_dir}/${stage_slug}_${timestamp_run}_${agent}.json"

  if [[ ${dry_run} -eq 1 ]]; then
    write_prompt_file "${agent}" "${prompt_content}"
    return 0
  fi

  if [[ ${execute} -ne 1 ]]; then
    echo "Skipping execution for ${agent}; use --execute to invoke models" >&2
    write_prompt_file "${agent}" "${prompt_content}"
    return 0
  fi

  if [[ ! -x "${CODEX_BIN}" ]]; then
    echo "ERROR: Codex CLI not executable at ${CODEX_BIN}" >&2
    exit 1
  fi

  local prompt_file
  prompt_file="$(mktemp)"
  printf '%s\n' "${prompt_content}" >"${prompt_file}"

  local last_message_file
  last_message_file="$(mktemp)"

  echo "Executing ${agent} via ${CODEX_BIN}" >&2
  if ! cat "${prompt_file}" | "${CODEX_BIN}" \
      exec \
      --sandbox read-only \
      --model "${agent_model_id[$agent]}" \
      --reasoning "${agent_reasoning[$agent]}" \
      --output-last-message "${last_message_file}" \
      --skip-git-repo-check \
      --cd "${REPO_ROOT}" \
      --json \
      - 2>&1; then
    echo "ERROR: agent ${agent} failed" >&2
    cat "${last_message_file}" >&2 || true
    rm -f "${prompt_file}" "${last_message_file}"
    exit 1
  fi

  rm -f "${prompt_file}"

  if [[ ! -s "${last_message_file}" ]]; then
    echo "ERROR: agent ${agent} produced empty output" >&2
    rm -f "${last_message_file}"
    exit 1
  fi

  if ! python3 -c "import json,sys; json.load(open('${last_message_file}'))" 2>/dev/null; then
    echo "ERROR: agent ${agent} output is not valid JSON" >&2
    rm -f "${last_message_file}"
    exit 1
  fi

  python3 -c "import json,sys; data=json.load(open('${last_message_file}')); open('${output_file}', 'w').write(json.dumps(data, indent=2, ensure_ascii=False))" 2>/dev/null || {
    echo "ERROR: failed to persist agent ${agent} output" >&2
    rm -f "${last_message_file}"
    exit 1
  }

  rm -f "${last_message_file}"

  previous_outputs["${agent}"]="$(cat "${output_file}")"
  echo "Agent ${agent} output saved to ${output_file}"
}

IFS=' ' read -r -a agents_array <<<"${agents}"

for agent in "${agents_array[@]}"; do
  prompt_text="$(render_prompt "${agent}")"
  run_agent "${agent}" "${prompt_text}"
done

if [[ ${dry_run} -eq 1 || ${execute} -ne 1 ]]; then
  echo "Consensus synthesis skipped (dry-run or execute disabled)." >&2
  exit 0
fi

synthesis_file="${output_dir}/${stage_slug}_${timestamp_run}_synthesis.json"

python3 - "$synthesis_file" "$allow_conflict" <<'PYCODE'
import json
import sys
from pathlib import Path

stage = "${stage}"
spec = "${spec}"
timestamp_run = "${timestamp_run}"

args = sys.argv[1:]
synthesis_path = Path(args[0])
allow_conflict = bool(int(args[1]))

agents = ${json.dumps(list(previous_outputs.keys()))}
outputs = {}

for agent in agents:
    file_path = Path(f"${output_dir}/${stage_slug}_{timestamp_run}_{agent}.json")
    outputs[agent] = json.load(file_path.open())

consensus = {"agreements": [], "conflicts": []}

for agent, payload in outputs.items():
    node = payload.get("final_plan") or payload.get("consensus") or {}
    if isinstance(node, dict):
        consensus["agreements"].extend(node.get("agreements", []))
        consensus["conflicts"].extend(node.get("conflicts", []))

status = "ok"
if consensus["conflicts"]:
    status = "conflict"
elif len(outputs) < len(agents):
    status = "degraded"

payload = {
    "stage": stage,
    "specId": "${spec}",
    "timestamp": "${timestamp_run}",
    "prompt_version": "${prompt_version}",
    "agents": [{"agent": agent, "path": f"${output_dir}/${stage_slug}_{timestamp_run}_{agent}.json"} for agent in agents],
    "consensus": consensus,
    "status": status,
}

json.dump(payload, synthesis_path.open("w"), indent=2)

if status == "conflict" and not allow_conflict:
    sys.exit(2)
if status == "degraded":
    sys.exit(3)
PYCODE

exit_code=$?
if [[ ${exit_code} -ne 0 ]]; then
  echo "Consensus synthesis failed (exit ${exit_code}). See ${synthesis_file}" >&2
  exit ${exit_code}
fi

echo "Consensus synthesis written to ${synthesis_file}"
