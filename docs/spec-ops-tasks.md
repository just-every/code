# /spec-ops-tasks Guardrail

## Purpose
- Seed Spec Kit task automation by collecting `tool.status` telemetry before multi-agent planning.
- Lock in housekeeping (hooks, lint scaffolding) so `/spec-tasks` can write `docs/SPEC-*/tasks.md` without conflicts.
- capture evidence JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/` for downstream validation.

## Required Environment
- `SPEC_OPS_CARGO_MANIFEST` if the Rust workspace root is not `codex-rs/Cargo.toml`.
- `SPEC_OPS_ALLOW_DIRTY=1` (temporary) when iterating in a branch with staged changes; unset once scripts pass clean-tree checks.

## Telemetry Envelope (schema v1)
- Common fields: `command`, `specId`, `sessionId`, `timestamp`, `schemaVersion`, `artifacts[]`.
- Stage payload: object `tool.status` (`ok|failed|skipped|ready`).
- Optional `hal.summary` attaches when `SPEC_OPS_TELEMETRY_HAL=1`; status must be `passed|failed|skipped`.

## Execution Flow
1. Run from the template repo root: `scripts/spec_ops_004/commands/spec_ops_tasks.sh <SPEC-ID>`.
2. Inspect log + telemetry paths announced in stdout.
3. Validate schema via `python3 scripts/spec_ops_004/validate_schema.py` (see SPEC-KIT-013) before continuing to `/spec-tasks`.

## HAL Integration
- Enable `SPEC_OPS_TELEMETRY_HAL=1` whenever `/spec-ops-validate` or `/spec-ops-audit` will be run later in the flow so telemetry remains consistent.
- Healthy/degraded HAL captures should live alongside the tasks run for auditing (`20250929-145435Z-hal-*`, `20250929-123303Z-hal-*`).

## Evidence Examples
- Healthy run: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/spec-validate_2025-09-29T14:54:35Z-3088619300.json` (references `hal.summary` status `passed`).
- Degraded run: `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/spec-validate_2025-09-29T12:33:03Z-3193628696.json` (contains failed checks and matching artifacts).

## Troubleshooting
- Missing `tool.status` → rerun `/spec-ops-tasks` and confirm guardrail script printed the hooks it invoked.
- Schema errors → re-run the guardrail and compare against docs/SPEC-KIT-013-telemetry-schema-guard/spec.md.
- HAL discrepancies → ensure the product repo has a valid `HAL_SECRET_KAVEDARR_API_KEY` and that the Kavedarr API is reachable before retrying downstream validation stages.
