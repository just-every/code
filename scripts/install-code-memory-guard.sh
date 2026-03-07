#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR=$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
REPO_ROOT=$(cd -- "${SCRIPT_DIR}/.." && pwd)

sudo install -Dm755 \
    "${REPO_ROOT}/scripts/code-memory-guard.sh" \
    /usr/local/bin/code-memory-guard.sh

sudo install -Dm644 \
    "${REPO_ROOT}/systemd/code-memory-guard.service" \
    /etc/systemd/system/code-memory-guard.service

sudo systemctl daemon-reload
sudo systemctl enable code-memory-guard.service
sudo systemctl restart code-memory-guard.service
sudo systemctl --no-pager --full status code-memory-guard.service
