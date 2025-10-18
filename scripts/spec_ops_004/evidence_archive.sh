#!/usr/bin/env bash
#
# Evidence archive automation (MAINT-4)
#
# Compresses consensus artifacts for completed SPECs >30 days old
# per docs/spec-kit/evidence-policy.md retention policy.

set -euo pipefail

EVIDENCE_DIR="docs/SPEC-OPS-004-integrated-coder-hooks/evidence"
CONSENSUS_DIR="${EVIDENCE_DIR}/consensus"
ARCHIVE_DIR="${EVIDENCE_DIR}/archives"
DRY_RUN=0
RETENTION_DAYS=30

usage() {
  cat <<'USAGE' >&2
Usage: evidence_archive.sh [OPTIONS]

Compress consensus artifacts for completed SPECs >30 days old.

Options:
  --dry-run           Show what would be archived without making changes
  --retention-days N  Override 30-day default retention period
  --help, -h          Show this help

Policy: docs/spec-kit/evidence-policy.md section 5.2-5.3
USAGE
  exit 1
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dry-run)
      DRY_RUN=1
      shift
      ;;
    --retention-days)
      [[ $# -ge 2 ]] || usage
      RETENTION_DAYS="$2"
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

if [[ ! -d "${CONSENSUS_DIR}" ]]; then
  echo "Consensus directory not found: ${CONSENSUS_DIR}" >&2
  exit 1
fi

# Create archive directory if needed
if [[ ${DRY_RUN} -eq 0 ]]; then
  mkdir -p "${ARCHIVE_DIR}"
fi

echo "=== Evidence Archive Automation ==="
echo "Retention period: ${RETENTION_DAYS} days"
echo "Dry run: $([[ ${DRY_RUN} -eq 1 ]] && echo "YES" || echo "NO")"
echo

# Find SPECs in consensus directory
spec_count=0
archived_count=0
skipped_count=0
total_before=0
total_after=0

for spec_dir in "${CONSENSUS_DIR}"/SPEC-*; do
  [[ -d "${spec_dir}" ]] || continue

  spec_id=$(basename "${spec_dir}")
  spec_count=$((spec_count + 1))

  # Find latest synthesis file (indicator of last activity)
  latest_file=$(find "${spec_dir}" -type f -name "*_synthesis.json" -printf '%T@ %p\n' 2>/dev/null | sort -rn | head -1 | cut -d' ' -f2-)

  if [[ -z "${latest_file}" ]]; then
    echo "⚠️  ${spec_id}: No synthesis files found, skipping"
    skipped_count=$((skipped_count + 1))
    continue
  fi

  # Check file age
  file_age_days=$(( ($(date +%s) - $(stat -c %Y "${latest_file}" 2>/dev/null || stat -f %m "${latest_file}" 2>/dev/null)) / 86400 ))

  if [[ ${file_age_days} -lt ${RETENTION_DAYS} ]]; then
    echo "⏭️  ${spec_id}: ${file_age_days} days old (< ${RETENTION_DAYS}), skipping"
    skipped_count=$((skipped_count + 1))
    continue
  fi

  # Check SPEC.md for status (must be Done)
  # Simple heuristic: If SPEC has consensus but is old, likely completed or abandoned
  # Future: Parse SPEC.md for actual status

  # Calculate size before compression
  spec_size=$(du -sb "${spec_dir}" 2>/dev/null | awk '{print $1}')
  spec_size_mb=$(echo "scale=1; ${spec_size} / 1048576" | bc)
  total_before=$((total_before + spec_size))

  echo "📦 ${spec_id}: ${spec_size_mb} MB, ${file_age_days} days old"

  if [[ ${DRY_RUN} -eq 1 ]]; then
    echo "   [DRY-RUN] Would compress to ${ARCHIVE_DIR}/${spec_id}-consensus-$(date +%Y%m%d).tar.gz"
    archived_count=$((archived_count + 1))
    # Estimate 75% compression for dry-run
    compressed_size=$(echo "scale=0; ${spec_size} * 0.25 / 1" | bc)
    total_after=$((total_after + compressed_size))
  else
    # Compress consensus artifacts
    archive_file="${ARCHIVE_DIR}/${spec_id}-consensus-$(date +%Y%m%d).tar.gz"

    if tar czf "${archive_file}" -C "${CONSENSUS_DIR}" "${spec_id}" 2>/dev/null; then
      archive_size=$(du -sb "${archive_file}" | awk '{print $1}')
      archive_size_mb=$(echo "scale=1; ${archive_size} / 1048576" | bc)
      compression_ratio=$(echo "scale=1; (1 - ${archive_size} / ${spec_size}) * 100" | bc)
      total_after=$((total_after + archive_size))

      echo "   ✅ Archived to ${archive_file}"
      echo "   📊 Size: ${spec_size_mb} MB → ${archive_size_mb} MB (${compression_ratio}% compression)"

      # Calculate SHA256 checksum for integrity
      if command -v sha256sum &>/dev/null; then
        checksum=$(sha256sum "${archive_file}" | awk '{print $1}')
        echo "   🔐 SHA256: ${checksum}"
      fi

      archived_count=$((archived_count + 1))

      # Keep latest synthesis file uncompressed for quick access
      latest_synthesis=$(find "${spec_dir}" -type f -name "*_synthesis.json" -printf '%T@ %p\n' | sort -rn | head -1 | cut -d' ' -f2-)
      if [[ -n "${latest_synthesis}" ]]; then
        echo "   📄 Keeping latest synthesis: $(basename "${latest_synthesis}")"
      fi
    else
      echo "   ❌ Failed to create archive"
      skipped_count=$((skipped_count + 1))
    fi
  fi

  echo
done

# Summary
echo "=== Summary ==="
echo "SPECs scanned: ${spec_count}"
echo "Archived: ${archived_count}"
echo "Skipped: ${skipped_count}"

if [[ ${archived_count} -gt 0 ]]; then
  total_before_mb=$(echo "scale=1; ${total_before} / 1048576" | bc)
  total_after_mb=$(echo "scale=1; ${total_after} / 1048576" | bc)
  total_saved_mb=$(echo "scale=1; (${total_before} - ${total_after}) / 1048576" | bc)
  total_compression=$(echo "scale=1; (1 - ${total_after} / ${total_before}) * 100" | bc 2>/dev/null || echo "N/A")

  echo
  echo "Size before: ${total_before_mb} MB"
  echo "Size after: ${total_after_mb} MB"
  echo "Saved: ${total_saved_mb} MB (${total_compression}% compression)"
fi

if [[ ${DRY_RUN} -eq 1 ]]; then
  echo
  echo "🔍 DRY RUN - No changes made"
  echo "Run without --dry-run to execute archival"
fi
