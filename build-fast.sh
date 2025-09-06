#!/usr/bin/env bash
# Fast build script for local development - optimized for speed
set -euo pipefail

# Usage banner
usage() {
  cat <<USAGE
Usage: ./build-fast.sh [env flags]

Environment flags:
  PROFILE=dev-fast|dev|release-prod   Build profile (default: dev-fast)
  TRACE_BUILD=1                       Print toolchain/env and artifact SHA
  KEEP_ENV=1                          Do NOT sanitize env (use your current env)
  DETERMINISTIC=1                     Add -C debuginfo=0; promotes to release-prod unless DETERMINISTIC_FORCE_RELEASE=0
  DETERMINISTIC_FORCE_RELEASE=0|1     Keep dev-fast (0) or switch to release-prod (1, default)
  DETERMINISTIC_NO_UUID=1             macOS only: strip LC_UUID on final executables

Examples:
  ./build-fast.sh
  TRACE_BUILD=1 ./build-fast.sh
  DETERMINISTIC=1 DETERMINISTIC_FORCE_RELEASE=0 ./build-fast.sh
  DETERMINISTIC=1 DETERMINISTIC_NO_UUID=1 ./build-fast.sh
USAGE
}

case "${1:-}" in
  -h|--help) usage; exit 0 ;;
esac

# Resolve repository paths relative to this script so absolute invocation works
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"

# Change to the Rust project root directory (codex-rs) regardless of caller CWD
cd "${SCRIPT_DIR}/codex-rs"

# Compute repository root (the directory containing this script)
# Keep caches inside the workspace sandbox.
REPO_ROOT="${SCRIPT_DIR}"

# Use dev-fast profile by default for quick iteration
# Can override with: PROFILE=release ./build-fast.sh
PROFILE="${PROFILE:-dev-fast}"

# Determine the correct binary path based on profile
if [ "$PROFILE" = "dev-fast" ]; then
    BIN_PATH="./target/dev-fast/code"
elif [ "$PROFILE" = "dev" ]; then
    BIN_PATH="./target/debug/code"
else
    BIN_PATH="./target/${PROFILE}/code"
fi

# Optional deterministic mode: aim for more stable hashes by removing
# UUIDs on macOS, disabling debuginfo, and preferring a single-codegen
# optimized profile when user hasn't explicitly chosen a profile.
if [ "${DETERMINISTIC:-}" = "1" ]; then
    echo "Deterministic build: enabled"
    DET_FORCE_REL="${DETERMINISTIC_FORCE_RELEASE:-1}"
    if [ "$PROFILE" = "dev-fast" ] && [ "$DET_FORCE_REL" = "1" ]; then
        PROFILE="release-prod"
        BIN_PATH="./target/${PROFILE}/code"
        echo "Deterministic build: switching profile to ${PROFILE}"
    elif [ "$PROFILE" = "dev-fast" ]; then
        echo "Deterministic build: keeping profile ${PROFILE} (DETERMINISTIC_FORCE_RELEASE=0)"
    fi
    # Use SOURCE_DATE_EPOCH if available to stabilize timestamps
    if command -v git >/dev/null 2>&1 && git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        export SOURCE_DATE_EPOCH="$(git -C "$REPO_ROOT" log -1 --pretty=%ct 2>/dev/null || true)"
    fi
    # Disable debuginfo (safer to apply globally); avoid touching UUID here
    # since some proc-macro dylibs require LC_UUID and will fail to load.
    export RUSTFLAGS="${RUSTFLAGS:-} -C debuginfo=0"
fi

echo "Building code binary (${PROFILE} mode)..."

# Select the cargo/rustc toolchain to match deploy
# Prefer rustup with the toolchain pinned in rust-toolchain.toml or $RUSTUP_TOOLCHAIN
USE_CARGO="cargo"
if command -v rustup >/dev/null 2>&1; then
  # Determine desired toolchain
  if [ -n "${RUSTUP_TOOLCHAIN:-}" ]; then
    TOOLCHAIN="$RUSTUP_TOOLCHAIN"
  else
    # Try parse channel from rust-toolchain.toml in repo root
    if [ -f "rust-toolchain.toml" ]; then
      TOOLCHAIN=$(sed -n 's/^channel\s*=\s*"\(.*\)"/\1/p' rust-toolchain.toml | head -n1)
    fi
    # Fallback to active default if none found
    TOOLCHAIN="${TOOLCHAIN:-$(rustup show active-toolchain 2>/dev/null | awk '{print $1}')}"
  fi

  if [ -n "$TOOLCHAIN" ]; then
    # Ensure toolchain is installed; if not, attempt install quietly
    if ! rustup which rustc --toolchain "$TOOLCHAIN" >/dev/null 2>&1; then
      echo "rustup: installing toolchain $TOOLCHAIN ..."
      rustup toolchain install "$TOOLCHAIN" >/dev/null 2>&1 || true
    fi
    USE_CARGO="rustup run $TOOLCHAIN cargo"
    echo "Using rustup toolchain: $TOOLCHAIN"
    rustup run "$TOOLCHAIN" rustc --version || true
  else
    echo "rustup found but no toolchain detected; using system cargo"
  fi
