#!/usr/bin/env bash

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
COMMAND_DIR="${SCRIPT_DIR}/commands"
ENV_RUN="${REPO_ROOT}/scripts/env_run.sh"

usage() {
  cat <<'USAGE' >&2
Usage: spec_auto.sh <SPEC-ID> [--from <stage>] [-- <additional args>]

Runs the SPEC-OPS-004 guardrail stages (`spec-plan` through `spec-unlock`) in
sequence for the given SPEC. Use `--from <stage>` to resume from a later stage
and `--` to pass additional arguments to every guardrail command.

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
declare -a PASS_ARGS=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --from)
      [[ $# -ge 2 ]] || usage
      START_STAGE="${2}"
      shift 2
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

  if [[ -x "${ENV_RUN}" ]]; then
    "${ENV_RUN}" "${command_path}" "${SPEC_ID}" "${PASS_ARGS[@]}"
  else
    "${command_path}" "${SPEC_ID}" "${PASS_ARGS[@]}"
  fi
done

echo "=== [spec-auto] guardrail sequence complete for ${SPEC_ID} ==="
