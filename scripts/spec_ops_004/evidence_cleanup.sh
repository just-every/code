#!/usr/bin/env bash
#
# Evidence cleanup automation (MAINT-4)
#
# Offloads archived SPECs >90 days old to external storage and optionally
# purges SPECs >180 days per docs/spec-kit/evidence-policy.md.

set -euo pipefail

EVIDENCE_DIR="docs/SPEC-OPS-004-integrated-coder-hooks/evidence"
ARCHIVE_DIR="${EVIDENCE_DIR}/archives"
OFFLOAD_DIR="${EVIDENCE_OFFLOAD_DIR:-}"
DRY_RUN=0
OFFLOAD_DAYS=90
PURGE_DAYS=180
ENABLE_PURGE=0

usage() {
  cat <<'USAGE' >&2
Usage: evidence_cleanup.sh [OPTIONS]

Offload/purge old archived evidence per retention policy.

Options:
  --dry-run           Show what would be cleaned without making changes
  --offload-days N    Override 90-day default for offload (default: 90)
  --purge-days N      Override 180-day default for purge (default: 180)
  --enable-purge      Enable purging (disabled by default for safety)
  --offload-dir PATH  External storage path (or set EVIDENCE_OFFLOAD_DIR env var)
  --help, -h          Show this help

Environment:
  EVIDENCE_OFFLOAD_DIR  Default external storage location

Policy: docs/spec-kit/evidence-policy.md section 4.2, 5.3
USAGE
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --offload-days)
      [[ $# -ge 2 ]] || usage
      OFFLOAD_DAYS="$2"
      shift 2
      ;;
    --purge-days)
      [[ $# -ge 2 ]] || usage
      PURGE_DAYS="$2"
      shift 2
      ;;
    --enable-purge)
      ENABLE_PURGE=1
      shift
      ;;
    --offload-dir)
      [[ $# -ge 2 ]] || usage
      OFFLOAD_DIR="$2"
      shift 2
      ;;
    --help|-h)
      usage
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage
      ;;
  esac
done

if [[ ! -d "${ARCHIVE_DIR}" ]]; then
  echo "Archive directory not found: ${ARCHIVE_DIR}" >&2
  echo "Run evidence_archive.sh first to create archives" >&2
  exit 1
fi

echo "=== Evidence Cleanup Automation ==="
echo "Offload threshold: ${OFFLOAD_DAYS} days"
echo "Purge threshold: ${PURGE_DAYS} days (enabled: $([[ ${ENABLE_PURGE} -eq 1 ]] && echo "YES" || echo "NO"))"
echo "Dry run: $([[ ${DRY_RUN} -eq 1 ]] && echo "YES" || echo "NO")"
echo

# Offload aged archives
if [[ -n "${OFFLOAD_DIR}" ]]; then
  echo "Offload directory: ${OFFLOAD_DIR}"

  if [[ ${DRY_RUN} -eq 0 ]]; then
    mkdir -p "${OFFLOAD_DIR}"
  fi

  offloaded=0
  for archive in "${ARCHIVE_DIR}"/*.tar.gz; do
    [[ -f "${archive}" ]] || continue

    archive_age_days=$(( ($(date +%s) - $(stat -c %Y "${archive}" 2>/dev/null || stat -f %m "${archive}" 2>/dev/null)) / 86400 ))

    if [[ ${archive_age_days} -ge ${OFFLOAD_DAYS} ]]; then
      archive_name=$(basename "${archive}")
      archive_size=$(du -sh "${archive}" | awk '{print $1}')

      echo "📤 Offloading ${archive_name} (${archive_size}, ${archive_age_days} days old)"

      if [[ ${DRY_RUN} -eq 1 ]]; then
        echo "   [DRY-RUN] Would move to ${OFFLOAD_DIR}/${archive_name}"
      else
        if mv "${archive}" "${OFFLOAD_DIR}/${archive_name}"; then
          echo "   ✅ Offloaded to ${OFFLOAD_DIR}/${archive_name}"

          # Create metadata file
          metadata="${OFFLOAD_DIR}/${archive_name}.meta"
          {
            echo "archive: ${archive_name}"
            echo "offloaded: $(date -Iseconds)"
            echo "age_days: ${archive_age_days}"
            echo "size: ${archive_size}"
            if command -v sha256sum &>/dev/null; then
              echo "sha256: $(sha256sum "${OFFLOAD_DIR}/${archive_name}" | awk '{print $1}')"
            fi
          } > "${metadata}"

          offloaded=$((offloaded + 1))
        else
          echo "   ❌ Failed to offload"
        fi
      fi
    fi
  done

  echo
  echo "Archives offloaded: ${offloaded}"
else
  echo "⚠️  No offload directory configured (set EVIDENCE_OFFLOAD_DIR or use --offload-dir)"
  echo "Archives will remain in ${ARCHIVE_DIR}"
fi

# Purge very old archives (if enabled)
if [[ ${ENABLE_PURGE} -eq 1 ]]; then
  echo
  echo "=== Purge Phase (${PURGE_DAYS}+ days) ==="

  purged=0
  for archive in "${ARCHIVE_DIR}"/*.tar.gz "${OFFLOAD_DIR}"/*.tar.gz 2>/dev/null; do
    [[ -f "${archive}" ]] || continue

    archive_age_days=$(( ($(date +%s) - $(stat -c %Y "${archive}" 2>/dev/null || stat -f %m "${archive}" 2>/dev/null)) / 86400 ))

    if [[ ${archive_age_days} -ge ${PURGE_DAYS} ]]; then
      archive_name=$(basename "${archive}")

      echo "🗑️  Purging ${archive_name} (${archive_age_days} days old)"

      if [[ ${DRY_RUN} -eq 1 ]]; then
        echo "   [DRY-RUN] Would delete ${archive}"
      else
        if rm "${archive}"; then
          echo "   ✅ Deleted"
          purged=$((purged + 1))

          # Delete metadata if exists
          [[ -f "${archive}.meta" ]] && rm "${archive}.meta"
        else
          echo "   ❌ Failed to delete"
        fi
      fi
    fi
  done

  echo "Archives purged: ${purged}"
else
  echo
  echo "⚠️  Purge disabled (use --enable-purge to delete archives >${PURGE_DAYS} days)"
fi

echo
if [[ ${DRY_RUN} -eq 1 ]]; then
  echo "🔍 DRY RUN - No changes made"
  echo "Run without --dry-run to execute cleanup"
fi