else
  echo "Error: rustup is required for consistent builds."
  echo "Please install rustup: https://rustup.rs/"
  exit 1
fi

# Canonicalize build environment so everyone shares the same cache by default.
# Set KEEP_ENV=1 to skip this sanitization.
if [ "${KEEP_ENV:-}" != "1" ]; then
  # Only define RUSTFLAGS via feature flags like DETERMINISTIC=1; otherwise clear.
  if [ -z "${DETERMINISTIC:-}" ]; then
    export RUSTFLAGS=""
  fi
  unset RUSTC_WRAPPER CARGO_BUILD_RUSTC_WRAPPER SCCACHE SCCACHE_BIN CARGO_TARGET_DIR MACOSX_DEPLOYMENT_TARGET
  unset CARGO_PROFILE_RELEASE_LTO CARGO_PROFILE_DEV_FAST_LTO CARGO_PROFILE_RELEASE_CODEGEN_UNITS CARGO_PROFILE_DEV_FAST_CODEGEN_UNITS
  # Ensure incremental uses profile default
  unset CARGO_INCREMENTAL
  CANONICAL_ENV_APPLIED=1
else
  CANONICAL_ENV_APPLIED=0
fi

# Ensure Cargo writes inside the workspace when running under sandboxes that
# make $HOME read-only (e.g. CI workspace-write). Allow explicit overrides.
if [ -z "${CARGO_HOME:-}" ]; then
  export CARGO_HOME="${REPO_ROOT}/.cargo-home"
fi
if [ -z "${CARGO_TARGET_DIR:-}" ]; then
  export CARGO_TARGET_DIR="${SCRIPT_DIR}/codex-rs/target"
fi
mkdir -p "${CARGO_HOME}" "${CARGO_TARGET_DIR}" 2>/dev/null || true
# Use sparse registry for faster index updates when available
export CARGO_REGISTRIES_CRATES_IO_PROTOCOL="sparse"

# Resolve actual cargo/rustc binaries that will be used (via rustup) for fingerprinting
REAL_CARGO_BIN="$(rustup which cargo 2>/dev/null || command -v cargo || echo cargo)"
REAL_RUSTC_BIN="$(rustup which rustc 2>/dev/null || command -v rustc || echo rustc)"

# Determine current host triple for dynamic symlink naming
HOST_TRIPLE="$(rustup run "$TOOLCHAIN" rustc -vV 2>/dev/null | awk -F': ' '/^host: /{print $2}')"
TRIPLE="${HOST_TRIPLE:-}"
if [ -z "$TRIPLE" ]; then
  # Fallback for Darwin when rustup is unavailable for some reason
  if [ "$(uname -s)" = "Darwin" ]; then
    TRIPLE="$(uname -m)-apple-darwin"
    [ "$TRIPLE" = "arm64-apple-darwin" ] && TRIPLE="aarch64-apple-darwin"
  else
    TRIPLE="unknown-unknown-unknown"
  fi
fi

# Check Cargo.lock validity (fast, non-blocking check) using the selected cargo
if ! ${USE_CARGO} metadata --locked --format-version 1 >/dev/null 2>&1; then
    echo "⚠️  Warning: Cargo.lock appears out of date or inconsistent"
    echo "  This might mean:"
    echo "  • You've modified Cargo.toml dependencies"
    echo "  • You've changed workspace crate versions"
    echo "  • The lockfile is missing entries"
    echo ""
    echo "  Run 'cargo update' to update all dependencies, or"
    echo "  Run 'cargo update -p <crate-name>' to update specific crates"
    echo ""
    echo "  Continuing with unlocked build for development..."
    echo ""
    USE_LOCKED=""
else
    # Lockfile is valid, use it for consistent builds
    USE_LOCKED="--locked"
fi

