# Spec: SPEC-KIT-045 Mini (Rehearsal Bundle)

## Context
Provide a tiny rehearsal target for `/spec-ops-*` stages that exercises the critical behaviours of SPEC-KIT-045 without invoking the full evidence footprint. Operators should be able to run any single stage end-to-end in under five minutes.

## Objectives
- Confirm all four agents spawn when `/spec-ops-plan` is invoked in a permissive sandbox.
- Validate telemetry schema v1 fields using the bundled sample JSON.
- Document HAL mock/live switching using the `--hal` flag without requiring real credentials.

## Scope
- Confirm all four agents spawn when `/spec-ops-plan` is invoked in a permissive sandbox.
- Validate telemetry schema v1 fields using the bundled sample JSON.
- Document HAL mock/live switching using the `--hal` flag without requiring real credentials.

## Non-Goals
- Producing production-ready evidence bundles.
- Exercising the full 90-minute methodology documented in the primary SPEC.

## Acceptance Criteria
1. `/spec-ops-plan SPEC-KIT-045-mini` records the agent roster in TUI history.
2. `/spec-ops-validate SPEC-KIT-045-mini --hal mock` emits telemetry that matches the sample schema and notes the mock HAL rationale.
3. Docs in this fixture reference the exact evidence filenames and rerun instructions.
