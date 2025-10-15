# Evidence Baseline (October 2025)

## Snapshot

- Date: 2025-10-02 (23:36 UTC)
- Tool: `/spec-evidence-stats` (wraps `scripts/spec_ops_004/evidence_stats.sh`)
- Root size: `580K`
- Command telemetry (top entries):
  - `SPEC-KIT-018` — `272K`
  - `SPEC-KIT-DEMO` — `84K`
  - `SPEC-KIT-013` — `56K`
  - `SPEC-KIT-010` — `36K`
  - `SPEC-KIT-015` — `24K`
- Consensus artifacts: directory still empty; automation not yet implemented (manual captures required).

## Guidance

- Re-run `scripts/spec_ops_004/evidence_stats.sh [--spec <SPEC-ID>]` after large implementations to monitor repository footprint.
- Threshold proposal: revisit external storage when any single SPEC exceeds **25 MB** of committed telemetry or consensus evidence, or when cloning time becomes problematic.
- Evidence remains git-backed; no immediate change required.