# Optional trace mode for diagnosing environment differences
if [ "${TRACE_BUILD:-}" = "1" ]; then
  echo "--- TRACE_BUILD environment ---"
  echo "whoami: $(whoami)"
  echo "pwd: $(pwd)"
  echo "SHELL: ${SHELL:-}"
  bash --version | head -n1 || true
  if [ -n "${TOOLCHAIN:-}" ]; then
    echo "TOOLCHAIN: ${TOOLCHAIN}"
    rustup run "$TOOLCHAIN" rustc -vV || true
    rustup run "$TOOLCHAIN" cargo -vV || true
  fi
  echo "CANONICAL_ENV_APPLIED: ${CANONICAL_ENV_APPLIED} (KEEP_ENV=${KEEP_ENV:-})"
  echo "Filtered env (CARGO|RUST*|PROFILE|CODE_HOME|CODEX_HOME):"
  env | egrep '^(CARGO|RUST|RUSTUP|PROFILE|CODE_HOME|CODEX_HOME)=' | sort || true
  echo "--------------------------------"
fi

# Compute and compare build cache fingerprint to explain incremental behavior
FPRINT_FILE="./target/${PROFILE}/.env-fingerprint"
# Collect fingerprint inputs (only env/toolchain/settings that affect codegen/caches)
collect_fingerprint() {
  local cargo_v rustc_v host which_cargo which_rustc uname_srm
  cargo_v="$(${USE_CARGO} -V 2>/dev/null || true)"
  rustc_v="$(rustup run "$TOOLCHAIN" rustc -vV 2>/dev/null || true)"
  host="$(printf "%s\n" "$rustc_v" | awk -F': ' '/^host: /{print $2}' || true)"
  which_cargo="${REAL_CARGO_BIN}"
  which_rustc="${REAL_RUSTC_BIN}"
  uname_srm="$(uname -srm 2>/dev/null || true)"
  cat <<FP
profile=${PROFILE}
toolchain=${TOOLCHAIN:-}
host=${host}
cargo_bin=${which_cargo}
rustc_bin=${which_rustc}
cargo_version=${cargo_v}
rustc_version=$(printf "%s" "$rustc_v" | tr '\n' ' ')
uname=${uname_srm}
RUSTUP_TOOLCHAIN=${RUSTUP_TOOLCHAIN:-}
CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-}
RUSTFLAGS=${RUSTFLAGS:-}
RUSTC_WRAPPER=${RUSTC_WRAPPER:-}
CARGO_BUILD_RUSTC_WRAPPER=${CARGO_BUILD_RUSTC_WRAPPER:-}
SCCACHE=${SCCACHE:-}
SCCACHE_BIN=${SCCACHE_BIN:-}
CARGO_INCREMENTAL=${CARGO_INCREMENTAL:-}
MACOSX_DEPLOYMENT_TARGET=${MACOSX_DEPLOYMENT_TARGET:-}
CODE_HOME=${CODE_HOME:-}
CODEX_HOME=${CODEX_HOME:-}
FP
}

NEW_FPRINT_TEXT="$(collect_fingerprint)"
NEW_FPRINT_HASH="$(printf "%s" "$NEW_FPRINT_TEXT" | shasum -a 256 2>/dev/null | awk '{print $1}')"

FPRINT_CHANGED="0"
if [ -f "$FPRINT_FILE" ]; then
  OLD_FPRINT_HASH="$(sed -n 's/^HASH=//p' "$FPRINT_FILE" 2>/dev/null | head -n1)"
  if [ "${OLD_FPRINT_HASH:-}" != "$NEW_FPRINT_HASH" ]; then
    FPRINT_CHANGED="1"
    echo "⚠️  Build cache fingerprint changed since last run for profile '${PROFILE}'."
    echo "   This can trigger incremental rebuilds the first time you build."
    if [ "${TRACE_BUILD:-}" = "1" ]; then
      echo "--- previous fingerprint (hash: ${OLD_FPRINT_HASH:-none}) ---"; sed -n '1,200p' "$FPRINT_FILE" 2>/dev/null | sed '1d'; echo "--------------------------------"
      echo "--- current fingerprint (hash: ${NEW_FPRINT_HASH}) ---"; printf "%s\n" "$NEW_FPRINT_TEXT"; echo "--------------------------------"
    else
      echo "   Run with TRACE_BUILD=1 to see detailed differences."
    fi
  fi
fi

# Build for native target (no --target flag) for maximum speed
# This reuses the host stdlib and normal cache

# Build with or without --locked based on lockfile validity
# Keep stderr and stdout separate so downstream tools can capture both streams.
${USE_CARGO} build ${USE_LOCKED} --profile "${PROFILE}" --bin code --bin code-tui --bin code-exec

