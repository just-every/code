# Migration Runbook (T10)

## Prerequisites
- Cached Byterover export JSON generated via MCP or retrieved from previous runs.
- Local-memory SQLite database path (`$LOCAL_MEMORY_HOME/unified-memories.db`) or a staging copy for rehearsal.
- `LOCAL_MEMORY_BIN` override when the default `local-memory` binary is not on `PATH`.

## Baseline Audit
1. Run `scripts/spec-kit/local_memory_baseline.py --out docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-010/baseline_local_memory_<timestamp>.md`.
2. Inspect domain counts and tag coverage; confirm spec-related domains (spec-tracker, docs-ops, impl-notes, infra-ci, governance) are present.

## Dry-run Procedure
1. Execute `scripts/spec-kit/migrate_local_memory.py --source <export.json> --dry-run --out-json <dry_run.json>`.
2. Review the generated summary; confirm `mode` is `dry-run` and totals match the export.
3. Store the JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-010/` (example: `migration_dry_run_20250928T1800Z.json`).

## Apply Procedure (Staging Copy Recommended)
1. Copy production `unified-memories.db` to a safe location or use a staging database.
2. Run `scripts/spec-kit/migrate_local_memory.py --source <export.json> --apply --database <db_path> --out-json <apply.json>`.
3. Inspect the `results` array for `inserted`, `skipped_existing_id`, and `skipped_existing_slug`; investigate unexpected statuses before applying to prod.
4. Archive the JSON output and updated database snapshot under the SPEC-KIT-010 evidence directory (sample files committed on 2025-09-28).

## Post-migration Verification
- Launch `/spec-plan` and `/spec-tasks`; confirm the local-memory context block appears without Byterover round-trips.
- Run `cargo test -p codex-tui spec_auto` to ensure CLI/TUI fallbacks remain covered.
- Record operator notes or anomalies in `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-010/`.

## Evidence References (2025-09-28)
- `baseline_local_memory_20250927T231710Z.{json,md}` (baseline snapshot)
- `migration_dry_run_20250928T1800Z.json`
- `migration_apply_20250928T1800Z.json`
- `sample_local_memory.db`
