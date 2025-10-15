# Plan: T14 Documentation Refresh
## Inputs
- Spec: docs/SPEC-KIT-014-docs-refresh/spec.md (3f3c34f0)
- Constitution: memory/constitution.md (8bcab66e)

## Work Breakdown
1. **Preflight & dependencies.** Review docs/SPEC-OPS-004-integrated-coder-hooks/notes/guardrail-hardening.md alongside T18/T20 owners, capture the current HAL telemetry sample (`spec_ops_validate` unhealthy + healthy runs), and confirm whether any pending guardrail fixes must land before doc edits.
2. **Slash command reference update.** Revise docs/slash-commands.md to document telemetry schema v1 per stage, flag new guardrail flags/env vars (e.g., `SPEC_OPS_CARGO_MANIFEST`, `--allow-fail`), and restate the model metadata requirement for `/spec-*` commands with links to docs/spec-kit/model-strategy.md.
3. **Guardrail constitution refresh.** Update AGENTS.md to summarize the telemetry schema envelope + stage payloads, add explicit evidence path guidance, remove stale references to missing templates, and cross-link the guardrail hardening plan.
4. **Onboarding & troubleshooting consolidation.** Expand docs/getting-started.md with workflow quickstart, telemetry evidence locations, HAL troubleshooting, and consensus escalation guidance; trim RESTART.md to session-specific recovery while linking back to the new canonical troubleshooting section.
5. **Validation & sign-off.** Run `scripts/doc-structure-validate.sh --mode=templates` (dry-run first) and `scripts/spec-kit/lint_tasks.py`, perform manual doc review for links/citations, and stage evidence notes for SPEC.md before requesting review.

## Acceptance Mapping
| Requirement (Spec) | Validation Step | Test/Check Artifact |
| --- | --- | --- |
| R1: Slash command reference updated | Manual diff review ensuring `/spec-ops-*` entries list schema v1 fields, new flags, and model metadata pointers | docs/slash-commands.md |
| R2: Guardrail constitution covers telemetry schema | Confirm AGENTS.md lists envelope + per-stage keys and links to docs/SPEC-KIT-013-telemetry-schema-guard/spec.md and docs/spec-kit/model-strategy.md | AGENTS.md |
| R3: Onboarding references evidence + validation commands | Verify docs/getting-started.md (and cross-link from RESTART.md) include evidence paths, HAL helper commands, and validation checklist | docs/getting-started.md |
| R4: Troubleshooting guidance added | Ensure consolidated troubleshooting section covers telemetry failures, degraded consensus, and HAL smoke fallbacks with actionable steps | docs/getting-started.md section + RESTART.md pointer |

## Risks & Unknowns
- Guardrail fixes from T20 may change CLI flags or telemetry fields mid-update; coordinate to avoid documenting stale behavior.
- Product requirements file remains absent, so references to “canonical PRD” must clarify current location.
- HAL environment may fluctuate between healthy/degraded states; need reliable capture windows for screenshots/evidence.

## Consensus & Risks (Multi-AI)
- Agreement: Claude (docs architect), Gemini (planner), and Code (risk manager) all prioritize updating slash-commands, AGENTS.md, onboarding, and troubleshooting with telemetry schema, HAL evidence, and model metadata.
- Disagreement & resolution: Gemini’s risk analysis recommended deferring docs until HAL false-positive handling and missing templates are fixed; resolved by adding Work Breakdown step 1 to gate documentation on prerequisite guardrail outcomes and by tracking blockers with T20 owners.

## Exit Criteria (Done)
- All targeted docs merged with updated telemetry schema guidance, model strategy links, and troubleshooting content.
- Fresh HAL telemetry samples (healthy + degraded) referenced in docs and stored under docs/SPEC-OPS-004-integrated-coder-hooks/evidence/commands/SPEC-KIT-018/.
- `scripts/doc-structure-validate.sh --mode=templates` and `scripts/spec-kit/lint_tasks.py` pass, and reviewer sign-off confirms instructions align with latest guardrail behavior.
