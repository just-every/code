# PRD: Documentation Refresh for Spec Kit Workflow (T14)

## Summary
- **Objective.** Align public/operator-facing docs with the new Spec Kit multi-agent pipeline, telemetry guardrails, and model strategy so teams can follow `/spec-*` workflows without stale guidance.
- **Problem.** Existing docs (AGENTS.md, docs/slash-commands.md, onboarding materials) pre-date the consensus diff reviewer, telemetry schema enforcement, and updated model map; they still reference deprecated aliases and omit telemetry requirements.
- **Outcome.** Repository docs clearly describe the `/spec-ops-*` vs `/spec-*` pairing, model lineup, telemetry schema expectations, and guardrail evidence flows, reducing onboarding time and avoiding stale CLI usage.

## Users & Jobs
- **New Codex operators** need accurate slash-command usage and onboarding steps for configuring Spec Kit pipelines.
- **Maintainers** require reference docs to validate telemetry/evidence expectations and understand model fallbacks.
- **Reviewers** expect SPEC documentation to state how consensus verdicts, telemetry, and guardrails interplay.

## Goals
1. Update `docs/slash-commands.md` with current command descriptions, telemetry expectations, and alias deprecations.
2. Refresh `AGENTS.md` (guardrails) and onboarding docs with model strategy links and telemetry schema summary.
3. Create troubleshooting guidance covering degraded consensus, telemetry failures, and evidence locations.
4. Ensure all docs mention the requirement for new telemetry schema fields (command/specId/... and stage payloads).

## Non-Goals
- Rewriting product requirements in `SPEC.md` beyond aligning terminology.
- Changing the actual guardrail scripts or code; T14 focuses on documentation.
- Authoring marketing copy; scope limited to internal/operator docs.

## Requirements
| ID | Description | Acceptance |
| --- | --- | --- |
| R1 | Slash command reference reflects `/spec-ops-*` + `/spec-*` roles, model metadata, and telemetry schema. | Updated table/section with explicit schema reminders and consensus behavior. |
| R2 | Guardrail Constitution (AGENTS.md) documents telemetry schema expectations and references model strategy. | AGENTS.md includes telemetry schema summary + link to Spec Kit model doc. |
| R3 | Onboarding/getting-started docs highlight telemetry evidence location and validation commands. | docs/getting-started.md or onboarding section includes updated steps. |
| R4 | Add troubleshooting section for degraded consensus/telemetry failures with actionable steps. | New subsection in relevant doc detailing how to resolve failed schema checks. |

## Dependencies & Risks
- Depends on T13 telemetry schema guard to stay stable; docs will reference schema version 1.
- Constitution/product requirement files still absent from repo root; note limitation or restore references.

## Rollout & Success Metrics
- Merge doc changes alongside T13 code updates on `feat/spec-auto-telemetry`.
- After merge, run `scripts/spec-kit/lint_tasks.py` and have operators confirm docs cover new flow.
- Success measured by updated docs being cited in upcoming Spec Ops runs without confusion.

