#!/usr/bin/env bash
set -euo pipefail

# Mint or register an npm granular publish token for CI and optionally push it to a GitHub Actions secret.
# Defaults target the @just-every scope and the org-level secret NPM_TOKEN.
#
# Usage examples:
#   # (preferred) create token in npm UI, then:
#   NEW_NPM_TOKEN=xxxxx UPDATE_GH_SECRET=1 scripts/refresh-npm-token.sh
#
#   # dry-run / upload only:
#   NEW_NPM_TOKEN=xxxxx UPDATE_GH_SECRET=0 scripts/refresh-npm-token.sh
#
# Environment knobs:
#   NPM_SCOPE          Scope to grant (default @just-every)
#   NPM_TOKEN_EXPIRY   Desired expiry (default 90d; capped by npm)
#   NPM_BYPASS_2FA     true|false (default true for CI)
#   GH_ORG             GitHub org for the secret (default just-every)
#   NPM_SECRET_NAME    Secret name (default NPM_TOKEN)
#   UPDATE_GH_SECRET   1 to push to GitHub; 0 to skip (default 0)
#   NEW_NPM_TOKEN      If set, skips creation and only registers/updates secret

SCOPE=${NPM_SCOPE:-@just-every}
EXPIRY=${NPM_TOKEN_EXPIRY:-90d}
BYPASS_2FA=${NPM_BYPASS_2FA:-true}
GH_ORG=${GH_ORG:-just-every}
SECRET_NAME=${NPM_SECRET_NAME:-NPM_TOKEN}
UPDATE_GH_SECRET=${UPDATE_GH_SECRET:-0}

require_tool() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "$1 is required; install it first" >&2
    exit 1
  fi
}

require_tool jq
require_tool npm

if [[ -z "${NEW_NPM_TOKEN:-}" ]]; then
  if ! npm whoami >/dev/null 2>&1; then
    echo "Run 'npm login' (with 2FA) before minting a token" >&2
    exit 1
  fi

  # Detect whether this npm supports granular token creation via CLI.
  if ! npm token create --help 2>&1 | grep -qi "granular"; then
    cat >&2 <<'EOF'
This npm CLI does not yet create granular access tokens. Use the npm UI instead:
  1) https://www.npmjs.com/settings/<your-user>/tokens
  2) Generate Token → Granular Access Token
     - Scope: @just-every (or desired scope)
     - Permissions: publish (write)
     - Bypass 2FA: enable for CI
     - Expiry: up to 90 days
  3) Copy the token, then rerun:
       NEW_NPM_TOKEN=<token> UPDATE_GH_SECRET=1 scripts/refresh-npm-token.sh
EOF
    exit 1
  fi

  flags=(--read-write --json)
  flags+=(--scope "${SCOPE}")
  flags+=(--expires "${EXPIRY}")
  if [[ "${BYPASS_2FA}" == "true" ]]; then
    flags+=(--bypass-2fa)
  fi

  echo "Creating granular publish token for scope ${SCOPE} (expires ${EXPIRY})..." >&2
  token_json=$(npm token create "${flags[@]}")
  NEW_NPM_TOKEN=$(echo "${token_json}" | jq -r '.token // empty')

  if [[ -z "${NEW_NPM_TOKEN}" ]]; then
    echo "Failed to extract token from npm response:" >&2
    echo "${token_json}" >&2
    exit 1
  fi

  echo "New npm token (copy and store securely):" >&2
  echo "${NEW_NPM_TOKEN}" >&2

  echo "Token details:" >&2
  echo "${token_json}" | jq '{id, created, expires, scopes}' >&2
fi

if [[ -z "${NEW_NPM_TOKEN:-}" ]]; then
  echo "No token available. Set NEW_NPM_TOKEN or generate via npm UI." >&2
  exit 1
fi

if [[ "${UPDATE_GH_SECRET}" == "1" ]]; then
  require_tool gh
  echo "Updating GitHub Actions secret ${SECRET_NAME} at org ${GH_ORG}..." >&2
  printf '%s' "${NEW_NPM_TOKEN}" | gh secret set "${SECRET_NAME}" -o "${GH_ORG}" --app actions
  echo "Secret updated." >&2
else
  echo "To store in GitHub Actions: printf '%s' \"${NEW_NPM_TOKEN}\" | gh secret set ${SECRET_NAME} -o ${GH_ORG} --app actions" >&2
fi

echo "Done." >&2
