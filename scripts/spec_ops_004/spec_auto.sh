#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
COMMAND_DIR="${SCRIPT_DIR}/commands"
ENV_RUN="${REPO_ROOT}/scripts/env_run.sh"
CONSENSUS_RUNNER="${SCRIPT_DIR}/consensus_runner.sh"
SYNTHESIS_CHECKER="${SCRIPT_DIR}/check_synthesis.py"
EVIDENCE_ROOT="${REPO_ROOT}/docs/SPEC-OPS-004-integrated-coder-hooks/evidence"

usage() {
  cat <<'USAGE' >&2
Usage: spec_auto.sh <SPEC-ID> [--from <stage>] [--skip-consensus] [-- <additional args>]

Runs the SPEC-OPS-004 guardrail stages AND consensus automation for the given SPEC.
Each stage: guardrail → consensus → synthesis validation → advance or halt

Options:
  --from <stage>       Resume from specified stage (plan|tasks|implement|validate|audit|unlock)
  --skip-consensus     Run guardrails only, skip multi-agent consensus (faster, less thorough)
  --                   Pass remaining arguments to every guardrail command

Stages: plan, tasks, implement, validate, audit, unlock
USAGE
  exit 1
}

if [[ $# -lt 1 ]]; then
  usage
fi

SPEC_ID="$1"
shift

START_STAGE="plan"
SKIP_CONSENSUS="${SPEC_AUTO_SKIP_CONSENSUS:-0}"
declare -a PASS_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from)
      [[ $# -ge 2 ]] || usage
      START_STAGE="${2}"
      shift 2
      ;;
    --skip-consensus)
      SKIP_CONSENSUS=1
      shift
      ;;
    --)
      shift
      PASS_ARGS=("$@")
      break
      ;;
    *)
      usage
      ;;
  esac
done

case "${START_STAGE}" in
  plan|tasks|implement|validate|audit|unlock) ;;
  *)
    echo "Unknown stage '${START_STAGE}'. Expected one of plan, tasks, implement, validate, audit, unlock." >&2
    exit 1
    ;;
esac

stages=(plan tasks implement validate audit unlock)

script_for_stage() {
  case "$1" in
    plan) echo "spec_ops_plan.sh" ;;
    tasks) echo "spec_ops_tasks.sh" ;;
    implement) echo "spec_ops_implement.sh" ;;
    validate) echo "spec_ops_validate.sh" ;;
    audit) echo "spec_ops_audit.sh" ;;
    unlock) echo "spec_ops_unlock.sh" ;;
    *)
      echo "Unknown stage '$1'" >&2
      exit 1
      ;;
  esac
}

should_run=0
for stage in "${stages[@]}"; do
  if [[ ${should_run} -eq 0 ]]; then
    if [[ "${stage}" == "${START_STAGE}" ]]; then
      should_run=1
    else
      continue
    fi
  fi

  script_name="$(script_for_stage "${stage}")"
  command_path="${COMMAND_DIR}/${script_name}"
  if [[ ! -x "${command_path}" ]]; then
    echo "Guardrail command missing or not executable: ${command_path}" >&2
    exit 1
  fi

  echo "=== [spec-auto] ${stage} stage → ${SPEC_ID} ==="

  # Step 1: Run guardrail
  echo "[spec-auto] Running guardrail: spec-ops-${stage}"
  if [[ -x "${ENV_RUN}" ]]; then
    "${ENV_RUN}" "${command_path}" "${SPEC_ID}" "${PASS_ARGS[@]}"
  else
    "${command_path}" "${SPEC_ID}" "${PASS_ARGS[@]}"
  fi

  # Step 2: Run consensus (unless skipped)
  if [[ "${SKIP_CONSENSUS}" != "1" ]]; then
    echo "[spec-auto] Running consensus: spec-${stage}"

    if [[ ! -x "${CONSENSUS_RUNNER}" ]]; then
      echo "ERROR: Consensus runner not found at ${CONSENSUS_RUNNER}" >&2
      exit 1
    fi

    # Execute multi-agent consensus
    if ! "${CONSENSUS_RUNNER}" \
      --stage "spec-${stage}" \
      --spec "${SPEC_ID}" \
      --execute; then
      echo "ERROR: Consensus execution failed for spec-${stage}" >&2
      exit 1
    fi

    # Step 3: Validate synthesis status
    echo "[spec-auto] Checking consensus status: spec-${stage}"

    if [[ ! -x "${SYNTHESIS_CHECKER}" ]]; then
      echo "ERROR: Synthesis checker not found at ${SYNTHESIS_CHECKER}" >&2
      exit 1
    fi

    if ! python3 "${SYNTHESIS_CHECKER}" "${SPEC_ID}" "${stage}" "${EVIDENCE_ROOT}"; then
      exit_code=$?
      echo "ERROR: Consensus failed for spec-${stage} (exit ${exit_code})" >&2
      echo "Run manually: ${CONSENSUS_RUNNER} --stage spec-${stage} --spec ${SPEC_ID} --execute" >&2
      exit ${exit_code}
    fi

    echo "[spec-auto] ✓ Consensus OK for spec-${stage}"
  else
    echo "[spec-auto] Skipping consensus for ${stage} (--skip-consensus enabled)"
  fi

  echo ""
done

echo "=== [spec-auto] Pipeline complete for ${SPEC_ID} ==="
if [[ "${SKIP_CONSENSUS}" == "1" ]]; then
  echo "    (guardrails only - consensus was skipped)"
else
  echo "    (guardrails + consensus validated)"
fi
