#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
REPO_ROOT="${SCRIPT_DIR}/.."

notes_file="${REPO_ROOT}/docs/release-notes/RELEASE_NOTES.md"
pkg_json="${REPO_ROOT}/codex-cli/package.json"

if [ ! -f "$notes_file" ]; then
  echo "release notes file missing: $notes_file" >&2
  exit 1
fi

if [ ! -f "$pkg_json" ]; then
  echo "package.json missing: $pkg_json" >&2
  exit 1
fi

package_version=$(jq -r '.version // empty' "$pkg_json")

if [ -z "$package_version" ]; then
  echo "Failed to read version from $pkg_json" >&2
  exit 1
fi

expected_header="## @just-every/code v${package_version}"
actual_header=$(grep -m1 '^## @just-every/code v' "$notes_file" || true)

if [ "$actual_header" != "$expected_header" ]; then
  echo "release notes header mismatch" >&2
  echo "  expected: $expected_header" >&2
  echo "  actual:   ${actual_header:-<none>}" >&2
  exit 1
fi

exit 0
