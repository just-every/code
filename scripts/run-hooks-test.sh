#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
CODE_HOME="${ROOT}/.tmp-hooks-test"

mkdir -p "$CODE_HOME"

cat > "${CODE_HOME}/config.toml" <<EOF
[tui]
auto_review_enabled = false
review_auto_resolve = false

[projects."${ROOT}"]

# Ask for approval only when prompt starts with "hook:".
[[projects."${ROOT}".hooks]]
name = "prompt-approval-gate"
event = "user.prompt_submit"
run = ["python3", "${ROOT}/hooks/ask_user_prompt.py"]
timeout_ms = 1500
run_in_background = true

# Block risky shell commands (tool.before).
[[projects."${ROOT}".hooks]]
name = "block-risky-shell"
event = "tool.before"
run = ["python3", "${ROOT}/hooks/check_shell.py"]
timeout_ms = 1500
run_in_background = true
EOF

BIN=$(ls -1t "${ROOT}/.code/working/_target-cache/code"/*/code-rs/dev-fast/code | head -n 1)

if [[ -z "${BIN}" ]]; then
  echo "No dev-fast binary found. Run ./build-fast.sh first." >&2
  exit 1
fi

CODE_HOME="${CODE_HOME}" "${BIN}" -C "${ROOT}"
