# Spec: Telemetry Schema Guard (T13)

## Context
- Branch: `feat/spec-auto-telemetry`
- Guardrail scripts already emit per-stage telemetry JSON under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/<SPEC-ID>/<stage>_*.json`.
- `/spec-auto` currently verifies artifact existence but not telemetry structure, allowing silent drift.

## Objectives
1. Introduce stage-aware telemetry schema validation before evidence checks inside `collect_guardrail_outcome`.
2. Provide actionable error messages that surface which required fields are missing or mis-typed.
3. Keep schema definitions close to enforcement (Rust) and document them for shell maintainers.

## Telemetry Schema Overview

### Common Envelope (all stages)
- `command`: string; must match the guardrail entry point (`spec-ops-plan`, `spec-ops-tasks`, `spec-ops-implement`, `spec-ops-validate`, `spec-ops-audit`, `spec-ops-unlock`).
- `specId`: string identifier (`SPEC-AREA-slug`).
- `sessionId`: unique identifier per guardrail run (UTC timestamp + entropy).
- `timestamp`: ISO8601 UTC string.
- `artifacts`: array of objects `{ "path": "..." }`; required for all stages except Validate, which may omit the array.
- Optional `schemaVersion`: integer; default `1`.

### Stage Requirements
| Stage | Required Fields | Notes |
| --- | --- | --- |
| Plan | `baseline.mode` (`skip|no-run|quick`), `baseline.artifact` (path), `baseline.status` (`passed|skipped|failed`), `hooks.session.start` (`ok|failed|skipped`) | Mirrors existing guardrail evaluation logic and ensures session hook status is recorded. |
| Tasks | `tool.status` (`ok|failed|skipped|ready`) | Guardrail success hinges on tool status; additional metadata optional. |
| Implement | `lock_status` (`locked|failed`), `hook_status` (`ok|failed`) | Telemetry may also include `status`; schema requires dedicated lock/hook fields so evaluation remains deterministic. |
| Validate | `scenarios` array of `{ "name": string, "status": "passed|failed|skipped" }` | Empty or missing array is a schema failure. `errors` array optional. |
| Audit | Same as Validate (`scenarios` array). | Audit guardrail shares validation flow with Validate stage. |
| Unlock | `unlock_status` (`unlocked|failed`) | Optional `status` mirrored for backward compatibility. |

## Implementation Plan (High-Level)
1. **Schema representation.** Implement lightweight validation helpers (`require_string_field`, etc.) that encode stage-specific checks without introducing a JSON Schema dependency.
2. **Validation entry point.** Extend `collect_guardrail_outcome` to call `validate_guardrail_schema(stage, &value)` prior to artifact checks. Combine failures with existing artifact validation results.
3. **Error reporting.** On schema failure, include bullet list of missing/invalid fields in history notice and treat guardrail as unsuccessful.
4. **Tests.** Add unit tests for each stage verifying acceptance/rejection paths. Provide fixtures using `serde_json::json!` to simulate telemetry payloads.

## Data Model Changes
- No database changes. In-memory validations only.
- Introduce helper functions that emit human-readable field-level errors surfaced in the TUI.

## Test Strategy
- `codex-rs/tui/src/chatwidget.rs` tests:
  - `spec_auto_plan_schema_validation_fails_without_baseline`
  - `spec_auto_tasks_schema_validation_requires_status`
  - `spec_auto_validate_schema_detects_bad_scenarios`
- Integration: extend existing `spec_auto_evidence_*` tests to include schema success case.

## Rollout
- Feature flag not required; schema operates immediately.
- If unexpected telemetry appears in existing evidence files, operators rerun guardrails after updating shells.

## Open Questions
- Should telemetry schema allow versioning? (For now embed `schemaVersion: 1` optional field; treat missing as default.)
- How strict should Validate stage be about scenario count? Proposed approach: empty array triggers failure; missing field triggers failure.
