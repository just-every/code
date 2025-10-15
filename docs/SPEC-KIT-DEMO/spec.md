# Spec: SPEC-KIT-DEMO Guardrail Baseline (T26)

## Context
- According to Byterover memory layer, SPEC-KIT-DEMO currently has guardrail telemetry but lacks foundational artefacts (`docs/SPEC-KIT-DEMO/{spec.md, plan.md, tasks.md}`) and a tracker row in `SPEC.md`, causing policy prefilters to keep `/spec-plan`, `/spec-validate`, and `/spec-unlock` in a blocked state.
- Product requirements and RESTART.md position SPEC-KIT-DEMO as the reference flow for demonstrating consensus halt gating, telemetry capture, and `/spec-auto` orchestration; without canonical docs the branch `feat/spec-auto-telemetry` cannot reach completion.
- Multi-agent plan and tasks prompts rely on spec docs to seed consensus prompts; missing files produce degraded consensus verdicts and leave ChatWidget without acceptance criteria to enforce.

## Objectives
1. Establish a canonical SPEC-KIT-DEMO document set (spec/plan/tasks) that captures scope, work breakdown, and validation expectations for consensus + telemetry gating.
2. Register SPEC-KIT-DEMO in `SPEC.md` with an active task row so guardrail policy layers can resolve ownership, status, and evidence locations.
3. Provide actionable acceptance criteria that map guardrail telemetry, consensus artefacts, and policy hooks to concrete validation steps.

## Scope
- Author `docs/SPEC-KIT-DEMO/spec.md`, `plan.md`, and `tasks.md`, ensuring they reference current branch work (`feat/spec-auto-telemetry`) and relevant evidence paths under `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/`.
- Update `SPEC.md` with a new task entry (T26) covering SPEC-KIT-DEMO guardrail baseline, including PRD path, status, branch, and notes for evidence collections.
- Capture dependencies on ChatWidget consensus telemetry (per-agent JSON, synthesis.json, telemetry.jsonl) and guardrail scripts so `/spec-auto` can halt on conflict or degraded verdicts.
- Document risks, acceptance mapping, and next steps to re-run guardrail stages once docs and tracker updates land.

## Non-Goals
- Shipping additional consensus automation beyond documenting expectations (e.g., no new CLI features in this spec).
- Replacing HAL integration or extending telemetry schema v1; those remain in existing SPEC-KIT initiatives.
- Automating policy resolution inside this spec; enforcement continues to live in `scripts/spec_ops_004` and ChatWidget logic.

## Acceptance Criteria
- Spec, plan, and tasks docs exist under `docs/SPEC-KIT-DEMO/` and describe scope, work breakdown, acceptance mapping, and risks consistent with RESTART.md guidance.
- `SPEC.md` includes task T26 (SPEC-KIT-DEMO Guardrail Baseline) with Status `In Progress`, PRD column pointing to `docs/SPEC-KIT-DEMO/spec.md`, and notes citing the latest consensus evidence timestamp.
- ChatWidget consensus evidence directory `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/consensus/SPEC-KIT-DEMO/` contains at least one complete bundle (per-agent JSON, synthesis.json, telemetry.jsonl) dated 2025-10-04 or later and referenced in plan/tasks docs.
- Plan and tasks deliverables reuse the required Spec Kit templates, map requirements to telemetry/guardrail validations, and enumerate next steps for rerunning `/spec-plan`, `/spec-auto --from plan`, and related stages once conflicts are resolved.
