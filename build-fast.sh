#!/usr/bin/env bash
# Fast build script for local development - optimized for speed
set -euo pipefail

# Usage banner
usage() {
  cat <<USAGE
Usage: ./build-fast.sh [env flags]

Environment flags:
  PROFILE=dev-fast|dev|release|perf   Build profile (default: dev-fast)
  TRACE_BUILD=1                       Print toolchain/env and artifact SHA
  KEEP_ENV=0                          Sanitize env for reproducible builds (default skips)
  DETERMINISTIC=1                     Add -C debuginfo=0; promotes to release unless DETERMINISTIC_FORCE_RELEASE=0
  DETERMINISTIC_FORCE_RELEASE=0|1     Keep dev-fast (0) or switch to release (1, default)
  DETERMINISTIC_NO_UUID=1             macOS only: strip LC_UUID on final executables

Examples:
  ./build-fast.sh
  TRACE_BUILD=1 ./build-fast.sh
  DETERMINISTIC=1 DETERMINISTIC_FORCE_RELEASE=0 ./build-fast.sh
  DETERMINISTIC=1 DETERMINISTIC_NO_UUID=1 ./build-fast.sh
  ./build-fast.sh run
  ./build-fast.sh perf
  ./build-fast.sh perf run
USAGE
}

resolve_bin_path() {
  case "$PROFILE" in
    dev-fast)
      BIN_SUBDIR="dev-fast"
      ;;
    dev)
      BIN_SUBDIR="debug"
      ;;
    *)
      BIN_SUBDIR="$PROFILE"
      ;;
  esac

  local target_root
  if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    target_root="${CARGO_TARGET_DIR}"
  else
    target_root="${REPO_ROOT}/codex-rs/target"
  fi

  if [[ "${target_root}" != /* ]]; then
    target_root="$(cd "${target_root}" >/dev/null 2>&1 && pwd)"
  fi

  TARGET_DIR_ABS="${target_root}"
  BIN_CARGO_FILENAME="code"
  BIN_FILENAME="code"
  if [ "$PROFILE" = "perf" ]; then
    BIN_FILENAME="code-perf"
  fi
  BIN_SUBPATH="${BIN_SUBDIR}/${BIN_FILENAME}"
  BIN_CARGO_SUBPATH="${BIN_SUBDIR}/${BIN_CARGO_FILENAME}"
  BIN_PATH="${TARGET_DIR_ABS}/${BIN_SUBPATH}"
  BIN_CARGO_PATH="${TARGET_DIR_ABS}/${BIN_CARGO_SUBPATH}"
  BIN_LINK_PATH="./target/${BIN_SUBPATH}"

  if [ -n "${REPO_TARGET_ABS:-}" ] && [ "${TARGET_DIR_ABS}" = "${REPO_TARGET_ABS}" ]; then
    BIN_DISPLAY_PATH="./codex-rs/target/${BIN_SUBPATH}"
  else
    BIN_DISPLAY_PATH="${BIN_PATH}"
  fi
}

case "${1:-}" in
  -h|--help) usage; exit 0 ;;
esac

RUN_AFTER_BUILD=0
ARG_PROFILE=""
while [ $# -gt 0 ]; do
  case "$1" in
    run)
      RUN_AFTER_BUILD=1
      ;;
    *)
      if [ -n "$ARG_PROFILE" ]; then
        echo "Error: Multiple profile arguments provided ('${ARG_PROFILE}' and '$1')." >&2
        usage
        exit 1
      fi
      ARG_PROFILE="$1"
      ;;
  esac
  shift
done

if [ "$ARG_PROFILE" = "pref" ]; then
  ARG_PROFILE="perf"
fi

# Resolve repository paths relative to this script so absolute invocation works
SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" >/dev/null 2>&1 && pwd)"
CALLER_CWD="$(pwd)"

# Change to the Rust project root directory (codex-rs) regardless of caller CWD
cd "${SCRIPT_DIR}/codex-rs"

# Compute repository root (the directory containing this script)
# Note: We intentionally set REPO_ROOT to SCRIPT_DIR so any defaults (like CARGO_HOME)
# resolve inside the repository, not its parent. This prevents permission issues on CI
# where the parent folder may be owned by a different user.
REPO_ROOT="${SCRIPT_DIR}"

# Default to preserving caller environment unless explicitly disabled
KEEP_ENV="${KEEP_ENV:-1}"

# Track whether the caller explicitly set PROFILE (env or CLI)
PROFILE_ENV_SUPPLIED=0
if [ -n "${PROFILE+x}" ]; then
  PROFILE_ENV_SUPPLIED=1
  PROFILE_VALUE="$PROFILE"
else
  PROFILE_VALUE="dev-fast"
fi

if [ -n "$ARG_PROFILE" ]; then
  PROFILE_VALUE="$ARG_PROFILE"
fi

PROFILE_EXPLICIT=0
if [ "$PROFILE_ENV_SUPPLIED" -eq 1 ] || [ -n "$ARG_PROFILE" ]; then
  PROFILE_EXPLICIT=1
fi

PROFILE="$PROFILE_VALUE"

# Optional deterministic mode: aim for more stable hashes by removing
# UUIDs on macOS, disabling debuginfo, and preferring a single-codegen
# optimized profile when user hasn't explicitly chosen a profile.
if [ "${DETERMINISTIC:-}" = "1" ]; then
    echo "Deterministic build: enabled"
    DET_FORCE_REL="${DETERMINISTIC_FORCE_RELEASE:-1}"
    if [ "$PROFILE" = "dev-fast" ] && [ "$DET_FORCE_REL" = "1" ]; then
        PROFILE="release"  # MAINT-6: Use release (release-prod removed)
        echo "Deterministic build: switching profile to ${PROFILE}"
    elif [ "$PROFILE" = "dev-fast" ]; then
        echo "Deterministic build: keeping profile ${PROFILE} (DETERMINISTIC_FORCE_RELEASE=0)"
    fi
    # Use SOURCE_DATE_EPOCH if available to stabilize timestamps
    if command -v git >/dev/null 2>&1 && git -C "$REPO_ROOT" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
        export SOURCE_DATE_EPOCH="$(git -C "$REPO_ROOT" log -1 --pretty=%ct 2>/dev/null || true)"
    fi
    # Keep debuginfo intact so profiling tools can resolve symbols.
fi

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

# Canonicalize build environment only when requested.
# Set KEEP_ENV=0 to force sanitization.
if [ "${KEEP_ENV}" != "1" ]; then
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

# Optional debug symbol override for profiling sessions
if [ "${DEBUG_SYMBOLS:-}" = "1" ]; then
  if [ "$PROFILE" = "perf" ]; then
    echo "Debug symbols: profile 'perf' already preserves debuginfo"
  elif [ "$PROFILE_EXPLICIT" -eq 0 ] && [ "$PROFILE" = "dev-fast" ]; then
    echo "Debug symbols requested: switching profile to perf"
    PROFILE="perf"
  else
    PROFILE_ENV_KEY="$(printf "%s" "$PROFILE" | tr '[:lower:]-' '[:upper:]_')"
    DEBUG_VAR="CARGO_PROFILE_${PROFILE_ENV_KEY}_DEBUG"
    STRIP_VAR="CARGO_PROFILE_${PROFILE_ENV_KEY}_STRIP"
    SPLIT_VAR="CARGO_PROFILE_${PROFILE_ENV_KEY}_SPLIT_DEBUGINFO"
    printf -v "$DEBUG_VAR" '%s' '2'
    printf -v "$STRIP_VAR" '%s' 'none'
    printf -v "$SPLIT_VAR" '%s' 'packed'
    export "$DEBUG_VAR" "$STRIP_VAR" "$SPLIT_VAR"
    echo "Debug symbols: forcing debuginfo for profile ${PROFILE}"
  fi

  if [ -n "${RUSTFLAGS:-}" ]; then
    CLEAN_RUSTFLAGS="${RUSTFLAGS//-C debuginfo=0/}"
    CLEAN_RUSTFLAGS="${CLEAN_RUSTFLAGS//  / }"
    CLEAN_RUSTFLAGS="${CLEAN_RUSTFLAGS## }"
    CLEAN_RUSTFLAGS="${CLEAN_RUSTFLAGS%% }"
    export RUSTFLAGS="${CLEAN_RUSTFLAGS}"
  fi

  export CARGO_PROFILE_RELEASE_STRIP="none"
  # MAINT-6: CARGO_PROFILE_RELEASE_PROD_STRIP removed (profile deleted)
fi

echo "Building code binary (${PROFILE} mode)..."

# Ensure Cargo cache locations are stable.
# In CI, we can optionally enforce a specific CARGO_HOME regardless of caller env
# by setting STRICT_CARGO_HOME=1 (used by Issue Triage workflow to keep caching deterministic).
# Cargo/rustup home directories
# When STRICT_CARGO_HOME=1, honor CARGO_HOME_ENFORCED if provided; otherwise
# force a per-repo location to avoid HOME permission/caching issues on CI.
if [ "${STRICT_CARGO_HOME:-}" = "1" ]; then
  export CARGO_HOME="${CARGO_HOME_ENFORCED:-${REPO_ROOT}/.cargo-home}"
else
  # Default: write inside workspace if not set to avoid HOME permission issues
  if [ -z "${CARGO_HOME:-}" ]; then
    export CARGO_HOME="${REPO_ROOT}/.cargo-home"
  fi
fi
# Keep rustup’s home alongside Cargo by default when not explicitly set
if [ -z "${RUSTUP_HOME:-}" ]; then
  export RUSTUP_HOME="${CARGO_HOME%/}/rustup"
fi
if [ -z "${CARGO_TARGET_DIR:-}" ]; then
  export CARGO_TARGET_DIR="${SCRIPT_DIR}/codex-rs/target"
fi
mkdir -p "${CARGO_HOME}" "${CARGO_TARGET_DIR}" 2>/dev/null || true
# Ensure repo-local target directory exists for compatibility symlinks
mkdir -p ./target
REPO_TARGET_ABS="$(cd ./target >/dev/null 2>&1 && pwd)"
resolve_bin_path
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
if ! CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" ${USE_CARGO} metadata --locked --format-version 1 >/dev/null 2>&1; then
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
  echo "CANONICAL_ENV_APPLIED: ${CANONICAL_ENV_APPLIED} (KEEP_ENV=${KEEP_ENV})"
  echo "Filtered env (CARGO|RUST*|PROFILE|CODE_HOME|CODEX_HOME):"
  env | egrep '^(CARGO|RUST|RUSTUP|PROFILE|CODE_HOME|CODEX_HOME)=' | sort || true
  echo "--------------------------------"
fi

# Compute and compare build cache fingerprint to explain incremental behavior
FPRINT_FILE="./target/${PROFILE}/.env-fingerprint"
# Collect fingerprint inputs (only env/toolchain/settings that affect codegen/caches)
collect_fingerprint() {
  local cargo_v rustc_v host which_cargo which_rustc uname_srm
  cargo_v="$(CARGO_HOME="$CARGO_HOME" RUSTUP_HOME="$RUSTUP_HOME" ${USE_CARGO} -V 2>/dev/null || true)"
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

# Determine exec binary name based on workspace (support forks)
EXEC_BIN="codex-exec"
# Detect legacy bin name used in some forks
if grep -q '^name\s*=\s*"code-exec"' ./exec/Cargo.toml 2>/dev/null; then
  EXEC_BIN="code-exec"
fi

# Build with or without --locked based on lockfile validity
# Keep stderr and stdout separate so downstream tools can capture both streams.
echo "Using exec bin: ${EXEC_BIN}"
${USE_CARGO} build ${USE_LOCKED} --profile "${PROFILE}" --bin code --bin code-tui --bin "${EXEC_BIN}"

# Check if build succeeded
if [ $? -eq 0 ]; then
    resolve_bin_path

    if [ "$PROFILE" = "perf" ]; then
      PERF_SOURCE="${BIN_CARGO_PATH}"
      PERF_TARGET="${BIN_PATH}"
      if [ -e "${PERF_SOURCE}" ]; then
        PERF_DIR="$(dirname "${PERF_TARGET}")"
        mkdir -p "${PERF_DIR}"
        if [ -e "${PERF_TARGET}" ] || [ -L "${PERF_TARGET}" ]; then
          rm -f "${PERF_TARGET}"
        fi
        (
          cd "${PERF_DIR}" >/dev/null 2>&1
          ln -sf "$(basename "${PERF_SOURCE}")" "$(basename "${PERF_TARGET}")"
        )
      fi
    fi

    echo "✅ Build successful!"
    echo "Binary location: ${BIN_DISPLAY_PATH}"
    echo ""

    # Keep old symlink locations working for compatibility
    # Create symlink in target/release for npm wrapper expectations
    release_link_target="../${BIN_SUBDIR}/${BIN_FILENAME}"
    cli_link_target="../../codex-rs/target/${BIN_SUBDIR}/${BIN_FILENAME}"
    dev_fast_link_target="../${BIN_SUBDIR}/${BIN_FILENAME}"

    if [ "${TARGET_DIR_ABS}" != "${REPO_TARGET_ABS}" ]; then
      release_link_target="${BIN_PATH}"
      cli_link_target="${BIN_PATH}"
      dev_fast_link_target="${BIN_PATH}"

      # Maintain repo-local path for downstream tooling when target dir is external
      if [ -n "${BIN_LINK_PATH:-}" ]; then
        mkdir -p "$(dirname "${BIN_LINK_PATH}")"
        if [ -e "${BIN_LINK_PATH}" ]; then
          rm -f "${BIN_LINK_PATH}"
        fi
        ln -sf "${BIN_PATH}" "${BIN_LINK_PATH}"
      fi
    fi

    mkdir -p ./target/release
    if [ -e "./target/release/code" ]; then
        rm -f ./target/release/code
    fi
    ln -sf "${release_link_target}" "./target/release/code"

    # Update the symlinks in codex-cli/bin
    CLI_BIN_DIR="../codex-cli/bin"
    mkdir -p "$CLI_BIN_DIR"
    # Dynamic arch-targeted names
    for LINK in "code-${TRIPLE}" "coder-${TRIPLE}"; do
      DEST="${CLI_BIN_DIR}/${LINK}"
      [ -e "$DEST" ] && rm -f "$DEST"
      ln -sf "${cli_link_target}" "$DEST"
    done
    # Back-compat fixed names (Apple Silicon triple)
    for LINK in code-aarch64-apple-darwin coder-aarch64-apple-darwin; do
      DEST="${CLI_BIN_DIR}/${LINK}"
      [ -e "$DEST" ] && rm -f "$DEST"
      ln -sf "${cli_link_target}" "$DEST"
    done

    # Ensure repo-local 'code-dev' path stays mapped to latest build output
    # so the user's alias `code-dev` (if pointing at target/dev-fast/code) keeps working
    # Only create this symlink if we're not already building in dev-fast profile
    if [ "$PROFILE" != "dev-fast" ]; then
      mkdir -p ./target/dev-fast
      if [ -e "./target/dev-fast/code" ]; then
        rm -f ./target/dev-fast/code
      fi
      ln -sf "${dev_fast_link_target}" "./target/dev-fast/code"
    fi

    # Optional post-link step for deterministic builds: re-link executables
    # with -no_uuid only on macOS. Apply per-bin via `cargo rustc` so
    # dependencies/proc-macro dylibs are not affected.
    if [ "${DETERMINISTIC_NO_UUID:-}" = "1" ] && [ "$(uname -s)" = "Darwin" ]; then
      echo "Deterministic post-link: removing LC_UUID from executables"
      ${USE_CARGO} rustc ${USE_LOCKED} --profile "${PROFILE}" -p codex-cli --bin code -- -C link-arg=-Wl,-no_uuid || true
      ${USE_CARGO} rustc ${USE_LOCKED} --profile "${PROFILE}" -p codex-tui --bin code-tui -- -C link-arg=-Wl,-no_uuid || true
      if [ "$EXEC_BIN" = "codex-exec" ]; then
        ${USE_CARGO} rustc ${USE_LOCKED} --profile "${PROFILE}" -p codex-exec --bin codex-exec -- -C link-arg=-Wl,-no_uuid || true
      else
        ${USE_CARGO} rustc ${USE_LOCKED} --profile "${PROFILE}" -p codex-exec --bin code-exec -- -C link-arg=-Wl,-no_uuid || true
      fi
    fi

    # Compute absolute path and SHA256 for clarity (after any post-linking)
    ABS_BIN_PATH="${BIN_PATH}"
    if [[ "${ABS_BIN_PATH}" != /* ]]; then
      ABS_BIN_PATH="$(cd "$(dirname "${ABS_BIN_PATH}")" >/dev/null 2>&1 && pwd)/$(basename "${ABS_BIN_PATH}")"
    fi

    BIN_SHA=""
    if [ -e "${ABS_BIN_PATH}" ]; then
      if command -v shasum >/dev/null 2>&1; then
        BIN_SHA="$(shasum -a 256 "${ABS_BIN_PATH}" | awk '{print $1}')"
      elif command -v sha256sum >/dev/null 2>&1; then
        BIN_SHA="$(sha256sum "${ABS_BIN_PATH}" | awk '{print $1}')"
      fi
    fi

    if [ -n "$BIN_SHA" ]; then
      echo "Binary Hash: ${BIN_SHA} ($(du -sh "${ABS_BIN_PATH}" | awk '{print $1}'))"
    elif [ -e "${ABS_BIN_PATH}" ]; then
      echo "Binary Size: $(du -h "${ABS_BIN_PATH}" | cut -f1)"
    else
      echo "Binary artifact not found at ${ABS_BIN_PATH}"
    fi

    if [ "$RUN_AFTER_BUILD" -eq 1 ]; then
      if [ ! -x "${ABS_BIN_PATH}" ]; then
        echo "❌ Run failed: ${ABS_BIN_PATH} is missing or not executable"
        exit 1
      fi
      echo "Running ${ABS_BIN_PATH} (cwd: ${CALLER_CWD})..."
      (
        cd "${CALLER_CWD}" && "${ABS_BIN_PATH}"
      )
      RUN_STATUS=$?
      if [ $RUN_STATUS -ne 0 ]; then
        echo "❌ Run failed with status ${RUN_STATUS}"
        exit $RUN_STATUS
      fi
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
