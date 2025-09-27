# PRD: Telemetry Schema Guard (T13)

## Summary
- **Objective.** Guarantee that every `/spec-ops-*` guardrail emits well-formed telemetry so `/spec-auto` can trust evidence before promoting prompts or consensus runs.
- **Problem.** Current guardrail validation (`collect_guardrail_outcome`) only checks for evidence artifacts. If the telemetry JSON is missing required fields (e.g., `specId`, `sessionId`, stage payloads) the pipeline silently proceeds, leading to mismatched evidence logs and confusing consensus failures.
- **Outcome.** `/spec-auto` must stop the pipeline when telemetry is malformed, surfacing actionable errors and evidence pointers. Valid telemetry should remain transparent to agents via summaries.

## Users & Jobs
- **Spec Kit operator** – wants `/spec-auto` to fail fast if guardrail telemetry is corrupt or incomplete.
- **Consensus reviewer** – expects reliable telemetry metadata (command, timestamps, artifacts) when diffing agent verdicts.
- **Observability auditor** – needs consistent JSON schema for ingestion into external tooling.

## Goals
1. Define a canonical telemetry schema per guardrail stage (Plan, Tasks, Implement, Validate, Audit, Unlock).
2. Enforce schema validation inside `/spec-auto`; malformed telemetry must abort the run with clear failure messages.
3. Backfill automated tests covering valid & invalid payloads (missing fields, wrong types, unknown command).
4. Document schema + operator runbook so future guardrail changes stay consistent.

## Non-Goals
- Changing guardrail shell scripts beyond the data they already emit (TBD improvements tracked separately).
- Restructuring evidence storage locations.
- Building a telemetry ingestion/analytics pipeline; focus is on enforcement inside `/spec-auto` today.

## Requirements
| ID | Description | Acceptance |
| --- | --- | --- |
| R1 | `/spec-auto` must validate telemetry JSON against stage-specific schema before evidence checks. | Invalid JSON (structural or type errors) halts the pipeline with user-facing errors referencing the offending file. |
| R2 | Guardrail telemetry must include common metadata: `command`, `specId`, `sessionId`, ISO8601 UTC `timestamp`. | Missing metadata triggers schema failure; valid metadata is surfaced in history notice. |
| R3 | Stage payload requirements: Plan captures `baseline.mode`, `baseline.artifact`, `baseline.status`, and `hooks.session.start`; Tasks report `tool.status`; Implement includes `lock_status` + `hook_status`; Validate/Audit require scenario arrays; Unlock reports `unlock_status`. | Tests cover each stage with malformed payload to ensure failure. |
| R4 | Schema validator differentiates between parse errors (invalid JSON) and schema violations, logging precise reasons. | Unit tests assert error strings contain root cause; history notice summarises failure. |
| R5 | Documentation summarises telemetry schema and troubleshooting steps. | `docs/SPEC-KIT-013-telemetry-schema-guard/spec.md` includes schema tables + operator guidance. |

## Dependencies & Risks
- Depends on existing guardrail shell scripts continuing to emit JSON; future shell changes must update schema tables.
- Risk: overly strict schema could break older artifacts; mitigation via versioned schema + backwards-compatible allowances (e.g., optional fields for Validate `status = skipped`).
- Risk: synchronous validation may slow `/spec-auto`; expectation is negligible due to small JSON payloads.

## Rollout & Success Metrics
- Roll out on `feat/spec-auto-telemetry` branch; run `cargo test -p codex-tui spec_auto` and new schema tests.
- Success metric: malformed telemetry fixtures cause `/spec-auto` to abort within guardrail summary; no regressions for valid telemetry runs (existing integration tests pass).
- Post-merge: document schema in slash-command docs (tracked by T14 docs refresh).
