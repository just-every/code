# Spec: Documentation Refresh for Spec Kit Workflow (T14)

## Context
- Branch: `feat/spec-auto-telemetry`
- T13 introduced telemetry schema enforcement and stage-aware guardrail payloads.
- Slash-command docs and AGENTS guardrails still mention deprecated aliases and omit schema/model metadata.
- Note: `memory/constitution.md` and `product-requirements.md` are missing in this repo; reference templates exist in sibling repos.

## Objectives
1. Document the full `/spec-ops-*` + `/spec-*` pipeline, highlighting telemetry schema requirements.
2. Surface the model strategy table (docs/spec-kit/model-strategy.md) from relevant docs.
3. Provide troubleshooting flow for consensus degradation and telemetry schema failures.
4. Ensure onboarding references telemetry evidence paths (`docs/SPEC-OPS-004-integrated-coder-hooks/evidence/`).

## Target Docs
- `docs/slash-commands.md`
- `AGENTS.md`
- `docs/getting-started.md` (onboarding section)
- `docs/spec-kit/model-strategy.md` (ensure cross-links)
- Optional: `RESTART.md` and `SPEC-KIT.md` quick updates if references outdated.

## Key Updates
- Clarify difference between guardrail commands (`/spec-ops-*`) and multi-agent commands (`/spec-*`), referencing telemetry schema fields (command/specId/sessionId/timestamp/schemaVersion + stage payload).
- Add telemetry schema summary table (common envelope + per-stage requirements) in AGENTS.md or referencing docs/SPEC-KIT-013-telemetry-schema-guard/spec.md.
- Update slash command descriptions to mention model metadata requirement (model/model_release/reasoning_mode) and consensus behavior.
- Include troubleshooting guidance for telemetry schema failures (e.g., rerun guardrail, inspect JSON path) and consensus degradation (rerun stage/higher model budget).
- Mention updated evidence directory layout and requirement to keep `docs/SPEC-OPS-004-integrated-coder-hooks/evidence/` under version control.

## Acceptance Criteria
- Documentation changes reviewed for accuracy with T13 schema.
- Linting/pipelines unaffected (docs only).
- SPEC tracker entry updated with doc paths.

