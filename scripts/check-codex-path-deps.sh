#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
# Prefer git to locate the repository root when available, but gracefully fall
# back to the script directory so source tarballs without .git still work.
ROOT_DIR="$(cd -- "${SCRIPT_DIR}/.." >/dev/null 2>&1 && pwd)"
if command -v git >/dev/null 2>&1; then
  if git -C "$ROOT_DIR" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    ROOT_DIR="$(git -C "$ROOT_DIR" rev-parse --show-toplevel)"
  fi
fi

CODE_RS_DIR="$ROOT_DIR/code-rs"
FORBIDDEN_DIR="$ROOT_DIR/codex-rs"

if [[ ! -d "$CODE_RS_DIR" || ! -d "$FORBIDDEN_DIR" ]]; then
  echo "ERROR: Expected both code-rs/ and codex-rs/ to exist next to this script." >&2
  exit 1
fi

violations=0

echo "Scanning Cargo manifests under code-rs/ for forbidden ../codex-rs references…"
while IFS= read -r -d '' cargo_file; do
  matches=$(grep -nE '\\.\\./codex-rs|codex-[^\"]*\s*=\s*\{[^}]*path' "$cargo_file" || true)
  if [[ -z "$matches" ]]; then
    continue
  fi
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue
    echo "❌ $cargo_file:$line" >&2
    violations=1
  done <<<"$matches"
done < <(find "$CODE_RS_DIR" -name Cargo.toml -print0)

if command -v jq >/dev/null 2>&1; then
  echo "Running cargo metadata guard…"
  metadata=$(cd "$CODE_RS_DIR" && cargo metadata --format-version 1 --all-features 2>/dev/null)
  if [[ -n "$metadata" ]]; then
    offenders=$(jq -r \
      --arg forbidden "$FORBIDDEN_DIR" \
      '[.packages[]
        | select(.manifest_path | startswith($forbidden))
        | .manifest_path] | .[]' <<<"$metadata" || true)
    if [[ -n "$offenders" ]]; then
      echo "❌ cargo metadata found forbidden manifests:" >&2
      echo "$offenders" >&2
      violations=1
    fi
  fi
else
  echo "(jq not found; skipping cargo metadata check)"
fi

if [[ $violations -ne 0 ]]; then
  echo "" >&2
  echo "ERROR: Forbidden ../codex-rs path dependencies detected. Remove them before proceeding." >&2
  exit 1
fi

echo "✅ No forbidden ../codex-rs dependencies detected in code-rs/."
