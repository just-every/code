#!/usr/bin/env bash

# Lightweight wrapper used by Spec Ops scripts to ensure environment parity.
# Loads variables from a project .env file when present, then execs the target
# command. This matches the expectations baked into the TUI guardrail helpers.

set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "Usage: env_run.sh <command> [args...]" >&2
  exit 1
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if [[ -f "${REPO_ROOT}/.env" ]]; then
  # shellcheck disable=SC1090
  set -a
  source "${REPO_ROOT}/.env"
  set +a
fi

exec "$@"