# Check if build succeeded
if [ $? -eq 0 ]; then
    echo "✅ Build successful!"
    echo "Binary location: ./codex-rs/target/${PROFILE}/code"
    echo ""
    
    # Keep old symlink locations working for compatibility
    # Create symlink in target/release for npm wrapper expectations
    mkdir -p ./target/release
    if [ -e "./target/release/code" ]; then
        rm -f ./target/release/code
    fi
    ln -sf "../${PROFILE}/code" "./target/release/code"
    
    # Update the symlinks in codex-cli/bin
    CLI_BIN_DIR="../codex-cli/bin"
    mkdir -p "$CLI_BIN_DIR"
    # Dynamic arch-targeted names
    for LINK in "code-${TRIPLE}" "coder-${TRIPLE}"; do
      DEST="${CLI_BIN_DIR}/${LINK}"
      [ -e "$DEST" ] && rm -f "$DEST"
      ln -sf "../../codex-rs/target/${PROFILE}/code" "$DEST"
    done
    # Back-compat fixed names (Apple Silicon triple)
    for LINK in code-aarch64-apple-darwin coder-aarch64-apple-darwin; do
      DEST="${CLI_BIN_DIR}/${LINK}"
      [ -e "$DEST" ] && rm -f "$DEST"
      ln -sf "../../codex-rs/target/${PROFILE}/code" "$DEST"
    done
    
    # Optional post-link step for deterministic builds: re-link executables
    # with -no_uuid only on macOS. Apply per-bin via `cargo rustc` so
    # dependencies/proc-macro dylibs are not affected.
    if [ "${DETERMINISTIC_NO_UUID:-}" = "1" ] && [ "$(uname -s)" = "Darwin" ]; then
      echo "Deterministic post-link: removing LC_UUID from executables"
      ${USE_CARGO} rustc ${USE_LOCKED} --profile "${PROFILE}" -p codex-cli --bin code -- -C link-arg=-Wl,-no_uuid || true
      ${USE_CARGO} rustc ${USE_LOCKED} --profile "${PROFILE}" -p codex-tui --bin code-tui -- -C link-arg=-Wl,-no_uuid || true
      ${USE_CARGO} rustc ${USE_LOCKED} --profile "${PROFILE}" -p codex-exec --bin code-exec -- -C link-arg=-Wl,-no_uuid || true
    fi

    # Compute absolute path and SHA256 for clarity (after any post-linking)
    ABS_BIN_PATH="$(cd "$(dirname "${BIN_PATH}")" && pwd)/$(basename "${BIN_PATH}")"
    if command -v shasum >/dev/null 2>&1; then
      BIN_SHA="$(shasum -a 256 "${ABS_BIN_PATH}" | awk '{print $1}')"
    elif command -v sha256sum >/dev/null 2>&1; then
      BIN_SHA="$(sha256sum "${ABS_BIN_PATH}" | awk '{print $1}')"
    else
      BIN_SHA=""
    fi

    # Ensure repo-local 'code-dev' path stays mapped to latest build output
    # so the user's alias `code-dev` (if pointing at target/dev-fast/code) keeps working
    # Only create this symlink if we're not already building in dev-fast profile
    if [ "$PROFILE" != "dev-fast" ]; then
      mkdir -p ./target/dev-fast
      if [ -e "./target/dev-fast/code" ]; then
        rm -f ./target/dev-fast/code
      fi
      ln -sf "../${PROFILE}/code" "./target/dev-fast/code"
    fi

    if [ -n "$BIN_SHA" ]; then
      echo "Binary Hash: ${BIN_SHA} ($(du -sh "${ABS_BIN_PATH}" | awk '{print $1}'))"
    else
      echo "Binary Size: $(du -h "${ABS_BIN_PATH}" | cut -f1)"
    fi
    
    if [ "${TRACE_BUILD:-}" = "1" ] && [ -n "$BIN_SHA" ]; then
      echo "--- TRACE_BUILD artifact ---"
      echo "ABS_BIN_PATH: ${ABS_BIN_PATH}"
      echo "SHA256: ${BIN_SHA}"
      echo "--------------------------------"
    fi

    # Persist the current fingerprint to explain future behavior
    mkdir -p "./target/${PROFILE}" 2>/dev/null || true
    {
      echo "HASH=${NEW_FPRINT_HASH}"
      printf "%s\n" "$NEW_FPRINT_TEXT"
    } >"$FPRINT_FILE"
    if [ "$FPRINT_CHANGED" = "1" ]; then
      echo "🧰 Cache normalized to current environment (fingerprint ${NEW_FPRINT_HASH})."
    fi

    # If lockfile was out of date, remind user
    if [ -z "$USE_LOCKED" ]; then
        echo ""
        echo "⚠️  Remember: Built without --locked due to Cargo.lock issues"
        echo "  Consider running 'cargo update' and committing the changes"
    fi
else
    echo "❌ Build failed"
    exit 1
fi
