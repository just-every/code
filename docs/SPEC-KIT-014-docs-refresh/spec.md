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

## Task Breakdown (2025-09-28)
### Task Slices
- **Docs lead (Claude)** – Preflight with T18/T20 owners to confirm guardrail fixes are merged and capture fresh HAL healthy/degraded telemetry snapshots before editing.
- **Docs lead (Claude)** – Refresh `docs/slash-commands.md` with stage-by-stage telemetry schema v1 fields, new guardrail flags/env vars, and explicit model metadata requirements linking to `docs/spec-kit/model-strategy.md`.
- **Docs lead (Claude)** – Update `AGENTS.md` guardrail guidance (telemetry envelope + per-stage payload summary, evidence path reminders, cross-link to T20 plan).
- **Onboarding writer (Gemini)** – Expand `docs/getting-started.md` troubleshooting + consensus escalation guidance, trim `RESTART.md` to point to the canonical troubleshooting section, and add HAL validation quickstart notes.
- **Tracker steward (Code)** – Re-run doc/telemetry lint hooks, capture evidence references for SPEC.md row T14, and assemble review package once upstream dependencies clear.

**Dependencies**
- T20 guardrail fixes (baseline enforcement + HAL failure propagation) must land before major doc edits proceed.
- Updated HAL evidence (healthy + degraded) from T18 execution windows.
- Clarification on canonical PRD location referenced in this spec (currently external template).

**Validation**
- `scripts/doc-structure-validate.sh --mode=templates --dry-run` followed by a full run once draft sections stabilize.
- `python3 scripts/spec-kit/lint_tasks.py` to keep tracker hygiene.
- Peer review ensuring telemetry tables align with docs/SPEC-KIT-013-telemetry-schema-guard/spec.md and evidence links resolve.

**Docs**
- `docs/slash-commands.md`, `AGENTS.md`, `docs/getting-started.md`, `RESTART.md`, and references to `docs/spec-kit/model-strategy.md`.

**Risks & Assumptions**
- Guardrail flag churn from T20 could invalidate prepared copy; mitigation: stage edits behind placeholders until fixes merge.
- HAL evidence may stale if T18 reruns slip; coordinate capture windows before doc freeze.
- Operators continue to rely on external PRD; assume documentation will call this out explicitly.

**Consensus**
- Agreement (Claude/Gemini/Code): Docs should trail guardrail telemetry stabilization and include healthy/degraded HAL examples.
- Divergence: Gemini advocated waiting for full automation CI coverage; resolved by gating work on T20 Step 1 and marking blocked sections `[DRAFT – awaiting guardrail fix]`.
- Degraded participation: Only GPT-5 Codex responded directly; other agent positions inferred from plan history, so schedule a quick cross-agent review before execution.
