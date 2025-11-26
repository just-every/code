#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$ROOT_DIR/target"}

echo "[pre-release] building CLI (dev-fast)"
cd "$ROOT_DIR/code-rs"
cargo build --locked --profile dev-fast --bin code

echo "[pre-release] running CLI smokes (skip cargo tests)"
SKIP_CARGO_TESTS=1 CI_CLI_BIN="$CARGO_TARGET_DIR/dev-fast/code" \
  bash "$ROOT_DIR/scripts/ci-tests.sh"

echo "[pre-release] running workspace tests (nextest)"
cargo nextest run --no-fail-fast --locked
