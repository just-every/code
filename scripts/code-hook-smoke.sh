#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"

BIN="${CODE_HOOK_SMOKE_BIN:-${REPO_ROOT}/code-rs/target/debug/code}"
PROMPT_TEXT="${CODE_HOOK_SMOKE_PROMPT:-finish the task}"
KEEP_ROOT="${CODE_HOOK_SMOKE_KEEP_ROOT:-0}"

if [[ ! -x "${BIN}" ]]; then
  echo "[code-hook-smoke] missing CLI binary: ${BIN}" >&2
  echo "[code-hook-smoke] build it first with: (cd code-rs && cargo build --bin code --bin code-tui --bin code-exec)" >&2
  exit 1
fi

ROOT="$(mktemp -d /tmp/code-hook-smoke.XXXXXX)"
SMOKE_HOME="${ROOT}/home"
PROJECT="${ROOT}/project"
HOOK_LOG="${ROOT}/hooks.log"
REQ_LOG="${ROOT}/requests.log"
CLI_OUT="${ROOT}/cli.out"
CLI_ERR="${ROOT}/cli.err"
PORT_FILE="${ROOT}/port"

cleanup() {
  if [[ -n "${SERVER_PID:-}" ]]; then
    kill "${SERVER_PID}" >/dev/null 2>&1 || true
    wait "${SERVER_PID}" 2>/dev/null || true
  fi
  if [[ "${KEEP_ROOT}" != "1" ]]; then
    rm -rf "${ROOT}"
  fi
}
trap cleanup EXIT

mkdir -p "${SMOKE_HOME}" "${PROJECT}"
git -C "${PROJECT}" init -q

cat > "${SMOKE_HOME}/config.toml" <<EOF
model = "gpt-5.1-codex"

[projects."${PROJECT}"]
trust_level = "trusted"

[[projects."${PROJECT}".hooks]]
event = "user.prompt_submit"
run = ["bash", "-c", "echo 'Injected smoke context'; echo prompt:\$CODE_HOOK_EVENT >> '${HOOK_LOG}'"]

[[projects."${PROJECT}".hooks]]
event = "stop"
run = ["bash", "-c", "if printf '%s' \"\$CODE_HOOK_PAYLOAD\" | grep -q '\"stop_hook_active\":true'; then echo stop:\$CODE_HOOK_EVENT >> '${HOOK_LOG}'; exit 0; fi; echo 'continue via smoke stop' >&2; echo stop:\$CODE_HOOK_EVENT >> '${HOOK_LOG}'; exit 2"]
EOF

python3 - "${PORT_FILE}" "${REQ_LOG}" <<'PY' &
import http.server
import json
import socketserver
import sys

port_file, req_log = sys.argv[1:3]
state = {"count": 0}


class Handler(http.server.BaseHTTPRequestHandler):
    def log_message(self, *args):
        return

    def do_POST(self):
        length = int(self.headers.get("content-length", "0"))
        body = self.rfile.read(length)
        state["count"] += 1
        with open(req_log, "ab") as fh:
            fh.write(b"===REQUEST===\n")
            fh.write(body)
            fh.write(b"\n")

        idx = state["count"]
        text = "draft complete" if idx == 1 else "all set"
        events = [
            {
                "type": "response.output_item.done",
                "item": {
                    "type": "message",
                    "id": f"msg-{idx}",
                    "role": "assistant",
                    "content": [{"type": "output_text", "text": text}],
                },
            },
            {
                "type": "response.completed",
                "response": {
                    "id": f"resp-{idx}",
                    "usage": {
                        "input_tokens": 0,
                        "input_tokens_details": None,
                        "output_tokens": 0,
                        "output_tokens_details": None,
                        "total_tokens": 0,
                    },
                },
            },
        ]

        self.send_response(200)
        self.send_header("content-type", "text/event-stream")
        self.end_headers()
        for event in events:
            payload = json.dumps(event)
            self.wfile.write(f"event: {event['type']}\ndata: {payload}\n\n".encode())
        self.wfile.flush()


class Server(socketserver.ThreadingMixIn, http.server.HTTPServer):
    daemon_threads = True


server = Server(("127.0.0.1", 0), Handler)
with open(port_file, "w", encoding="utf-8") as fh:
    fh.write(str(server.server_port))
server.serve_forever()
PY
SERVER_PID=$!

for _ in $(seq 1 50); do
  if [[ -s "${PORT_FILE}" ]]; then
    break
  fi
  sleep 0.1
done
PORT="$(cat "${PORT_FILE}")"

set +e
(
  cd "${PROJECT}"
  CODE_HOME="${SMOKE_HOME}" \
  CODEX_HOME="${SMOKE_HOME}" \
  CODEX_API_KEY=test \
  OPENAI_BASE_URL="http://127.0.0.1:${PORT}/v1" \
  "${BIN}" exec --skip-git-repo-check --sandbox danger-full-access "${PROMPT_TEXT}"
) >"${CLI_OUT}" 2>"${CLI_ERR}"
STATUS=$?
set -e

REQUEST_COUNT="$(grep -c '^===REQUEST===' "${REQ_LOG}")"
INJECTED_CONTEXT_COUNT="$(grep -o 'Injected smoke context' "${REQ_LOG}" | wc -l | tr -d ' ')"
STOP_PROMPT_COUNT="$(grep -o 'continue via smoke stop' "${REQ_LOG}" | wc -l | tr -d ' ')"

echo "SMOKE_ROOT=${ROOT}"
echo "SMOKE_EXIT=${STATUS}"
echo "REQUEST_COUNT=${REQUEST_COUNT}"
echo "HOOK_LOG_EXISTS=$([ -f "${HOOK_LOG}" ] && echo yes || echo no)"
echo "INJECTED_CONTEXT_PRESENT=$(grep -q 'Injected smoke context' "${REQ_LOG}" && echo yes || echo no)"
echo "STOP_PROMPT_PRESENT=$(grep -q 'continue via smoke stop' "${REQ_LOG}" && echo yes || echo no)"
echo "INJECTED_CONTEXT_COUNT=${INJECTED_CONTEXT_COUNT}"
echo "STOP_PROMPT_COUNT=${STOP_PROMPT_COUNT}"
echo "--- HOOK LOG ---"
cat "${HOOK_LOG}"
echo "--- REQUEST LOG SNIPPET ---"
sed -n '1,80p' "${REQ_LOG}"
echo "--- CLI STDOUT ---"
cat "${CLI_OUT}"
echo "--- CLI STDERR ---"
cat "${CLI_ERR}"

if [[ "${STATUS}" -ne 0 ]]; then
  exit "${STATUS}"
fi
if [[ ! -f "${HOOK_LOG}" ]]; then
  echo "missing hook log" >&2
  exit 1
fi
if ! grep -q 'prompt:user.prompt_submit' "${HOOK_LOG}"; then
  echo "user.prompt_submit did not fire" >&2
  exit 1
fi
if [[ "$(grep -c '^stop:stop$' "${HOOK_LOG}")" -lt 2 ]]; then
  echo "stop hook did not fire twice" >&2
  exit 1
fi
if ! grep -q 'Injected smoke context' "${REQ_LOG}"; then
  echo "injected context missing from model input" >&2
  exit 1
fi
if ! grep -q 'continue via smoke stop' "${REQ_LOG}"; then
  echo "stop continuation prompt missing from model input" >&2
  exit 1
fi
if [[ "${STOP_PROMPT_COUNT}" -ne 1 ]]; then
  echo "stop continuation prompt should appear exactly once in model input" >&2
  exit 1
fi
