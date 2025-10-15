#!/usr/bin/env bash

set -Eeuo pipefail

MODE="templates"
ROOT_DIR=$(git rev-parse --show-toplevel 2>/dev/null || pwd)

usage() {
  cat <<'USAGE'
Usage: scripts/doc-structure-validate.sh [--mode=<mode>]

Modes:
  templates   Validate Spec Kit documentation directories (default)

Examples:
  scripts/doc-structure-validate.sh --mode=templates
USAGE
}

for arg in "$@"; do
  case "$arg" in
    --mode=*)
      MODE=${arg#*=}
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $arg" >&2
      usage >&2
      exit 2
      ;;
  esac
done

errors=()

expect_heading() {
  local file="$1"
  local heading="$2"
  if [[ ! -f "$file" ]]; then
    errors+=("missing file: $file")
    return
  fi

  local first_line
  first_line=$(head -n 1 "$file" | tr -d '\r')
  if [[ $first_line != "$heading"* ]]; then
    errors+=("$file: expected first heading to start with '$heading' (found '$first_line')")
  fi
}

require_section() {
  local file="$1"
  local section="$2"
  if [[ ! -f "$file" ]]; then
    errors+=("missing file: $file")
    return
  fi
  if ! grep -q "^$section$" "$file"; then
    errors+=("$file: missing section heading '$section'")
  fi
}

validate_templates() {
  local spec_dirs
  mapfile -t spec_dirs < <(find "$ROOT_DIR/docs" -maxdepth 1 -type d -name 'SPEC-*' | sort)

  if [[ ${#spec_dirs[@]} -eq 0 ]]; then
    errors+=("no SPEC-* directories found under docs/")
    return
  fi

  for dir in "${spec_dirs[@]}"; do
    local spec_file="$dir/spec.md"
    local plan_file="$dir/plan.md"
    local tasks_file="$dir/tasks.md"

    expect_heading "$spec_file" "# Spec"
    expect_heading "$plan_file" "# Plan"
    expect_heading "$tasks_file" "# Tasks"

    require_section "$plan_file" "## Inputs"
    require_section "$plan_file" "## Work Breakdown"
    require_section "$plan_file" "## Acceptance Mapping"
    require_section "$plan_file" "## Risks & Unknowns"
    require_section "$plan_file" "## Consensus & Risks (Multi-AI)"
    require_section "$plan_file" "## Exit Criteria (Done)"

    require_section "$spec_file" "## Context"
    require_section "$spec_file" "## Objectives"

    require_section "$tasks_file" "| Order | Task | Owner | Status | Validation |"
  done
}

case "$MODE" in
  templates)
    validate_templates
    ;;
  *)
    echo "Unsupported mode: $MODE" >&2
    usage >&2
    exit 2
    ;;
esac

if [[ ${#errors[@]} -gt 0 ]]; then
  printf 'Doc structure validation failed:\n' >&2
  for err in "${errors[@]}"; do
    printf '  - %s\n' "$err" >&2
  done
  exit 1
fi

printf 'Doc structure validation passed (mode=%s)\n' "$MODE"
